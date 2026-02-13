//! Status-line formatter so full/minimal HUD modes share consistent semantics.

use std::sync::OnceLock;

use crate::audio_meter::format_waveform;
use crate::buttons::ButtonAction;
use crate::config::{HudRightPanel, HudStyle};
use crate::hud::{HudRegistry, HudState, LatencyModule, MeterModule, Mode as HudMode, QueueModule};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};

use super::animation::{
    get_processing_spinner, get_recording_indicator, heartbeat_glyph, transition_marker,
};
use super::buttons::{
    format_hidden_launcher_with_button, format_minimal_strip_with_button,
    format_shortcuts_row_with_positions,
};
use super::layout::breakpoints;
use super::state::{Pipeline, RecordingState, StatusBanner, StatusLineState, VoiceMode};
use super::text::{display_width, truncate_display};

const MAIN_ROW_DURATION_PLACEHOLDER: &str = "--.-s";
const MAIN_ROW_WAVEFORM_MIN_WIDTH: usize = 3;
const RIGHT_PANEL_MAX_WAVEFORM_WIDTH: usize = 20;
const RIGHT_PANEL_MIN_CONTENT_WIDTH: usize = 4;

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

fn pipeline_tag_short(pipeline: Pipeline) -> &'static str {
    match pipeline {
        Pipeline::Rust => "R",
        Pipeline::Python => "PY",
    }
}

fn full_mode_voice_label(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Auto => "AUTO",
        VoiceMode::Manual => "PTT",
        VoiceMode::Idle => "IDLE",
    }
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
/// Hidden mode: Branded launcher when idle; dim indicator when recording (e.g., "● rec 5s")
pub fn format_status_banner(state: &StatusLineState, theme: Theme, width: usize) -> StatusBanner {
    let colors = theme.colors();
    let borders = &colors.borders;

    // Handle HUD style
    match state.hud_style {
        HudStyle::Hidden => {
            if state.recording_state == RecordingState::Recording
                || state.recording_state == RecordingState::Processing
            {
                let line = format_hidden_strip(state, &colors, width);
                StatusBanner::new(vec![line])
            } else {
                // Idle hidden mode still renders a branded launcher to stay discoverable.
                let (line, button) = format_hidden_launcher_with_button(state, &colors, width);
                StatusBanner::with_buttons(vec![line], button.into_iter().collect())
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
            let message_anchor_col = buttons
                .iter()
                .find(|button| button.action == ButtonAction::SettingsToggle)
                .map(|button| button.start_x as usize);

            let lines = vec![
                format_top_border(&colors, borders, width),
                format_main_row(
                    state,
                    &colors,
                    borders,
                    theme,
                    inner_width,
                    message_anchor_col,
                ),
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
    message_anchor_col: Option<usize>,
) -> String {
    // Build content sections
    let mode_section = format_mode_indicator(state, colors);
    let duration_section = format_duration_section(state, colors);
    let meter_section = format_meter_section(state, colors);

    let message_section = format_full_hud_message(state, colors);

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
    let message_leading_space = if message_section.starts_with(' ') {
        1
    } else {
        0
    };
    let message_anchor_padding = if message_section.is_empty() {
        0
    } else {
        let target_prefix_width = message_anchor_col
            .map(|anchor_col| anchor_col.saturating_sub(2 + message_leading_space))
            .unwrap_or(content_width);
        let desired_padding = target_prefix_width.saturating_sub(content_width);
        desired_padding.min(message_available.saturating_sub(1))
    };
    let padded_message = if message_anchor_padding == 0 {
        message_section
    } else {
        format!("{}{}", " ".repeat(message_anchor_padding), message_section)
    };
    let truncated_message = truncate_display(&padded_message, message_available);
    let message_width = display_width(&truncated_message);
    let interior = format!("{content}{truncated_message}");

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

fn format_full_hud_message(state: &StatusLineState, colors: &ThemeColors) -> String {
    if state.recording_state != RecordingState::Idle {
        if state.message.is_empty() {
            return String::new();
        }
        let status_type = StatusType::from_message(&state.message);
        return match status_type {
            // Keep only high-signal warnings/errors during active states; state itself is shown on the left.
            StatusType::Warning => format!(" {}{}{}", colors.warning, state.message, colors.reset),
            StatusType::Error => format!(" {}{}{}", colors.error, state.message, colors.reset),
            StatusType::Recording
            | StatusType::Processing
            | StatusType::Success
            | StatusType::Info => String::new(),
        };
    }

    if state.queue_depth > 0 {
        // Queue is shown on the shortcuts row (`Q:n`); avoid duplicate text on main row.
        return String::new();
    }

    if state.message.is_empty() {
        return String::new();
    }

    let status_type = StatusType::from_message(&state.message);
    match status_type {
        // Keep idle success state concise in the shortcut row badge.
        StatusType::Success => String::new(),
        // Show actionable/info toggles in the center message lane.
        StatusType::Info => format!(" {}{}{}", colors.info, state.message, colors.reset),
        StatusType::Warning => format!(" {}{}{}", colors.warning, state.message, colors.reset),
        StatusType::Error => format!(" {}{}{}", colors.error, state.message, colors.reset),
        StatusType::Processing | StatusType::Recording => {
            format!(
                " {}{}{}",
                status_type.color(colors),
                state.message,
                colors.reset
            )
        }
    }
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
    // Return only the panel content width; caller handles row-level padding/right alignment.
    truncate_display(&with_pad, max_width)
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

fn format_heartbeat_panel(state: &StatusLineState, colors: &ThemeColors) -> String {
    let recording_active = state.recording_state == RecordingState::Recording;
    let animate = !state.hud_right_panel_recording_only || recording_active;
    let (glyph, is_peak) = heartbeat_glyph(animate);

    let mut content = String::with_capacity(16);
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
                VoiceMode::Manual => (colors.indicator_manual, "PTT", ""),
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

    if state.recording_state == RecordingState::Idle && state.transition_progress > 0.0 {
        let marker = transition_marker(state.transition_progress);
        if !marker.is_empty() {
            result.push(' ');
            result.push_str(colors.info);
            result.push_str(marker);
            result.push_str(colors.reset);
        }
    }

    result.push(' ');
    result
}

fn format_transition_suffix(state: &StatusLineState, colors: &ThemeColors) -> String {
    if state.recording_state != RecordingState::Idle || state.transition_progress <= 0.0 {
        return String::new();
    }
    let marker = transition_marker(state.transition_progress);
    if marker.is_empty() {
        String::new()
    } else {
        format!(" {}{}{}", colors.info, marker, colors.reset)
    }
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
    let mode_width = display_width(&mode);
    let module_budget = width.saturating_sub(mode_width + 1);

    let registry = compact_hud_registry(state, module_budget);
    let hud_state = HudState {
        mode: match state.voice_mode {
            VoiceMode::Auto => HudMode::Auto,
            VoiceMode::Manual => HudMode::Manual,
            VoiceMode::Idle => HudMode::Insert,
        },
        is_recording: state.recording_state == RecordingState::Recording,
        recording_duration_secs: state.recording_duration.unwrap_or(0.0),
        audio_level_db: state.meter_db.unwrap_or(-60.0),
        audio_levels: state.meter_levels.clone(),
        queue_depth: state.queue_depth,
        last_latency_ms: state.last_latency_ms,
        latency_history_ms: state.latency_history_ms.clone(),
        backend_name: String::new(),
    };
    let modules = registry.render_all(&hud_state, module_budget, " · ");
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

#[derive(Debug, Clone, Copy)]
enum CompactHudProfile {
    Recording,
    Busy,
    Idle,
}

fn compact_hud_registry(state: &StatusLineState, module_budget: usize) -> &'static HudRegistry {
    let profile = if state.recording_state == RecordingState::Recording && module_budget >= 12 {
        CompactHudProfile::Recording
    } else if state.queue_depth > 0 {
        CompactHudProfile::Busy
    } else {
        CompactHudProfile::Idle
    };
    compact_hud_registry_for_profile(profile)
}

fn compact_hud_registry_for_profile(profile: CompactHudProfile) -> &'static HudRegistry {
    static RECORDING: OnceLock<HudRegistry> = OnceLock::new();
    static BUSY: OnceLock<HudRegistry> = OnceLock::new();
    static IDLE: OnceLock<HudRegistry> = OnceLock::new();

    match profile {
        CompactHudProfile::Recording => RECORDING.get_or_init(|| {
            let mut registry = HudRegistry::new();
            registry.register(Box::new(MeterModule::with_bar_count(8)));
            registry.register(Box::new(LatencyModule::new()));
            registry.register(Box::new(QueueModule::new()));
            registry
        }),
        CompactHudProfile::Busy => BUSY.get_or_init(|| {
            let mut registry = HudRegistry::new();
            registry.register(Box::new(QueueModule::new()));
            registry.register(Box::new(LatencyModule::new()));
            registry
        }),
        CompactHudProfile::Idle => IDLE.get_or_init(|| {
            let mut registry = HudRegistry::new();
            registry.register(Box::new(LatencyModule::new()));
            registry
        }),
    }
}

/// Format compact left section for medium terminals.
fn format_left_compact(state: &StatusLineState, colors: &ThemeColors) -> String {
    let parts = compact_mode_parts(state, colors);
    let mode_indicator = format_compact_indicator(&parts, colors);
    let mode_label = parts.label;
    let transition = format_transition_suffix(state, colors);

    if mode_label.is_empty() {
        format!(
            "{}{} │ {:.0}dB",
            mode_indicator, transition, state.sensitivity_db
        )
    } else {
        format!(
            "{}{mode_label}{} │ {:.0}dB",
            mode_indicator, transition, state.sensitivity_db
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
    let transition = format_transition_suffix(state, colors);
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
        RecordingState::Recording => format!("REC {pipeline_tag}{transition}"),
        RecordingState::Processing => format!("processing {pipeline_tag}{transition}"),
        RecordingState::Idle => {
            format!("{}{}", full_mode_voice_label(state.voice_mode), transition)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        // Hidden mode when idle should keep a discoverable launcher row.
        assert_eq!(banner.height, 1);
        assert_eq!(banner.lines.len(), 1);
        assert!(banner.lines[0].contains("Vox"));
        assert!(banner.lines[0].contains("Ctrl+U"));
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

    #[test]
    fn format_status_banner_full_mode_shows_ready_with_ribbon_panel() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.hud_right_panel = HudRightPanel::Ribbon;
        state.recording_state = RecordingState::Idle;
        state.message.clear();

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert_eq!(banner.height, 4);
        assert!(banner.lines.iter().any(|line| line.contains("Ready")));
    }

    #[test]
    fn format_status_banner_full_mode_collapses_idle_success_to_ready() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.message = "Transcript ready (Rust pipeline)".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert!(banner.lines.iter().any(|line| line.contains("Ready")));
        assert!(!banner
            .lines
            .iter()
            .any(|line| line.contains("Transcript ready")));
    }

    #[test]
    fn format_status_banner_full_mode_avoids_duplicate_queue_text() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.queue_depth = 1;
        state.message = "Transcript queued (1)".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert!(!banner
            .lines
            .iter()
            .any(|line| line.contains("Transcript queued")));
        assert!(banner.lines.iter().any(|line| line.contains("Q:1")));
    }

    #[test]
    fn format_status_banner_full_mode_uses_ptt_label_for_manual() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.voice_mode = VoiceMode::Manual;

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert!(banner.lines.iter().any(|line| line.contains("PTT")));
        assert!(!banner.lines.iter().any(|line| line.contains("MANUAL")));
    }

    #[test]
    fn format_status_banner_full_mode_recording_suppresses_stale_ready_text() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Recording;
        state.message = "Transcript ready (Rust pipeline)".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert!(!banner.lines.iter().any(|line| line.contains("Ready")));
    }

    #[test]
    fn format_status_banner_full_mode_processing_does_not_duplicate_processing_text() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Processing;
        state.message = "Processing...".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 96);
        let main_row = &banner.lines[1];
        let count = main_row.to_lowercase().matches("processing").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn format_status_banner_full_mode_shows_latency_on_shortcuts_row() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(228);

        let banner = format_status_banner(&state, Theme::Coral, 96);
        assert!(!banner.lines[1].contains("228ms"));
        assert!(banner.lines[2].contains("228ms"));
    }

    #[test]
    fn format_status_banner_full_mode_shows_idle_info_message_on_main_row() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.message = "Auto-voice disabled (capture cancelled)".to_string();

        let banner = format_status_banner(&state, Theme::Coral, 120);
        assert!(banner.lines[1].contains("Auto-voice disabled"));
    }

    #[test]
    fn format_status_banner_full_mode_aligns_info_message_over_settings_lane() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.voice_mode = VoiceMode::Manual;
        state.message = "Auto-voice disabled (capture cancelled)".to_string();

        let banner = format_status_banner(&state, Theme::None, 140);
        let message_col = banner.lines[1]
            .find("Auto-voice")
            .expect("main row should include message");
        let settings_col = banner.lines[2]
            .find("[set]")
            .expect("shortcuts row should include [set]");
        // Message text should align with the `set` label lane (one column past `[`)
        assert_eq!(message_col, settings_col + 1);
    }

    #[test]
    fn format_status_line_shows_transition_marker_when_active() {
        let mut state = StatusLineState::new();
        state.voice_mode = VoiceMode::Auto;
        state.transition_progress = 0.9;
        let line = format_status_line(&state, Theme::Coral, 80);
        assert!(line.contains("✦") || line.contains("•") || line.contains("·"));
    }

    #[test]
    fn compact_registry_adapts_to_queue_state() {
        let mut state = StatusLineState::new();
        state.queue_depth = 2;
        let registry = compact_hud_registry(&state, 16);
        let ids: Vec<&str> = registry.iter().map(|module| module.id()).collect();
        assert_eq!(ids.first().copied(), Some("queue"));
    }
}
