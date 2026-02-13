//! Terminal rendering primitives so status and overlays update without flicker.

use std::io::{self, Write};

use crate::status_line::StatusBanner;
use crate::status_style::StatusType;
use crate::theme::Theme;

use super::sanitize::{sanitize_status, truncate_status};
use super::state::OverlayPanel;

const SAVE_CURSOR: &[u8] = b"\x1b[s\x1b7";
const RESTORE_CURSOR: &[u8] = b"\x1b[u\x1b8";

pub(super) fn set_scroll_region(
    stdout: &mut dyn Write,
    rows: u16,
    reserved_rows: usize,
) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    if reserved_rows == 0 {
        return reset_scroll_region(stdout);
    }
    let reserved = reserved_rows.min(rows as usize) as u16;
    let bottom = rows.saturating_sub(reserved).max(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(format!("\x1b[1;{bottom}r").as_bytes());
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

pub(super) fn reset_scroll_region(stdout: &mut dyn Write) -> io::Result<()> {
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(b"\x1b[r");
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

pub(super) fn write_status_line(
    stdout: &mut dyn Write,
    text: &str,
    rows: u16,
    cols: u16,
    theme: Theme,
) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let sanitized = sanitize_status(text);
    let status_type = StatusType::from_message(&sanitized);
    let display_width = status_type.prefix_display_width() + sanitized.chars().count();
    let prefix = status_type.prefix_with_theme(theme);
    let formatted = if display_width <= cols as usize {
        format!("{prefix}{sanitized}")
    } else {
        // Truncate the text portion, keeping room for the prefix
        let max_text_len = (cols as usize).saturating_sub(status_type.prefix_display_width());
        let truncated = truncate_status(&sanitized, max_text_len);
        format!("{prefix}{truncated}")
    };
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(formatted.as_bytes());
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

/// Write a multi-line status banner at the bottom of the terminal.
pub(super) fn write_status_banner(
    stdout: &mut dyn Write,
    banner: &StatusBanner,
    rows: u16,
) -> io::Result<()> {
    if rows == 0 || banner.height == 0 {
        return Ok(());
    }
    let height = banner.height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR); // Save cursor

    for (idx, line) in banner.lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
        sequence.extend_from_slice(line.as_bytes()); // Write content
    }

    sequence.extend_from_slice(RESTORE_CURSOR); // Restore cursor
    stdout.write_all(&sequence)
}

/// Clear a multi-line status banner.
/// Also clears extra rows above to catch ghost content from terminal scrolling.
pub(super) fn clear_status_banner(
    stdout: &mut dyn Write,
    rows: u16,
    height: usize,
) -> io::Result<()> {
    if rows == 0 || height == 0 {
        return Ok(());
    }
    // Only clear the banner rows to avoid erasing PTY content above the HUD.
    let clear_height = height.min(rows as usize);
    let start_row = rows.saturating_sub(clear_height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR); // Save cursor

    for idx in 0..clear_height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
    }

    sequence.extend_from_slice(RESTORE_CURSOR); // Restore cursor
    stdout.write_all(&sequence)
}

pub(super) fn clear_status_line(stdout: &mut dyn Write, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

pub(super) fn write_overlay_panel(
    stdout: &mut dyn Write,
    panel: &OverlayPanel,
    rows: u16,
) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    let lines: Vec<&str> = panel.content.lines().collect();
    let height = panel.height.min(lines.len()).min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    for (idx, line) in lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
        sequence.extend_from_slice(line.as_bytes());
    }
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

pub(super) fn clear_overlay_panel(
    stdout: &mut dyn Write,
    rows: u16,
    height: usize,
) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    let height = height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    for idx in 0..height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
    }
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_clear_status_line_respect_dimensions() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        write_status_line(&mut buf, "hi", 0, 10, theme).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 0, theme).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));
        assert!(output.contains("hi"));
        // Should contain color codes (info prefix for generic message)
        assert!(output.contains("\u{1b}[94m")); // Blue for info

        buf.clear();
        clear_status_line(&mut buf, 2, 10).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));

        buf.clear();
        clear_status_line(&mut buf, 2, 0).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn write_status_line_includes_colored_prefix() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        write_status_line(&mut buf, "Listening Manual Mode", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Recording status should have red prefix
        assert!(output.contains("\u{1b}[91m")); // Red
        assert!(output.contains("● REC"));

        buf.clear();
        write_status_line(&mut buf, "Processing...", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Processing status should have yellow prefix
        assert!(output.contains("\u{1b}[93m")); // Yellow
        assert!(output.contains("◐"));

        buf.clear();
        write_status_line(&mut buf, "Transcript ready", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Success status should have green prefix
        assert!(output.contains("\u{1b}[92m")); // Green
        assert!(output.contains("✓"));
    }

    #[test]
    fn write_status_line_truncation_preserves_status_type() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        let long_msg = "Transcript ready (Rust pipeline with extra detail)";
        // Force truncation so the status keyword would be removed from the visible text.
        write_status_line(&mut buf, long_msg, 2, 12, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Success status should still have green prefix even if text is truncated.
        assert!(output.contains("\u{1b}[92m")); // Green
        assert!(output.contains("✓"));
    }

    #[test]
    fn write_status_line_respects_no_color_theme() {
        let mut buf = Vec::new();
        write_status_line(&mut buf, "Processing...", 2, 80, Theme::None).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Should have the indicator but no escape codes for color
        assert!(output.contains("◐"));
        assert!(output.contains("Processing..."));
        // The only escape codes should be cursor positioning, not color
        let color_codes = output.matches("\u{1b}[9").count();
        assert_eq!(color_codes, 0, "Should not contain color codes");
    }

    #[test]
    fn set_scroll_region_writes_reserved_bottom_margin() {
        let mut buf = Vec::new();
        set_scroll_region(&mut buf, 40, 4).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[1;36r"));
    }

    #[test]
    fn reset_scroll_region_writes_full_region_reset() {
        let mut buf = Vec::new();
        reset_scroll_region(&mut buf).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[r"));
    }
}
