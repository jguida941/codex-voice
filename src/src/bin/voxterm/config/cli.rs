//! CLI flag schema so overlay startup behavior is explicit and discoverable.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use voxterm::config::AppConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub(crate) enum VoiceSendMode {
    #[default]
    Auto,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub(crate) enum HudRightPanel {
    #[default]
    Ribbon,
    Dots,
    Heartbeat,
    Off,
}

/// HUD display style - controls overall banner visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub(crate) enum HudStyle {
    /// Full 4-row banner with borders and shortcuts (default)
    #[default]
    Full,
    /// Single-line minimal indicator (just colored text, no borders)
    Minimal,
    /// Hidden unless recording
    Hidden,
}

impl std::fmt::Display for HudStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            HudStyle::Full => "Full",
            HudStyle::Minimal => "Minimal",
            HudStyle::Hidden => "Hidden",
        };
        write!(f, "{label}")
    }
}

impl std::fmt::Display for HudRightPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            HudRightPanel::Off => "Off",
            HudRightPanel::Ribbon => "Ribbon",
            HudRightPanel::Dots => "Dots",
            HudRightPanel::Heartbeat => "Heartbeat",
        };
        write!(f, "{label}")
    }
}

#[derive(Debug, Parser, Clone)]
#[command(about = "VoxTerm", author, version)]
pub(crate) struct OverlayConfig {
    #[command(flatten)]
    pub(crate) app: AppConfig,

    /// Regex used to detect the AI prompt line (overrides auto-detection)
    #[arg(long = "prompt-regex")]
    pub(crate) prompt_regex: Option<String>,

    /// Log file path for prompt detection diagnostics
    #[arg(long = "prompt-log")]
    pub(crate) prompt_log: Option<PathBuf>,

    /// Start in auto-voice mode
    #[arg(long = "auto-voice", default_value_t = false)]
    pub(crate) auto_voice: bool,

    /// Idle time before auto-voice triggers when prompt detection is unknown (ms)
    #[arg(long = "auto-voice-idle-ms", default_value_t = 1200)]
    pub(crate) auto_voice_idle_ms: u64,

    /// Idle time before transcripts auto-send when a prompt has not been detected (ms)
    #[arg(long = "transcript-idle-ms", default_value_t = 250)]
    pub(crate) transcript_idle_ms: u64,

    /// Voice transcript handling (auto = send newline, insert = leave for editing)
    #[arg(long = "voice-send-mode", value_enum, default_value_t = VoiceSendMode::Auto)]
    pub(crate) voice_send_mode: VoiceSendMode,

    /// Color theme for status line (chatgpt, claude, codex, coral, catppuccin, dracula, gruvbox, nord, tokyonight, ansi, none)
    /// Defaults to the backend-specific theme if not provided.
    #[arg(long = "theme")]
    pub(crate) theme_name: Option<String>,

    /// Disable colors in status line output
    #[arg(long = "no-color", default_value_t = false)]
    pub(crate) no_color: bool,

    /// Right-side HUD panel (off, ribbon, dots, heartbeat)
    #[arg(long = "hud-right-panel", value_enum, default_value_t = HudRightPanel::Ribbon)]
    pub(crate) hud_right_panel: HudRightPanel,

    /// Only animate the right-side panel while recording
    #[arg(long = "hud-right-panel-recording-only", default_value_t = true)]
    pub(crate) hud_right_panel_recording_only: bool,

    /// HUD display style (full, minimal, hidden)
    #[arg(long = "hud-style", value_enum, default_value_t = HudStyle::Full)]
    pub(crate) hud_style: HudStyle,

    /// Shorthand for --hud-style minimal
    #[arg(long = "minimal-hud", default_value_t = false)]
    pub(crate) minimal_hud: bool,

    /// Backend CLI to run (codex, claude, gemini, or custom command)
    ///
    /// Use a preset name or provide a custom command string.
    /// Examples:
    ///   --backend codex
    ///   --backend claude
    ///   --backend "my-tool --flag"
    #[arg(long = "backend", default_value = "codex")]
    pub(crate) backend: String,

    /// Shorthand for --backend codex
    #[arg(long = "codex", default_value_t = false)]
    pub(crate) codex: bool,

    /// Shorthand for --backend claude
    #[arg(long = "claude", default_value_t = false)]
    pub(crate) claude: bool,

    /// Shorthand for --backend gemini
    #[arg(long = "gemini", default_value_t = false)]
    pub(crate) gemini: bool,

    /// Run backend login before starting the overlay
    #[arg(long = "login", default_value_t = false)]
    pub(crate) login: bool,
}
