//! Theme resolution policy so color mode, flags, and backend defaults agree.

use crate::color_mode::ColorMode;
use crate::config::cli::OverlayConfig;
use crate::theme::Theme;

impl OverlayConfig {
    /// Get the resolved theme, respecting --no-color/NO_COLOR and backend defaults.
    pub(crate) fn theme_for_backend(&self, backend_label: &str) -> Theme {
        if self.no_color || std::env::var("NO_COLOR").is_ok() {
            return Theme::None;
        }
        let requested = self
            .theme_name
            .as_deref()
            .and_then(Theme::from_name)
            .unwrap_or_else(|| default_theme_for_backend(backend_label));
        let mode = self.color_mode();
        if !mode.supports_color() {
            Theme::None
        } else if !mode.supports_truecolor() {
            requested.fallback_for_ansi()
        } else {
            requested
        }
    }

    /// Get the detected color mode for the terminal.
    pub(crate) fn color_mode(&self) -> ColorMode {
        if self.no_color {
            ColorMode::None
        } else {
            ColorMode::detect()
        }
    }
}

pub(crate) fn default_theme_for_backend(backend_label: &str) -> Theme {
    match backend_label.to_lowercase().as_str() {
        "claude" => Theme::Claude,
        "codex" => Theme::Codex,
        _ => Theme::Coral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::cli::OverlayConfig;
    use clap::Parser;
    use std::sync::{Mutex, OnceLock};

    static ENV_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

    fn with_truecolor_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev_colorterm = std::env::var("COLORTERM").ok();
        let prev_term = std::env::var("TERM").ok();
        let prev_no_color = std::env::var("NO_COLOR").ok();
        std::env::set_var("COLORTERM", "truecolor");
        std::env::set_var("TERM", "xterm-256color");
        std::env::remove_var("NO_COLOR");
        let result = f();
        match prev_colorterm {
            Some(value) => std::env::set_var("COLORTERM", value),
            None => std::env::remove_var("COLORTERM"),
        }
        match prev_term {
            Some(value) => std::env::set_var("TERM", value),
            None => std::env::remove_var("TERM"),
        }
        match prev_no_color {
            Some(value) => std::env::set_var("NO_COLOR", value),
            None => std::env::remove_var("NO_COLOR"),
        }
        result
    }

    #[test]
    fn default_theme_for_backend_maps_expected() {
        assert_eq!(default_theme_for_backend("claude"), Theme::Claude);
        assert_eq!(default_theme_for_backend("codex"), Theme::Codex);
        assert_eq!(default_theme_for_backend("custom"), Theme::Coral);
    }

    #[test]
    fn theme_for_backend_uses_backend_default_when_unset() {
        with_truecolor_env(|| {
            let config = OverlayConfig::parse_from(["test"]);
            assert!(config.theme_name.is_none());
            assert_eq!(config.theme_for_backend("claude"), Theme::Claude);
            assert_eq!(config.theme_for_backend("codex"), Theme::Codex);
            assert_eq!(config.theme_for_backend("custom"), Theme::Coral);
        });
    }
}
