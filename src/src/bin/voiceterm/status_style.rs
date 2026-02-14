//! Status styling rules that keep message severity visually consistent.
//!
//! Provides visual differentiation for different status message types
//! in the overlay status line.

use crate::theme::{Theme, ThemeColors};

/// Status message categories for visual styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusType {
    /// Recording/listening state (red ●)
    Recording,
    /// Processing/working state (yellow ◐)
    Processing,
    /// Success/ready state (green ✓)
    Success,
    /// Warning state (yellow ⚠)
    Warning,
    /// Error state (red ✗)
    Error,
    /// Informational state (blue ℹ)
    Info,
}

impl StatusType {
    /// Infer status type from message content.
    pub fn from_message(text: &str) -> Self {
        let lower = text.to_lowercase();

        // Recording states
        if lower.contains("listening") || lower.contains("already running") {
            return Self::Recording;
        }

        // Processing states
        if lower.contains("processing") {
            return Self::Processing;
        }

        // Error states
        if lower.contains("failed") || lower.contains("error") {
            return Self::Error;
        }

        // Warning states
        if lower.contains("no speech")
            || lower.contains("queue full")
            || lower.contains("cancelled")
            || lower.contains("dropped")
        {
            return Self::Warning;
        }

        // Success states
        if lower.contains("transcript ready") {
            return Self::Success;
        }

        // Info states (default for toggles, sensitivity, etc.)
        Self::Info
    }

    /// Get the unicode indicator for this status type.
    pub fn indicator(&self) -> &'static str {
        match self {
            Self::Recording => "● REC",
            Self::Processing => "◐",
            Self::Success => "✓",
            Self::Warning => "⚠",
            Self::Error => "✗",
            Self::Info => "ℹ",
        }
    }

    /// Get the color code for this status type from a theme.
    pub fn color<'a>(&self, colors: &'a ThemeColors) -> &'a str {
        match self {
            Self::Recording => colors.recording,
            Self::Processing => colors.processing,
            Self::Success => colors.success,
            Self::Warning => colors.warning,
            Self::Error => colors.error,
            Self::Info => colors.info,
        }
    }

    /// Get the colored prefix for this status type using default theme.
    /// For backward compatibility.
    #[allow(dead_code)]
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Recording => "\x1b[91m● REC\x1b[0m ",
            Self::Processing => "\x1b[93m◐\x1b[0m ",
            Self::Success => "\x1b[92m✓\x1b[0m ",
            Self::Warning => "\x1b[93m⚠\x1b[0m ",
            Self::Error => "\x1b[91m✗\x1b[0m ",
            Self::Info => "\x1b[94mℹ\x1b[0m ",
        }
    }

    /// Get the colored prefix for this status type using a specific theme.
    pub fn prefix_with_theme(&self, theme: Theme) -> String {
        let colors = theme.colors();
        format!(
            "{}{}{} ",
            self.color(&colors),
            self.indicator(),
            colors.reset
        )
    }

    /// Get the display width of the prefix (for truncation calculations).
    /// Unicode indicators are 1-2 chars, "● REC" is 5 chars, plus space.
    pub fn prefix_display_width(&self) -> usize {
        match self {
            Self::Recording => 6, // "● REC "
            _ => 2,               // "X " where X is unicode char
        }
    }
}

/// Format a status message with colored prefix based on content (default theme).
#[allow(dead_code)]
pub fn format_status(text: &str) -> String {
    let status_type = StatusType::from_message(text);
    format!("{}{}", status_type.prefix(), text)
}

/// Format a status message with colored prefix using a specific theme.
#[allow(dead_code)]
pub fn format_status_with_theme(text: &str, theme: Theme) -> String {
    let status_type = StatusType::from_message(text);
    format!("{}{}", status_type.prefix_with_theme(theme), text)
}

/// Calculate the display width of a formatted status (excluding ANSI codes).
#[allow(dead_code)]
pub fn status_display_width(text: &str) -> usize {
    let status_type = StatusType::from_message(text);
    status_type.prefix_display_width() + text.chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_type_from_message_recording() {
        assert_eq!(
            StatusType::from_message("Listening Manual Mode"),
            StatusType::Recording
        );
        assert_eq!(
            StatusType::from_message("Voice capture already running"),
            StatusType::Recording
        );
    }

    #[test]
    fn status_type_from_message_processing() {
        assert_eq!(
            StatusType::from_message("Processing..."),
            StatusType::Processing
        );
    }

    #[test]
    fn status_type_from_message_error() {
        assert_eq!(
            StatusType::from_message("Voice capture failed"),
            StatusType::Error
        );
        assert_eq!(
            StatusType::from_message("Voice capture error (see log)"),
            StatusType::Error
        );
    }

    #[test]
    fn status_type_from_message_warning() {
        assert_eq!(
            StatusType::from_message("No speech detected"),
            StatusType::Warning
        );
        assert_eq!(
            StatusType::from_message("Transcript queue full"),
            StatusType::Warning
        );
        assert_eq!(
            StatusType::from_message("Capture cancelled"),
            StatusType::Warning
        );
    }

    #[test]
    fn status_type_from_message_success() {
        assert_eq!(
            StatusType::from_message("Transcript ready (Rust pipeline)"),
            StatusType::Success
        );
    }

    #[test]
    fn status_type_from_message_info() {
        assert_eq!(
            StatusType::from_message("Auto-voice enabled"),
            StatusType::Info
        );
        assert_eq!(
            StatusType::from_message("Mic sensitivity: -35 dB"),
            StatusType::Info
        );
    }

    #[test]
    fn format_status_includes_prefix() {
        let formatted = format_status("Processing...");
        assert!(formatted.contains("◐"));
        assert!(formatted.contains("Processing..."));
    }

    #[test]
    fn prefix_display_width_correct() {
        assert_eq!(StatusType::Recording.prefix_display_width(), 6);
        assert_eq!(StatusType::Processing.prefix_display_width(), 2);
        assert_eq!(StatusType::Success.prefix_display_width(), 2);
    }

    #[test]
    fn format_status_with_theme_catppuccin() {
        let formatted = format_status_with_theme("Processing...", Theme::Catppuccin);
        assert!(formatted.contains("◐"));
        assert!(formatted.contains("Processing..."));
        // Catppuccin uses RGB colors
        assert!(formatted.contains("\x1b[38;2;"));
    }

    #[test]
    fn format_status_with_theme_none() {
        let formatted = format_status_with_theme("Processing...", Theme::None);
        assert!(formatted.contains("◐"));
        assert!(formatted.contains("Processing..."));
        // No color theme should not have escape codes
        assert!(!formatted.contains("\x1b["));
    }

    #[test]
    fn indicator_returns_unicode() {
        assert_eq!(StatusType::Recording.indicator(), "● REC");
        assert_eq!(StatusType::Processing.indicator(), "◐");
        assert_eq!(StatusType::Success.indicator(), "✓");
    }
}
