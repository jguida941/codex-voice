//! Terminal color-capability detection so theme fallbacks match host support.
//!
//! Detects what color modes the terminal supports and provides fallbacks.

use std::env;

/// Color mode capabilities of the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// 24-bit true color (16 million colors)
    #[default]
    TrueColor,
    /// 256 color mode
    Color256,
    /// Basic 16 ANSI colors
    Ansi16,
    /// No color support
    None,
}

impl ColorMode {
    /// Detect the terminal's color capabilities from environment variables.
    pub fn detect() -> Self {
        // Check NO_COLOR first (standard convention)
        // https://no-color.org/
        if env::var("NO_COLOR").is_ok() {
            return Self::None;
        }

        // Check COLORTERM for truecolor support
        if let Ok(colorterm) = env::var("COLORTERM") {
            if colorterm == "truecolor" || colorterm == "24bit" {
                return Self::TrueColor;
            }
        }

        // Check TERM for color capabilities
        if let Ok(term) = env::var("TERM") {
            if term.contains("256color") || term.contains("256-color") {
                return Self::Color256;
            }
            if term.contains("color") || term.contains("xterm") || term.contains("screen") {
                return Self::Ansi16;
            }
            if term == "dumb" {
                return Self::None;
            }
        }

        // Default to ANSI 16 colors as a safe fallback
        Self::Ansi16
    }

    /// Check if colors are supported at all.
    pub fn supports_color(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Check if 256 colors are supported.
    #[allow(dead_code)]
    pub fn supports_256(&self) -> bool {
        matches!(self, Self::TrueColor | Self::Color256)
    }

    /// Check if true color (24-bit) is supported.
    pub fn supports_truecolor(&self) -> bool {
        matches!(self, Self::TrueColor)
    }
}

impl std::fmt::Display for ColorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrueColor => write!(f, "truecolor"),
            Self::Color256 => write!(f, "256"),
            Self::Ansi16 => write!(f, "ansi"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Convert a 24-bit RGB color to the closest ANSI 256 color.
#[allow(dead_code)]
pub fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    // Check for grayscale
    if r == g && g == b {
        if r < 8 {
            return 16;
        }
        if r > 248 {
            return 231;
        }
        return ((r as u16 - 8) / 10 + 232) as u8;
    }

    // Convert to 6x6x6 color cube
    let r_idx = (r as u16 * 5 / 255) as u8;
    let g_idx = (g as u16 * 5 / 255) as u8;
    let b_idx = (b as u16 * 5 / 255) as u8;

    16 + 36 * r_idx + 6 * g_idx + b_idx
}

/// Convert a 24-bit RGB color to the closest ANSI 16 color.
#[allow(dead_code)]
pub fn rgb_to_ansi16(r: u8, g: u8, b: u8) -> u8 {
    // Simple brightness-based conversion
    let brightness = (r as u16 + g as u16 + b as u16) / 3;
    let is_bright = brightness > 127;

    // Determine primary color component
    let max = r.max(g).max(b);
    let base = if max == 0 {
        0 // Black
    } else if r == max && g == max && b == max {
        7 // White/gray
    } else if r == max && g >= b {
        if g > r / 2 {
            3 // Yellow
        } else {
            1 // Red
        }
    } else if g == max {
        if b > g / 2 {
            6 // Cyan
        } else {
            2 // Green
        }
    } else if r > b / 2 {
        5 // Magenta
    } else {
        4 // Blue
    };

    if is_bright {
        base + 8 // Bright variant
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_mode_supports_color() {
        assert!(ColorMode::TrueColor.supports_color());
        assert!(ColorMode::Color256.supports_color());
        assert!(ColorMode::Ansi16.supports_color());
        assert!(!ColorMode::None.supports_color());
    }

    #[test]
    fn color_mode_supports_256() {
        assert!(ColorMode::TrueColor.supports_256());
        assert!(ColorMode::Color256.supports_256());
        assert!(!ColorMode::Ansi16.supports_256());
        assert!(!ColorMode::None.supports_256());
    }

    #[test]
    fn color_mode_supports_truecolor() {
        assert!(ColorMode::TrueColor.supports_truecolor());
        assert!(!ColorMode::Color256.supports_truecolor());
        assert!(!ColorMode::Ansi16.supports_truecolor());
        assert!(!ColorMode::None.supports_truecolor());
    }

    #[test]
    fn color_mode_display() {
        assert_eq!(format!("{}", ColorMode::TrueColor), "truecolor");
        assert_eq!(format!("{}", ColorMode::None), "none");
    }

    #[test]
    fn rgb_to_256_grayscale() {
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
    }

    #[test]
    fn rgb_to_256_colors() {
        // Pure red should map to color cube
        let red = rgb_to_256(255, 0, 0);
        assert!(red >= 16 && red < 232);
    }

    #[test]
    fn rgb_to_ansi16_basic() {
        // Black
        assert_eq!(rgb_to_ansi16(0, 0, 0), 0);
        // Bright white
        assert_eq!(rgb_to_ansi16(255, 255, 255), 15);
    }
}
