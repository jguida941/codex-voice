//! Startup banner renderer that surfaces key runtime mode/settings at launch.
//!
//! Displays version and configuration info on startup.

use crate::theme::Theme;
use crossterm::terminal::size as terminal_size;
use std::io::{self, Write};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const STARTUP_SPLASH_CLEAR_MS: u64 = 10_000;

/// ASCII art logo for VoxTerm - displayed on startup.
const ASCII_LOGO: &[&str] = &[
    r"██╗   ██╗ ██████╗ ██╗  ██╗████████╗███████╗██████╗ ███╗   ███╗",
    r"██║   ██║██╔═══██╗╚██╗██╔╝╚══██╔══╝██╔════╝██╔══██╗████╗ ████║",
    r"██║   ██║██║   ██║ ╚███╔╝    ██║   █████╗  ██████╔╝██╔████╔██║",
    r"╚██╗ ██╔╝██║   ██║ ██╔██╗    ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║",
    r" ╚████╔╝ ╚██████╔╝██╔╝ ██╗   ██║   ███████╗██║  ██║██║ ╚═╝ ██║",
    r"  ╚═══╝   ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝",
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

/// Format the shiny purple ASCII art banner with tagline underneath.
pub fn format_ascii_banner(use_color: bool, terminal_width: u16) -> String {
    let reset = "\x1b[0m";
    let dim = "\x1b[90m";
    let mut output = String::new();
    output.push('\n');

    let logo_width = ASCII_LOGO
        .iter()
        .map(|line| UnicodeWidthStr::width(*line))
        .max()
        .unwrap_or(0);
    // Calculate padding to center the logo
    let padding = if (terminal_width as usize) > logo_width {
        (terminal_width as usize - logo_width) / 2
    } else {
        0
    };
    let pad_str: String = " ".repeat(padding);

    // Print the ASCII art logo with purple gradient
    for (i, line) in ASCII_LOGO.iter().enumerate() {
        output.push_str(&pad_str);
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

    // Add tagline underneath with shortcuts
    let tagline = format!(
        "v{} │ Ctrl+R record │ Ctrl+V auto-voice │ Ctrl+Q quit",
        VERSION
    );
    let tagline_padding = if (terminal_width as usize) > tagline.len() {
        (terminal_width as usize - tagline.len()) / 2
    } else {
        0
    };

    output.push('\n');
    output.push_str(&" ".repeat(tagline_padding));
    if use_color {
        output.push_str(dim);
        output.push_str(&tagline);
        output.push_str(reset);
    } else {
        output.push_str(&tagline);
    }
    output.push_str("\n\n");

    // Add "Initializing..." in golden yellow
    let init_text = "Initializing...";
    let init_padding = if (terminal_width as usize) > init_text.len() {
        (terminal_width as usize - init_text.len()) / 2
    } else {
        0
    };
    output.push_str(&" ".repeat(init_padding));
    if use_color {
        // Golden yellow color
        output.push_str(&rgb_fg(255, 200, 50));
        output.push_str(init_text);
        output.push_str(reset);
    } else {
        output.push_str(init_text);
    }
    output.push_str("\n\n");

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

    let shortcuts = format!(
        "{}Ctrl+R record │ Ctrl+V auto-voice │ Ctrl+Q quit{}",
        colors.dim, colors.reset
    );

    format!(
        "{}VoxTerm{} v{} │ {} │ {} │ theme: {} │ auto-voice: {} │ {:.0}dB\n{}\n",
        colors.info,
        colors.reset,
        VERSION,
        config.backend,
        config.pipeline,
        config.theme,
        auto_voice_status,
        config.sensitivity_db,
        shortcuts
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

pub(crate) fn should_skip_banner(no_startup_banner: bool) -> bool {
    no_startup_banner
}

fn use_minimal_banner(cols: u16) -> bool {
    cols < 60
}

fn build_startup_banner(config: &BannerConfig, theme: Theme) -> String {
    let use_color = theme != Theme::None;
    match terminal_size() {
        Ok((cols, _)) if cols >= 66 => format_ascii_banner(use_color, cols),
        Ok((cols, _)) if use_minimal_banner(cols) => format_minimal_banner(theme),
        _ => format_startup_banner(config, theme),
    }
}

pub(crate) fn show_startup_splash(config: &BannerConfig, theme: Theme) -> io::Result<()> {
    let banner = build_startup_banner(config, theme).replace('\n', "\r\n");
    let mut stdout = io::stdout();
    write!(stdout, "\x1b[2J\x1b[H")?;
    write!(stdout, "{banner}")?;
    stdout.flush()?;
    std::thread::sleep(Duration::from_millis(STARTUP_SPLASH_CLEAR_MS));
    write!(stdout, "\x1b[2J\x1b[H")?;
    stdout.flush()
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
    fn should_skip_banner_matches_flags() {
        assert!(!should_skip_banner(false));
        assert!(should_skip_banner(true));
    }

    #[test]
    fn use_minimal_banner_threshold() {
        assert!(use_minimal_banner(59));
        assert!(!use_minimal_banner(60));
    }

    #[test]
    fn startup_splash_min_duration_is_at_least_10s() {
        assert!(STARTUP_SPLASH_CLEAR_MS >= 10_000);
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
        let banner = format_ascii_banner(false, 80);
        assert!(banner.contains("██╗"));
        assert!(banner.contains("╚═╝"));
    }

    #[test]
    fn ascii_banner_with_color_has_ansi_codes() {
        let banner = format_ascii_banner(true, 80);
        // Should contain truecolor ANSI codes
        assert!(banner.contains("\x1b[38;2;"));
        // Should contain reset codes
        assert!(banner.contains("\x1b[0m"));
    }

    #[test]
    fn ascii_banner_no_color_is_plain() {
        let banner = format_ascii_banner(false, 80);
        // Should NOT contain any ANSI codes
        assert!(!banner.contains("\x1b["));
    }

    #[test]
    fn ascii_banner_contains_tagline() {
        let banner = format_ascii_banner(false, 80);
        assert!(banner.contains("Ctrl+R record"));
        assert!(banner.contains("Ctrl+V auto-voice"));
        assert!(banner.contains("Ctrl+Q quit"));
        assert!(banner.contains(VERSION));
    }

    #[test]
    fn ascii_banner_centers_with_wide_terminal() {
        let banner = format_ascii_banner(false, 120);
        // With 120 cols, there should be some leading spaces for centering
        let lines: Vec<&str> = banner.lines().collect();
        // Find a line with the logo (not empty)
        let logo_line = lines.iter().find(|l| l.contains("██")).unwrap();
        assert!(logo_line.starts_with(" ")); // Should have padding
    }

    #[test]
    fn purple_gradient_has_six_colors() {
        assert_eq!(PURPLE_GRADIENT.len(), 6);
    }
}
