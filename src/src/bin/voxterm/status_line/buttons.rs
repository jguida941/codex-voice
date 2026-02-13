//! Status-line button layout and hitbox logic so keyboard/mouse actions map reliably.

use crate::buttons::ButtonAction;
use crate::config::{HudStyle, VoiceSendMode};
use crate::status_style::StatusType;
use crate::theme::{BorderSet, Theme, ThemeColors};

use super::animation::{get_processing_spinner, get_recording_indicator};
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
            let (_, buttons) = format_button_row_with_positions(state, &colors, inner_width, 2);
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
    let send_label = if state.review_before_send {
        "review"
    } else {
        match state.send_mode {
            VoiceSendMode::Auto => "send",
            VoiceSendMode::Insert => "edit",
        }
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
                VoiceSendMode::Auto => {
                    if state.review_before_send {
                        colors.warning
                    } else {
                        colors.success
                    }
                }
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
                VoiceSendMode::Auto => {
                    if state.review_before_send {
                        colors.warning
                    } else {
                        colors.success
                    }
                }
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
    let (send_label, send_color) = if state.review_before_send {
        ("review", colors.warning)
    } else {
        match state.send_mode {
            VoiceSendMode::Auto => ("send", colors.success), // green = auto-send
            VoiceSendMode::Insert => ("edit", colors.warning), // yellow = insert/edit mode
        }
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
pub(super) fn format_button(
    colors: &ThemeColors,
    label: &str,
    highlight: &str,
    focused: bool,
) -> String {
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
    fn button_defs_use_review_label_when_enabled() {
        let mut state = StatusLineState::new();
        state.review_before_send = true;
        let defs = get_button_defs(&state);
        let send = defs
            .iter()
            .find(|def| def.action == ButtonAction::ToggleSendMode)
            .expect("send button");
        assert_eq!(send.label, "review");
    }
}
