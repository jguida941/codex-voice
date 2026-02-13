use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use voxterm::audio;
use voxterm::pty_session::PtyOverlaySession;

use crate::buttons::ButtonRegistry;
use crate::config::OverlayConfig;
use crate::input::InputEvent;
use crate::overlays::OverlayMode;
use crate::prompt::PromptTracker;
use crate::session_stats::SessionStats;
use crate::settings::SettingsMenuState;
use crate::status_line::StatusLineState;
use crate::theme::Theme;
use crate::transcript::PendingTranscript;
use crate::voice_control::VoiceManager;
use crate::writer::WriterMessage;

pub(crate) struct EventLoopState {
    pub(crate) config: OverlayConfig,
    pub(crate) status_state: StatusLineState,
    pub(crate) auto_voice_enabled: bool,
    pub(crate) theme: Theme,
    pub(crate) overlay_mode: OverlayMode,
    pub(crate) settings_menu: SettingsMenuState,
    pub(crate) meter_levels: VecDeque<f32>,
    pub(crate) theme_picker_selected: usize,
    pub(crate) theme_picker_digits: String,
    pub(crate) current_status: Option<String>,
    pub(crate) pending_transcripts: VecDeque<PendingTranscript>,
    pub(crate) session_stats: SessionStats,
    pub(crate) prompt_tracker: PromptTracker,
    pub(crate) terminal_rows: u16,
    pub(crate) terminal_cols: u16,
    pub(crate) last_recording_duration: f32,
    pub(crate) processing_spinner_index: usize,
    pub(crate) pending_pty_output: Option<Vec<u8>>,
    pub(crate) pending_pty_input: VecDeque<Vec<u8>>,
    pub(crate) pending_pty_input_offset: usize,
    pub(crate) pending_pty_input_bytes: usize,
}

pub(crate) struct EventLoopTimers {
    pub(crate) theme_picker_digit_deadline: Option<Instant>,
    pub(crate) status_clear_deadline: Option<Instant>,
    pub(crate) preview_clear_deadline: Option<Instant>,
    pub(crate) last_auto_trigger_at: Option<Instant>,
    pub(crate) last_enter_at: Option<Instant>,
    pub(crate) recording_started_at: Option<Instant>,
    pub(crate) last_recording_update: Instant,
    pub(crate) last_processing_tick: Instant,
    pub(crate) last_heartbeat_tick: Instant,
    pub(crate) last_meter_update: Instant,
}

pub(crate) struct EventLoopDeps {
    pub(crate) session: PtyOverlaySession,
    pub(crate) voice_manager: VoiceManager,
    pub(crate) writer_tx: Sender<WriterMessage>,
    pub(crate) input_rx: Receiver<InputEvent>,
    pub(crate) button_registry: ButtonRegistry,
    pub(crate) backend_label: String,
    pub(crate) sound_on_complete: bool,
    pub(crate) sound_on_error: bool,
    pub(crate) live_meter: audio::LiveMeter,
    pub(crate) meter_update_ms: u64,
    pub(crate) auto_idle_timeout: Duration,
    pub(crate) transcript_idle_timeout: Duration,
}
