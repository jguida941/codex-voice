//! Voice-job drain logic so capture results integrate safely with transcript queues.

use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use voxterm::{log_debug, VoiceCaptureSource, VoiceCaptureTrigger, VoiceJobMessage};

use crate::config::{OverlayConfig, VoiceSendMode};
use crate::prompt::PromptTracker;
use crate::session_stats::SessionStats;
use crate::status_line::{Pipeline, RecordingState, StatusLineState};
use crate::transcript::{
    deliver_transcript, push_pending_transcript, send_transcript, transcript_ready,
    try_flush_pending, PendingTranscript, TranscriptIo, TranscriptSession,
};
use crate::voice_macros::VoiceMacros;
use crate::writer::{set_status, WriterMessage};

use super::manager::{start_voice_capture, VoiceManager};
use super::pipeline::pipeline_status_label;
use super::{PREVIEW_CLEAR_MS, STATUS_TOAST_SECS, TRANSCRIPT_PREVIEW_MAX};

pub(crate) fn clear_capture_metrics(status_state: &mut StatusLineState) {
    status_state.recording_duration = None;
    status_state.meter_db = None;
    status_state.meter_levels.clear();
}

pub(crate) fn handle_voice_message(
    message: VoiceJobMessage,
    ctx: &mut VoiceMessageContext<'_, impl TranscriptSession>,
) {
    let VoiceMessageContext {
        config,
        session,
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        session_stats,
        auto_voice_enabled,
    } = ctx;
    let auto_voice_enabled = *auto_voice_enabled;
    match message {
        VoiceJobMessage::Transcript {
            text,
            source,
            metrics,
        } => {
            let duration_secs = metrics
                .as_ref()
                .map(|metrics| metrics.speech_ms as f32 / 1000.0)
                .unwrap_or(0.0);
            session_stats.record_transcript(duration_secs);
            status_state.recording_state = RecordingState::Idle;
            clear_capture_metrics(status_state);
            status_state.pipeline = match source {
                VoiceCaptureSource::Native => Pipeline::Rust,
                VoiceCaptureSource::Python => Pipeline::Python,
            };
            let label = pipeline_status_label(source);
            let drop_note = metrics
                .as_ref()
                .filter(|metrics| metrics.frames_dropped > 0)
                .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
            let status = if let Some(note) = drop_note {
                format!("Transcript ready ({label}, {note})")
            } else {
                format!("Transcript ready ({label})")
            };
            set_status(
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                &status,
                Some(Duration::from_secs(STATUS_TOAST_SECS)),
            );
            if let Err(err) = send_transcript(*session, &text, config.voice_send_mode) {
                log_debug(&format!("failed to send transcript: {err:#}"));
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                    "Failed to send transcript (see log)",
                    Some(Duration::from_secs(STATUS_TOAST_SECS)),
                );
            }
        }
        VoiceJobMessage::Empty { source, metrics } => {
            session_stats.record_empty();
            status_state.recording_state = RecordingState::Idle;
            clear_capture_metrics(status_state);
            status_state.pipeline = match source {
                VoiceCaptureSource::Native => Pipeline::Rust,
                VoiceCaptureSource::Python => Pipeline::Python,
            };
            let label = pipeline_status_label(source);
            let drop_note = metrics
                .as_ref()
                .filter(|metrics| metrics.frames_dropped > 0)
                .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
            if auto_voice_enabled {
                log_debug(&format!("auto voice capture detected no speech ({label})"));
                // Don't show redundant "Auto-voice enabled" - the mode indicator shows it
                // Only show a note if frames were dropped
                if let Some(note) = drop_note {
                    set_status(
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                        &format!("Listening... ({note})"),
                        Some(Duration::from_secs(STATUS_TOAST_SECS)),
                    );
                }
                // Otherwise leave the message empty - mode indicator shows we're in auto mode
            } else {
                let status = if let Some(note) = drop_note {
                    format!("No speech detected ({label}, {note})")
                } else {
                    format!("No speech detected ({label})")
                };
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                    &status,
                    Some(Duration::from_secs(STATUS_TOAST_SECS)),
                );
            }
        }
        VoiceJobMessage::Error(message) => {
            session_stats.record_error();
            status_state.recording_state = RecordingState::Idle;
            clear_capture_metrics(status_state);
            set_status(
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                "Voice capture error (see log)",
                Some(Duration::from_secs(STATUS_TOAST_SECS)),
            );
            log_debug(&format!("voice capture error: {message}"));
        }
    }
}

pub(crate) struct VoiceMessageContext<'a, S: TranscriptSession> {
    pub config: &'a OverlayConfig,
    pub session: &'a mut S,
    pub writer_tx: &'a Sender<WriterMessage>,
    pub status_clear_deadline: &'a mut Option<Instant>,
    pub current_status: &'a mut Option<String>,
    pub status_state: &'a mut StatusLineState,
    pub session_stats: &'a mut SessionStats,
    pub auto_voice_enabled: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn drain_voice_messages<S: TranscriptSession>(
    voice_manager: &mut VoiceManager,
    config: &OverlayConfig,
    voice_macros: &VoiceMacros,
    session: &mut S,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    session_stats: &mut SessionStats,
    pending_transcripts: &mut VecDeque<PendingTranscript>,
    prompt_tracker: &mut PromptTracker,
    last_enter_at: &mut Option<Instant>,
    now: Instant,
    transcript_idle_timeout: Duration,
    recording_started_at: &mut Option<Instant>,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
    last_auto_trigger_at: &mut Option<Instant>,
    auto_voice_enabled: bool,
    sound_on_complete: bool,
    sound_on_error: bool,
) {
    let Some(message) = voice_manager.poll_message() else {
        return;
    };
    let rearm_auto = matches!(
        message,
        VoiceJobMessage::Empty { .. } | VoiceJobMessage::Error(_)
    );
    match message {
        VoiceJobMessage::Transcript {
            text,
            source,
            metrics,
        } => {
            let expanded = voice_macros.apply(&text, config.voice_send_mode);
            let text = expanded.text;
            let transcript_mode = expanded.mode;
            let macro_note = expanded
                .matched_trigger
                .as_ref()
                .map(|trigger| format!("macro '{}'", trigger));
            update_last_latency(status_state, *recording_started_at, metrics.as_ref(), now);
            let ready =
                transcript_ready(prompt_tracker, *last_enter_at, now, transcript_idle_timeout);
            if auto_voice_enabled {
                prompt_tracker.note_activity(now);
            }
            status_state.recording_state = RecordingState::Idle;
            clear_capture_metrics(status_state);
            status_state.pipeline = match source {
                VoiceCaptureSource::Native => Pipeline::Rust,
                VoiceCaptureSource::Python => Pipeline::Python,
            };
            let preview = format_transcript_preview(&text, TRANSCRIPT_PREVIEW_MAX);
            if preview.is_empty() {
                status_state.transcript_preview = None;
                *preview_clear_deadline = None;
            } else {
                status_state.transcript_preview = Some(preview);
                *preview_clear_deadline = Some(now + Duration::from_millis(PREVIEW_CLEAR_MS));
            }
            let drop_note = metrics
                .as_ref()
                .filter(|metrics| metrics.frames_dropped > 0)
                .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
            let delivery_note = match (drop_note.as_deref(), macro_note.as_deref()) {
                (Some(drop), Some(macro_note)) => Some(format!("{drop}, {macro_note}")),
                (Some(drop), None) => Some(drop.to_string()),
                (None, Some(macro_note)) => Some(macro_note.to_string()),
                (None, None) => None,
            };
            let duration_secs = metrics
                .as_ref()
                .map(|metrics| metrics.speech_ms as f32 / 1000.0)
                .unwrap_or(0.0);
            session_stats.record_transcript(duration_secs);
            let queued_suffix = delivery_note
                .as_ref()
                .map(|note| format!(", {note}"))
                .unwrap_or_default();
            if ready && pending_transcripts.is_empty() {
                let mut io = TranscriptIo {
                    session,
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                };
                let sent_newline = deliver_transcript(
                    &text,
                    source.label(),
                    transcript_mode,
                    &mut io,
                    0,
                    delivery_note.as_deref(),
                );
                if sent_newline {
                    *last_enter_at = Some(now);
                }
            } else {
                let dropped = push_pending_transcript(
                    pending_transcripts,
                    PendingTranscript {
                        text,
                        source,
                        mode: transcript_mode,
                    },
                );
                status_state.queue_depth = pending_transcripts.len();
                if dropped {
                    set_status(
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                        "Transcript queue full (oldest dropped)",
                        Some(Duration::from_secs(2)),
                    );
                }
                if ready {
                    let mut io = TranscriptIo {
                        session,
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                    };
                    try_flush_pending(
                        pending_transcripts,
                        prompt_tracker,
                        last_enter_at,
                        &mut io,
                        now,
                        transcript_idle_timeout,
                    );
                } else if !dropped {
                    let status = format!(
                        "Transcript queued ({}{})",
                        pending_transcripts.len(),
                        queued_suffix
                    );
                    set_status(
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                        &status,
                        None,
                    );
                }
            }
            if auto_voice_enabled
                && config.voice_send_mode == VoiceSendMode::Insert
                && pending_transcripts.is_empty()
                && voice_manager.is_idle()
            {
                if let Err(err) = start_voice_capture(
                    voice_manager,
                    VoiceCaptureTrigger::Auto,
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                ) {
                    log_debug(&format!("auto voice capture failed: {err:#}"));
                } else {
                    *last_auto_trigger_at = Some(now);
                    *recording_started_at = Some(now);
                    reset_capture_visuals(status_state, preview_clear_deadline, last_meter_update);
                }
            }
            if sound_on_complete {
                let _ = writer_tx.send(WriterMessage::Bell { count: 1 });
            }
        }
        VoiceJobMessage::Empty { source, metrics } => {
            update_last_latency(status_state, *recording_started_at, metrics.as_ref(), now);
            let mut ctx = VoiceMessageContext {
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
            };
            handle_voice_message(VoiceJobMessage::Empty { source, metrics }, &mut ctx);
        }
        other => {
            if sound_on_error && matches!(other, VoiceJobMessage::Error(_)) {
                let _ = writer_tx.send(WriterMessage::Bell { count: 2 });
            }
            let mut ctx = VoiceMessageContext {
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
            };
            handle_voice_message(other, &mut ctx);
        }
    }
    if auto_voice_enabled && rearm_auto {
        prompt_tracker.note_activity(now);
    }
    if status_state.recording_state != RecordingState::Recording {
        *recording_started_at = None;
    }
}

fn update_last_latency(
    status_state: &mut StatusLineState,
    recording_started_at: Option<Instant>,
    metrics: Option<&voxterm::audio::CaptureMetrics>,
    now: Instant,
) {
    #[inline]
    fn clamp_u64_to_u32(value: u64) -> u32 {
        value.min(u64::from(u32::MAX)) as u32
    }

    let Some(started_at) = recording_started_at else {
        return;
    };
    let Some(elapsed) = now.checked_duration_since(started_at) else {
        return;
    };
    let elapsed_ms = elapsed.as_millis().min(u128::from(u32::MAX)) as u32;

    let capture_ms = metrics
        .filter(|m| m.capture_ms > 0)
        .map(|m| clamp_u64_to_u32(m.capture_ms));
    let stt_ms = metrics
        .filter(|m| m.transcribe_ms > 0)
        .map(|m| clamp_u64_to_u32(m.transcribe_ms));

    // HUD latency is intentionally "processing after capture" rather than
    // full utterance time, because recording duration is already shown separately.
    // If we do not have enough metrics to estimate processing latency, hide it.
    let latency_ms = match (stt_ms, capture_ms) {
        (Some(stt), _) => Some(stt),
        (None, Some(capture)) => Some(elapsed_ms.saturating_sub(capture)),
        (None, None) => None,
    };

    status_state.last_latency_ms = latency_ms;

    let display_field = latency_ms
        .map(|v| v.to_string())
        .unwrap_or_else(|| "na".to_string());
    let capture_field = capture_ms
        .map(|v| v.to_string())
        .unwrap_or_else(|| "na".to_string());
    let stt_field = stt_ms
        .map(|v| v.to_string())
        .unwrap_or_else(|| "na".to_string());
    log_debug(&format!(
        "latency_audit|display_ms={display_field}|elapsed_ms={elapsed_ms}|capture_ms={capture_field}|stt_ms={stt_field}"
    ));
}

fn format_transcript_preview(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut collapsed = String::new();
    let mut last_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() || ch.is_ascii_control() {
            if !last_space {
                collapsed.push(' ');
                last_space = true;
            }
        } else {
            collapsed.push(ch);
            last_space = false;
        }
    }
    let cleaned = collapsed.trim();
    let max_len = max_len.max(4);
    if cleaned.chars().count() > max_len {
        let keep = max_len.saturating_sub(3);
        let prefix: String = cleaned.chars().take(keep).collect();
        format!("{prefix}...")
    } else {
        cleaned.to_string()
    }
}

pub(crate) fn reset_capture_visuals(
    status_state: &mut StatusLineState,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
) {
    status_state.transcript_preview = None;
    *preview_clear_deadline = None;
    *last_meter_update = Instant::now();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::VoiceSendMode;
    use crate::transcript::TranscriptSession;
    use clap::Parser;
    use voxterm::audio::CaptureMetrics;
    use voxterm::config::AppConfig;

    #[derive(Default)]
    struct StubSession {
        sent: Vec<String>,
        sent_with_newline: Vec<String>,
    }

    impl TranscriptSession for StubSession {
        fn send_text(&mut self, text: &str) -> anyhow::Result<()> {
            self.sent.push(text.to_string());
            Ok(())
        }

        fn send_text_with_newline(&mut self, text: &str) -> anyhow::Result<()> {
            self.sent_with_newline.push(text.to_string());
            Ok(())
        }
    }

    #[test]
    fn handle_voice_message_sends_status_and_transcript() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        let mut session = StubSession::default();
        let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut session_stats = SessionStats::new();
        let mut ctx = VoiceMessageContext {
            config: &config,
            session: &mut session,
            writer_tx: &writer_tx,
            status_clear_deadline: &mut deadline,
            current_status: &mut current_status,
            status_state: &mut status_state,
            session_stats: &mut session_stats,
            auto_voice_enabled: false,
        };

        handle_voice_message(
            VoiceJobMessage::Transcript {
                text: " hello ".to_string(),
                source: VoiceCaptureSource::Native,
                metrics: None,
            },
            &mut ctx,
        );

        let msg = writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("Transcript ready"));
            }
            _ => panic!("unexpected writer message"),
        }
        assert_eq!(session.sent_with_newline, vec!["hello"]);
    }

    #[test]
    fn update_last_latency_prefers_stt_metrics_when_available() {
        let mut status_state = StatusLineState::new();
        let now = Instant::now();
        let started_at = now - Duration::from_millis(1800);
        let metrics = CaptureMetrics {
            capture_ms: 1450,
            transcribe_ms: 220,
            ..Default::default()
        };

        update_last_latency(&mut status_state, Some(started_at), Some(&metrics), now);

        assert_eq!(status_state.last_latency_ms, Some(220));
    }

    #[test]
    fn update_last_latency_uses_elapsed_minus_capture_when_stt_missing() {
        let mut status_state = StatusLineState::new();
        let now = Instant::now();
        let started_at = now - Duration::from_millis(2000);
        let metrics = CaptureMetrics {
            capture_ms: 1500,
            transcribe_ms: 0,
            ..Default::default()
        };

        update_last_latency(&mut status_state, Some(started_at), Some(&metrics), now);

        assert_eq!(status_state.last_latency_ms, Some(500));
    }

    #[test]
    fn update_last_latency_hides_badge_when_metrics_missing() {
        let mut status_state = StatusLineState::new();
        status_state.last_latency_ms = Some(777);
        let now = Instant::now();
        let started_at = now - Duration::from_millis(1400);

        update_last_latency(&mut status_state, Some(started_at), None, now);

        assert_eq!(status_state.last_latency_ms, None);
    }

    #[test]
    fn clear_capture_metrics_resets_recording_artifacts() {
        let mut status_state = StatusLineState::new();
        status_state.recording_duration = Some(3.2);
        status_state.meter_db = Some(-12.0);
        status_state.meter_levels.extend_from_slice(&[-40.0, -20.0]);

        clear_capture_metrics(&mut status_state);

        assert!(status_state.recording_duration.is_none());
        assert!(status_state.meter_db.is_none());
        assert!(status_state.meter_levels.is_empty());
    }
}
