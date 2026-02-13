//! Shared status-line state so rendering and interactions read one source of truth.

use crate::buttons::ButtonAction;
use crate::config::{HudRightPanel, HudStyle, VoiceSendMode};

/// Current voice mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VoiceMode {
    /// Auto-voice mode (hands-free)
    Auto,
    /// Manual voice mode (push-to-talk)
    Manual,
    /// Voice disabled/idle
    #[default]
    Idle,
}

impl VoiceMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Manual => "MANUAL",
            Self::Idle => "IDLE",
        }
    }

    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Auto => "◉",
            Self::Manual => "●",
            Self::Idle => "○",
        }
    }
}

/// Current recording state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordingState {
    /// Not recording
    #[default]
    Idle,
    /// Recording in progress
    Recording,
    /// Processing recorded audio
    Processing,
}

/// Pipeline being used for voice capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pipeline {
    /// Native Rust pipeline
    #[default]
    Rust,
    /// Python fallback pipeline
    Python,
}

impl Pipeline {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
        }
    }
}

/// Maximum number of meter level samples to keep for waveform display.
pub const METER_HISTORY_MAX: usize = 24;

/// A clickable button's position in the status bar.
#[derive(Debug, Clone)]
pub struct ButtonPosition {
    /// Start column (1-based, inclusive)
    pub start_x: u16,
    /// End column (1-based, inclusive)
    pub end_x: u16,
    /// Row from bottom of HUD (1 = bottom border row, 2 = shortcuts row in full mode)
    pub row: u16,
    /// Action to trigger when clicked
    pub action: ButtonAction,
}

/// Multi-row status banner output.
#[derive(Debug, Clone)]
#[must_use = "StatusBanner contains the formatted output to display"]
pub struct StatusBanner {
    /// Individual lines to render (top to bottom)
    pub lines: Vec<String>,
    /// Number of rows this banner occupies
    pub height: usize,
    /// Clickable button positions
    #[allow(dead_code)]
    pub buttons: Vec<ButtonPosition>,
}

impl StatusBanner {
    pub fn new(lines: Vec<String>) -> Self {
        let height = lines.len();
        Self {
            lines,
            height,
            buttons: Vec::new(),
        }
    }

    pub fn with_buttons(lines: Vec<String>, buttons: Vec<ButtonPosition>) -> Self {
        let height = lines.len();
        Self {
            lines,
            height,
            buttons,
        }
    }
}

/// State for the enhanced status line.
#[derive(Debug, Clone, Default)]
pub struct StatusLineState {
    /// Current voice mode (auto/manual/idle)
    pub voice_mode: VoiceMode,
    /// Current recording state
    pub recording_state: RecordingState,
    /// Pipeline in use
    pub pipeline: Pipeline,
    /// Microphone sensitivity in dB
    pub sensitivity_db: f32,
    /// Main status message
    pub message: String,
    /// Recording duration in seconds (if recording)
    pub recording_duration: Option<f32>,
    /// Whether auto-voice is enabled
    pub auto_voice_enabled: bool,
    /// Recent audio meter samples in dBFS for waveform display (capped at METER_HISTORY_MAX)
    pub meter_levels: Vec<f32>,
    /// Latest audio meter level in dBFS
    pub meter_db: Option<f32>,
    /// Optional transcript preview snippet
    pub transcript_preview: Option<String>,
    /// Number of pending transcripts in queue
    pub queue_depth: usize,
    /// Last measured transcription latency in milliseconds
    pub last_latency_ms: Option<u32>,
    /// Current voice send mode
    pub send_mode: VoiceSendMode,
    /// Right-side HUD panel mode
    pub hud_right_panel: HudRightPanel,
    /// Only animate the right-side panel while recording
    pub hud_right_panel_recording_only: bool,
    /// HUD display style (Full, Minimal, Hidden)
    pub hud_style: HudStyle,
    /// Whether mouse clicking on HUD buttons is enabled
    pub mouse_enabled: bool,
    /// Focused HUD button (for arrow key navigation)
    pub hud_button_focus: Option<ButtonAction>,
}

impl StatusLineState {
    pub fn new() -> Self {
        Self {
            sensitivity_db: -35.0,
            meter_levels: Vec::with_capacity(METER_HISTORY_MAX),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_mode_labels() {
        assert_eq!(VoiceMode::Auto.label(), "AUTO");
        assert_eq!(VoiceMode::Manual.label(), "MANUAL");
        assert_eq!(VoiceMode::Idle.label(), "IDLE");
    }

    #[test]
    fn pipeline_labels() {
        assert_eq!(Pipeline::Rust.label(), "Rust");
        assert_eq!(Pipeline::Python.label(), "Python");
    }

    #[test]
    fn status_line_state_default() {
        let state = StatusLineState::new();
        assert_eq!(state.sensitivity_db, -35.0);
        assert!(!state.auto_voice_enabled);
        assert!(state.message.is_empty());
    }
}
