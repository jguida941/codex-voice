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
        key: "Ctrl+]",
        description: "Increase mic sensitivity",
    },
    Shortcut {
        key: "Ctrl+\\",
        description: "Decrease mic sensitivity",
    },
    Shortcut {
        key: "Ctrl+Q",
        description: "Exit Codex Voice",
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
    let mut lines = Vec::new();

    // Calculate box width
    let content_width = width.clamp(30, 50);

    // Top border
    lines.push(format_box_top(&colors, content_width));

    // Title
    lines.push(format_title_line(
        &colors,
        "Codex Voice - Shortcuts",
        content_width,
    ));

    // Separator
    lines.push(format_separator(&colors, content_width));

    // Shortcuts
    for shortcut in SHORTCUTS {
        lines.push(format_shortcut_line(&colors, shortcut, content_width));
    }

    // Separator
    lines.push(format_separator(&colors, content_width));

    // Footer
    lines.push(format_title_line(
        &colors,
        "Press any key to close",
        content_width,
    ));

    // Bottom border
    lines.push(format_box_bottom(&colors, content_width));

    lines.join("\n")
}

fn format_box_top(colors: &ThemeColors, width: usize) -> String {
    format!("{}┌{}┐{}", colors.info, "─".repeat(width + 2), colors.reset)
}

fn format_box_bottom(colors: &ThemeColors, width: usize) -> String {
    format!("{}└{}┘{}", colors.info, "─".repeat(width + 2), colors.reset)
}

fn format_separator(colors: &ThemeColors, width: usize) -> String {
    format!("{}├{}┤{}", colors.info, "─".repeat(width + 2), colors.reset)
}

fn format_title_line(colors: &ThemeColors, title: &str, width: usize) -> String {
    let padding = width.saturating_sub(title.len());
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;
    format!(
        "{}│{} {}{}{} {}│{}",
        colors.info,
        colors.reset,
        " ".repeat(left_pad),
        title,
        " ".repeat(right_pad),
        colors.info,
        colors.reset
    )
}

fn format_shortcut_line(colors: &ThemeColors, shortcut: &Shortcut, width: usize) -> String {
    let key_width = 10;
    let desc_width = width.saturating_sub(key_width + 3);
    let key_padded = format!("{:>width$}", shortcut.key, width = key_width);
    let desc_truncated: String = shortcut.description.chars().take(desc_width).collect();
    let desc_padded = format!("{:<width$}", desc_truncated, width = desc_width);

    format!(
        "{}│{} {}{}{}   {} {}│{}",
        colors.info,
        colors.reset,
        colors.success,
        key_padded,
        colors.reset,
        desc_padded,
        colors.info,
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
