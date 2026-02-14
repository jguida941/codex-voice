//! Status-string sanitization so control characters never corrupt terminal output.

pub(super) fn sanitize_status(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_graphic() || ch == ' ' {
                ch
            } else {
                ' '
            }
        })
        .collect()
}

pub(super) fn truncate_status(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_helpers_sanitize_and_truncate() {
        let sanitized = sanitize_status("ok\tbad\n");
        assert_eq!(sanitized, "ok bad ");
        assert_eq!(truncate_status("hello", 0), "");
        assert_eq!(truncate_status("hello", 2), "he");
    }
}
