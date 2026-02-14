//! Shared icon vocabulary so HUD symbols stay consistent across render paths.
//!
//! Provides Unicode and ASCII fallback icons for all UI elements.

#![allow(dead_code)]

/// Collection of icons used throughout the HUD.
#[derive(Debug, Clone, Copy)]
pub struct IconSet {
    // State indicators
    /// Recording active indicator
    pub recording: &'static str,
    /// Idle/ready indicator
    pub idle: &'static str,
    /// Success/complete indicator
    pub success: &'static str,
    /// Error indicator
    pub error: &'static str,
    /// Warning indicator
    pub warning: &'static str,
    /// Info indicator
    pub info: &'static str,

    // HUD elements
    /// Audio level indicator
    pub audio_indicator: &'static str,
    /// Latency/timing indicator
    pub latency: &'static str,
    /// Queue depth indicator
    pub queue: &'static str,
    /// Network connected indicator
    pub network_ok: &'static str,
    /// Network disconnected indicator
    pub network_down: &'static str,
    /// Section separator
    pub separator: &'static str,
    /// Selection cursor
    pub selection: &'static str,
}

/// Unicode icon set for terminals with full Unicode support.
pub static UNICODE_ICONS: IconSet = IconSet {
    // State indicators
    recording: "●",
    idle: "○",
    success: "✓",
    error: "✗",
    warning: "⚠",
    info: "ℹ",

    // HUD elements
    audio_indicator: "▸",
    latency: "◷",
    queue: "▤",
    network_ok: "◆",
    network_down: "◇",
    separator: "│",
    selection: "▶",
};

/// ASCII icon set for terminals without Unicode support.
pub static ASCII_ICONS: IconSet = IconSet {
    // State indicators
    recording: "*",
    idle: "-",
    success: "ok",
    error: "err",
    warning: "!",
    info: "i",

    // HUD elements
    audio_indicator: ">",
    latency: "t:",
    queue: "Q:",
    network_ok: "+",
    network_down: "x",
    separator: "|",
    selection: ">",
};

/// Braille-based spinner frames for smooth animation.
pub const SPINNER_BRAILLE: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Circle-based spinner frames for a rotating effect.
pub const SPINNER_CIRCLE: &[char] = &['◐', '◓', '◑', '◒'];

/// ASCII spinner frames for terminals without Unicode.
pub const SPINNER_ASCII: &[&str] = &["[.  ]", "[.. ]", "[...]", "[ ..]", "[  .]"];

/// Vertical meter bar characters for audio level visualization.
pub const METER_BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Select the appropriate icon set based on Unicode support.
pub fn get_icons(unicode: bool) -> &'static IconSet {
    if unicode {
        &UNICODE_ICONS
    } else {
        &ASCII_ICONS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_icons_defined() {
        assert!(!UNICODE_ICONS.recording.is_empty());
        assert!(!UNICODE_ICONS.idle.is_empty());
        assert!(!UNICODE_ICONS.success.is_empty());
        assert!(!UNICODE_ICONS.error.is_empty());
        assert!(!UNICODE_ICONS.separator.is_empty());
    }

    #[test]
    fn ascii_icons_defined() {
        assert!(!ASCII_ICONS.recording.is_empty());
        assert!(!ASCII_ICONS.idle.is_empty());
        assert!(!ASCII_ICONS.success.is_empty());
        assert!(!ASCII_ICONS.error.is_empty());
        assert!(!ASCII_ICONS.separator.is_empty());
    }

    #[test]
    fn get_icons_returns_correct_set() {
        let unicode = get_icons(true);
        assert_eq!(unicode.recording, "●");

        let ascii = get_icons(false);
        assert_eq!(ascii.recording, "*");
    }

    #[test]
    fn spinner_braille_has_frames() {
        assert_eq!(SPINNER_BRAILLE.len(), 10);
        assert_eq!(SPINNER_BRAILLE[0], '⠋');
    }

    #[test]
    fn spinner_circle_has_frames() {
        assert_eq!(SPINNER_CIRCLE.len(), 4);
        assert_eq!(SPINNER_CIRCLE[0], '◐');
    }

    #[test]
    fn spinner_ascii_has_frames() {
        assert_eq!(SPINNER_ASCII.len(), 5);
        assert_eq!(SPINNER_ASCII[0], "[.  ]");
    }

    #[test]
    fn meter_bars_ordered() {
        assert_eq!(METER_BARS.len(), 8);
        assert_eq!(METER_BARS[0], '▁');
        assert_eq!(METER_BARS[7], '█');
    }
}
