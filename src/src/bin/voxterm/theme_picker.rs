//! Theme picker overlay that lets users switch palettes without restarting sessions.
//!
//! Displays available themes and allows selecting by number.
//! Now shows a visual preview of each theme's unique style.

use crate::theme::{Theme, ThemeColors};

/// Theme options with labels and descriptions.
pub const THEME_OPTIONS: &[(Theme, &str, &str)] = &[
    (Theme::ChatGpt, "chatgpt", "Emerald green (ChatGPT)"),
    (Theme::Claude, "claude", "Warm neutrals (Anthropic)"),
    (Theme::Codex, "codex", "Cool blue (Codex-style)"),
    (Theme::Coral, "coral", "Default red accents"),
    (Theme::Catppuccin, "catppuccin", "Pastel elegance"),
    (Theme::Dracula, "dracula", "Bold high contrast"),
    (Theme::Nord, "nord", "Rounded arctic blue"),
    (Theme::TokyoNight, "tokyonight", "Elegant purple/blue"),
    (Theme::Gruvbox, "gruvbox", "Warm retro earthy"),
    (Theme::Ansi, "ansi", "16-color compatible"),
    (Theme::None, "none", "No color styling"),
];

pub const THEME_PICKER_FOOTER: &str = "[×] close · ↑/↓ move · Enter select";
pub const THEME_PICKER_OPTION_START_ROW: usize = 4;

pub fn theme_picker_inner_width_for_terminal(width: usize) -> usize {
    width.clamp(40, 60)
}

pub fn theme_picker_total_width_for_terminal(width: usize) -> usize {
    theme_picker_inner_width_for_terminal(width).saturating_add(2)
}

pub fn format_theme_picker(current_theme: Theme, selected_idx: usize, width: usize) -> String {
    let colors = current_theme.colors();
    let borders = &colors.borders;
    let mut lines = Vec::new();
    // Inner width is what goes between the left and right border characters
    // All rows must have exactly this many visible characters between borders
    let inner_width = theme_picker_inner_width_for_terminal(width);

    // Top border: corner + inner_width horizontal + corner
    let top_inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    lines.push(format!(
        "{}{}{}{}{}",
        colors.border, borders.top_left, top_inner, borders.top_right, colors.reset
    ));

    // Title row
    lines.push(format_title_line(
        &colors,
        borders,
        "VoxTerm - Themes",
        inner_width,
    ));

    // Separator
    let sep_inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    lines.push(format!(
        "{}{}{}{}{}",
        colors.border, borders.t_left, sep_inner, borders.t_right, colors.reset
    ));

    // Theme options with visual preview
    for (idx, (theme, name, desc)) in THEME_OPTIONS.iter().enumerate() {
        let theme_colors = theme.colors();
        let is_current = *theme == current_theme;
        let is_selected = idx == selected_idx;
        let marker = if is_selected {
            ">"
        } else if is_current {
            "*"
        } else {
            " "
        };
        lines.push(format_option_line_with_preview(
            &colors,
            borders,
            &theme_colors,
            idx + 1,
            name,
            desc,
            marker,
            inner_width,
        ));
    }

    // Bottom separator
    lines.push(format!(
        "{}{}{}{}{}",
        colors.border, borders.t_left, sep_inner, borders.t_right, colors.reset
    ));

    // Footer with clickable close button
    lines.push(format_title_line(
        &colors,
        borders,
        THEME_PICKER_FOOTER,
        inner_width,
    ));

    // Bottom border
    let bottom_inner: String = std::iter::repeat_n(borders.horizontal, inner_width).collect();
    lines.push(format!(
        "{}{}{}{}{}",
        colors.border, borders.bottom_left, bottom_inner, borders.bottom_right, colors.reset
    ));

    lines.join("\n")
}

use crate::theme::BorderSet;

fn format_title_line(
    colors: &ThemeColors,
    borders: &BorderSet,
    title: &str,
    inner_width: usize,
) -> String {
    // Content between borders must be exactly inner_width characters
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

#[allow(clippy::too_many_arguments)]
fn format_option_line_with_preview(
    colors: &ThemeColors,
    borders: &BorderSet,
    theme_colors: &ThemeColors,
    num: usize,
    name: &str,
    desc: &str,
    marker: &str,
    inner_width: usize,
) -> String {
    // Label: "1. coral"
    let label = format!("{}. {}", num, name);
    let label_col = 14;
    let label_padded = format!("{:<width$}", label, width = label_col);

    // Calculate remaining space for description
    // Layout: indicator(1) + space(1) + marker(1) + space(1) + label(14) + space(1) = 19 fixed
    let fixed_visible = 19;
    let desc_col = inner_width.saturating_sub(fixed_visible);
    let desc_truncated: String = desc.chars().take(desc_col).collect();
    let desc_padded = format!("{:<width$}", desc_truncated, width = desc_col);

    // Build the row: exactly inner_width visible characters between borders
    // "{indicator} {marker} {label} {desc}" = 1+1+1+1+14+1+desc_col = 19+desc_col = inner_width
    format!(
        "{}{}{}{}{}{} {} {} {}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        theme_colors.recording,
        theme_colors.indicator_rec,
        colors.reset,
        marker,
        label_padded,
        desc_padded,
        colors.border,
        borders.vertical,
        colors.reset
    )
}

pub fn theme_picker_height() -> usize {
    // Top border + title + separator + options + separator + footer + bottom border
    1 + 1 + 1 + THEME_OPTIONS.len() + 1 + 1 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_picker_contains_options() {
        let output = format_theme_picker(Theme::Coral, 0, 60);
        assert!(output.contains("1. chatgpt"));
        assert!(output.contains("11. none")); // 11 themes total now
    }

    #[test]
    fn theme_picker_has_borders() {
        let output = format_theme_picker(Theme::Coral, 0, 60);
        // Uses theme-specific borders
        let colors = Theme::Coral.colors();
        assert!(output.contains(colors.borders.top_left));
        assert!(output.contains(colors.borders.bottom_left));
        assert!(output.contains(colors.borders.vertical));
    }

    #[test]
    fn theme_picker_shows_current_theme() {
        let output = format_theme_picker(Theme::Dracula, 5, 60);
        // Should have marker for current theme (Dracula = option 6)
        assert!(output.contains(">"));
        assert!(output.contains("6. dracula"));
    }

    #[test]
    fn theme_picker_height_positive() {
        assert!(theme_picker_height() > 5);
    }
}
