//! Startup banner renderer that surfaces key runtime mode/settings at launch.
//!
//! Displays version and configuration info on startup.

use crate::theme::Theme;
use crossterm::terminal::size as terminal_size;
use std::env;
use std::io::{self, Write};
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_STARTUP_SPLASH_CLEAR_MS: u64 = 1_500;
const MAX_STARTUP_SPLASH_CLEAR_MS: u64 = 30_000;

/// ASCII art logo for VoiceTerm - displayed on startup.
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

fn centered_padding(terminal_width: u16, text: &str) -> usize {
    let width = UnicodeWidthStr::width(text);
    if (terminal_width as usize) > width {
        (terminal_width as usize - width) / 2
    } else {
        0
    }
}

fn splash_duration_ms() -> u64 {
    env::var("VOICETERM_STARTUP_SPLASH_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.min(MAX_STARTUP_SPLASH_CLEAR_MS))
        .unwrap_or(DEFAULT_STARTUP_SPLASH_CLEAR_MS)
}

fn clear_screen(stdout: &mut dyn Write) -> io::Result<()> {
    // Use both clear-screen and clear-scrollback sequences for IDE terminals.
    write!(stdout, "\x1b[0m\x1b[2J\x1b[3J\x1b[H")
}

/// Format the shiny purple ASCII art banner with tagline underneath.
pub fn format_ascii_banner(use_color: bool, terminal_width: u16) -> String {
    let reset = "\x1b[0m";
    let dim = "\x1b[90m";
    let mut output = String::new();

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
    let tagline_padding = centered_padding(terminal_width, &tagline);

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
    let init_padding = centered_padding(terminal_width, init_text);
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
        "{}VoiceTerm{} v{} │ {} │ {} │ theme: {} │ auto-voice: {} │ {:.0}dB\n{}\n",
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
        "{}VoiceTerm{} v{} │ Ctrl+R rec │ Ctrl+V auto │ Ctrl+Q quit\n",
        colors.info, colors.reset, VERSION
    )
}

fn is_jetbrains_terminal() -> bool {
    const HINT_KEYS: &[&str] = &[
        "PYCHARM_HOSTED",
        "JETBRAINS_IDE",
        "IDEA_INITIAL_DIRECTORY",
        "IDEA_INITIAL_PROJECT",
        "CLION_IDE",
        "WEBSTORM_IDE",
    ];

    for key in HINT_KEYS {
        if env::var(key).map(|v| !v.trim().is_empty()).unwrap_or(false) {
            return true;
        }
    }

    if let Ok(term_program) = env::var("TERM_PROGRAM") {
        let value = term_program.to_lowercase();
        if value.contains("jetbrains")
            || value.contains("jediterm")
            || value.contains("pycharm")
            || value.contains("intellij")
            || value.contains("idea")
        {
            return true;
        }
    }

    if let Ok(terminal_emulator) = env::var("TERMINAL_EMULATOR") {
        let value = terminal_emulator.to_lowercase();
        if value.contains("jetbrains") || value.contains("jediterm") {
            return true;
        }
    }

    false
}

pub(crate) fn should_skip_banner(no_startup_banner: bool) -> bool {
    no_startup_banner || is_jetbrains_terminal()
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
    // Render splash in the alternate screen so it always tears down cleanly.
    write!(stdout, "\x1b[?1049h")?;
    clear_screen(&mut stdout)?;
    write!(stdout, "{banner}")?;
    stdout.flush()?;
    std::thread::sleep(Duration::from_millis(splash_duration_ms()));
    clear_screen(&mut stdout)?;
    write!(stdout, "\x1b[?1049l")?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
        static ENV_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        f()
    }

    fn with_splash_env<T>(value: Option<&str>, f: impl FnOnce() -> T) -> T {
        with_env_lock(|| {
            let prev = std::env::var("VOICETERM_STARTUP_SPLASH_MS").ok();
            match value {
                Some(v) => std::env::set_var("VOICETERM_STARTUP_SPLASH_MS", v),
                None => std::env::remove_var("VOICETERM_STARTUP_SPLASH_MS"),
            }
            let result = f();
            match prev {
                Some(v) => std::env::set_var("VOICETERM_STARTUP_SPLASH_MS", v),
                None => std::env::remove_var("VOICETERM_STARTUP_SPLASH_MS"),
            }
            result
        })
    }

    #[test]
    fn version_defined() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn format_startup_banner_contains_version() {
        let config = BannerConfig::default();
        let banner = format_startup_banner(&config, Theme::Coral);
        assert!(banner.contains(VERSION));
        assert!(banner.contains("VoiceTerm"));
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
        with_env_lock(|| {
            let keys = [
                "PYCHARM_HOSTED",
                "JETBRAINS_IDE",
                "IDEA_INITIAL_DIRECTORY",
                "IDEA_INITIAL_PROJECT",
                "CLION_IDE",
                "WEBSTORM_IDE",
                "TERM_PROGRAM",
                "TERMINAL_EMULATOR",
            ];
            let prev: Vec<(String, Option<String>)> = keys
                .iter()
                .map(|key| ((*key).to_string(), std::env::var(key).ok()))
                .collect();
            for key in keys {
                std::env::remove_var(key);
            }

            assert!(!should_skip_banner(false));
            assert!(should_skip_banner(true));

            for (key, value) in prev {
                match value {
                    Some(v) => std::env::set_var(key, v),
                    None => std::env::remove_var(key),
                }
            }
        });
    }

    #[test]
    fn should_skip_banner_in_jetbrains_env() {
        with_env_lock(|| {
            let prev = std::env::var("PYCHARM_HOSTED").ok();
            std::env::set_var("PYCHARM_HOSTED", "1");
            assert!(should_skip_banner(false));
            match prev {
                Some(v) => std::env::set_var("PYCHARM_HOSTED", v),
                None => std::env::remove_var("PYCHARM_HOSTED"),
            }
        });
    }

    #[test]
    fn use_minimal_banner_threshold() {
        assert!(use_minimal_banner(59));
        assert!(!use_minimal_banner(60));
    }

    #[test]
    fn startup_splash_default_duration_is_short() {
        assert!(DEFAULT_STARTUP_SPLASH_CLEAR_MS <= 2_000);
    }

    #[test]
    fn splash_duration_honors_env_override() {
        with_splash_env(Some("900"), || {
            assert_eq!(splash_duration_ms(), 900);
        });
    }

    #[test]
    fn splash_duration_caps_large_env_override() {
        with_splash_env(Some("999999"), || {
            assert_eq!(splash_duration_ms(), MAX_STARTUP_SPLASH_CLEAR_MS);
        });
    }

    #[test]
    fn banner_no_color() {
        let config = BannerConfig::default();
        let banner = format_startup_banner(&config, Theme::None);
        assert!(banner.contains("VoiceTerm"));
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
    fn ascii_banner_has_no_leading_blank_line() {
        let banner = format_ascii_banner(false, 120);
        assert!(!banner.starts_with('\n'));
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
    fn ascii_banner_centers_tagline_using_display_width() {
        let width = 120;
        let banner = format_ascii_banner(false, width);
        let line = banner
            .lines()
            .find(|line| line.contains("Ctrl+R record"))
            .expect("tagline line should exist");
        let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
        let tagline = format!(
            "v{} │ Ctrl+R record │ Ctrl+V auto-voice │ Ctrl+Q quit",
            VERSION
        );
        let expected = centered_padding(width, &tagline);
        assert_eq!(leading_spaces, expected);
    }

    #[test]
    fn purple_gradient_has_six_colors() {
        assert_eq!(PURPLE_GRADIENT.len(), 6);
    }
}
