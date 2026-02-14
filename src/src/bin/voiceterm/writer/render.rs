use std::io::{self, Write};
use std::{env, sync::OnceLock};

use crate::status_line::StatusBanner;
use crate::status_style::StatusType;
use crate::theme::Theme;

use super::sanitize::{sanitize_status, truncate_status};
use super::state::OverlayPanel;

const SAVE_CURSOR_COMBINED: &[u8] = b"\x1b[s\x1b7";
const RESTORE_CURSOR_COMBINED: &[u8] = b"\x1b[u\x1b8";
const SAVE_CURSOR_DEC: &[u8] = b"\x1b7";
const RESTORE_CURSOR_DEC: &[u8] = b"\x1b8";
const SAVE_CURSOR_ANSI: &[u8] = b"\x1b[s";
const RESTORE_CURSOR_ANSI: &[u8] = b"\x1b[u";
const WRAP_DISABLE: &[u8] = b"\x1b[?7l";
const WRAP_ENABLE: &[u8] = b"\x1b[?7h";

fn contains_jetbrains_hint(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("jetbrains")
        || value.contains("jediterm")
        || value.contains("pycharm")
        || value.contains("intellij")
        || value.contains("idea")
}

fn contains_cursor_hint(value: &str) -> bool {
    value.to_ascii_lowercase().contains("cursor")
}

fn is_jetbrains_terminal() -> bool {
    static IS_JETBRAINS: OnceLock<bool> = OnceLock::new();
    *IS_JETBRAINS.get_or_init(|| {
        const HINT_KEYS: &[&str] = &[
            "PYCHARM_HOSTED",
            "JETBRAINS_IDE",
            "IDEA_INITIAL_DIRECTORY",
            "IDEA_INITIAL_PROJECT",
            "CLION_IDE",
            "WEBSTORM_IDE",
        ];

        if HINT_KEYS
            .iter()
            .any(|key| env::var(key).map(|v| !v.trim().is_empty()).unwrap_or(false))
        {
            return true;
        }

        if let Ok(term_program) = env::var("TERM_PROGRAM") {
            if contains_jetbrains_hint(&term_program) {
                return true;
            }
        }

        if let Ok(terminal_emulator) = env::var("TERMINAL_EMULATOR") {
            if contains_jetbrains_hint(&terminal_emulator) {
                return true;
            }
        }

        false
    })
}

fn is_cursor_terminal() -> bool {
    static IS_CURSOR: OnceLock<bool> = OnceLock::new();
    *IS_CURSOR.get_or_init(|| {
        if let Ok(term_program) = env::var("TERM_PROGRAM") {
            if contains_cursor_hint(&term_program) {
                return true;
            }
        }

        if let Ok(terminal_emulator) = env::var("TERMINAL_EMULATOR") {
            if contains_cursor_hint(&terminal_emulator) {
                return true;
            }
        }

        for key in [
            "CURSOR_TRACE_ID",
            "CURSOR_APP_VERSION",
            "CURSOR_VERSION",
            "CURSOR_BUILD_VERSION",
        ] {
            if env::var(key).map(|v| !v.trim().is_empty()).unwrap_or(false) {
                return true;
            }
        }

        false
    })
}

fn save_cursor_sequence() -> &'static [u8] {
    if is_jetbrains_terminal() {
        SAVE_CURSOR_DEC
    } else if is_cursor_terminal() {
        SAVE_CURSOR_ANSI
    } else {
        SAVE_CURSOR_COMBINED
    }
}

fn restore_cursor_sequence() -> &'static [u8] {
    if is_jetbrains_terminal() {
        RESTORE_CURSOR_DEC
    } else if is_cursor_terminal() {
        RESTORE_CURSOR_ANSI
    } else {
        RESTORE_CURSOR_COMBINED
    }
}

fn should_disable_autowrap_during_redraw() -> bool {
    is_jetbrains_terminal()
}

fn push_cursor_prefix(sequence: &mut Vec<u8>) {
    sequence.extend_from_slice(save_cursor_sequence());
    if should_disable_autowrap_during_redraw() {
        sequence.extend_from_slice(WRAP_DISABLE);
    }
}

fn push_cursor_suffix(sequence: &mut Vec<u8>) {
    if should_disable_autowrap_during_redraw() {
        sequence.extend_from_slice(WRAP_ENABLE);
    }
    sequence.extend_from_slice(restore_cursor_sequence());
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
    push_cursor_prefix(&mut sequence);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(formatted.as_bytes());
    // Clear only the remainder of the line to avoid clear-then-paint flicker.
    sequence.extend_from_slice(b"\x1b[K");
    push_cursor_suffix(&mut sequence);
    stdout.write_all(&sequence)
}

/// Write a multi-line status banner at the bottom of the terminal.
pub(super) fn write_status_banner(
    stdout: &mut dyn Write,
    banner: &StatusBanner,
    rows: u16,
    previous_lines: Option<&[String]>,
) -> io::Result<()> {
    if rows == 0 || banner.height == 0 {
        return Ok(());
    }
    let height = banner.height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    let mut any_changed = false;

    for (idx, line) in banner.lines.iter().take(height).enumerate() {
        if previous_lines
            .and_then(|lines| lines.get(idx))
            .is_some_and(|prev| prev == line)
        {
            continue;
        }
        if !any_changed {
            push_cursor_prefix(&mut sequence);
            any_changed = true;
        }
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(line.as_bytes()); // Write content
        sequence.extend_from_slice(b"\x1b[K"); // Clear any trailing stale content
    }

    if !any_changed {
        return Ok(());
    }

    push_cursor_suffix(&mut sequence);
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
    push_cursor_prefix(&mut sequence);

    for idx in 0..clear_height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
    }

    push_cursor_suffix(&mut sequence);
    stdout.write_all(&sequence)
}

pub(super) fn clear_status_line(stdout: &mut dyn Write, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let mut sequence = Vec::new();
    push_cursor_prefix(&mut sequence);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    push_cursor_suffix(&mut sequence);
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
    push_cursor_prefix(&mut sequence);
    for (idx, line) in lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(line.as_bytes());
        sequence.extend_from_slice(b"\x1b[K");
    }
    push_cursor_suffix(&mut sequence);
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
    push_cursor_prefix(&mut sequence);
    for idx in 0..height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
    }
    push_cursor_suffix(&mut sequence);
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
    fn jetbrains_hint_detection_matches_known_values() {
        assert!(contains_jetbrains_hint("JetBrains-JediTerm"));
        assert!(contains_jetbrains_hint("PyCharm"));
        assert!(contains_jetbrains_hint("IntelliJ"));
        assert!(!contains_jetbrains_hint("xterm-256color"));
        assert!(!contains_jetbrains_hint("cursor"));
    }

    #[test]
    fn cursor_hint_detection_matches_known_values() {
        assert!(contains_cursor_hint("cursor"));
        assert!(contains_cursor_hint("Cursor"));
        assert!(!contains_cursor_hint("vscode"));
        assert!(!contains_cursor_hint("JetBrains-JediTerm"));
    }

    #[test]
    fn write_status_banner_full_hud_clears_trailing_content() {
        let mut buf = Vec::new();
        let banner = StatusBanner::new(vec![
            "top".to_string(),
            "main".to_string(),
            "shortcuts".to_string(),
            "bottom".to_string(),
        ]);

        write_status_banner(&mut buf, &banner, 24, None).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[21;1H"));
        assert!(output.contains("\u{1b}[K"));
    }

    #[test]
    fn write_status_banner_single_line_keeps_trailing_clear() {
        let mut buf = Vec::new();
        let banner = StatusBanner::new(vec!["minimal hud".to_string()]);

        write_status_banner(&mut buf, &banner, 24, None).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[24;1H"));
        assert!(output.contains("\u{1b}[K"));
    }

    #[test]
    fn write_status_banner_skips_unchanged_lines_when_previous_provided() {
        let mut buf = Vec::new();
        let previous = vec![
            "top".to_string(),
            "main old".to_string(),
            "shortcuts".to_string(),
            "bottom".to_string(),
        ];
        let banner = StatusBanner::new(vec![
            "top".to_string(),
            "main new".to_string(),
            "shortcuts".to_string(),
            "bottom".to_string(),
        ]);

        write_status_banner(&mut buf, &banner, 24, Some(&previous)).unwrap();
        let output = String::from_utf8_lossy(&buf);

        // Only row 22 (the main row in a 4-line banner at 24 rows) should redraw.
        assert!(output.contains("\u{1b}[22;1H"));
        assert!(!output.contains("\u{1b}[21;1H"));
        assert!(!output.contains("\u{1b}[23;1H"));
        assert!(!output.contains("\u{1b}[24;1H"));
    }
}
