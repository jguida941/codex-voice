//! Status-line button layout and hitbox logic so keyboard/mouse actions map reliably.

use crate::buttons::ButtonAction;
use crate::config::{HudRightPanel, HudStyle, LatencyDisplayMode, VoiceSendMode};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};

use super::animation::{get_processing_spinner, get_recording_indicator, heartbeat_glyph};
use super::layout::breakpoints;
use super::state::{ButtonPosition, RecordingState, StatusLineState, VoiceMode};
use super::text::{display_width, truncate_display};

/// Get clickable button positions for the current state.
/// Returns button positions for full HUD mode (row 2 from bottom) and minimal mode (row 1).
/// Hidden mode exposes an "open" launcher button while idle.
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
            let (_, buttons) =
                format_button_row_with_positions(state, &colors, inner_width, 2, true, false);
            buttons
        }
        HudStyle::Minimal => {
            let colors = theme.colors();
            let (_, button) = format_minimal_strip_with_button(state, &colors, width);
            button.into_iter().collect()
        }
        HudStyle::Hidden => {
            if state.recording_state != RecordingState::Idle {
                return Vec::new();
            }
            let colors = theme.colors();
            let (_, button) = format_hidden_launcher_with_button(state, &colors, width);
            button.into_iter().collect()
        }
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
        RecordingState::Processing | RecordingState::Idle => {}
    }

    if let Some(panel) = minimal_right_panel(state, colors) {
        line.push(' ');
        line.push_str(colors.dim);
        line.push('·');
        line.push_str(colors.reset);
        line.push(' ');
        line.push_str(&panel);
    }

    if let Some(status) = minimal_status_text(state, colors) {
        if !status.is_empty() {
            line.push(' ');
            line.push_str(colors.dim);
            line.push('·');
            line.push_str(colors.reset);
            line.push(' ');
            line.push_str(&status);
        }
    }

    line
}

fn minimal_status_text(state: &StatusLineState, colors: &ThemeColors) -> Option<String> {
    if state.queue_depth > 0 {
        return Some(format!(
            "{}Queued {}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }

    if state.message.is_empty() {
        return if state.recording_state == RecordingState::Idle {
            Some(format!("{}Ready{}", colors.success, colors.reset))
        } else {
            None
        };
    }

    let status_type = StatusType::from_message(&state.message);
    if state.recording_state != RecordingState::Idle {
        return match status_type {
            StatusType::Warning => Some(format!(
                "{}{}{}",
                colors.warning, state.message, colors.reset
            )),
            StatusType::Error => Some(format!("{}{}{}", colors.error, state.message, colors.reset)),
            StatusType::Info => Some(format!("{}{}{}", colors.info, state.message, colors.reset)),
            // Recording/processing/success are already represented by the left state lane while active.
            StatusType::Recording | StatusType::Processing | StatusType::Success => None,
        };
    }

    let colored = match status_type {
        StatusType::Success => format!("{}Ready{}", colors.success, colors.reset),
        StatusType::Info => format!("{}{}{}", colors.info, state.message, colors.reset),
        StatusType::Warning => format!("{}{}{}", colors.warning, state.message, colors.reset),
        StatusType::Error => format!("{}{}{}", colors.error, state.message, colors.reset),
        StatusType::Recording | StatusType::Processing => format!(
            "{}{}{}",
            status_type.color(colors),
            state.message,
            colors.reset
        ),
    };
    Some(colored)
}

fn minimal_right_panel(state: &StatusLineState, colors: &ThemeColors) -> Option<String> {
    if state.hud_right_panel == HudRightPanel::Off {
        return None;
    }
    let recording_active = state.recording_state == RecordingState::Recording;
    let animate_panel = !state.hud_right_panel_recording_only || recording_active;

    let panel = match state.hud_right_panel {
        HudRightPanel::Ribbon => {
            let levels = if animate_panel {
                &state.meter_levels
            } else {
                &[][..]
            };
            let waveform = minimal_waveform(levels, 6, colors);
            format!(
                "{}[{}{}{}]{}",
                colors.dim, colors.reset, waveform, colors.dim, colors.reset
            )
        }
        HudRightPanel::Dots => {
            let level = if animate_panel {
                state.meter_db.unwrap_or(-60.0)
            } else {
                -60.0
            };
            minimal_pulse_dots(level, colors)
        }
        HudRightPanel::Heartbeat => {
            let animate =
                should_animate_heartbeat(state.hud_right_panel_recording_only, recording_active);
            let (glyph, is_peak) = heartbeat_glyph(animate);
            let color = if is_peak { colors.info } else { colors.dim };
            format!(
                "{}[{}{}{}{}]{}",
                colors.dim, colors.reset, color, glyph, colors.reset, colors.reset
            )
        }
        HudRightPanel::Off => return None,
    };
    Some(panel)
}

#[inline]
fn should_animate_heartbeat(recording_only: bool, recording_active: bool) -> bool {
    !recording_only || recording_active
}

fn minimal_waveform(levels: &[f32], width: usize, colors: &ThemeColors) -> String {
    const GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    if width == 0 {
        return String::new();
    }
    if levels.is_empty() {
        return format!("{}{}{}", colors.dim, "▁".repeat(width), colors.reset);
    }

    let mut out = String::with_capacity(width * 8);
    let start = levels.len().saturating_sub(width);
    let slice = &levels[start..];
    if slice.len() < width {
        out.push_str(colors.dim);
        out.push_str(&"▁".repeat(width - slice.len()));
        out.push_str(colors.reset);
    }
    for db in slice {
        let normalized = ((*db + 60.0) / 60.0).clamp(0.0, 1.0);
        let idx = (normalized * (GLYPHS.len() as f32 - 1.0)).round() as usize;
        let color = if normalized < 0.6 {
            colors.success
        } else if normalized < 0.85 {
            colors.warning
        } else {
            colors.error
        };
        out.push_str(color);
        out.push(GLYPHS[idx]);
        out.push_str(colors.reset);
    }
    out
}

fn minimal_pulse_dots(level_db: f32, colors: &ThemeColors) -> String {
    let normalized = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0);
    let active = (normalized * 5.0).round() as usize;
    let mut result = String::with_capacity(64);
    result.push_str(colors.dim);
    result.push('[');
    result.push_str(colors.reset);
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

pub(super) fn format_minimal_strip_with_button(
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

fn hidden_launcher_text(state: &StatusLineState, colors: &ThemeColors) -> String {
    let brand = format!(
        "{}Vox{}{}Term{}",
        colors.info, colors.reset, colors.recording, colors.reset
    );
    if state.message.is_empty() {
        return format!(
            "{brand} {}hidden{} {}·{} {}Ctrl+U{}",
            colors.dim, colors.reset, colors.dim, colors.reset, colors.dim, colors.reset
        );
    }
    let status_color = StatusType::from_message(&state.message).color(colors);
    let status = if status_color.is_empty() {
        state.message.clone()
    } else {
        format!("{}{}{}", status_color, state.message, colors.reset)
    };
    format!("{brand} {}·{} {status}", colors.dim, colors.reset)
}

pub(super) fn format_hidden_launcher_with_button(
    state: &StatusLineState,
    colors: &ThemeColors,
    width: usize,
) -> (String, Option<ButtonPosition>) {
    if width == 0 {
        return (String::new(), None);
    }

    let base = hidden_launcher_text(state, colors);
    let focused = state.hud_button_focus == Some(ButtonAction::ToggleHudStyle);
    let button = format_button(colors, "open", colors.info, focused);
    let button_width = display_width(&button);

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
            action: ButtonAction::ToggleHudStyle,
        };
        return (line, Some(button_pos));
    }

    let line = truncate_display(&base, width);
    (line, None)
}

/// Format the shortcuts row with dimmed styling and return button positions.
pub(super) fn format_shortcuts_row_with_positions(
    state: &StatusLineState,
    colors: &ThemeColors,
    borders: &BorderSet,
    inner_width: usize,
) -> (String, Vec<ButtonPosition>) {
    let row_width = inner_width.saturating_sub(1);
    // Row 2 from bottom of HUD (row 1 = bottom border)
    let (shortcuts_str, buttons) =
        format_button_row_with_positions(state, colors, row_width, 2, true, false);

    // Add leading space to match main row's left margin
    let interior = truncate_display(&format!(" {}", shortcuts_str), inner_width);
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
    show_latency_badge: bool,
    show_ready_badge: bool,
) -> (String, Vec<ButtonPosition>) {
    let button_defs = get_button_defs(state);
    let mut items = Vec::new();
    let mut positions = Vec::new();

    // Track current x position (1-based, after border + leading space = column 3)
    let mut current_x: u16 = 3; // border(1) + space(1) + first char at (3)
    let separator_visible_width = 3u16; // " · " = 3 visible chars

    for (idx, def) in button_defs.iter().enumerate() {
        if idx > 0 {
            current_x += separator_visible_width;
        }

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
        current_x += visible_width;
    }

    // Queue badge (not clickable)
    if state.queue_depth > 0 {
        items.push(format!(
            "{}Q:{}{}",
            colors.warning, state.queue_depth, colors.reset
        ));
    }

    // Ready badge (not clickable)
    let ready_badge = if show_ready_badge
        && state.recording_state == RecordingState::Idle
        && state.queue_depth == 0
    {
        Some(format!("{}Ready{}", colors.success, colors.reset))
    } else {
        None
    };
    let latency_badge = if show_latency_badge {
        state.last_latency_ms.and_then(|latency| {
            if state.latency_display == LatencyDisplayMode::Off {
                return None;
            }
            let latency_color = if latency < 300 {
                colors.success
            } else if latency < 500 {
                colors.warning
            } else {
                colors.error
            };
            let text = match state.latency_display {
                LatencyDisplayMode::Short => format!("{latency}ms"),
                LatencyDisplayMode::Label => format!("Latency: {latency}ms"),
                LatencyDisplayMode::Off => return None,
            };
            Some(format!("{latency_color}{text}{}", colors.reset))
        })
    } else {
        None
    };
    match (ready_badge, latency_badge) {
        (Some(ready), Some(latency)) => items.push(format!("{ready} {latency}")),
        (Some(ready), None) => items.push(ready),
        (None, Some(latency)) => items.push(latency),
        (None, None) => {}
    }

    // Use a plain separator to keep glyph rendering stable during rapid redraws.
    let separator = " · ".to_string();
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
        if i > 0 {
            current_x += 1; // space separator in compact mode
        }

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
    let (row, _) = format_button_row_with_positions(state, colors, inner_width, 2, true, false);
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

    // Use a plain separator to keep glyph rendering stable during rapid redraws.
    let separator = " · ".to_string();
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
pub(super) fn format_button(
    colors: &ThemeColors,
    label: &str,
    highlight: &str,
    focused: bool,
) -> String {
    // Keep one active color context through bracket+label to avoid transient
    // default-color flashes in terminals that repaint aggressively.
    let mut content = String::with_capacity(16 + label.len());
    if !highlight.is_empty() {
        content.push_str(highlight);
    }
    content.push_str(label);
    format_shortcut_pill(&content, colors, focused)
}

/// Format a button in clickable pill style with brackets.
/// Style: `[label]` with dim (or focused) brackets.
fn format_shortcut_pill(content: &str, colors: &ThemeColors, focused: bool) -> String {
    let bracket_color = if focused { colors.info } else { colors.dim };
    let mut result =
        String::with_capacity(content.len() + bracket_color.len() * 3 + colors.reset.len() + 2);
    result.push_str(bracket_color);
    result.push('[');
    result.push_str(content);
    result.push_str(bracket_color);
    result.push(']');
    result.push_str(colors.reset);
    result
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status_line::layout::breakpoints;

    fn count_substring(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }

    #[test]
    fn get_button_positions_hidden_idle_has_open_button() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Hidden;
        let positions = get_button_positions(&state, Theme::None, 80);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].row, 1);
        assert_eq!(positions[0].action, ButtonAction::ToggleHudStyle);
    }

    #[test]
    fn get_button_positions_full_has_buttons() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        let positions = get_button_positions(&state, Theme::None, 80);
        assert!(!positions.is_empty());
        assert_eq!(positions[0].row, 2);
    }

    #[test]
    fn get_button_positions_minimal_has_back_button() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Minimal;
        let positions = get_button_positions(&state, Theme::None, 40);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].row, 1);
        assert_eq!(positions[0].action, ButtonAction::HudBack);
    }

    #[test]
    fn hidden_launcher_text_contains_hint() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let line = hidden_launcher_text(&state, &colors);
        assert!(line.contains("Vox"));
        assert!(line.contains("Ctrl+U"));
    }

    #[test]
    fn button_defs_use_send_label_from_send_mode() {
        let mut state = StatusLineState::new();
        let defs = get_button_defs(&state);
        let send = defs
            .iter()
            .find(|def| def.action == ButtonAction::ToggleSendMode)
            .expect("send button");
        assert_eq!(send.label, "send");

        state.send_mode = VoiceSendMode::Insert;
        let defs = get_button_defs(&state);
        let send = defs
            .iter()
            .find(|def| def.action == ButtonAction::ToggleSendMode)
            .expect("send button");
        assert_eq!(send.label, "edit");
    }

    #[test]
    fn minimal_right_panel_respects_recording_only() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_right_panel = HudRightPanel::Dots;
        state.hud_right_panel_recording_only = true;
        state.recording_state = RecordingState::Idle;
        state.meter_db = Some(-12.0);
        let idle_panel = minimal_right_panel(&state, &colors).expect("idle panel");
        assert!(idle_panel.contains("·"));

        state.recording_state = RecordingState::Recording;
        assert!(minimal_right_panel(&state, &colors).is_some());
    }

    #[test]
    fn minimal_right_panel_ribbon_shows_waveform() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_right_panel = HudRightPanel::Ribbon;
        state.hud_right_panel_recording_only = false;
        state.recording_state = RecordingState::Recording;
        state
            .meter_levels
            .extend_from_slice(&[-55.0, -42.0, -30.0, -18.0]);
        let panel = minimal_right_panel(&state, &colors).expect("panel");
        assert!(panel.contains("▁") || panel.contains("▂") || panel.contains("▃"));
    }

    #[test]
    fn minimal_strip_text_includes_panel_when_enabled() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_right_panel = HudRightPanel::Dots;
        state.hud_right_panel_recording_only = false;
        state.recording_state = RecordingState::Recording;
        state.meter_db = Some(-8.0);
        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("•"));
    }

    #[test]
    fn minimal_strip_idle_success_collapses_to_ready() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.voice_mode = VoiceMode::Auto;
        state.message = "Transcript ready (Rust pipeline)".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Ready"));
        assert!(!line.contains("Transcript ready"));
    }

    #[test]
    fn minimal_strip_idle_shows_queue_state() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.queue_depth = 2;
        state.message = "Auto-voice enabled".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Queued 2"));
    }

    #[test]
    fn minimal_strip_idle_shows_info_message_text() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.message = "Edit mode: press Enter to send".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Edit mode: press Enter to send"));
    }

    #[test]
    fn minimal_strip_idle_shows_full_warning_message_text() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.message = "Auto-voice disabled (capture cancelled)".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Auto-voice disabled (capture cancelled)"));
    }

    #[test]
    fn minimal_strip_recording_shows_info_message_text() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Recording;
        state.meter_db = Some(-28.0);
        state.message = "Edit mode: press Enter to send".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Edit mode: press Enter to send"));
    }

    #[test]
    fn minimal_ribbon_waveform_uses_level_colors() {
        let colors = Theme::Coral.colors();
        let mut state = StatusLineState::new();
        state.hud_right_panel = HudRightPanel::Ribbon;
        state.hud_right_panel_recording_only = false;
        state.recording_state = RecordingState::Recording;
        state
            .meter_levels
            .extend_from_slice(&[-55.0, -45.0, -35.0, -20.0, -10.0, -5.0]);

        let panel = minimal_right_panel(&state, &colors).expect("panel");
        assert!(panel.contains(colors.success));
    }

    #[test]
    fn full_row_ready_and_latency_render_without_separator_dot_between_them() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(199);

        let row = format_button_row(&state, &colors, 120);
        assert!(!row.contains("Ready"));
        assert!(row.contains("199ms"));
    }

    #[test]
    fn full_row_latency_label_mode_shows_prefixed_text() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(300);
        state.latency_display = LatencyDisplayMode::Label;

        let row = format_button_row(&state, &colors, 120);
        assert!(row.contains("Latency: 300ms"));
    }

    #[test]
    fn full_row_latency_off_mode_hides_badge() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(199);
        state.latency_display = LatencyDisplayMode::Off;

        let row = format_button_row(&state, &colors, 120);
        assert!(!row.contains("199ms"));
    }

    #[test]
    fn shortcuts_row_stays_within_banner_width() {
        let colors = Theme::Coral.colors();
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(316);
        state.latency_display = LatencyDisplayMode::Short;

        let inner_width = 90;
        let (row, _) =
            format_shortcuts_row_with_positions(&state, &colors, &colors.borders, inner_width);

        // +2 accounts for left/right border columns in full HUD rows.
        assert!(
            display_width(&row) <= inner_width + 2,
            "shortcuts row should not exceed full HUD width"
        );
    }

    #[test]
    fn hidden_launcher_button_aligns_to_right_edge_when_space_available() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let open_button = format_button(&colors, "open", colors.info, false);
        let button_width = display_width(&open_button);
        let width = button_width + 18;

        let (line, button) = format_hidden_launcher_with_button(&state, &colors, width);
        let button = button.expect("open button should be present");

        assert_eq!(display_width(&line), width);
        assert_eq!(button.start_x, (width - button_width + 1) as u16);
        assert_eq!(button.end_x, width as u16);
        assert_eq!(button.row, 1);
        assert_eq!(button.action, ButtonAction::ToggleHudStyle);
    }

    #[test]
    fn hidden_launcher_hides_button_when_width_too_small() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let open_button = format_button(&colors, "open", colors.info, false);
        let width = display_width(&open_button) + 1;

        let (line, button) = format_hidden_launcher_with_button(&state, &colors, width);

        assert!(button.is_none());
        assert_eq!(display_width(&line), width);
    }

    #[test]
    fn minimal_strip_button_geometry_is_stable() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Recording;
        state.meter_db = Some(-12.0);

        let back_button = format_button(&colors, "back", colors.border, false);
        let button_width = display_width(&back_button);
        let width = button_width + 24;

        let (line, button) = format_minimal_strip_with_button(&state, &colors, width);
        let button = button.expect("back button should be present");

        assert_eq!(display_width(&line), width);
        assert_eq!(button.start_x, (width - button_width + 1) as u16);
        assert_eq!(button.end_x, width as u16);
        assert_eq!(button.row, 1);
        assert_eq!(button.action, ButtonAction::HudBack);
    }

    #[test]
    fn compact_button_row_omits_hud_button_and_recomputes_positions() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 1;
        state.last_latency_ms = Some(312);

        let (full_row, full_positions) =
            format_button_row_with_positions(&state, &colors, 300, 2, true, false);
        let compact_width = display_width(&full_row).saturating_sub(1);
        let (compact_row, compact_positions) =
            format_button_row_with_positions(&state, &colors, compact_width, 2, true, false);

        assert_eq!(full_positions.len(), 7);
        assert_eq!(compact_positions.len(), 6);
        assert!(display_width(&compact_row) <= compact_width);
        assert!(compact_positions
            .iter()
            .all(|pos| pos.action != ButtonAction::ToggleHudStyle));
        assert!(compact_row.contains("Q:1"));
    }

    #[test]
    fn button_row_ready_badge_requires_idle_and_empty_queue() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.last_latency_ms = Some(250);

        let (idle_row, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, true);
        assert!(idle_row.contains("Ready"));
        assert!(idle_row.contains("250ms"));

        state.queue_depth = 1;
        let (queued_row, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, true);
        assert!(!queued_row.contains("Ready"));
        assert!(queued_row.contains("Q:1"));
    }

    #[test]
    fn legacy_button_row_compact_mode_drops_hud_and_theme_entries() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 2;
        state.last_latency_ms = Some(420);

        let full = format_button_row_legacy(&state, &colors, 240);
        let narrow_width = display_width(&full).saturating_sub(1);
        let compact = format_button_row_legacy(&state, &colors, narrow_width);

        assert!(display_width(&compact) <= narrow_width);
        assert!(!compact.contains("hud"));
        assert!(!compact.contains("theme"));
        assert!(compact.contains("help"));
    }

    #[test]
    fn shortcuts_row_legacy_matches_positioned_renderer_when_untruncated() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 1;
        state.last_latency_ms = Some(345);
        state.latency_display = LatencyDisplayMode::Label;

        let inner_width = 200;
        let (positioned, buttons) =
            format_shortcuts_row_with_positions(&state, &colors, &colors.borders, inner_width);
        let legacy = format_shortcuts_row_legacy(&state, &colors, &colors.borders, inner_width);

        assert_eq!(positioned, legacy);
        assert_eq!(display_width(&positioned), inner_width + 2);
        assert!(!buttons.is_empty());
        assert!(buttons.iter().all(|button| button.row == 2));
    }

    #[test]
    fn full_hud_button_positions_match_expected_geometry() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let (_, positions) = format_button_row_with_positions(&state, &colors, 300, 2, true, false);
        let expected = [
            (ButtonAction::VoiceTrigger, 3, 7),
            (ButtonAction::ToggleAutoVoice, 11, 15),
            (ButtonAction::ToggleSendMode, 19, 24),
            (ButtonAction::SettingsToggle, 28, 32),
            (ButtonAction::ToggleHudStyle, 36, 40),
            (ButtonAction::HelpToggle, 44, 49),
            (ButtonAction::ThemePicker, 53, 59),
        ];

        assert_eq!(positions.len(), expected.len());
        for (pos, (action, start, end)) in positions.iter().zip(expected) {
            assert_eq!(pos.action, action);
            assert_eq!(pos.start_x, start);
            assert_eq!(pos.end_x, end);
            assert_eq!(pos.row, 2);
        }
    }

    #[test]
    fn compact_hud_button_positions_match_expected_geometry() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let (_, positions) = format_button_row_with_positions(&state, &colors, 20, 2, true, false);
        let expected = [
            (ButtonAction::VoiceTrigger, 3, 7),
            (ButtonAction::ToggleAutoVoice, 9, 13),
            (ButtonAction::ToggleSendMode, 15, 20),
            (ButtonAction::SettingsToggle, 22, 26),
            (ButtonAction::HelpToggle, 28, 33),
            (ButtonAction::ThemePicker, 35, 41),
        ];

        assert_eq!(positions.len(), expected.len());
        for (pos, (action, start, end)) in positions.iter().zip(expected) {
            assert_eq!(pos.action, action);
            assert_eq!(pos.start_x, start);
            assert_eq!(pos.end_x, end);
            assert_eq!(pos.row, 2);
        }
    }

    #[test]
    fn get_button_positions_full_hud_respects_breakpoint_boundary() {
        let mut state = StatusLineState::new();
        state.hud_style = HudStyle::Full;

        let below = get_button_positions(&state, Theme::None, breakpoints::COMPACT - 1);
        let at = get_button_positions(&state, Theme::None, breakpoints::COMPACT);

        assert!(below.is_empty());
        assert!(!at.is_empty());
    }

    #[test]
    fn minimal_status_text_shows_processing_message_when_idle() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;
        state.message = "Processing...".to_string();

        let line = minimal_strip_text(&state, &colors);
        assert!(line.contains("Processing..."));
    }

    #[test]
    fn hidden_launcher_boundary_width_shows_button_at_exact_threshold() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let open_button = format_button(&colors, "open", colors.info, false);
        let width = display_width(&open_button) + 2;

        let (_, button) = format_hidden_launcher_with_button(&state, &colors, width);
        assert!(button.is_some());
    }

    #[test]
    fn focused_buttons_use_info_brackets() {
        let colors = Theme::Coral.colors();

        let mut hidden_state = StatusLineState::new();
        hidden_state.hud_button_focus = Some(ButtonAction::ToggleHudStyle);
        let (hidden_line, hidden_button) =
            format_hidden_launcher_with_button(&hidden_state, &colors, 80);
        assert!(hidden_button.is_some());
        assert!(hidden_line.contains(&format!("{}[", colors.info)));

        let mut minimal_state = StatusLineState::new();
        minimal_state.hud_button_focus = Some(ButtonAction::HudBack);
        let (minimal_line, minimal_button) =
            format_minimal_strip_with_button(&minimal_state, &colors, 80);
        assert!(minimal_button.is_some());
        assert!(minimal_line.contains(&format!("{}[", colors.info)));
    }

    #[test]
    fn shortcut_pill_does_not_reset_immediately_after_open_bracket() {
        let colors = Theme::Coral.colors();
        let pill = format_button(&colors, "rec", colors.recording, false);
        assert!(
            !pill.contains("[\u{1b}[0m"),
            "pill should keep active color context through bracket and label"
        );
    }

    #[test]
    fn minimal_waveform_handles_padding_and_boundaries() {
        let none = Theme::None.colors();
        assert_eq!(minimal_waveform(&[-30.0], 3, &none), "▁▁▅");
        assert_eq!(minimal_waveform(&[-30.0, -30.0, -30.0], 3, &none), "▅▅▅");

        let colors = Theme::Coral.colors();
        let waveform = minimal_waveform(&[-24.0, -9.0], 2, &colors);
        let expected = format!(
            "{}▅{}{}▇{}",
            colors.warning, colors.reset, colors.error, colors.reset
        );
        assert_eq!(waveform, expected);
    }

    #[test]
    fn minimal_pulse_dots_respect_activity_and_color_thresholds() {
        let none = Theme::None.colors();
        assert_eq!(minimal_pulse_dots(-60.0, &none), "[·····]");
        assert_eq!(minimal_pulse_dots(-48.0, &none), "[•····]");
        assert_eq!(minimal_pulse_dots(-30.0, &none), "[•••··]");
        assert_eq!(minimal_pulse_dots(0.0, &none), "[•••••]");

        let colors = Theme::Coral.colors();
        let warning = minimal_pulse_dots(-24.0, &colors);
        assert!(warning.contains(&format!("{}•{}", colors.warning, colors.reset)));
        assert!(!warning.contains(&format!("{}•{}", colors.success, colors.reset)));

        let error = minimal_pulse_dots(-9.0, &colors);
        assert!(error.contains(&format!("{}•{}", colors.error, colors.reset)));
    }

    #[test]
    fn latency_threshold_colors_are_correct_in_full_and_legacy_rows() {
        let colors = Theme::Coral.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Idle;

        state.last_latency_ms = Some(300);
        let (row_300, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        assert!(row_300.contains(&format!("{}300ms{}", colors.warning, colors.reset)));

        state.last_latency_ms = Some(500);
        let (row_500, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        assert!(row_500.contains(&format!("{}500ms{}", colors.error, colors.reset)));

        state.last_latency_ms = Some(300);
        let legacy_300 = format_button_row_legacy(&state, &colors, 200);
        assert!(legacy_300.contains(&format!("{}300ms{}", colors.warning, colors.reset)));

        state.last_latency_ms = Some(500);
        let legacy_500 = format_button_row_legacy(&state, &colors, 200);
        assert!(legacy_500.contains(&format!("{}500ms{}", colors.error, colors.reset)));
    }

    #[test]
    fn wrappers_and_legacy_helpers_emit_structured_shortcut_text() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();

        let shortcuts = format_shortcuts_row(&state, &colors, &colors.borders, 120);
        assert!(!shortcuts.is_empty());
        assert!(shortcuts.contains("rec"));

        let shortcut = format_shortcut_colored(&colors, "u", "help", colors.info);
        assert!(shortcut.contains("u"));
        assert!(shortcut.contains("help"));
        assert!(shortcut.contains("["));
    }

    #[test]
    fn minimal_status_text_non_idle_empty_message_is_none() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.recording_state = RecordingState::Processing;

        assert!(minimal_status_text(&state, &colors).is_none());
    }

    #[test]
    fn minimal_right_panel_dots_without_meter_defaults_to_silent_level() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.hud_right_panel = HudRightPanel::Dots;
        state.hud_right_panel_recording_only = false;
        state.recording_state = RecordingState::Idle;
        state.meter_db = None;

        let panel = minimal_right_panel(&state, &colors).expect("dots panel");
        assert_eq!(panel, "[·····]");
    }

    #[test]
    fn heartbeat_animation_truth_table() {
        assert!(should_animate_heartbeat(false, false));
        assert!(should_animate_heartbeat(false, true));
        assert!(!should_animate_heartbeat(true, false));
        assert!(should_animate_heartbeat(true, true));
    }

    #[test]
    fn minimal_strip_shows_button_at_exact_width_threshold() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let back_button = format_button(&colors, "back", colors.border, false);
        let button_width = display_width(&back_button);
        let width = button_width + 2;

        let (_, button) = format_minimal_strip_with_button(&state, &colors, width);
        assert!(button.is_some());
    }

    #[test]
    fn minimal_strip_hides_button_just_below_width_threshold() {
        let colors = Theme::None.colors();
        let state = StatusLineState::new();
        let back_button = format_button(&colors, "back", colors.border, false);
        let button_width = display_width(&back_button);
        let width = button_width + 1;

        let (_, button) = format_minimal_strip_with_button(&state, &colors, width);
        assert!(button.is_none());
    }

    #[test]
    fn full_row_focus_marks_exactly_one_button_bracket() {
        let colors = Theme::Coral.colors();
        let mut state = StatusLineState::new();
        state.hud_button_focus = Some(ButtonAction::ToggleSendMode);
        let (row, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        let focused_bracket = format!("{}[", colors.info);
        assert_eq!(count_substring(&row, &focused_bracket), 1);
    }

    #[test]
    fn compact_row_focus_marks_exactly_one_button_bracket() {
        let colors = Theme::Coral.colors();
        let mut state = StatusLineState::new();
        state.hud_button_focus = Some(ButtonAction::HelpToggle);
        let (full_row, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        // Force compact path while still leaving room for the full compact row.
        let compact_width = display_width(&full_row).saturating_sub(1);
        let (row, positions) =
            format_button_row_with_positions(&state, &colors, compact_width, 2, true, false);
        assert_eq!(positions.len(), 6);
        let focused_bracket = format!("{}[", colors.info);
        assert_eq!(count_substring(&row, &focused_bracket), 1);
    }

    #[test]
    fn queue_badge_zero_is_not_rendered_in_any_row_mode() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 0;

        let (full, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        assert!(!full.contains("Q:0"));

        let (compact, _) = format_button_row_with_positions(&state, &colors, 20, 2, true, false);
        assert!(!compact.contains("Q:0"));

        let legacy_full = format_button_row_legacy(&state, &colors, 200);
        assert!(!legacy_full.contains("Q:0"));

        let legacy_compact = format_button_row_legacy(&state, &colors, breakpoints::COMPACT);
        assert!(!legacy_compact.contains("Q:0"));
    }

    #[test]
    fn compact_row_queue_zero_not_rendered_when_untruncated() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 0;

        let (full_row, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        let compact_width = display_width(&full_row).saturating_sub(1);
        let (compact_row, compact_positions) =
            format_button_row_with_positions(&state, &colors, compact_width, 2, true, false);
        assert_eq!(compact_positions.len(), 6);
        assert!(!compact_row.contains("Q:0"));
    }

    #[test]
    fn queue_badge_positive_renders_in_full_and_legacy_rows() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 1;

        let (full, _) = format_button_row_with_positions(&state, &colors, 200, 2, true, false);
        assert!(full.contains("Q:1"));

        let legacy = format_button_row_legacy(&state, &colors, 200);
        assert!(legacy.contains("Q:1"));
    }

    #[test]
    fn legacy_compact_row_queue_positive_renders_when_untruncated() {
        let colors = Theme::None.colors();
        let mut state = StatusLineState::new();
        state.queue_depth = 1;

        let full_row = format_button_row_legacy(&state, &colors, 200);
        let compact_width = display_width(&full_row).saturating_sub(1);
        let compact_row = format_button_row_legacy(&state, &colors, compact_width);
        assert!(compact_row.contains("Q:1"));
    }

    #[test]
    fn format_button_includes_non_empty_highlight_color() {
        let colors = Theme::Coral.colors();

        let highlighted = format_button(&colors, "send", colors.success, false);
        let plain = format_button(&colors, "send", "", false);

        assert!(highlighted.contains(colors.success));
        assert!(!plain.contains(colors.success));
    }
}
