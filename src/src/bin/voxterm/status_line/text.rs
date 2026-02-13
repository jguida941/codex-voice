//! ANSI-aware width/truncation logic so status text clips without visual drift.

use unicode_width::UnicodeWidthChar;

/// Calculate display width excluding ANSI escape codes.
#[inline]
pub(super) fn display_width(s: &str) -> usize {
    let mut width: usize = 0;
    let mut in_escape = false;

    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
        } else if in_escape {
            if ch == 'm' {
                in_escape = false;
            }
        } else {
            width += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }

    width
}

/// Truncate a string to a maximum display width.
#[inline]
pub(super) fn truncate_display(s: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut width: usize = 0;
    let mut in_escape = false;
    let mut escape_seq = String::new();

    for ch in s.chars() {
        if ch == '\x1b' {
            in_escape = true;
            escape_seq.push(ch);
        } else if in_escape {
            escape_seq.push(ch);
            if ch == 'm' {
                result.push_str(&escape_seq);
                escape_seq.clear();
                in_escape = false;
            }
        } else {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if width.saturating_add(ch_width) > max_width {
                break;
            }
            result.push(ch);
            width = width.saturating_add(ch_width);
        }
    }

    // Ensure we close any open escape sequences
    if !result.is_empty() && result.contains("\x1b[") && !result.ends_with("\x1b[0m") {
        result.push_str("\x1b[0m");
    }

    result
}

pub(super) fn pad_display(s: &str, width: usize) -> String {
    let current = display_width(s);
    if current >= width {
        return truncate_display(s, width);
    }
    let mut result = String::with_capacity(s.len() + width.saturating_sub(current));
    result.push_str(s);
    result.push_str(&" ".repeat(width.saturating_sub(current)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_excludes_ansi() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("\x1b[91mhello\x1b[0m"), 5);
        assert_eq!(display_width("\x1b[38;2;255;0;0mred\x1b[0m"), 3);
    }

    #[test]
    fn truncate_display_respects_width() {
        assert_eq!(truncate_display("hello", 3), "hel");
        assert_eq!(truncate_display("hello", 10), "hello");
        assert_eq!(truncate_display("hello", 0), "");
    }

    #[test]
    fn truncate_display_preserves_ansi() {
        let colored = "\x1b[91mhello\x1b[0m";
        let truncated = truncate_display(colored, 3);
        assert!(truncated.contains("\x1b[91m"));
        assert!(truncated.contains("hel"));
    }
}
