//! Enhanced status line layout with sections.
//!
//! Provides a structured status line with mode indicator, pipeline tag,
//! sensitivity level, status message, and keyboard shortcuts.
//!
//! Now supports a multi-row banner layout with themed borders.

use crate::audio_meter::format_waveform;
use crate::hud::{HudRegistry, HudState, LatencyModule, MeterModule, Mode as HudMode, QueueModule};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};
use std::sync::OnceLock;
use unicode_width::UnicodeWidthChar;

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
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
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
    /// Recent audio meter samples in dBFS for waveform display
    pub meter_levels: Vec<f32>,
    /// Latest audio meter level in dBFS
    pub meter_db: Option<f32>,
    /// Optional transcript preview snippet
    pub transcript_preview: Option<String>,
    /// Number of pending transcripts in queue
    pub queue_depth: usize,
    /// Last measured transcription latency in milliseconds
    pub last_latency_ms: Option<u32>,
}

impl StatusLineState {
    pub fn new() -> Self {
        Self {
            sensitivity_db: -35.0,
            ..Default::default()
        }
    }
}

/// Keyboard shortcuts to display.
const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+R", "rec"),
    ("Ctrl+V", "auto"),
    ("Ctrl+T", "send"),
    ("Ctrl+O", "settings"),
    ("?", "help"),
    ("Ctrl+Y", "theme"),
];

/// Compact shortcuts for narrow terminals.
const SHORTCUTS_COMPACT: &[(&str, &str)] = &[
    ("^R", "rec"),
    ("^V", "auto"),
    ("^T", "send"),
    ("^O", "settings"),
    ("?", "help"),
    ("^Y", "theme"),
];

/// Multi-row status banner output.
#[derive(Debug, Clone)]
pub struct StatusBanner {
    /// Individual lines to render (top to bottom)
    pub lines: Vec<String>,
    /// Number of rows this banner occupies
    pub height: usize,
}

impl StatusBanner {
    pub fn new(lines: Vec<String>) -> Self {
        let height = lines.len();
        Self { lines, height }
    }
}

/// Terminal width breakpoints for responsive layout.
mod breakpoints {
    /// Full layout with all sections
    pub const FULL: usize = 80;
    /// Medium layout - shorter shortcuts
    pub const MEDIUM: usize = 60;
    /// Compact layout - minimal left section
    pub const COMPACT: usize = 40;
    /// Minimal layout - message only
    pub const MINIMAL: usize = 25;
}

/// Return the number of rows used by the status banner for a given width.
pub fn status_banner_height(width: usize) -> usize {
    if width < breakpoints::COMPACT {
        1
    } else {
        4
    }
}

/// Format the status as a multi-row banner with themed borders.
///
/// Layout (4 rows):
/// ```text
/// ╭─────────────────────────────────────────────────────── theme ─╮
/// │ ● AUTO │ Rust │ -40dB  ▁▂▃▅▆▇█▅  -51dB  Status message here  │
/// │ ^R rec  ^V auto  ^T send  ? help  ^Y theme                   │
/// ╰──────────────────────────────────────────────────────────────╯
/// ```
pub fn format_status_banner(state: &StatusLineState, theme: Theme, width: usize) -> StatusBanner {
    let colors = theme.colors();
    let borders = &colors.borders;

    // For very narrow terminals, fall back to simple single-line
    if width < breakpoints::COMPACT {
        let line = format_status_line(state, theme, width);
        return StatusBanner::new(vec![line]);
    }

    let inner_width = width.saturating_sub(2); // Account for left/right borders

    let lines = vec![
        format_top_border(&colors, borders, theme, width),
        format_main_row(state, &colors, borders, theme, inner_width),
        format_shortcuts_row(&colors, borders, inner_width),
        format_bottom_border(&colors, borders, width),
    ];

    StatusBanner::new(lines)
}

/// Format the top border with theme name badge.
fn format_top_border(
    colors: &ThemeColors,
    borders: &BorderSet,
    theme: Theme,
    width: usize,
) -> String {
    let theme_name = format!(" {} ", theme);
    let theme_badge_len = theme_name.len();

    // Calculate border segments
    // Total: top_left(1) + left_segment + theme_name + right_segment + top_right(1) = width
    let left_border_len = 2;
    let right_border_len = width.saturating_sub(left_border_len + theme_badge_len + 2); // +2 for corners

    let left_segment: String = std::iter::repeat_n(borders.horizontal, left_border_len).collect();
    let right_segment: String = std::iter::repeat_n(borders.horizontal, right_border_len).collect();

    format!(
        "{}{}{}{}{}{}{}{}{}",
        colors.border,
        borders.top_left,
        left_segment,
        colors.reset,
        colors.dim,
        theme_name,
        colors.reset,
        colors.border,
        right_segment,
        // borders.top_right,
        // colors.reset
    ) + &format!("{}{}", borders.top_right, colors.reset)
}

/// Format the main status row with mode, sensitivity, meter, and message.
fn format_main_row(
    state: &StatusLineState,
    colors: &ThemeColors,
    borders: &BorderSet,
    theme: Theme,
    inner_width: usize,
) -> String {
    // Build content sections
    let mode_section = format_mode_indicator(state, colors);
    let sensitivity_section = format!(" {:>3.0}dB ", state.sensitivity_db);

    // Duration (if recording)
    let duration_section = if let Some(dur) = state.recording_duration {
        format!(" {:.1}s ", dur)
    } else {
        String::new()
    };

    // Waveform and meter (if recording)
    let meter_section =
        if state.recording_state == RecordingState::Recording && !state.meter_levels.is_empty() {
            let waveform = format_waveform(&state.meter_levels, 8, theme);
            if let Some(db) = state.meter_db {
                format!(
                    " {} {}{:>4.0}dB{} ",
                    waveform, colors.info, db, colors.reset
                )
            } else {
                format!(" {} ", waveform)
            }
        } else {
            String::new()
        };

    // Status message with color
    let status_type = StatusType::from_message(&state.message);
    let status_color = status_type.color(colors);
    let message_section = if state.message.is_empty() {
        format!(
            " {}{}{}",
            colors.dim, "Ready | Ctrl+O settings | ? help", colors.reset
        )
    } else {
        format!(" {}{}{}", status_color, state.message, colors.reset)
    };

    // Combine all sections
    let sep = format!("{}│{}", colors.dim, colors.reset);
    let content = format!(
        "{}{}{}{}{}",
        mode_section, sep, sensitivity_section, duration_section, meter_section,
    );

    let content_width = display_width(&content);
    let message_available = inner_width.saturating_sub(content_width + 1);
    let truncated_message = truncate_display(&message_section, message_available);

    let interior = format!("{content}{truncated_message}");
    let message_width = display_width(&truncated_message);

    // Padding to fill the row
    let padding_needed = inner_width.saturating_sub(content_width + message_width);
    let padding = " ".repeat(padding_needed);

    // No background colors - use transparent backgrounds for terminal compatibility
    format!(
        "{}{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        interior,
        padding,
        colors.border,
        borders.vertical,
        colors.reset,
    )
}

/// Format the mode indicator with appropriate color and symbol.
fn format_mode_indicator(state: &StatusLineState, colors: &ThemeColors) -> String {
    let pipeline_tag = match state.pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    };
    let (indicator, label, color) = match state.recording_state {
        RecordingState::Recording => (
            colors.indicator_rec,
            format!("REC {pipeline_tag}"),
            colors.recording,
        ),
        RecordingState::Processing => ("◐", format!("... {pipeline_tag}"), colors.processing),
        RecordingState::Idle => match state.voice_mode {
            VoiceMode::Auto => (colors.indicator_auto, "AUTO".to_string(), colors.info),
            VoiceMode::Manual => (colors.indicator_manual, "MANUAL".to_string(), ""),
            VoiceMode::Idle => (colors.indicator_idle, "IDLE".to_string(), ""),
        },
    };

    if color.is_empty() {
        format!(" {} {} ", indicator, label)
    } else {
        format!(" {}{} {}{} ", color, indicator, label, colors.reset)
    }
}

/// Format the shortcuts row with dimmed styling.
fn format_shortcuts_row(colors: &ThemeColors, borders: &BorderSet, inner_width: usize) -> String {
    let shortcuts: Vec<String> = SHORTCUTS_COMPACT
        .iter()
        .map(|(key, action)| format!("{}{}{} {}", colors.info, key, colors.reset, action))
        .collect();
    let shortcuts_str = shortcuts.join("  ");

    let shortcuts_width = display_width(&shortcuts_str);
    let padding_needed = inner_width.saturating_sub(shortcuts_width + 1);
    let padding = " ".repeat(padding_needed);

    // No background colors - use transparent backgrounds for terminal compatibility
    format!(
        "{}{}{}{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        colors.dim,
        " ",
        shortcuts_str,
        padding,
        colors.reset,
        colors.border,
        borders.vertical,
    ) + colors.reset
}

/// Format the bottom border.
fn format_bottom_border(colors: &ThemeColors, borders: &BorderSet, width: usize) -> String {
    let inner: String = std::iter::repeat_n(borders.horizontal, width.saturating_sub(2)).collect();

    format!(
        "{}{}{}{}{}",
        colors.border, borders.bottom_left, inner, borders.bottom_right, colors.reset
    )
}

/// Format the enhanced status line with responsive layout.
pub fn format_status_line(state: &StatusLineState, theme: Theme, width: usize) -> String {
    let colors = theme.colors();

    if width < breakpoints::MINIMAL {
        // Ultra-narrow: just the essential indicator and truncated message
        return format_minimal(state, &colors, width);
    }

    if width < breakpoints::COMPACT {
        // Compact: indicator + message only
        return format_compact(state, &colors, theme, width);
    }

    // Build sections based on available width
    let left = if width >= breakpoints::MEDIUM {
        format_left_section(state, &colors)
    } else {
        format_left_compact(state, &colors)
    };

    let right = if width >= breakpoints::FULL {
        format_shortcuts(&colors)
    } else if width >= breakpoints::MEDIUM {
        format_shortcuts_compact(&colors)
    } else {
        String::new()
    };

    let center = format_message(state, &colors, theme, width);

    // Calculate display widths (excluding ANSI codes)
    let left_width = display_width(&left);
    let right_width = display_width(&right);
    let center_width = display_width(&center);

    // Combine with proper spacing
    let total_content_width = left_width + center_width + right_width + 2;

    if total_content_width <= width {
        // Everything fits - add padding between center and right
        let padding = width.saturating_sub(total_content_width);
        if right.is_empty() {
            format!("{} {}", left, center)
        } else {
            format!(
                "{} {}{:padding$}{}",
                left,
                center,
                "",
                right,
                padding = padding
            )
        }
    } else if left_width + right_width + 4 <= width {
        // Truncate center message
        let available = width.saturating_sub(left_width + right_width + 3);
        let truncated_center = truncate_display(&center, available);
        if right.is_empty() {
            format!("{} {}", left, truncated_center)
        } else {
            format!("{} {} {}", left, truncated_center, right)
        }
    } else {
        // Very narrow - just show left + truncated message
        let available = width.saturating_sub(left_width + 1);
        let truncated_center = truncate_display(&center, available);
        format!("{} {}", left, truncated_center)
    }
}

/// Format minimal status for very narrow terminals.
fn format_minimal(state: &StatusLineState, colors: &ThemeColors, width: usize) -> String {
    let indicator = match state.recording_state {
        RecordingState::Recording => format!("{}●{}", colors.recording, colors.reset),
        RecordingState::Processing => format!("{}◐{}", colors.processing, colors.reset),
        RecordingState::Idle => {
            if state.voice_mode == VoiceMode::Auto {
                format!(
                    "{}{}{}",
                    colors.info,
                    state.voice_mode.indicator(),
                    colors.reset
                )
            } else {
                state.voice_mode.indicator().to_string()
            }
        }
    };

    let msg = if state.message.is_empty() {
        if state.voice_mode == VoiceMode::Auto {
            "auto"
        } else {
            "ready"
        }
        .to_string()
    } else {
        state.message.clone()
    };

    let available = width.saturating_sub(2); // indicator + space
    format!("{} {}", indicator, truncate_display(&msg, available))
}

/// Format compact status for narrow terminals.
fn format_compact(
    state: &StatusLineState,
    colors: &ThemeColors,
    theme: Theme,
    width: usize,
) -> String {
    let pipeline_tag = match state.pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    };
    let mode = match state.recording_state {
        RecordingState::Recording => {
            format!("{}● {}{}", colors.recording, pipeline_tag, colors.reset)
        }
        RecordingState::Processing => {
            format!("{}◐ {}{}", colors.processing, pipeline_tag, colors.reset)
        }
        RecordingState::Idle => {
            let label = match state.voice_mode {
                VoiceMode::Auto => "A",
                VoiceMode::Manual => "M",
                VoiceMode::Idle => "",
            };
            if state.voice_mode == VoiceMode::Auto {
                format!(
                    "{}{} {}{}",
                    colors.info,
                    state.voice_mode.indicator(),
                    label,
                    colors.reset
                )
            } else if state.voice_mode == VoiceMode::Manual {
                format!("{} {}", state.voice_mode.indicator(), label)
            } else {
                state.voice_mode.indicator().to_string()
            }
        }
    };

    let registry = compact_hud_registry();
    let hud_state = HudState {
        mode: match state.voice_mode {
            VoiceMode::Auto => HudMode::Auto,
            VoiceMode::Manual => HudMode::Manual,
            VoiceMode::Idle => HudMode::Insert,
        },
        is_recording: state.recording_state == RecordingState::Recording,
        recording_duration_secs: state.recording_duration.unwrap_or(0.0),
        audio_level_db: state.meter_db.unwrap_or(-60.0),
        queue_depth: state.queue_depth,
        last_latency_ms: state.last_latency_ms,
        backend_name: String::new(),
    };
    let modules = registry.render_all(&hud_state, width, " ");
    let left = if modules.is_empty() {
        mode.clone()
    } else {
        format!("{} {}", mode, modules)
    };

    let msg = format_message(state, colors, theme, width);
    let left_width = display_width(&left);
    let available = width.saturating_sub(left_width + 1);
    format!("{} {}", left, truncate_display(&msg, available))
}

fn compact_hud_registry() -> &'static HudRegistry {
    static REGISTRY: OnceLock<HudRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = HudRegistry::new();
        registry.register(Box::new(MeterModule::new()));
        registry.register(Box::new(LatencyModule::new()));
        registry.register(Box::new(QueueModule::new()));
        registry
    })
}

/// Format compact left section for medium terminals.
fn format_left_compact(state: &StatusLineState, colors: &ThemeColors) -> String {
    let pipeline_tag = match state.pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    };
    let mode_indicator = match state.recording_state {
        RecordingState::Recording => format!("{}●{}", colors.recording, colors.reset),
        RecordingState::Processing => format!("{}◐{}", colors.processing, colors.reset),
        RecordingState::Idle => {
            if state.voice_mode == VoiceMode::Auto {
                format!(
                    "{}{}{}",
                    colors.info,
                    state.voice_mode.indicator(),
                    colors.reset
                )
            } else {
                state.voice_mode.indicator().to_string()
            }
        }
    };

    let mode_label = match state.recording_state {
        RecordingState::Recording => pipeline_tag,
        RecordingState::Processing => pipeline_tag,
        RecordingState::Idle => match state.voice_mode {
            VoiceMode::Auto => "A",
            VoiceMode::Manual => "M",
            VoiceMode::Idle => "",
        },
    };

    if mode_label.is_empty() {
        format!("{} │ {:.0}dB", mode_indicator, state.sensitivity_db)
    } else {
        format!(
            "{}{} │ {:.0}dB",
            mode_indicator, mode_label, state.sensitivity_db
        )
    }
}

/// Format compact shortcuts.
fn format_shortcuts_compact(colors: &ThemeColors) -> String {
    let mut parts = Vec::new();
    for (key, action) in SHORTCUTS_COMPACT {
        parts.push(format!("{}{}{} {}", colors.info, key, colors.reset, action));
    }
    parts.join(" ")
}

fn format_left_section(state: &StatusLineState, colors: &ThemeColors) -> String {
    let pipeline_tag = match state.pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    };
    let mode_color = match state.recording_state {
        RecordingState::Recording => colors.recording,
        RecordingState::Processing => colors.processing,
        RecordingState::Idle => {
            if state.voice_mode == VoiceMode::Auto {
                colors.info
            } else {
                ""
            }
        }
    };

    let mode_indicator = match state.recording_state {
        RecordingState::Recording => "●",
        RecordingState::Processing => "◐",
        RecordingState::Idle => state.voice_mode.indicator(),
    };

    let mode_label = match state.recording_state {
        RecordingState::Recording => format!("REC {pipeline_tag}"),
        RecordingState::Processing => format!("... {pipeline_tag}"),
        RecordingState::Idle => state.voice_mode.label().to_string(),
    };

    let sensitivity = format!("{:.0}dB", state.sensitivity_db);

    // Add recording duration if active
    let duration_part = if let Some(dur) = state.recording_duration {
        format!(" {:.1}s", dur)
    } else {
        String::new()
    };

    if mode_color.is_empty() {
        format!(
            "{} {} │ {}{}",
            mode_indicator, mode_label, sensitivity, duration_part
        )
    } else {
        format!(
            "{}{} {}{} │ {}{}",
            mode_color, mode_indicator, mode_label, colors.reset, sensitivity, duration_part
        )
    }
}

fn format_message(
    state: &StatusLineState,
    colors: &ThemeColors,
    theme: Theme,
    width: usize,
) -> String {
    let mut message = if state.message.is_empty() {
        String::new()
    } else {
        state.message.clone()
    };

    if let Some(preview) = state.transcript_preview.as_ref() {
        if message.is_empty() {
            message = preview.clone();
        } else {
            message = format!("{message} \"{preview}\"");
        }
    }

    if message.is_empty() {
        return message;
    }

    let mut prefix = String::new();
    if state.recording_state == RecordingState::Recording && !state.meter_levels.is_empty() {
        let wave_width = if width >= breakpoints::FULL {
            10
        } else if width >= breakpoints::MEDIUM {
            8
        } else {
            6
        };
        let waveform = format_waveform(&state.meter_levels, wave_width, theme);
        if let Some(db) = state.meter_db {
            prefix = format!("{} {}{:>4.0}dB{} ", waveform, colors.info, db, colors.reset);
        } else {
            prefix = format!("{waveform} ");
        }
    }

    let status_type = StatusType::from_message(&message);
    let color = status_type.color(colors);
    let colored_message = if color.is_empty() {
        message
    } else {
        format!("{}{}{}", color, message, colors.reset)
    };

    format!("{prefix}{colored_message}")
}

fn format_shortcuts(colors: &ThemeColors) -> String {
    let mut parts = Vec::new();
    for (key, action) in SHORTCUTS {
        parts.push(format!("{}{}{} {}", colors.info, key, colors.reset, action));
    }
    parts.join("  ")
}

/// Calculate display width excluding ANSI escape codes.
fn display_width(s: &str) -> usize {
    let mut width: usize = 0;
    let mut in_escape = false;

    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            width += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }

    width
}

/// Truncate a string to a maximum display width.
fn truncate_display(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut width: usize = 0;
    let mut in_escape = false;
    let mut escape_seq = String::new();

    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
            escape_seq.push(ch);
        } else if in_escape {
            escape_seq.push(ch);
            if ch == 'm' {
                result.push_str(&escape_seq);
                escape_seq.clear();
                in_escape = false;
            }
        } else {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if width.saturating_add(ch_width) > max_width {
                break;
            }
            result.push(ch);
            width = width.saturating_add(ch_width);
        }
    }

    // Ensure we close any open escape sequences
    if !result.is_empty() && result.contains("\x1b[") && !result.ends_with("\x1b[0m") {
        result.push_str("\x1b[0m");
    }

    result
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
    fn display_width_excludes_ansi() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("\x1b[91mhello\x1b[0m"), 5);
        assert_eq!(display_width("\x1b[38;2;255;0;0mred\x1b[0m"), 3);
    }

    #[test]
    fn truncate_display_respects_width() {
        assert_eq!(truncate_display("hello", 3), "hel");
        assert_eq!(truncate_display("hello", 10), "hello");
        assert_eq!(truncate_display("hello", 0), "");
    }

    #[test]
    fn truncate_display_preserves_ansi() {
        let colored = "\x1b[91mhello\x1b[0m";
        let truncated = truncate_display(colored, 3);
        assert!(truncated.contains("\x1b[91m"));
        assert!(truncated.contains("hel"));
    }

    #[test]
    fn format_status_line_basic() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            pipeline: Pipeline::Rust,
            sensitivity_db: -35.0,
            message: "Ready".to_string(),
            ..Default::default()
        };
        let line = format_status_line(&state, Theme::Coral, 80);
        assert!(line.contains("AUTO"));
        assert!(line.contains("-35dB"));
        assert!(line.contains("Ready"));
    }

    #[test]
    fn format_status_line_with_duration() {
        let state = StatusLineState {
            recording_state: RecordingState::Recording,
            recording_duration: Some(2.5),
            message: "Recording...".to_string(),
            ..Default::default()
        };
        let line = format_status_line(&state, Theme::Coral, 80);
        assert!(line.contains("2.5s"));
        assert!(line.contains("REC"));
    }

    #[test]
    fn status_line_state_default() {
        let state = StatusLineState::new();
        assert_eq!(state.sensitivity_db, -35.0);
        assert!(!state.auto_voice_enabled);
        assert!(state.message.is_empty());
    }

    #[test]
    fn format_status_line_narrow_terminal() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            message: "Ready".to_string(),
            ..Default::default()
        };
        // Narrow terminal should still produce output
        let line = format_status_line(&state, Theme::Coral, 40);
        assert!(!line.is_empty());
        // Should have some content
        assert!(line.len() > 5);
    }

    #[test]
    fn format_status_line_very_narrow() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            message: "Ready".to_string(),
            ..Default::default()
        };
        // Very narrow terminal
        let line = format_status_line(&state, Theme::Coral, 20);
        assert!(!line.is_empty());
    }

    #[test]
    fn format_status_line_minimal() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            message: "Ready".to_string(),
            ..Default::default()
        };
        // Minimal width
        let line = format_status_line(&state, Theme::None, 15);
        assert!(!line.is_empty());
        // Should contain indicator
        assert!(line.contains("◉") || line.contains("auto") || line.contains("Ready"));
    }

    #[test]
    fn format_status_line_medium_shows_compact_shortcuts() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            message: "Ready".to_string(),
            ..Default::default()
        };
        // Medium terminal - should show compact shortcuts
        let line = format_status_line(&state, Theme::None, 65);
        // Should have some shortcut hint
        assert!(line.contains("R") || line.contains("V") || line.contains("rec"));
    }
}
