//! Settings overlay with arrow-key navigation.

use crate::config::VoiceSendMode;
use crate::status_line::Pipeline;
use crate::theme::{Theme, ThemeColors};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    AutoVoice,
    SendMode,
    Sensitivity,
    Theme,
    Backend,
    Pipeline,
    Close,
    Quit,
}

pub const SETTINGS_ITEMS: &[SettingsItem] = &[
    SettingsItem::AutoVoice,
    SettingsItem::SendMode,
    SettingsItem::Sensitivity,
    SettingsItem::Theme,
    SettingsItem::Backend,
    SettingsItem::Pipeline,
    SettingsItem::Close,
    SettingsItem::Quit,
];

#[derive(Debug, Clone)]
pub struct SettingsMenuState {
    pub selected: usize,
}

impl SettingsMenuState {
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    pub fn selected_item(&self) -> SettingsItem {
        SETTINGS_ITEMS
            .get(self.selected)
            .copied()
            .unwrap_or(SettingsItem::AutoVoice)
    }

    pub fn move_up(&mut self) {
        if self.selected == 0 {
            self.selected = SETTINGS_ITEMS.len().saturating_sub(1);
        } else {
            self.selected = self.selected.saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        if SETTINGS_ITEMS.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % SETTINGS_ITEMS.len();
    }
}

pub struct SettingsView<'a> {
    pub selected: usize,
    pub auto_voice_enabled: bool,
    pub send_mode: VoiceSendMode,
    pub sensitivity_db: f32,
    pub theme: Theme,
    pub backend_label: &'a str,
    pub pipeline: Pipeline,
}

pub fn settings_overlay_height() -> usize {
    // Top border + title + separator + items + separator + footer + bottom border
    SETTINGS_ITEMS.len() + 6
}

pub fn format_settings_overlay(view: &SettingsView<'_>, width: usize) -> String {
    let colors = view.theme.colors();
    let mut lines = Vec::new();
    let content_width = width.saturating_sub(4).clamp(24, 70);

    lines.push(format_box_top(&colors, content_width));
    lines.push(format_title_line(
        &colors,
        "VoxTerm Settings",
        content_width,
    ));
    lines.push(format_separator(&colors, content_width));

    for (idx, item) in SETTINGS_ITEMS.iter().enumerate() {
        let selected = idx == view.selected;
        let line = format_settings_row(view, *item, selected, &colors, content_width);
        lines.push(line);
    }

    lines.push(format_separator(&colors, content_width));
    // Use simpler ASCII footer to avoid Unicode width issues
    lines.push(format_title_line(
        &colors,
        "arrows move/adjust  Enter select  Esc close",
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
        SettingsItem::Quit => format!("{marker} {}", button_label("Quit VoxTerm")),
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
        VoiceSendMode::Insert => button_label("Insert"),
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
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.top_left, inner, borders.top_right, colors.reset
    )
}

fn format_box_bottom(colors: &ThemeColors, width: usize) -> String {
    let borders = &colors.borders;
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.bottom_left, inner, borders.bottom_right, colors.reset
    )
}

fn format_separator(colors: &ThemeColors, width: usize) -> String {
    let borders = &colors.borders;
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
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
