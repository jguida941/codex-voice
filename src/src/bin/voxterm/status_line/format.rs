//! Status-line formatter so full/minimal HUD modes share consistent semantics.

use std::sync::OnceLock;

use crate::audio_meter::format_waveform;
use crate::config::{HudRightPanel, HudStyle};
use crate::hud::{HudRegistry, HudState, LatencyModule, MeterModule, Mode as HudMode, QueueModule};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};

use super::animation::{get_processing_spinner, get_recording_indicator, heartbeat_glyph};
use super::buttons::{
    format_hidden_launcher_with_button, format_minimal_strip_with_button,
    format_shortcuts_row_with_positions,
};
use super::layout::breakpoints;
use super::state::{
    Pipeline, RecordingState, StatusBanner, StatusLineState, VoiceIntentMode, VoiceMode,
};
use super::text::{display_width, pad_display, truncate_display};

const MAIN_ROW_DURATION_PLACEHOLDER: &str = "--.-s";
const MAIN_ROW_WAVEFORM_MIN_WIDTH: usize = 3;
const RIGHT_PANEL_MAX_WAVEFORM_WIDTH: usize = 12;
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

fn intent_tag(state: &StatusLineState) -> String {
    if state.review_before_send {
        format!("{} RVW", state.voice_intent_mode.short_label())
    } else {
        state.voice_intent_mode.short_label().to_string()
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
    let intent_tag = intent_tag(state);

    let mut result = String::with_capacity(32);
    result.push(' ');

    match state.recording_state {
        RecordingState::Recording => {
            result.push_str(colors.recording);
            result.push_str(get_recording_indicator());
            result.push_str(" REC ");
            result.push_str(pipeline_tag);
            result.push(' ');
            result.push_str(&intent_tag);
            result.push_str(colors.reset);
        }
        RecordingState::Processing => {
            result.push_str(colors.processing);
            result.push_str(get_processing_spinner());
            result.push_str(" processing ");
            result.push_str(pipeline_tag);
            result.push(' ');
            result.push_str(&intent_tag);
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
            result.push(' ');
            result.push_str(&intent_tag);
            if !color.is_empty() {
                result.push_str(colors.reset);
            }
        }
    }

    result.push(' ');
    result
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
    let intent = match state.voice_intent_mode {
        VoiceIntentMode::Command => "cmd",
        VoiceIntentMode::Dictation => "dict",
    };
    let review = if state.review_before_send { " rvw" } else { "" };

    let msg = if state.message.is_empty() {
        if state.voice_mode == VoiceMode::Auto {
            format!("auto {intent}{review}")
        } else {
            format!(
                "{}Ready {}{}{}",
                colors.success, intent, review, colors.reset
            )
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
    let intent = intent_tag(state);

    if mode_label.is_empty() {
        format!(
            "{}{} │ {:.0}dB",
            mode_indicator, intent, state.sensitivity_db
        )
    } else {
        format!(
            "{}{}/{} │ {:.0}dB",
            mode_indicator, mode_label, intent, state.sensitivity_db
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

    let intent_tag = intent_tag(state);
    let mode_label = match state.recording_state {
        RecordingState::Recording => format!("REC {pipeline_tag} {intent_tag}"),
        RecordingState::Processing => format!("processing {pipeline_tag} {intent_tag}"),
        RecordingState::Idle => format!("{} {intent_tag}", state.voice_mode.label()),
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
    fn format_status_line_shows_review_tag_when_enabled() {
        let state = StatusLineState {
            auto_voice_enabled: true,
            voice_mode: VoiceMode::Auto,
            review_before_send: true,
            ..Default::default()
        };
        let line = format_status_line(&state, Theme::Coral, 80);
        assert!(line.contains("RVW"));
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
}
