use anyhow::Result;
use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use voxterm::log_debug;
use voxterm::pty_session::PtyOverlaySession;
use voxterm::VoiceCaptureSource;

use crate::config::VoiceSendMode;
use crate::prompt::PromptTracker;
use crate::status_line::StatusLineState;
use crate::writer::{set_status, WriterMessage};

const MAX_PENDING_TRANSCRIPTS: usize = 5;

/// Abstraction over a destination that can accept transcript text.
pub(crate) trait TranscriptSession {
    /// Send text without a trailing newline (insert mode).
    fn send_text(&mut self, text: &str) -> Result<()>;
    /// Send text and append a newline (auto-send mode).
    fn send_text_with_newline(&mut self, text: &str) -> Result<()>;
}

impl TranscriptSession for PtyOverlaySession {
    fn send_text(&mut self, text: &str) -> Result<()> {
        self.send_text(text)
    }

    fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
        self.send_text_with_newline(text)
    }
}

/// Transcript queued while the CLI is busy.
pub(crate) struct PendingTranscript {
    /// Raw transcript text.
    pub(crate) text: String,
    /// Pipeline that produced the transcript.
    pub(crate) source: VoiceCaptureSource,
    /// Send mode to apply when flushing.
    pub(crate) mode: VoiceSendMode,
}

struct PendingBatch {
    text: String,
    label: String,
    mode: VoiceSendMode,
}

/// Context bundle for transcript delivery and status updates.
pub(crate) struct TranscriptIo<'a, S: TranscriptSession> {
    /// Destination session that accepts transcript text.
    pub(crate) session: &'a mut S,
    /// Writer channel for UI status updates.
    pub(crate) writer_tx: &'a Sender<WriterMessage>,
    /// Deadline for clearing status text.
    pub(crate) status_clear_deadline: &'a mut Option<Instant>,
    /// Last status message shown.
    pub(crate) current_status: &'a mut Option<String>,
    /// Current status-line state for overlay rendering.
    pub(crate) status_state: &'a mut StatusLineState,
}

impl<'a, S: TranscriptSession> TranscriptIo<'a, S> {
    pub(crate) fn set_status(&mut self, text: &str, clear_after: Option<Duration>) {
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            text,
            clear_after,
        );
    }
}

pub(crate) fn push_pending_transcript(
    pending: &mut VecDeque<PendingTranscript>,
    transcript: PendingTranscript,
) -> bool {
    if pending.len() >= MAX_PENDING_TRANSCRIPTS {
        pending.pop_front();
        log_debug("pending transcript queue full; dropping oldest transcript");
        pending.push_back(transcript);
        return true;
    }
    pending.push_back(transcript);
    false
}

pub(crate) fn try_flush_pending<S: TranscriptSession>(
    pending: &mut VecDeque<PendingTranscript>,
    prompt_tracker: &PromptTracker,
    last_enter_at: &mut Option<Instant>,
    io: &mut TranscriptIo<'_, S>,
    now: Instant,
    transcript_idle_timeout: Duration,
) {
    if pending.is_empty()
        || !transcript_ready(prompt_tracker, *last_enter_at, now, transcript_idle_timeout)
    {
        return;
    }
    let Some(batch) = merge_pending_transcripts(pending) else {
        return;
    };
    let remaining = pending.len();
    io.status_state.queue_depth = remaining;
    let sent_newline =
        deliver_transcript(&batch.text, &batch.label, batch.mode, io, remaining, None);
    if sent_newline {
        *last_enter_at = Some(Instant::now());
    }
}

fn merge_pending_transcripts(pending: &mut VecDeque<PendingTranscript>) -> Option<PendingBatch> {
    // Batch consecutive transcripts with the same send mode to avoid mixing auto/insert.
    let mode = pending.front()?.mode;
    let mut parts: Vec<String> = Vec::new();
    let mut sources: Vec<VoiceCaptureSource> = Vec::new();
    while let Some(next) = pending.front() {
        if next.mode != mode {
            break;
        }
        let Some(next) = pending.pop_front() else {
            break;
        };
        let trimmed = next.text.trim();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
            sources.push(next.source);
        }
    }
    if parts.is_empty() {
        return None;
    }
    let label = if sources.iter().all(|source| *source == sources[0]) {
        sources[0].label().to_string()
    } else {
        "Mixed pipelines".to_string()
    };
    Some(PendingBatch {
        text: parts.join(" "),
        label,
        mode,
    })
}

pub(crate) fn send_transcript(
    session: &mut impl TranscriptSession,
    text: &str,
    mode: VoiceSendMode,
) -> Result<bool> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    match mode {
        VoiceSendMode::Auto => {
            session.send_text_with_newline(trimmed)?;
            Ok(true)
        }
        VoiceSendMode::Insert => {
            session.send_text(trimmed)?;
            Ok(false)
        }
    }
}

pub(crate) fn deliver_transcript<S: TranscriptSession>(
    text: &str,
    label: &str,
    mode: VoiceSendMode,
    io: &mut TranscriptIo<'_, S>,
    queued_remaining: usize,
    drop_note: Option<&str>,
) -> bool {
    let mut label = label.to_string();
    if let Some(note) = drop_note {
        label.push_str(", ");
        label.push_str(note);
    }
    let status = if queued_remaining > 0 {
        format!("Transcript ready ({label}) • queued {queued_remaining}")
    } else {
        format!("Transcript ready ({label})")
    };
    io.set_status(&status, Some(Duration::from_secs(2)));
    match send_transcript(io.session, text, mode) {
        Ok(sent_newline) => sent_newline,
        Err(err) => {
            log_debug(&format!("failed to send transcript: {err:#}"));
            io.set_status(
                "Failed to send transcript (see log)",
                Some(Duration::from_secs(2)),
            );
            false
        }
    }
}

fn prompt_ready(prompt_tracker: &PromptTracker, last_enter_at: Option<Instant>) -> bool {
    match (prompt_tracker.last_prompt_seen_at(), last_enter_at) {
        (Some(prompt_at), Some(enter_at)) => prompt_at > enter_at,
        (Some(_), None) => true,
        _ => false,
    }
}

pub(crate) fn transcript_ready(
    prompt_tracker: &PromptTracker,
    last_enter_at: Option<Instant>,
    now: Instant,
    transcript_idle_timeout: Duration,
) -> bool {
    if prompt_ready(prompt_tracker, last_enter_at) {
        return true;
    }
    let idle_ready = if let Some(last_output_at) = prompt_tracker.last_pty_output_at() {
        now.duration_since(last_output_at) >= transcript_idle_timeout
    } else {
        prompt_tracker.idle_ready(now, transcript_idle_timeout)
    };
    if prompt_tracker.last_prompt_seen_at().is_none() {
        return idle_ready;
    }
    if let (Some(enter_at), Some(last_output_at)) =
        (last_enter_at, prompt_tracker.last_pty_output_at())
    {
        if last_output_at >= enter_at && idle_ready {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::{PromptLogger, PromptTracker};
    use regex::Regex;
    use std::time::Duration;

    #[derive(Default)]
    struct StubSession {
        sent: Vec<String>,
        sent_with_newline: Vec<String>,
    }

    impl TranscriptSession for StubSession {
        fn send_text(&mut self, text: &str) -> Result<()> {
            self.sent.push(text.to_string());
            Ok(())
        }

        fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
            self.sent_with_newline.push(text.to_string());
            Ok(())
        }
    }

    fn recv_output_contains(rx: &crossbeam_channel::Receiver<Vec<u8>>, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut buffer = String::new();
        while Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if buffer.contains(needle) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn send_transcript_respects_mode_and_trims() {
        let mut session = StubSession::default();
        let sent = send_transcript(&mut session, " hello ", VoiceSendMode::Auto).unwrap();
        assert!(sent);
        assert_eq!(session.sent_with_newline, vec!["hello"]);

        let sent = send_transcript(&mut session, " hi ", VoiceSendMode::Insert).unwrap();
        assert!(!sent);
        assert_eq!(session.sent, vec!["hi"]);

        let sent = send_transcript(&mut session, "   ", VoiceSendMode::Insert).unwrap();
        assert!(!sent);
        assert_eq!(session.sent.len(), 1);
    }

    #[test]
    fn push_pending_transcript_drops_oldest_when_full() {
        let mut pending = VecDeque::new();
        for i in 0..MAX_PENDING_TRANSCRIPTS {
            let dropped = push_pending_transcript(
                &mut pending,
                PendingTranscript {
                    text: format!("t{i}"),
                    source: VoiceCaptureSource::Native,
                    mode: VoiceSendMode::Auto,
                },
            );
            assert!(!dropped);
        }
        let dropped = push_pending_transcript(
            &mut pending,
            PendingTranscript {
                text: "last".to_string(),
                source: VoiceCaptureSource::Native,
                mode: VoiceSendMode::Auto,
            },
        );
        assert!(dropped);
        assert_eq!(pending.len(), MAX_PENDING_TRANSCRIPTS);
        assert_eq!(pending.front().unwrap().text, "t1");
        assert_eq!(pending.back().unwrap().text, "last");
    }

    #[test]
    fn try_flush_pending_sends_when_idle_ready() {
        let mut pending = VecDeque::new();
        push_pending_transcript(
            &mut pending,
            PendingTranscript {
                text: "hello".to_string(),
                source: VoiceCaptureSource::Native,
                mode: VoiceSendMode::Auto,
            },
        );
        push_pending_transcript(
            &mut pending,
            PendingTranscript {
                text: "world".to_string(),
                source: VoiceCaptureSource::Native,
                mode: VoiceSendMode::Auto,
            },
        );

        let logger = PromptLogger::new(None);
        let mut tracker = PromptTracker::new(None, true, logger);
        let now = Instant::now();
        tracker.note_activity(now);

        let (writer_tx, _writer_rx) = crossbeam_channel::bounded(8);
        let mut session = StubSession::default();
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = crate::status_line::StatusLineState::new();
        let mut io = TranscriptIo {
            session: &mut session,
            writer_tx: &writer_tx,
            status_clear_deadline: &mut deadline,
            current_status: &mut current_status,
            status_state: &mut status_state,
        };
        let idle_timeout = Duration::from_millis(50);
        let mut last_enter_at = None;
        try_flush_pending(
            &mut pending,
            &tracker,
            &mut last_enter_at,
            &mut io,
            now + idle_timeout + Duration::from_millis(1),
            idle_timeout,
        );
        assert_eq!(session.sent_with_newline, vec!["hello world"]);
        assert!(pending.is_empty());
    }

    #[test]
    fn transcript_ready_falls_back_after_output_idle_since_enter() {
        let logger = PromptLogger::new(None);
        let regex = Regex::new(r"^> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), false, logger);

        tracker.feed_output(b"> \n");
        let last_enter_at = Some(Instant::now());
        tracker.feed_output(b"working...\n");

        let idle_timeout = Duration::from_millis(10);
        let now = Instant::now() + idle_timeout + Duration::from_millis(1);
        assert!(transcript_ready(&tracker, last_enter_at, now, idle_timeout));
    }

    #[test]
    fn transcript_session_impl_sends_text() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text(&mut session, "ping\n").expect("send text");
        assert!(recv_output_contains(&session.output_rx, "ping"));
    }

    #[test]
    fn transcript_session_impl_sends_text_with_newline() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text_with_newline(&mut session, "pong")
            .expect("send text with newline");
        assert!(recv_output_contains(&session.output_rx, "pong"));
    }

    #[test]
    fn deliver_transcript_injects_into_pty() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        let (writer_tx, _writer_rx) = crossbeam_channel::bounded(8);
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = crate::status_line::StatusLineState::new();
        let mut io = TranscriptIo {
            session: &mut session,
            writer_tx: &writer_tx,
            status_clear_deadline: &mut deadline,
            current_status: &mut current_status,
            status_state: &mut status_state,
        };
        let sent_newline =
            deliver_transcript("hello", "Rust", VoiceSendMode::Auto, &mut io, 0, None);
        assert!(sent_newline);
        assert!(recv_output_contains(&session.output_rx, "hello"));
    }
}
