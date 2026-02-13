//! Theme registry so users can switch visual style without changing behavior.
//!
//! Provides predefined color palettes that can be selected via CLI flags.

mod borders;
mod colors;
mod detect;
mod palettes;

pub use borders::{BorderSet, BORDER_DOUBLE, BORDER_HEAVY, BORDER_ROUNDED, BORDER_SINGLE};
#[allow(unused_imports)]
pub use borders::{BORDER_DOTTED, BORDER_NONE};
pub use colors::ThemeColors;
pub use palettes::{
    THEME_ANSI, THEME_CATPPUCCIN, THEME_CHATGPT, THEME_CLAUDE, THEME_CODEX, THEME_CORAL,
    THEME_DRACULA, THEME_GRUVBOX, THEME_NONE, THEME_NORD, THEME_TOKYONIGHT,
};

use self::detect::is_warp_terminal;

/// Available color themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// Default coral/red theme (matches existing TUI)
    #[default]
    Coral,
    /// Claude warm neutral theme (Anthropic-inspired)
    Claude,
    /// Codex cool blue theme (neutral, OpenAI-style)
    Codex,
    /// ChatGPT emerald theme (OpenAI ChatGPT brand)
    ChatGpt,
    /// Catppuccin Mocha - pastel dark theme
    Catppuccin,
    /// Dracula - high contrast dark theme
    Dracula,
    /// Nord - arctic blue-gray theme
    Nord,
    /// Tokyo Night - elegant purple/blue dark theme
    TokyoNight,
    /// Gruvbox - warm retro earthy colors
    Gruvbox,
    /// ANSI 16-color fallback for older terminals
    Ansi,
    /// No colors - plain text
    None,
}

impl Theme {
    /// Parse theme name from string.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "coral" | "default" => Some(Self::Coral),
            "claude" | "anthropic" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "chatgpt" | "gpt" | "openai" => Some(Self::ChatGpt),
            "catppuccin" | "mocha" => Some(Self::Catppuccin),
            "dracula" => Some(Self::Dracula),
            "nord" => Some(Self::Nord),
            "tokyonight" | "tokyo-night" | "tokyo" => Some(Self::TokyoNight),
            "gruvbox" | "gruv" => Some(Self::Gruvbox),
            "ansi" | "ansi16" | "basic" => Some(Self::Ansi),
            "none" | "plain" => Some(Self::None),
            _ => None,
        }
    }

    /// Get the color palette for this theme.
    pub fn colors(&self) -> ThemeColors {
        let mut colors = match self {
            Self::Coral => THEME_CORAL,
            Self::Claude => THEME_CLAUDE,
            Self::Codex => THEME_CODEX,
            Self::ChatGpt => THEME_CHATGPT,
            Self::Catppuccin => THEME_CATPPUCCIN,
            Self::Dracula => THEME_DRACULA,
            Self::Nord => THEME_NORD,
            Self::TokyoNight => THEME_TOKYONIGHT,
            Self::Gruvbox => THEME_GRUVBOX,
            Self::Ansi => THEME_ANSI,
            Self::None => THEME_NONE,
        };
        if is_warp_terminal() {
            colors.bg_primary = "";
            colors.bg_secondary = "";
        }
        colors
    }

    /// List all available theme names.
    #[allow(dead_code)]
    pub fn available() -> &'static [&'static str] {
        &[
            "chatgpt",
            "claude",
            "codex",
            "coral",
            "catppuccin",
            "dracula",
            "gruvbox",
            "nord",
            "tokyonight",
            "ansi",
            "none",
        ]
    }

    /// Check if this theme uses truecolor (24-bit RGB).
    pub fn is_truecolor(&self) -> bool {
        matches!(
            self,
            Self::Claude
                | Self::Codex
                | Self::ChatGpt
                | Self::Catppuccin
                | Self::Dracula
                | Self::Nord
                | Self::TokyoNight
                | Self::Gruvbox
        )
    }

    /// Get a fallback theme for terminals without truecolor support.
    pub fn fallback_for_ansi(&self) -> Self {
        if self.is_truecolor() {
            Self::Ansi
        } else {
            *self
        }
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coral => write!(f, "coral"),
            Self::Claude => write!(f, "claude"),
            Self::Codex => write!(f, "codex"),
            Self::ChatGpt => write!(f, "chatgpt"),
            Self::Catppuccin => write!(f, "catppuccin"),
            Self::Dracula => write!(f, "dracula"),
            Self::Nord => write!(f, "nord"),
            Self::TokyoNight => write!(f, "tokyonight"),
            Self::Gruvbox => write!(f, "gruvbox"),
            Self::Ansi => write!(f, "ansi"),
            Self::None => write!(f, "none"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_from_name_parses_valid() {
        assert_eq!(Theme::from_name("coral"), Some(Theme::Coral));
        assert_eq!(Theme::from_name("claude"), Some(Theme::Claude));
        assert_eq!(Theme::from_name("anthropic"), Some(Theme::Claude));
        assert_eq!(Theme::from_name("codex"), Some(Theme::Codex));
        assert_eq!(Theme::from_name("chatgpt"), Some(Theme::ChatGpt));
        assert_eq!(Theme::from_name("gpt"), Some(Theme::ChatGpt));
        assert_eq!(Theme::from_name("openai"), Some(Theme::ChatGpt));
        assert_eq!(Theme::from_name("CATPPUCCIN"), Some(Theme::Catppuccin));
        assert_eq!(Theme::from_name("Dracula"), Some(Theme::Dracula));
        assert_eq!(Theme::from_name("nord"), Some(Theme::Nord));
        assert_eq!(Theme::from_name("ansi"), Some(Theme::Ansi));
        assert_eq!(Theme::from_name("ansi16"), Some(Theme::Ansi));
        assert_eq!(Theme::from_name("none"), Some(Theme::None));
        assert_eq!(Theme::from_name("default"), Some(Theme::Coral));
    }

    #[test]
    fn theme_is_truecolor() {
        assert!(!Theme::Coral.is_truecolor());
        assert!(Theme::Claude.is_truecolor());
        assert!(Theme::Codex.is_truecolor());
        assert!(Theme::ChatGpt.is_truecolor());
        assert!(Theme::Catppuccin.is_truecolor());
        assert!(Theme::Dracula.is_truecolor());
        assert!(Theme::Nord.is_truecolor());
        assert!(!Theme::Ansi.is_truecolor());
        assert!(!Theme::None.is_truecolor());
    }

    #[test]
    fn theme_fallback_for_ansi() {
        assert_eq!(Theme::Coral.fallback_for_ansi(), Theme::Coral);
        assert_eq!(Theme::Claude.fallback_for_ansi(), Theme::Ansi);
        assert_eq!(Theme::Codex.fallback_for_ansi(), Theme::Ansi);
        assert_eq!(Theme::ChatGpt.fallback_for_ansi(), Theme::Ansi);
        assert_eq!(Theme::Catppuccin.fallback_for_ansi(), Theme::Ansi);
        assert_eq!(Theme::Dracula.fallback_for_ansi(), Theme::Ansi);
        assert_eq!(Theme::None.fallback_for_ansi(), Theme::None);
    }

    #[test]
    fn theme_from_name_rejects_invalid() {
        assert_eq!(Theme::from_name("invalid"), None);
        assert_eq!(Theme::from_name(""), None);
    }

    #[test]
    fn theme_colors_returns_palette() {
        let colors = Theme::Coral.colors();
        assert!(colors.recording.contains("\x1b["));
        assert!(colors.reset.contains("\x1b[0m"));

        let none_colors = Theme::None.colors();
        assert!(none_colors.recording.is_empty());
    }

    #[test]
    fn theme_display_matches_name() {
        assert_eq!(format!("{}", Theme::Coral), "coral");
        assert_eq!(format!("{}", Theme::Claude), "claude");
        assert_eq!(format!("{}", Theme::Codex), "codex");
        assert_eq!(format!("{}", Theme::ChatGpt), "chatgpt");
        assert_eq!(format!("{}", Theme::Catppuccin), "catppuccin");
    }

    #[test]
    fn theme_has_expected_borders() {
        // Spot-check representative border styles for a few themes.
        assert_eq!(Theme::Coral.colors().borders.horizontal, '─'); // Single
        assert_eq!(Theme::Catppuccin.colors().borders.horizontal, '═'); // Double
        assert_eq!(Theme::Codex.colors().borders.horizontal, '═'); // Double
        assert_eq!(Theme::Dracula.colors().borders.horizontal, '━'); // Heavy
        assert_eq!(Theme::TokyoNight.colors().borders.horizontal, '━'); // Heavy
        assert_eq!(Theme::Nord.colors().borders.top_left, '╭'); // Rounded
        assert_eq!(Theme::Claude.colors().borders.top_left, '╭'); // Rounded
        assert_eq!(Theme::ChatGpt.colors().borders.top_left, '╭'); // Rounded
    }

    #[test]
    fn theme_has_indicators() {
        let colors = Theme::Coral.colors();
        assert!(!colors.indicator_rec.is_empty());
        assert!(!colors.indicator_auto.is_empty());
    }
}
