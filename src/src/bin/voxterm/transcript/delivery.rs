//! Transcript delivery flow so queued text reaches PTY sessions with clear status updates.

use anyhow::Result;
use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use voxterm::{log_debug, VoiceCaptureSource};

use crate::config::VoiceSendMode;
use crate::prompt::PromptTracker;
use crate::status_line::StatusLineState;
use crate::writer::{set_status, WriterMessage};

use super::idle::transcript_ready;
use super::queue::PendingTranscript;
use super::session::TranscriptSession;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt::{PromptLogger, PromptTracker};
    use crate::transcript::push_pending_transcript;
    use crossbeam_channel::Receiver;
    use regex::Regex;

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

    fn recv_output_contains(rx: &Receiver<Vec<u8>>, needle: &str) -> bool {
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
    fn try_flush_pending_waits_for_prompt_when_busy() {
        let mut pending = VecDeque::new();
        push_pending_transcript(
            &mut pending,
            PendingTranscript {
                text: "hello".to_string(),
                source: VoiceCaptureSource::Native,
                mode: VoiceSendMode::Auto,
            },
        );

        let logger = PromptLogger::new(None);
        let regex = Regex::new(r"^> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), false, logger);

        let (writer_tx, _writer_rx) = crossbeam_channel::bounded(8);
        let mut session = StubSession::default();
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = crate::status_line::StatusLineState::new();
        let mut last_enter_at = Some(Instant::now());
        tracker.feed_output(b"working...\n");
        let now = Instant::now();
        {
            let mut io = TranscriptIo {
                session: &mut session,
                writer_tx: &writer_tx,
                status_clear_deadline: &mut deadline,
                current_status: &mut current_status,
                status_state: &mut status_state,
            };
            try_flush_pending(
                &mut pending,
                &tracker,
                &mut last_enter_at,
                &mut io,
                now,
                Duration::from_secs(2),
            );
        }
        assert!(!pending.is_empty());
        assert!(session.sent_with_newline.is_empty());

        tracker.feed_output(b"> \n");
        let now = Instant::now();
        {
            let mut io = TranscriptIo {
                session: &mut session,
                writer_tx: &writer_tx,
                status_clear_deadline: &mut deadline,
                current_status: &mut current_status,
                status_state: &mut status_state,
            };
            try_flush_pending(
                &mut pending,
                &tracker,
                &mut last_enter_at,
                &mut io,
                now,
                Duration::from_secs(2),
            );
        }
        assert!(pending.is_empty());
        assert_eq!(session.sent_with_newline, vec!["hello"]);
    }

    #[test]
    fn deliver_transcript_injects_into_pty() {
        let mut session =
            voxterm::pty_session::PtyOverlaySession::new("cat", ".", &[], "xterm-256color")
                .expect("pty session");
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
