//! Settings panel rendering so menu state maps to stable terminal output.

use crate::config::{HudBorderStyle, HudRightPanel, HudStyle, LatencyDisplayMode, VoiceSendMode};
use crate::status_line::Pipeline;
use crate::theme::ThemeColors;

use super::items::{
    settings_overlay_width_for_terminal, SettingsItem, SettingsView, SETTINGS_ITEMS,
    SETTINGS_OVERLAY_FOOTER,
};

pub fn format_settings_overlay(view: &SettingsView<'_>, width: usize) -> String {
    let colors = view.theme.colors();
    let mut lines = Vec::new();
    let content_width = settings_overlay_width_for_terminal(width);

    lines.push(format_box_top(&colors, content_width));
    lines.push(format_title_line(
        &colors,
        "VoiceTerm Settings",
        content_width,
    ));
    lines.push(format_separator(&colors, content_width));

    for (idx, item) in SETTINGS_ITEMS.iter().enumerate() {
        let selected = idx == view.selected;
        let line = format_settings_row(view, *item, selected, &colors, content_width);
        lines.push(line);
    }

    lines.push(format_separator(&colors, content_width));
    // Footer with clickable close button
    lines.push(format_title_line(
        &colors,
        SETTINGS_OVERLAY_FOOTER,
        content_width,
    ));
    lines.push(format_box_bottom(&colors, content_width));

    lines.join("\n")
}

fn format_settings_row(
    view: &SettingsView<'_>,
    item: SettingsItem,
    selected: bool,
    colors: &ThemeColors,
    width: usize,
) -> String {
    const LABEL_WIDTH: usize = 12;
    let marker = if selected { "▸" } else { " " };

    let row_text = match item {
        SettingsItem::AutoVoice => format!(
            "{marker} {:<width$} {}",
            "Auto-voice",
            toggle_button(view.auto_voice_enabled),
            width = LABEL_WIDTH
        ),
        SettingsItem::SendMode => format!(
            "{marker} {:<width$} {}",
            "Send mode",
            mode_button(view.send_mode),
            width = LABEL_WIDTH
        ),
        SettingsItem::Macros => format!(
            "{marker} {:<width$} {}",
            "Macros",
            toggle_button(view.macros_enabled),
            width = LABEL_WIDTH
        ),
        SettingsItem::Sensitivity => {
            let slider = format_slider(view.sensitivity_db, 14);
            format!(
                "{marker} {:<width$} {slider} {:>4.0} dB",
                "Sensitivity",
                view.sensitivity_db,
                width = LABEL_WIDTH
            )
        }
        SettingsItem::Theme => format!(
            "{marker} {:<width$} {}",
            "Theme",
            button_label(&view.theme.to_string()),
            width = LABEL_WIDTH
        ),
        SettingsItem::HudStyle => format!(
            "{marker} {:<width$} {}",
            "HUD style",
            hud_style_button(view.hud_style),
            width = LABEL_WIDTH
        ),
        SettingsItem::HudBorders => format!(
            "{marker} {:<width$} {}",
            "Borders",
            hud_border_style_button(view.hud_border_style),
            width = LABEL_WIDTH
        ),
        SettingsItem::HudPanel => format!(
            "{marker} {:<width$} {}",
            "Right panel",
            hud_panel_button(view.hud_right_panel),
            width = LABEL_WIDTH
        ),
        SettingsItem::HudAnimate => format!(
            "{marker} {:<width$} {}",
            "Anim only",
            toggle_button(view.hud_right_panel_recording_only),
            width = LABEL_WIDTH
        ),
        SettingsItem::Latency => format!(
            "{marker} {:<width$} {}",
            "Latency",
            latency_mode_button(view.latency_display),
            width = LABEL_WIDTH
        ),
        SettingsItem::Mouse => format!(
            "{marker} {:<width$} {}",
            "Mouse",
            toggle_button(view.mouse_enabled),
            width = LABEL_WIDTH
        ),
        SettingsItem::Backend => format!(
            "{marker} {:<width$} {}",
            "Backend",
            view.backend_label,
            width = LABEL_WIDTH
        ),
        SettingsItem::Pipeline => format!(
            "{marker} {:<width$} {}",
            "Pipeline",
            pipeline_label(view.pipeline),
            width = LABEL_WIDTH
        ),
        SettingsItem::Close => format!("{marker} {}", button_label("Close")),
        SettingsItem::Quit => format!("{marker} {}", button_label("Quit VoiceTerm")),
    };

    format_menu_row(colors, width, &row_text, selected)
}

fn pipeline_label(pipeline: Pipeline) -> &'static str {
    match pipeline {
        Pipeline::Rust => "Rust",
        Pipeline::Python => "Python",
    }
}

fn toggle_button(enabled: bool) -> String {
    if enabled {
        button_label("ON")
    } else {
        button_label("OFF")
    }
}

fn mode_button(mode: VoiceSendMode) -> String {
    match mode {
        VoiceSendMode::Auto => button_label("Auto"),
        VoiceSendMode::Insert => button_label("Edit"),
    }
}

fn hud_panel_button(panel: HudRightPanel) -> String {
    match panel {
        HudRightPanel::Off => button_label("Off"),
        HudRightPanel::Ribbon => button_label("Ribbon"),
        HudRightPanel::Dots => button_label("Dots"),
        HudRightPanel::Heartbeat => button_label("Heartbeat"),
    }
}

fn hud_style_button(style: HudStyle) -> String {
    match style {
        HudStyle::Full => button_label("Full"),
        HudStyle::Minimal => button_label("Minimal"),
        HudStyle::Hidden => button_label("Hidden"),
    }
}

fn hud_border_style_button(style: HudBorderStyle) -> String {
    match style {
        HudBorderStyle::Theme => button_label("Theme"),
        HudBorderStyle::Single => button_label("Single"),
        HudBorderStyle::Rounded => button_label("Rounded"),
        HudBorderStyle::Double => button_label("Double"),
        HudBorderStyle::Heavy => button_label("Heavy"),
        HudBorderStyle::None => button_label("None"),
    }
}

fn latency_mode_button(mode: LatencyDisplayMode) -> String {
    match mode {
        LatencyDisplayMode::Off => button_label("Off"),
        LatencyDisplayMode::Short => button_label("Nms"),
        LatencyDisplayMode::Label => button_label("Latency: Nms"),
    }
}

fn button_label(label: &str) -> String {
    format!("[ {label} ]")
}

fn format_slider(value_db: f32, width: usize) -> String {
    let min_db = -80.0;
    let max_db = -10.0;
    let clamped = value_db.clamp(min_db, max_db);
    let ratio = if (max_db - min_db).abs() < f32::EPSILON {
        0.0
    } else {
        (clamped - min_db) / (max_db - min_db)
    };
    let pos = ((width.saturating_sub(1)) as f32 * ratio).round() as usize;
    let mut bar = String::with_capacity(width);
    for idx in 0..width {
        if idx == pos {
            bar.push('●');
        } else {
            bar.push('─');
        }
    }
    bar
}

fn format_box_top(colors: &ThemeColors, width: usize) -> String {
    let borders = &colors.borders;
    // width is the total box width including corners
    // Inner horizontal chars = width - 2 (for the two corners)
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.top_left, inner, borders.top_right, colors.reset
    )
}

fn format_box_bottom(colors: &ThemeColors, width: usize) -> String {
    let borders = &colors.borders;
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.bottom_left, inner, borders.bottom_right, colors.reset
    )
}

fn format_separator(colors: &ThemeColors, width: usize) -> String {
    let borders = &colors.borders;
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.t_left, inner, borders.t_right, colors.reset
    )
}

fn format_title_line(colors: &ThemeColors, title: &str, width: usize) -> String {
    let borders = &colors.borders;
    // Content width between vertical borders
    let inner_width = width.saturating_sub(2);
    // Use character count, not byte length, for proper Unicode support
    let title_display_len = title.chars().count();
    let padding = inner_width.saturating_sub(title_display_len);
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;
    format!(
        "{}{}{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        " ".repeat(left_pad),
        title,
        " ".repeat(right_pad),
        colors.border,
        borders.vertical,
        colors.reset
    )
}

fn format_menu_row(colors: &ThemeColors, width: usize, text: &str, selected: bool) -> String {
    let borders = &colors.borders;
    // Content width between vertical borders
    let inner_width = width.saturating_sub(2);
    let truncated: String = text.chars().take(inner_width).collect();
    let padded = format!("{:<width$}", truncated, width = inner_width);
    // Use foreground color highlight for selected items (no background)
    let styled = if selected {
        format!("{}{}{}", colors.info, padded, colors.reset)
    } else {
        padded
    };

    format!(
        "{}{}{}{}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        styled,
        colors.border,
        borders.vertical,
        colors.reset
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        HudBorderStyle, HudRightPanel, HudStyle, LatencyDisplayMode, VoiceSendMode,
    };
    use crate::settings::settings_overlay_height;
    use crate::status_line::Pipeline;
    use crate::theme::Theme;

    #[test]
    fn settings_overlay_height_matches_items() {
        let height = settings_overlay_height();
        assert_eq!(height, SETTINGS_ITEMS.len() + 6);
    }

    #[test]
    fn settings_overlay_uses_edit_label_for_insert_send_mode() {
        let view = SettingsView {
            selected: 1,
            auto_voice_enabled: false,
            send_mode: VoiceSendMode::Insert,
            macros_enabled: true,
            sensitivity_db: -35.0,
            theme: Theme::Coral,
            hud_style: HudStyle::Full,
            hud_border_style: HudBorderStyle::Theme,
            hud_right_panel: HudRightPanel::Off,
            hud_right_panel_recording_only: false,
            latency_display: LatencyDisplayMode::Short,
            mouse_enabled: true,
            backend_label: "codex",
            pipeline: Pipeline::Rust,
        };
        let rendered = format_settings_overlay(&view, 90);
        assert!(rendered.contains("Send mode"));
        assert!(rendered.contains("[ Edit ]"));
        assert!(!rendered.contains("[ Insert ]"));
    }
}
