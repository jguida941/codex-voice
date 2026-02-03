//! Enhanced status line layout with sections.
//!
//! Provides a structured status line with mode indicator, pipeline tag,
//! sensitivity level, status message, and keyboard shortcuts.
//!
//! Now supports a multi-row banner layout with themed borders.
//! Buttons are clickable - click positions are tracked for mouse interaction.

use crate::audio_meter::format_waveform;
use crate::buttons::ButtonAction;
use crate::config::{HudRightPanel, HudStyle, VoiceSendMode};
use crate::hud::{HudRegistry, HudState, LatencyModule, MeterModule, Mode as HudMode, QueueModule};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
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
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
        }
    }
}

fn pipeline_tag_short(pipeline: Pipeline) -> &'static str {
    match pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    }
}

/// Maximum number of meter level samples to keep for waveform display.
pub const METER_HISTORY_MAX: usize = 24;

const MAIN_ROW_DURATION_PLACEHOLDER: &str = "--.-s";
const MAIN_ROW_WAVEFORM_MIN_WIDTH: usize = 3;
const RIGHT_PANEL_MAX_WAVEFORM_WIDTH: usize = 12;
const RIGHT_PANEL_MIN_CONTENT_WIDTH: usize = 4;
const HEARTBEAT_FRAMES: &[char] = &['·', '•', '●', '•'];

/// Pulsing recording indicator frames (cycles every ~400ms at 10fps).
const RECORDING_PULSE_FRAMES: &[&str] = &["●", "◉", "●", "○"];

/// Processing spinner frames (braille dots for smooth animation).
const PROCESSING_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Get the current animation frame based on system time.
/// Returns a frame index that cycles through the given frame count.
#[inline]
fn get_animation_frame(frame_count: usize, cycle_ms: u64) -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    ((now / cycle_ms) % frame_count as u64) as usize
}

/// Get the pulsing recording indicator.
#[inline]
fn get_recording_indicator() -> &'static str {
    let frame = get_animation_frame(RECORDING_PULSE_FRAMES.len(), 250);
    RECORDING_PULSE_FRAMES[frame]
}

/// Get the processing spinner character.
#[inline]
fn get_processing_spinner() -> &'static str {
    let frame = get_animation_frame(PROCESSING_SPINNER_FRAMES.len(), 100);
    PROCESSING_SPINNER_FRAMES[frame]
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

/// Keyboard shortcuts to display.
const SHORTCUTS: &[(&str, &str)] = &[
    ("Ctrl+R", "rec"),
    ("Ctrl+V", "auto"),
    ("Ctrl+T", "send"),
    ("Ctrl+U", "hud"),
    ("Ctrl+O", "settings"),
    ("?", "help"),
    ("Ctrl+Y", "theme"),
];

/// Compact shortcuts for narrow terminals.
const SHORTCUTS_COMPACT: &[(&str, &str)] = &[
    ("^R", "rec"),
    ("^V", "auto"),
    ("^T", "send"),
    ("^U", "hud"),
    ("^O", "settings"),
    ("?", "help"),
    ("^Y", "theme"),
];

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

/// Return the number of rows used by the status banner for a given width and HUD style.
#[must_use]
pub fn status_banner_height(width: usize, hud_style: HudStyle) -> usize {
    match hud_style {
        HudStyle::Hidden => 1,  // Reserve a row to avoid overlaying CLI output
        HudStyle::Minimal => 1, // Single line
        HudStyle::Full => {
            if width < breakpoints::COMPACT {
                1
            } else {
                4
            }
        }
    }
}

/// Get clickable button positions for the current state.
/// Returns button positions for full HUD mode (row 2 from bottom) and minimal mode (row 1).
/// Hidden mode returns empty vec (no clickable buttons).
pub fn get_button_positions(
    state: &StatusLineState,
    theme: Theme,
    width: usize,
) -> Vec<ButtonPosition> {
    match state.hud_style {
        HudStyle::Full => {
            if width < breakpoints::COMPACT {
                return Vec::new();
            }
            let colors = theme.colors();
            let inner_width = width.saturating_sub(2);
            let (_, buttons) = format_button_row_with_positions(state, &colors, inner_width, 2);
            buttons
        }
        HudStyle::Minimal => {
            let colors = theme.colors();
            let (_, button) = format_minimal_strip_with_button(state, &colors, width);
            button.into_iter().collect()
        }
        HudStyle::Hidden => Vec::new(),
    }
}

fn minimal_strip_text(state: &StatusLineState, colors: &ThemeColors) -> String {
    // Use animated indicators for recording and processing states
    // Minimal mode: theme-colored indicators for all states
    let (indicator, label, color) = match state.recording_state {
        RecordingState::Recording => (get_recording_indicator(), "REC", colors.recording),
        RecordingState::Processing => (get_processing_spinner(), "processing", colors.processing),
        RecordingState::Idle => match state.voice_mode {
            VoiceMode::Auto => ("◉", "AUTO", colors.info), // Blue filled - auto mode active
            VoiceMode::Manual => ("●", "PTT", colors.border), // Theme accent - push-to-talk ready
            VoiceMode::Idle => ("○", "IDLE", colors.dim),  // Dim - inactive
        },
    };

    let mut line = if color.is_empty() {
        format!("{indicator} {label}")
    } else {
        format!("{}{} {}{}", color, indicator, label, colors.reset)
    };

    match state.recording_state {
        RecordingState::Recording => {
            if let Some(db) = state.meter_db {
                line.push(' ');
                line.push_str(colors.dim);
                line.push('·');
                line.push_str(colors.reset);
                line.push(' ');
                line.push_str(colors.info);
                line.push_str(&format!("{:>3.0}dB", db));
                line.push_str(colors.reset);
            }
        }
        RecordingState::Processing => {}
        RecordingState::Idle => {
            let status_text = if state.message.is_empty() {
                "Ready"
            } else {
                state.message.as_str()
            };
            let status_color = if state.message.is_empty() {
                colors.success
            } else {
                StatusType::from_message(status_text).color(colors)
            };
            let status = if status_color.is_empty() {
                status_text.to_string()
            } else {
                format!("{}{}{}", status_color, status_text, colors.reset)
            };
            if !status.is_empty() {
                line.push(' ');
                line.push_str(colors.dim);
                line.push('·');
                line.push_str(colors.reset);
                line.push(' ');
                line.push_str(&status);
            }
        }
    }

    line
}

fn format_minimal_strip_with_button(
    state: &StatusLineState,
    colors: &ThemeColors,
    width: usize,
) -> (String, Option<ButtonPosition>) {
    if width == 0 {
        return (String::new(), None);
    }

    let base = minimal_strip_text(state, colors);
    let focused = state.hud_button_focus == Some(ButtonAction::HudBack);
    let button = format_button(colors, "back", colors.border, focused);
    let button_width = display_width(&button);

    // Require room for at least one space between status and button.
    if width >= button_width + 2 {
        let button_start = width.saturating_sub(button_width) + 1;
        let status_width = button_start.saturating_sub(2);
        let status = truncate_display(&base, status_width);
        let status_width = display_width(&status);
        let padding = button_start.saturating_sub(1 + status_width);
        let line = format!("{status}{}{}", " ".repeat(padding), button);
        let button_pos = ButtonPosition {
            start_x: button_start as u16,
            end_x: (button_start + button_width - 1) as u16,
            row: 1,
            action: ButtonAction::HudBack,
        };
        return (line, Some(button_pos));
    }

    let line = truncate_display(&base, width);
    (line, None)
}

/// Format hidden mode strip - grey/obscure, only shows essential info when active.
/// More subtle than minimal mode - all dim colors for minimal distraction.
fn format_hidden_strip(state: &StatusLineState, colors: &ThemeColors, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    // Hidden mode uses dim colors for everything - more obscure
    let (indicator, label) = match state.recording_state {
        RecordingState::Recording => ("●", "rec"),
        RecordingState::Processing => ("◌", "..."),
        RecordingState::Idle => return String::new(),
    };

    let mut line = format!("{}{} {}{}", colors.dim, indicator, label, colors.reset);

    // Add duration for recording, keep it minimal
    if state.recording_state == RecordingState::Recording {
        if let Some(dur) = state.recording_duration {
            line.push_str(&format!(" {}{:.0}s{}", colors.dim, dur, colors.reset));
        }
    }

    truncate_display(&line, width)
}

/// Format the status as a multi-row banner with themed borders.
///
/// Layout (4 rows for Full mode):
/// ```text
/// ╭──────────────────────────────────────────────────── VoxTerm ─╮
/// │ ● AUTO │ Rust │ ▁▂▃▅▆▇█▅  -51dB  Status message here          │
/// │ [rec] · [auto] · [send] · [set] · [hud] · [help] · [theme]   │
/// ╰──────────────────────────────────────────────────────────────╯
/// ```
///
/// Minimal mode: Theme-colored strip with indicator + status (e.g., "● PTT · Ready")
/// Hidden mode: Dim grey indicator only when recording (e.g., "● rec 5s")
pub fn format_status_banner(state: &StatusLineState, theme: Theme, width: usize) -> StatusBanner {
    let colors = theme.colors();
    let borders = &colors.borders;

    // Handle HUD style
    match state.hud_style {
        HudStyle::Hidden => {
            // Reserve a blank row when idle; show dim/grey indicator only when active
            if state.recording_state == RecordingState::Recording
                || state.recording_state == RecordingState::Processing
            {
                let line = format_hidden_strip(state, &colors, width);
                StatusBanner::new(vec![line])
            } else {
                StatusBanner::new(vec![String::new()])
            }
        }
        HudStyle::Minimal => {
            let (line, button) = format_minimal_strip_with_button(state, &colors, width);
            StatusBanner::with_buttons(vec![line], button.into_iter().collect())
        }
        HudStyle::Full => {
            // For very narrow terminals, fall back to simple single-line
            if width < breakpoints::COMPACT {
                let line = format_status_line(state, theme, width);
                return StatusBanner::new(vec![line]);
            }

            let inner_width = width.saturating_sub(2); // Account for left/right borders

            // Get shortcuts row with button positions
            let (shortcuts_line, buttons) =
                format_shortcuts_row_with_positions(state, &colors, borders, inner_width);

            let lines = vec![
                format_top_border(&colors, borders, width),
                format_main_row(state, &colors, borders, theme, inner_width),
                shortcuts_line,
                format_bottom_border(&colors, borders, width),
            ];

            StatusBanner::with_buttons(lines, buttons)
        }
    }
}

/// Format the top border with VoxTerm badge.
fn format_top_border(colors: &ThemeColors, borders: &BorderSet, width: usize) -> String {
    let brand_label = format_brand_label(colors);
    let label_width = display_width(&brand_label);

    // Calculate border segments
    // Total: top_left(1) + left_segment + label + right_segment + top_right(1) = width
    let left_border_len = 2;
    let right_border_len = width.saturating_sub(left_border_len + label_width + 2); // +2 for corners

    let left_segment: String = std::iter::repeat_n(borders.horizontal, left_border_len).collect();
    let right_segment: String = std::iter::repeat_n(borders.horizontal, right_border_len).collect();

    format!(
        "{}{}{}{}{}{}{}",
        colors.border,
        borders.top_left,
        left_segment,
        colors.reset,
        brand_label,
        colors.border,
        right_segment,
        // borders.top_right,
        // colors.reset
    ) + &format!("{}{}", borders.top_right, colors.reset)
}

fn format_brand_label(colors: &ThemeColors) -> String {
    format!(
        " {}Vox{}{}Term{} ",
        colors.info, colors.reset, colors.recording, colors.reset
    )
}

fn format_duration_section(state: &StatusLineState, colors: &ThemeColors) -> String {
    if let Some(dur) = state.recording_duration {
        if state.recording_state == RecordingState::Recording {
            format!(" {:.1}s ", dur)
        } else {
            format!(" {}{:.1}s{} ", colors.dim, dur, colors.reset)
        }
    } else {
        format!(
            " {}{}{} ",
            colors.dim, MAIN_ROW_DURATION_PLACEHOLDER, colors.reset
        )
    }
}

fn dim_waveform_placeholder(width: usize, colors: &ThemeColors) -> String {
    let mut result = String::with_capacity(width + colors.dim.len() + colors.reset.len());
    result.push_str(colors.dim);
    for _ in 0..width {
        result.push('▁');
    }
    result.push_str(colors.reset);
    result
}

/// Format a button in clickable pill style with brackets.
/// Style: `[label]` with dim (or focused) brackets.
fn format_shortcut_pill(content: &str, colors: &ThemeColors, focused: bool) -> String {
    let bracket_color = if focused { colors.info } else { colors.dim };
    let mut result =
        String::with_capacity(content.len() + bracket_color.len() * 2 + colors.reset.len() * 2 + 2);
    result.push_str(bracket_color);
    result.push('[');
    result.push_str(colors.reset);
    result.push_str(content);
    result.push_str(bracket_color);
    result.push(']');
    result.push_str(colors.reset);
    result
}

/// Legacy bracket style for backwards compatibility.
#[allow(dead_code)]
fn format_panel_brackets(content: &str, colors: &ThemeColors) -> String {
    let mut result =
        String::with_capacity(content.len() + colors.dim.len() * 2 + colors.reset.len() * 2 + 2);
    result.push_str(colors.dim);
    result.push('[');
    result.push_str(colors.reset);
    result.push_str(content);
    result.push_str(colors.dim);
    result.push(']');
    result.push_str(colors.reset);
    result
}

fn format_meter_section(state: &StatusLineState, colors: &ThemeColors) -> String {
    let recording_active = state.recording_state == RecordingState::Recording;
    let db_text = if let Some(db) = state.meter_db {
        format!("{:>4.0}dB", db)
    } else {
        format!("{:>4}dB", "--")
    };
    let db_color = if recording_active {
        colors.info
    } else {
        colors.dim
    };
    format!(" {}{}{} ", db_color, db_text, colors.reset)
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
    let duration_section = format_duration_section(state, colors);
    let meter_section = format_meter_section(state, colors);

    // Status message with color
    let status_type = StatusType::from_message(&state.message);
    let status_color = status_type.color(colors);
    let message_section = if state.message.is_empty() {
        format!(" {}Ready{}", colors.success, colors.reset)
    } else {
        format!(" {}{}{}", status_color, state.message, colors.reset)
    };

    // Combine all sections
    let sep = format!("{}│{}", colors.dim, colors.reset);
    let content = [mode_section, duration_section, meter_section].join(&sep);

    let content_width = display_width(&content);
    let right_panel = format_right_panel(
        state,
        colors,
        theme,
        inner_width.saturating_sub(content_width + 1),
    );
    let right_width = display_width(&right_panel);
    let message_available = inner_width.saturating_sub(content_width + right_width);
    let truncated_message = truncate_display(&message_section, message_available);

    let interior = format!("{content}{truncated_message}");
    let message_width = display_width(&truncated_message);

    // Padding to fill the row (leave room for right panel)
    let padding_needed = inner_width.saturating_sub(content_width + message_width + right_width);
    let padding = " ".repeat(padding_needed);

    // No background colors - use transparent backgrounds for terminal compatibility
    format!(
        "{}{}{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        interior,
        padding,
        right_panel,
        colors.border,
        borders.vertical,
        colors.reset,
    )
}

fn format_right_panel(
    state: &StatusLineState,
    colors: &ThemeColors,
    theme: Theme,
    max_width: usize,
) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mode = state.hud_right_panel;
    if mode == HudRightPanel::Off {
        return String::new();
    }

    let content_width = max_width.saturating_sub(1);
    if content_width < RIGHT_PANEL_MIN_CONTENT_WIDTH {
        return " ".repeat(max_width);
    }

    let show_live = !state.meter_levels.is_empty();
    let panel_width = content_width;

    let panel = match mode {
        HudRightPanel::Ribbon => {
            let reserved = 2; // brackets
            let available = panel_width.saturating_sub(reserved);
            let wave_width = if available < MAIN_ROW_WAVEFORM_MIN_WIDTH {
                available
            } else {
                available.min(RIGHT_PANEL_MAX_WAVEFORM_WIDTH)
            };
            let waveform = if show_live {
                format_waveform(&state.meter_levels, wave_width, theme)
            } else {
                dim_waveform_placeholder(wave_width, colors)
            };
            format_panel_brackets(&waveform, colors)
        }
        HudRightPanel::Dots => {
            let active = state.meter_db.unwrap_or(-60.0);
            truncate_display(&format_pulse_dots(active, colors), panel_width)
        }
        HudRightPanel::Heartbeat => {
            truncate_display(&format_heartbeat_panel(state, colors), panel_width)
        }
        HudRightPanel::Off => String::new(),
    };

    if panel.is_empty() {
        return String::new();
    }

    let with_pad = format!(" {}", panel);
    if mode == HudRightPanel::Ribbon || mode == HudRightPanel::Heartbeat {
        pad_display(&with_pad, max_width)
    } else {
        let truncated = truncate_display(&with_pad, max_width);
        pad_display(&truncated, max_width)
    }
}

#[inline]
fn format_pulse_dots(level_db: f32, colors: &ThemeColors) -> String {
    let normalized = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0);
    let active = (normalized * 5.0).round() as usize;
    // Pre-allocate for 5 dots with color codes
    let mut result = String::with_capacity(128);
    result.push_str(colors.dim);
    result.push('[');
    for idx in 0..5 {
        if idx < active {
            let color = if normalized < 0.6 {
                colors.success
            } else if normalized < 0.85 {
                colors.warning
            } else {
                colors.error
            };
            result.push_str(color);
            result.push('•');
            result.push_str(colors.reset);
        } else {
            result.push_str(colors.dim);
            result.push('·');
            result.push_str(colors.reset);
        }
    }
    result.push_str(colors.dim);
    result.push(']');
    result.push_str(colors.reset);
    result
}

#[inline]
fn heartbeat_frame_index() -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() % HEARTBEAT_FRAMES.len() as u64) as usize
}

fn format_heartbeat_panel(state: &StatusLineState, colors: &ThemeColors) -> String {
    let recording_active = state.recording_state == RecordingState::Recording;
    let animate = !state.hud_right_panel_recording_only || recording_active;
    let frame_idx = if animate { heartbeat_frame_index() } else { 0 };
    let glyph = HEARTBEAT_FRAMES.get(frame_idx).copied().unwrap_or('·');

    let mut content = String::with_capacity(16);
    let is_peak = frame_idx == 2;
    let color = if animate && is_peak {
        colors.info
    } else {
        colors.dim
    };
    content.push_str(color);
    content.push(glyph);
    content.push_str(colors.reset);

    format_panel_brackets(&content, colors)
}

/// Format the mode indicator with appropriate color and symbol.
/// Uses animated indicators for recording (pulsing) and processing (spinning).
#[inline]
fn format_mode_indicator(state: &StatusLineState, colors: &ThemeColors) -> String {
    let pipeline_tag = pipeline_tag_short(state.pipeline);

    let mut result = String::with_capacity(32);
    result.push(' ');

    match state.recording_state {
        RecordingState::Recording => {
            result.push_str(colors.recording);
            result.push_str(get_recording_indicator());
            result.push_str(" REC ");
            result.push_str(pipeline_tag);
            result.push_str(colors.reset);
        }
        RecordingState::Processing => {
            result.push_str(colors.processing);
            result.push_str(get_processing_spinner());
            result.push_str(" processing ");
            result.push_str(pipeline_tag);
            result.push_str(colors.reset);
        }
        RecordingState::Idle => {
            let (indicator, label, color) = match state.voice_mode {
                VoiceMode::Auto => (colors.indicator_auto, "AUTO", colors.info),
                VoiceMode::Manual => (colors.indicator_manual, "MANUAL", ""),
                VoiceMode::Idle => (colors.indicator_idle, "IDLE", ""),
            };
            if !color.is_empty() {
                result.push_str(color);
            }
            result.push_str(indicator);
            result.push(' ');
            result.push_str(label);
            if !color.is_empty() {
                result.push_str(colors.reset);
            }
        }
    }

    result.push(' ');
    result
}

/// Format the shortcuts row with dimmed styling and return button positions.
fn format_shortcuts_row_with_positions(
    state: &StatusLineState,
    colors: &ThemeColors,
    borders: &BorderSet,
    inner_width: usize,
) -> (String, Vec<ButtonPosition>) {
    // Row 2 from bottom of HUD (row 1 = bottom border)
    let (shortcuts_str, buttons) = format_button_row_with_positions(state, colors, inner_width, 2);

    // Add leading space to match main row's left margin
    let interior = format!(" {}", shortcuts_str);
    let interior_width = display_width(&interior);
    let padding_needed = inner_width.saturating_sub(interior_width);
    let padding = " ".repeat(padding_needed);

    // Match main row format: border + interior + padding + border
    let line = format!(
        "{}{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        interior,
        padding,
        colors.border,
        borders.vertical,
        colors.reset,
    );

    (line, buttons)
}

/// Format the shortcuts row with dimmed styling.
#[allow(dead_code)]
fn format_shortcuts_row(
    state: &StatusLineState,
    colors: &ThemeColors,
    borders: &BorderSet,
    inner_width: usize,
) -> String {
    let (line, _) = format_shortcuts_row_with_positions(state, colors, borders, inner_width);
    line
}

/// Legacy format_shortcuts_row without position tracking.
#[allow(dead_code)]
fn format_shortcuts_row_legacy(
    state: &StatusLineState,
    colors: &ThemeColors,
    borders: &BorderSet,
    inner_width: usize,
) -> String {
    let shortcuts_str = format_button_row(state, colors, inner_width);

    // Add leading space to match main row's left margin
    let interior = format!(" {}", shortcuts_str);
    let interior_width = display_width(&interior);
    let padding_needed = inner_width.saturating_sub(interior_width);
    let padding = " ".repeat(padding_needed);

    // Match main row format: border + interior + padding + border
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

/// Button definition for position tracking.
struct ButtonDef {
    label: &'static str,
    action: ButtonAction,
}

/// Build buttons with their labels and actions based on current state.
fn get_button_defs(state: &StatusLineState) -> Vec<ButtonDef> {
    let voice_label = if state.auto_voice_enabled {
        "auto"
    } else {
        "ptt"
    };
    let send_label = match state.send_mode {
        VoiceSendMode::Auto => "send",
        VoiceSendMode::Insert => "edit",
    };

    vec![
        ButtonDef {
            label: "rec",
            action: ButtonAction::VoiceTrigger,
        },
        ButtonDef {
            label: voice_label,
            action: ButtonAction::ToggleAutoVoice,
        },
        ButtonDef {
            label: send_label,
            action: ButtonAction::ToggleSendMode,
        },
        ButtonDef {
            label: "set",
            action: ButtonAction::SettingsToggle,
        },
        ButtonDef {
            label: "hud",
            action: ButtonAction::ToggleHudStyle,
        },
        ButtonDef {
            label: "help",
            action: ButtonAction::HelpToggle,
        },
        ButtonDef {
            label: "theme",
            action: ButtonAction::ThemePicker,
        },
    ]
}

/// Format button row and return (formatted_string, button_positions).
/// Button positions are relative to the row start (after border character).
fn format_button_row_with_positions(
    state: &StatusLineState,
    colors: &ThemeColors,
    inner_width: usize,
    hud_row: u16,
) -> (String, Vec<ButtonPosition>) {
    let button_defs = get_button_defs(state);
    let mut items = Vec::new();
    let mut positions = Vec::new();

    // Track current x position (1-based, after border + leading space = column 3)
    let mut current_x: u16 = 3; // border(1) + space(1) + first char at (3)
    let separator_visible_width = 3u16; // " · " = 3 visible chars

    for (idx, def) in button_defs.iter().enumerate() {
        // Get color for this button - static buttons use border/accent color
        let highlight = match def.action {
            ButtonAction::VoiceTrigger => match state.recording_state {
                RecordingState::Recording => colors.recording,
                RecordingState::Processing => colors.processing,
                RecordingState::Idle => colors.border, // Accent color when idle
            },
            ButtonAction::ToggleAutoVoice => {
                if state.auto_voice_enabled {
                    colors.info
                } else {
                    colors.border
                }
            }
            ButtonAction::ToggleSendMode => match state.send_mode {
                VoiceSendMode::Auto => colors.success,
                VoiceSendMode::Insert => colors.warning,
            },
            // Static buttons use border/accent color to pop
            ButtonAction::SettingsToggle
            | ButtonAction::ToggleHudStyle
            | ButtonAction::HudBack
            | ButtonAction::HelpToggle
            | ButtonAction::ThemePicker => colors.border,
        };

        let focused = state.hud_button_focus == Some(def.action);
        let formatted = format_button(colors, def.label, highlight, focused);
        let visible_width = def.label.len() as u16 + 2; // [label] = label + 2 brackets

        // Record position
        positions.push(ButtonPosition {
            start_x: current_x,
            end_x: current_x + visible_width - 1,
            row: hud_row,
            action: def.action,
        });

        items.push(formatted);

        // Move x position: button width + separator (if not last)
        current_x += visible_width;
        if idx < button_defs.len() - 1 {
            current_x += separator_visible_width;
        }
    }

    // Queue badge (not clickable)
    if state.queue_depth > 0 {
        items.push(format!(
            "{}Q:{}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }

    // Latency badge (not clickable)
    if let Some(latency) = state.last_latency_ms {
        let latency_color = if latency < 300 {
            colors.success
        } else if latency < 500 {
            colors.warning
        } else {
            colors.error
        };
        items.push(format!("{}{}ms{}", latency_color, latency, colors.reset));
    }

    // Modern separator: dim dot
    let separator = format!(" {}·{} ", colors.dim, colors.reset);
    let row = items.join(&separator);

    if display_width(&row) <= inner_width {
        return (row, positions);
    }

    // Compact mode: fewer buttons, recalculate positions
    let mut compact_items = Vec::new();
    let mut compact_positions = Vec::new();
    let compact_indices = [0, 1, 2, 3, 5, 6]; // rec, auto, send, set, help, theme

    current_x = 3;
    for (i, &idx) in compact_indices.iter().enumerate() {
        let def = &button_defs[idx];
        let highlight = match def.action {
            ButtonAction::VoiceTrigger => match state.recording_state {
                RecordingState::Recording => colors.recording,
                RecordingState::Processing => colors.processing,
                RecordingState::Idle => colors.border,
            },
            ButtonAction::ToggleAutoVoice => {
                if state.auto_voice_enabled {
                    colors.info
                } else {
                    colors.border
                }
            }
            ButtonAction::ToggleSendMode => match state.send_mode {
                VoiceSendMode::Auto => colors.success,
                VoiceSendMode::Insert => colors.warning,
            },
            ButtonAction::SettingsToggle
            | ButtonAction::ToggleHudStyle
            | ButtonAction::HudBack
            | ButtonAction::HelpToggle
            | ButtonAction::ThemePicker => colors.border,
        };

        let focused = state.hud_button_focus == Some(def.action);
        let formatted = format_button(colors, def.label, highlight, focused);
        let visible_width = def.label.len() as u16 + 2;

        compact_positions.push(ButtonPosition {
            start_x: current_x,
            end_x: current_x + visible_width - 1,
            row: hud_row,
            action: def.action,
        });

        compact_items.push(formatted);
        current_x += visible_width;
        if i < compact_indices.len() - 1 {
            current_x += 1; // space separator in compact mode
        }
    }

    if state.queue_depth > 0 {
        compact_items.push(format!(
            "{}Q:{}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }

    let compact_row = truncate_display(&compact_items.join(" "), inner_width);
    (compact_row, compact_positions)
}

fn format_button_row(state: &StatusLineState, colors: &ThemeColors, inner_width: usize) -> String {
    let (row, _) = format_button_row_with_positions(state, colors, inner_width, 2);
    row
}

#[allow(dead_code)]
fn format_button_row_legacy(
    state: &StatusLineState,
    colors: &ThemeColors,
    inner_width: usize,
) -> String {
    let mut items = Vec::new();

    // rec - RED when recording, yellow when processing, dim when idle
    let rec_color = match state.recording_state {
        RecordingState::Recording => colors.recording,
        RecordingState::Processing => colors.processing,
        RecordingState::Idle => "",
    };
    items.push(format_button(colors, "rec", rec_color, false));

    // auto/ptt - blue when auto-voice, dim when ptt
    let (voice_label, voice_color) = if state.auto_voice_enabled {
        ("auto", colors.info) // blue = auto-voice on
    } else {
        ("ptt", "") // dim = push-to-talk mode
    };
    items.push(format_button(colors, voice_label, voice_color, false));

    // send mode: auto/insert - green when auto-send, yellow when insert
    let (send_label, send_color) = match state.send_mode {
        VoiceSendMode::Auto => ("send", colors.success), // green = auto-send
        VoiceSendMode::Insert => ("edit", colors.warning), // yellow = insert/edit mode
    };
    items.push(format_button(colors, send_label, send_color, false));

    // Static buttons - always dim
    items.push(format_button(colors, "set", "", false));
    items.push(format_button(colors, "hud", "", false));
    items.push(format_button(colors, "help", "", false));
    items.push(format_button(colors, "theme", "", false));

    // Queue badge - modern pill style
    if state.queue_depth > 0 {
        items.push(format!(
            "{}Q:{}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }

    // Latency badge if available
    if let Some(latency) = state.last_latency_ms {
        let latency_color = if latency < 300 {
            colors.success
        } else if latency < 500 {
            colors.warning
        } else {
            colors.error
        };
        items.push(format!("{}{}ms{}", latency_color, latency, colors.reset));
    }

    // Modern separator: dim dot
    let separator = format!(" {}·{} ", colors.dim, colors.reset);
    let row = items.join(&separator);
    if display_width(&row) <= inner_width {
        return row;
    }

    // Compact: keep essentials (rec/auto/send/settings/help)
    let mut compact = vec![
        items[0].clone(),
        items[1].clone(),
        items[2].clone(),
        items[3].clone(),
        items[5].clone(),
    ];
    if state.queue_depth > 0 {
        compact.push(format!(
            "{}Q:{}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }
    truncate_display(&compact.join(" "), inner_width)
}

/// Format a clickable button - colored label when active, dim otherwise.
/// Style: `[label]` - brackets for clickable appearance, no shortcut prefix.
#[inline]
fn format_button(colors: &ThemeColors, label: &str, highlight: &str, focused: bool) -> String {
    // Pre-allocate capacity for typical button string
    let mut content = String::with_capacity(32);
    // Label color: highlight if active, dim otherwise
    if highlight.is_empty() {
        content.push_str(colors.dim);
        content.push_str(label);
        content.push_str(colors.reset);
    } else {
        content.push_str(highlight);
        content.push_str(label);
        content.push_str(colors.reset);
    }
    format_shortcut_pill(&content, colors, focused)
}

/// Legacy format with shortcut key prefix (for help display).
#[inline]
#[allow(dead_code)]
fn format_shortcut_colored(
    colors: &ThemeColors,
    key: &str,
    label: &str,
    highlight: &str,
) -> String {
    let mut content = String::with_capacity(48);
    content.push_str(colors.dim);
    content.push_str(key);
    content.push_str(colors.reset);
    content.push(' ');
    if highlight.is_empty() {
        content.push_str(colors.dim);
        content.push_str(label);
        content.push_str(colors.reset);
    } else {
        content.push_str(highlight);
        content.push_str(label);
        content.push_str(colors.reset);
    }
    format_shortcut_pill(&content, colors, false)
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
#[must_use]
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

struct CompactModeParts<'a> {
    indicator: &'a str,
    label: &'a str,
    color: &'a str,
}

fn compact_mode_parts<'a>(
    state: &'a StatusLineState,
    colors: &'a ThemeColors,
) -> CompactModeParts<'a> {
    let pipeline_tag = pipeline_tag_short(state.pipeline);
    match state.recording_state {
        RecordingState::Recording => CompactModeParts {
            indicator: "●",
            label: pipeline_tag,
            color: colors.recording,
        },
        RecordingState::Processing => CompactModeParts {
            indicator: "◐",
            label: pipeline_tag,
            color: colors.processing,
        },
        RecordingState::Idle => {
            let (label, color) = match state.voice_mode {
                VoiceMode::Auto => ("A", colors.info),
                VoiceMode::Manual => ("M", ""),
                VoiceMode::Idle => ("", ""),
            };
            CompactModeParts {
                indicator: state.voice_mode.indicator(),
                label,
                color,
            }
        }
    }
}

fn format_compact_indicator(parts: &CompactModeParts<'_>, colors: &ThemeColors) -> String {
    if parts.color.is_empty() {
        parts.indicator.to_string()
    } else {
        format!("{}{}{}", parts.color, parts.indicator, colors.reset)
    }
}

fn format_compact_mode(parts: &CompactModeParts<'_>, colors: &ThemeColors) -> String {
    if parts.label.is_empty() {
        format_compact_indicator(parts, colors)
    } else if parts.color.is_empty() {
        format!("{} {}", parts.indicator, parts.label)
    } else {
        format!(
            "{}{} {}{}",
            parts.color, parts.indicator, parts.label, colors.reset
        )
    }
}

/// Format minimal status for very narrow terminals.
fn format_minimal(state: &StatusLineState, colors: &ThemeColors, width: usize) -> String {
    let indicator = format_compact_indicator(&compact_mode_parts(state, colors), colors);

    let msg = if state.message.is_empty() {
        if state.voice_mode == VoiceMode::Auto {
            "auto".to_string()
        } else {
            format!("{}Ready{}", colors.success, colors.reset)
        }
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
    let mode = format_compact_mode(&compact_mode_parts(state, colors), colors);

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
    let parts = compact_mode_parts(state, colors);
    let mode_indicator = format_compact_indicator(&parts, colors);
    let mode_label = parts.label;

    if mode_label.is_empty() {
        format!("{} │ {:.0}dB", mode_indicator, state.sensitivity_db)
    } else {
        format!(
            "{}{} │ {:.0}dB",
            mode_indicator, mode_label, state.sensitivity_db
        )
    }
}

/// Format compact shortcuts with modern separator.
fn format_shortcuts_compact(colors: &ThemeColors) -> String {
    // Compact style: dot separator
    let sep = format!(" {}·{} ", colors.dim, colors.reset);
    format_shortcuts_list(colors, SHORTCUTS_COMPACT, &sep)
}

fn format_left_section(state: &StatusLineState, colors: &ThemeColors) -> String {
    let pipeline_tag = pipeline_tag_short(state.pipeline);
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

    // Use animated indicators for recording and processing
    let mode_indicator = match state.recording_state {
        RecordingState::Recording => get_recording_indicator(),
        RecordingState::Processing => get_processing_spinner(),
        RecordingState::Idle => state.voice_mode.indicator(),
    };

    let mode_label = match state.recording_state {
        RecordingState::Recording => format!("REC {pipeline_tag}"),
        RecordingState::Processing => format!("processing {pipeline_tag}"),
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
    // Modern style: dimmed separator between shortcuts
    let sep = format!(" {}│{} ", colors.dim, colors.reset);
    format_shortcuts_list(colors, SHORTCUTS, &sep)
}

fn format_shortcuts_list(
    colors: &ThemeColors,
    shortcuts: &[(&str, &str)],
    separator: &str,
) -> String {
    let mut parts = Vec::with_capacity(shortcuts.len());
    for (key, action) in shortcuts {
        // Modern style: dim key, normal label
        parts.push(format!("{}{}{} {}", colors.dim, key, colors.reset, action));
    }
    parts.join(separator)
}

/// Calculate display width excluding ANSI escape codes.
#[inline]
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
#[inline]
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

fn pad_display(s: &str, width: usize) -> String {
    let current = display_width(s);
    if current >= width {
        return truncate_display(s, width);
    }
    let mut result = String::with_capacity(s.len() + width.saturating_sub(current));
    result.push_str(s);
    result.push_str(&" ".repeat(width.saturating_sub(current)));
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

    #[test]
    fn status_banner_height_respects_hud_style() {
        // Full mode: 4 rows for wide terminals
        assert_eq!(status_banner_height(80, HudStyle::Full), 4);
        // Full mode: 1 row for narrow terminals
        assert_eq!(status_banner_height(30, HudStyle::Full), 1);

        // Minimal mode: always 1 row
        assert_eq!(status_banner_height(80, HudStyle::Minimal), 1);
        assert_eq!(status_banner_height(30, HudStyle::Minimal), 1);

        // Hidden mode: reserve 1 row (blank when idle)
        assert_eq!(status_banner_height(80, HudStyle::Hidden), 1);
        assert_eq!(status_banner_height(30, HudStyle::Hidden), 1);
    }

    #[test]
    fn format_status_banner_minimal_mode() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Minimal;
        state.voice_mode = VoiceMode::Auto;
        state.auto_voice_enabled = true;

        let banner = format_status_banner(&state, Theme::None, 80);
        // Minimal mode should produce a single-line banner
        assert_eq!(banner.height, 1);
        assert!(banner.lines[0].contains("AUTO"));
    }

    #[test]
    fn format_status_banner_hidden_mode_idle() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Hidden;
        state.voice_mode = VoiceMode::Auto;
        state.recording_state = RecordingState::Idle;

        let banner = format_status_banner(&state, Theme::None, 80);
        // Hidden mode when idle should reserve a blank row
        assert_eq!(banner.height, 1);
        assert_eq!(banner.lines.len(), 1);
        assert!(banner.lines[0].is_empty());
    }

    #[test]
    fn format_status_banner_hidden_mode_recording() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Hidden;
        state.recording_state = RecordingState::Recording;

        let banner = format_status_banner(&state, Theme::None, 80);
        // Hidden mode when recording should show dim/obscure indicator
        assert_eq!(banner.height, 1);
        assert!(banner.lines[0].contains("rec")); // lowercase, obscure style
    }

    #[test]
    fn format_status_banner_minimal_mode_recording() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Minimal;
        state.recording_state = RecordingState::Recording;

        let banner = format_status_banner(&state, Theme::None, 80);
        // Minimal mode when recording should show REC
        assert_eq!(banner.height, 1);
        assert!(banner.lines[0].contains("REC"));
    }

    #[test]
    fn format_status_banner_minimal_mode_processing() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Minimal;
        state.recording_state = RecordingState::Processing;

        let banner = format_status_banner(&state, Theme::None, 80);
        // Minimal mode when processing should show processing indicator
        assert_eq!(banner.height, 1);
        assert!(banner.lines[0].contains("processing"));
    }

    #[test]
    fn format_status_banner_full_mode_recording_shows_rec_and_meter() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.voice_mode = VoiceMode::Auto;
        state.auto_voice_enabled = true;
        state.recording_state = RecordingState::Recording;
        state
            .meter_levels
            .extend_from_slice(&[-60.0, -45.0, -30.0, -15.0]);
        state.meter_db = Some(-30.0);
        state.message = "Recording...".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 80);
        assert_eq!(banner.height, 4);
        assert!(banner.lines.iter().any(|line| line.contains("REC")));
        assert!(banner.lines.iter().any(|line| line.contains("dB")));
    }
}
