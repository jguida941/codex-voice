//! UTF-8-safe slicing helpers so terminal rendering never panics on multibyte text.
//!
//! These functions ensure all string operations respect UTF-8 character boundaries
//! to prevent panics when slicing strings containing multi-byte characters.

use unicode_width::UnicodeWidthChar;

/// Returns a prefix of the string up to `max_chars` characters.
/// Respects UTF-8 boundaries and won't panic on multi-byte characters.
pub fn safe_prefix(s: &str, max_chars: usize) -> &str {
    if s.is_empty() || max_chars == 0 {
        return "";
    }

    let mut end = s.len();
    for (count, (idx, ch)) in s.char_indices().enumerate() {
        if count == max_chars {
            return &s[..idx];
        }
        end = idx + ch.len_utf8();
    }

    &s[..end.min(s.len())]
}

/// Returns a slice of the string from `start_chars` for `len_chars` characters.
/// Respects UTF-8 boundaries and won't panic on multi-byte characters.
pub fn safe_slice(s: &str, start_chars: usize, len_chars: usize) -> &str {
    if len_chars == 0 || s.is_empty() {
        return "";
    }

    let mut start_byte = 0usize;
    let mut end_byte = s.len();
    let mut start_found = start_chars == 0;
    for (char_count, (i, _)) in s.char_indices().enumerate() {
        if char_count == start_chars {
            start_byte = i;
            start_found = true;
            break;
        }
    }
    if !start_found {
        return "";
    }

    // Find the ending byte position
    for (char_count, (i, _)) in s[start_byte..].char_indices().enumerate() {
        if char_count == len_chars {
            end_byte = start_byte + i;
            break;
        }
    }

    &s[start_byte..end_byte.min(s.len())]
}

/// Returns a suffix of the string starting from the last `max_chars` characters.
/// Respects UTF-8 boundaries and won't panic on multi-byte characters.
pub fn safe_suffix(s: &str, max_chars: usize) -> &str {
    if s.is_empty() || max_chars == 0 {
        return "";
    }

    let total_chars = s.chars().count();
    if total_chars <= max_chars {
        return s;
    }

    let skip_chars = total_chars - max_chars;
    let mut start_byte = 0;
    for (char_count, (i, _)) in s.char_indices().enumerate() {
        if char_count == skip_chars {
            start_byte = i;
            break;
        }
    }

    &s[start_byte..]
}

/// Truncates a string to `max_chars` characters and adds an ellipsis if truncated.
/// Respects UTF-8 boundaries and won't panic on multi-byte characters.
pub fn ellipsize(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }

    if max_chars == 0 {
        return String::from("…");
    }

    if max_chars == 1 {
        return String::from("…");
    }

    // Leave room for the ellipsis
    format!("{}…", safe_prefix(s, max_chars.saturating_sub(1)))
}

/// Return a slice of the string bounded by display columns rather than raw characters.
/// This prevents splitting a multi-byte or double-width glyph when showing a viewport.
pub fn window_by_columns(s: &str, start_cols: usize, width_cols: usize) -> &str {
    if width_cols == 0 || s.is_empty() {
        return "";
    }

    let mut col = 0usize;
    let mut start_byte = 0usize;
    let mut start_found = false;
    let mut end_byte = s.len();
    let target_end = start_cols.saturating_add(width_cols);

    for (idx, ch) in s.char_indices() {
        let glyph_width = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
        let next_col = col.saturating_add(glyph_width);

        if !start_found && col <= start_cols && start_cols < next_col {
            start_byte = idx;
            start_found = true;
        }

        if start_found && next_col > target_end {
            end_byte = idx;
            break;
        }

        col = next_col;
    }

    if !start_found {
        return "";
    }

    // Indices come from char_indices(), so they're on char boundaries
    // But add a runtime check to catch any logic errors
    if start_byte > end_byte || end_byte > s.len() {
        // This should never happen given our logic above, but log if it does
        #[cfg(debug_assertions)]
        panic!(
            "window_by_columns: invalid slice start={} end={} len={}",
            start_byte,
            end_byte,
            s.len()
        );
        #[cfg(not(debug_assertions))]
        {
            // In release, log and return empty to avoid panic
            eprintln!(
                "window_by_columns: invalid slice start={} end={} len={}",
                start_byte,
                end_byte,
                s.len()
            );
            return "";
        }
    }

    &s[start_byte..end_byte]
}

/// Safely splits a string at a byte position, returning None if the position
/// is not on a UTF-8 character boundary.
pub fn safe_split_at(s: &str, byte_pos: usize) -> Option<(&str, &str)> {
    if byte_pos > s.len() {
        return None;
    }

    if s.is_char_boundary(byte_pos) {
        Some(s.split_at(byte_pos))
    } else {
        None
    }
}

/// Returns a safe substring from byte positions, adjusting to nearest character boundaries.
/// If start or end are not on boundaries, they are adjusted inward.
pub fn safe_byte_slice(s: &str, start: usize, end: usize) -> &str {
    if start >= s.len() {
        return "";
    }

    let safe_start = if s.is_char_boundary(start) {
        start
    } else {
        // Find the next character boundary
        let mut pos = start.min(s.len());
        while pos < s.len() && !s.is_char_boundary(pos) {
            pos += 1;
        }
        pos
    };

    let safe_end = if end <= safe_start {
        safe_start
    } else if end >= s.len() {
        s.len()
    } else if s.is_char_boundary(end) {
        end
    } else {
        // Find the previous character boundary
        let mut pos = end;
        while pos > safe_start && !s.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    };

    &s[safe_start..safe_end]
}

/// Counts the number of characters in a string (not bytes).
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// Safely gets a character at a given character index (not byte index).
pub fn char_at(s: &str, char_idx: usize) -> Option<char> {
    s.chars().nth(char_idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_prefix() {
        // ASCII
        assert_eq!(safe_prefix("hello", 3), "hel");
        assert_eq!(safe_prefix("hello", 10), "hello");
        assert_eq!(safe_prefix("hello", 0), "");
        assert_eq!(safe_prefix("", 5), "");

        // Multi-byte UTF-8
        assert_eq!(safe_prefix("你好世界", 2), "你好");
        assert_eq!(safe_prefix("🦀Rust", 2), "🦀R");
        assert_eq!(safe_prefix("café", 3), "caf");
    }

    #[test]
    fn test_safe_slice() {
        // ASCII
        assert_eq!(safe_slice("hello world", 0, 5), "hello");
        assert_eq!(safe_slice("hello world", 6, 5), "world");
        assert_eq!(safe_slice("hello", 2, 2), "ll");

        // Multi-byte UTF-8
        assert_eq!(safe_slice("你好世界", 1, 2), "好世");
        assert_eq!(safe_slice("🦀Rust🔥", 1, 4), "Rust");

        // Edge cases
        assert_eq!(safe_slice("test", 10, 5), "");
        assert_eq!(safe_slice("test", 2, 10), "st");
    }

    #[test]
    fn test_ellipsize() {
        assert_eq!(ellipsize("hello", 10), "hello");
        assert_eq!(ellipsize("hello world", 8), "hello w…");
        assert_eq!(ellipsize("你好世界", 3), "你好…");
        assert_eq!(ellipsize("test", 1), "…");
        assert_eq!(ellipsize("test", 0), "…");
    }

    #[test]
    fn test_safe_suffix() {
        assert_eq!(safe_suffix("hello world", 5), "world");
        assert_eq!(safe_suffix("你好世界", 2), "世界");
        assert_eq!(safe_suffix("test", 10), "test");
        assert_eq!(safe_suffix("", 5), "");
    }

    #[test]
    fn test_window_by_columns_basic() {
        assert_eq!(window_by_columns("abcdef", 0, 3), "abc");
        assert_eq!(window_by_columns("abcdef", 2, 3), "cde");
        assert_eq!(window_by_columns("abcdef", 10, 5), "");
    }

    #[test]
    fn test_window_by_columns_multibyte() {
        assert_eq!(window_by_columns("你好世界", 0, 4), "你好");
        assert_eq!(window_by_columns("你好世界", 2, 4), "好世");
    }

    #[test]
    fn test_window_by_columns_handles_wide_glyphs() {
        let sample = "│> Testing. 😊 你好 0;0;0u";
        assert!(!window_by_columns(sample, 0, 12).is_empty());
        assert_eq!(window_by_columns(sample, 50, 10), "");
    }
}
