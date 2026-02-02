//! Help overlay for keyboard shortcuts.
//!
//! Displays available keyboard shortcuts in a formatted panel.

use crate::theme::{Theme, ThemeColors};

/// Keyboard shortcut definition.
pub struct Shortcut {
    /// Key combination (e.g., "Ctrl+R")
    pub key: &'static str,
    /// Description of what it does
    pub description: &'static str,
}

/// All available keyboard shortcuts.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        key: "?",
        description: "Show help",
    },
    Shortcut {
        key: "Ctrl+R",
        description: "Start voice capture",
    },
    Shortcut {
        key: "Ctrl+V",
        description: "Toggle auto-voice mode",
    },
    Shortcut {
        key: "Ctrl+T",
        description: "Toggle send mode (auto/insert)",
    },
    Shortcut {
        key: "Ctrl+Y",
        description: "Theme picker",
    },
    Shortcut {
        key: "Ctrl+O",
        description: "Settings menu (use arrows)",
    },
    Shortcut {
        key: "Ctrl+]",
        description: "Increase mic sensitivity",
    },
    Shortcut {
        key: "Ctrl+\\",
        description: "Decrease mic sensitivity",
    },
    Shortcut {
        key: "Ctrl+Q",
        description: "Exit VoxTerm",
    },
    Shortcut {
        key: "Ctrl+C",
        description: "Cancel / Forward to CLI",
    },
    Shortcut {
        key: "Enter",
        description: "Send prompt / Stop recording",
    },
];

/// Format the help overlay as a string.
pub fn format_help_overlay(theme: Theme, width: usize) -> String {
    let colors = theme.colors();
    let borders = &colors.borders;
    let mut lines = Vec::new();

    // Calculate box width
    let content_width = width.clamp(30, 50);

    // Top border
    lines.push(format_box_top(&colors, borders, content_width));

    // Title
    lines.push(format_title_line(
        &colors,
        borders,
        "VoxTerm - Shortcuts",
        content_width,
    ));

    // Separator
    lines.push(format_separator(&colors, borders, content_width));

    // Shortcuts
    for shortcut in SHORTCUTS {
        lines.push(format_shortcut_line(&colors, shortcut, content_width));
    }

    // Separator
    lines.push(format_separator(&colors, borders, content_width));

    // Footer
    lines.push(format_title_line(
        &colors,
        borders,
        "Ctrl+O settings  Esc close",
        content_width,
    ));

    // Bottom border
    lines.push(format_box_bottom(&colors, borders, content_width));

    lines.join("\n")
}

fn format_box_top(colors: &ThemeColors, borders: &crate::theme::BorderSet, width: usize) -> String {
    // width is the total box width including corners
    // Inner horizontal chars = width - 2 (for the two corners)
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.top_left, inner, borders.top_right, colors.reset
    )
}

fn format_box_bottom(
    colors: &ThemeColors,
    borders: &crate::theme::BorderSet,
    width: usize,
) -> String {
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.bottom_left, inner, borders.bottom_right, colors.reset
    )
}

fn format_separator(
    colors: &ThemeColors,
    borders: &crate::theme::BorderSet,
    width: usize,
) -> String {
    let inner_width = width.saturating_sub(2);
    let inner: String = std::iter::repeat(borders.horizontal).take(inner_width).collect();
    format!(
        "{}{}{}{}{}",
        colors.border, borders.t_left, inner, borders.t_right, colors.reset
    )
}

fn format_title_line(
    colors: &ThemeColors,
    borders: &crate::theme::BorderSet,
    title: &str,
    width: usize,
) -> String {
    // Content between borders must equal width - 2 (for the two border chars)
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

fn format_shortcut_line(colors: &ThemeColors, shortcut: &Shortcut, width: usize) -> String {
    let borders = &colors.borders;
    // Content width between vertical borders
    let inner_width = width.saturating_sub(2);
    let key_width = 10;
    // Layout: "  " (2) + key (10) + "  " (2) + desc = 14 + desc_width = inner_width
    let desc_width = inner_width.saturating_sub(key_width + 4);
    let key_padded = format!("{:>width$}", shortcut.key, width = key_width);
    let desc_truncated: String = shortcut.description.chars().take(desc_width).collect();
    let desc_padded = format!("{:<width$}", desc_truncated, width = desc_width);

    format!(
        "{}{}{}  {}{}{}  {}{}{}{}",
        colors.border,
        borders.vertical,
        colors.reset,
        colors.info,
        key_padded,
        colors.reset,
        desc_padded,
        colors.border,
        borders.vertical,
        colors.reset
    )
}

/// Calculate the height of the help overlay.
pub fn help_overlay_height() -> usize {
    // Top border + title + separator + shortcuts + separator + footer + bottom border
    3 + SHORTCUTS.len() + 3
}

/// Calculate the width of the help overlay.
#[allow(dead_code)]
pub fn help_overlay_width() -> usize {
    54 // Fixed width for consistent display
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_defined() {
        assert!(!SHORTCUTS.is_empty());
        assert!(SHORTCUTS.len() >= 5);
    }

    #[test]
    fn format_help_overlay_contains_shortcuts() {
        let help = format_help_overlay(Theme::Coral, 60);
        assert!(help.contains("Ctrl+R"));
        assert!(help.contains("Start voice capture"));
        assert!(help.contains("Ctrl+V"));
        assert!(help.contains("Toggle auto-voice"));
    }

    #[test]
    fn format_help_overlay_has_borders() {
        let help = format_help_overlay(Theme::Coral, 60);
        assert!(help.contains("┌"));
        assert!(help.contains("└"));
        assert!(help.contains("│"));
    }

    #[test]
    fn help_overlay_dimensions() {
        assert!(help_overlay_height() > 5);
        assert!(help_overlay_width() > 30);
    }

    #[test]
    fn format_help_overlay_no_color() {
        let help = format_help_overlay(Theme::None, 60);
        assert!(help.contains("Ctrl+R"));
        // Should not have ANSI color codes (only box drawing)
        assert!(!help.contains("\x1b[9")); // No color codes like \x1b[91m
    }
}
