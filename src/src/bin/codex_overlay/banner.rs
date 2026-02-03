//! Startup banner for VoxTerm.
//!
//! Displays version and configuration info on startup.

use crate::theme::Theme;

/// Version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// ASCII art logo for VoxTerm - displayed on startup
const ASCII_LOGO: &[&str] = &[
    " ██╗   ██╗ ██████╗ ██╗  ██╗████████╗███████╗██████╗ ███╗   ███╗",
    " ██║   ██║██╔═══██╗╚██╗██╔╝╚══██╔══╝██╔════╝██╔══██╗████╗ ████║",
    " ██║   ██║██║   ██║ ╚███╔╝    ██║   █████╗  ██████╔╝██╔████╔██║",
    " ╚██╗ ██╔╝██║   ██║ ██╔██╗    ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║",
    "  ╚████╔╝ ╚██████╔╝██╔╝ ██╗   ██║   ███████╗██║  ██║██║ ╚═╝ ██║",
    "   ╚═══╝   ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝",
];

/// Purple gradient colors for shiny effect (light to deep purple)
const PURPLE_GRADIENT: &[(u8, u8, u8)] = &[
    (224, 176, 255), // Light lavender
    (200, 162, 255), // Soft purple
    (187, 154, 247), // Bright purple (TokyoNight)
    (157, 124, 216), // Medium purple
    (138, 106, 196), // Deep purple
    (118, 88, 176),  // Rich purple
];

/// Format RGB color as ANSI truecolor foreground code
fn rgb_fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Format the shiny purple ASCII art banner
pub fn format_ascii_banner(use_color: bool) -> String {
    let reset = "\x1b[0m";
    let mut output = String::new();
    output.push('\n');

    for (i, line) in ASCII_LOGO.iter().enumerate() {
        if use_color {
            let (r, g, b) = PURPLE_GRADIENT[i % PURPLE_GRADIENT.len()];
            output.push_str(&rgb_fg(r, g, b));
            output.push_str(line);
            output.push_str(reset);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    output.push('\n');
    output
}

/// Configuration to display in banner.
pub struct BannerConfig {
    /// Whether auto-voice is enabled
    pub auto_voice: bool,
    /// Current theme name
    pub theme: String,
    /// Pipeline in use (Rust or Python)
    pub pipeline: String,
    /// Microphone sensitivity in dB
    pub sensitivity_db: f32,
    /// Backend CLI name (e.g., "claude", "gemini", "aider")
    pub backend: String,
}

impl Default for BannerConfig {
    fn default() -> Self {
        Self {
            auto_voice: false,
            theme: "coral".to_string(),
            pipeline: "Rust".to_string(),
            sensitivity_db: -35.0,
            backend: "codex".to_string(),
        }
    }
}

/// Format a compact startup banner.
pub fn format_startup_banner(config: &BannerConfig, theme: Theme) -> String {
    let colors = theme.colors();

    let auto_voice_status = if config.auto_voice {
        format!("{}on{}", colors.success, colors.reset)
    } else {
        format!("{}off{}", colors.warning, colors.reset)
    };

    format!(
        "{}VoxTerm{} v{} │ {} │ {} │ theme: {} │ auto-voice: {} │ {:.0}dB\n",
        colors.info,
        colors.reset,
        VERSION,
        config.backend,
        config.pipeline,
        config.theme,
        auto_voice_status,
        config.sensitivity_db
    )
}

/// Format a minimal one-line banner.
pub fn format_minimal_banner(theme: Theme) -> String {
    let colors = theme.colors();
    format!(
        "{}VoxTerm{} v{} │ Ctrl+R rec │ Ctrl+V auto │ Ctrl+Q quit\n",
        colors.info, colors.reset, VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_defined() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn format_startup_banner_contains_version() {
        let config = BannerConfig::default();
        let banner = format_startup_banner(&config, Theme::Coral);
        assert!(banner.contains(VERSION));
        assert!(banner.contains("VoxTerm"));
    }

    #[test]
    fn format_startup_banner_shows_config() {
        let config = BannerConfig {
            auto_voice: true,
            theme: "catppuccin".to_string(),
            pipeline: "Rust".to_string(),
            sensitivity_db: -40.0,
            backend: "gemini".to_string(),
        };
        let banner = format_startup_banner(&config, Theme::Coral);
        assert!(banner.contains("Rust"));
        assert!(banner.contains("-40dB"));
        assert!(banner.contains("on")); // auto-voice on
        assert!(banner.contains("gemini")); // backend shown
    }

    #[test]
    fn format_minimal_banner_contains_shortcuts() {
        let banner = format_minimal_banner(Theme::Coral);
        assert!(banner.contains("Ctrl+R"));
        assert!(banner.contains("Ctrl+V"));
        assert!(banner.contains("Ctrl+Q"));
    }

    #[test]
    fn banner_no_color() {
        let config = BannerConfig::default();
        let banner = format_startup_banner(&config, Theme::None);
        assert!(banner.contains("VoxTerm"));
        // No color codes
        assert!(!banner.contains("\x1b[9"));
    }

    #[test]
    fn ascii_banner_contains_logo() {
        let banner = format_ascii_banner(false);
        assert!(banner.contains("██╗"));
        assert!(banner.contains("╚═╝"));
    }

    #[test]
    fn ascii_banner_with_color_has_ansi_codes() {
        let banner = format_ascii_banner(true);
        // Should contain truecolor ANSI codes
        assert!(banner.contains("\x1b[38;2;"));
        // Should contain reset codes
        assert!(banner.contains("\x1b[0m"));
    }

    #[test]
    fn ascii_banner_no_color_is_plain() {
        let banner = format_ascii_banner(false);
        // Should NOT contain any ANSI codes
        assert!(!banner.contains("\x1b["));
    }

    #[test]
    fn purple_gradient_has_six_colors() {
        assert_eq!(PURPLE_GRADIENT.len(), 6);
    }
}
