//! Startup banner for Codex Voice.
//!
//! Displays version and configuration info on startup.

use crate::theme::Theme;

/// Version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
        "{}Codex Voice{} v{} │ {} │ {} │ theme: {} │ auto-voice: {} │ {:.0}dB\n",
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
        "{}Codex Voice{} v{} │ Ctrl+R rec │ Ctrl+V auto │ Ctrl+Q quit\n",
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
        assert!(banner.contains("Codex Voice"));
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
        assert!(banner.contains("Codex Voice"));
        // No color codes
        assert!(!banner.contains("\x1b[9"));
    }
}
