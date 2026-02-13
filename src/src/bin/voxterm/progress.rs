//! Progress rendering utilities so long tasks remain visible without blocking output.
//!
//! Provides visual progress indicators for long-running operations.

#![allow(dead_code)]

use crate::theme::{Theme, ThemeColors};

/// Progress bar style.
#[derive(Debug, Clone, Copy, Default)]
pub enum ProgressStyle {
    /// Standard bar: [████░░░░░░]
    #[default]
    Bar,
    /// Compact bar: ████░░░░
    Compact,
    /// Blocks: ▓▓▓▓░░░░
    Blocks,
}

/// Progress bar configuration.
#[derive(Debug, Clone)]
pub struct ProgressConfig {
    /// Width in characters
    pub width: usize,
    /// Style of the progress bar
    pub style: ProgressStyle,
    /// Show percentage
    pub show_percent: bool,
    /// Show ETA
    pub show_eta: bool,
}

impl Default for ProgressConfig {
    fn default() -> Self {
        Self {
            width: 30,
            style: ProgressStyle::Bar,
            show_percent: true,
            show_eta: false,
        }
    }
}

/// Progress state.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// Current progress (0.0 to 1.0)
    pub progress: f32,
    /// Optional message
    pub message: String,
    /// Elapsed time in seconds (for ETA calculation)
    pub elapsed_secs: f32,
}

impl Progress {
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            ..Default::default()
        }
    }

    pub fn with_message(mut self, message: &str) -> Self {
        self.message = message.to_string();
        self
    }

    /// Calculate estimated time remaining.
    pub fn eta_secs(&self) -> Option<f32> {
        if self.progress > 0.0 && self.progress < 1.0 && self.elapsed_secs > 0.0 {
            let remaining = 1.0 - self.progress;
            let rate = self.progress / self.elapsed_secs;
            Some(remaining / rate)
        } else {
            None
        }
    }
}

/// Bar characters.
const BAR_FILLED: char = '█';
const BAR_EMPTY: char = '░';
const BAR_PARTIAL: &[char] = &['░', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Format a progress bar.
pub fn format_progress_bar(progress: &Progress, config: &ProgressConfig, theme: Theme) -> String {
    let colors = theme.colors();
    let ratio = progress.progress.clamp(0.0, 1.0);

    // Build the bar
    let bar = match config.style {
        ProgressStyle::Bar => format_bar_standard(ratio, config.width, &colors),
        ProgressStyle::Compact => format_bar_compact(ratio, config.width, &colors),
        ProgressStyle::Blocks => format_bar_blocks(ratio, config.width, &colors),
    };

    // Build suffix parts
    let mut suffix = String::new();

    if config.show_percent {
        suffix.push_str(&format!(" {:>3.0}%", ratio * 100.0));
    }

    if config.show_eta {
        if let Some(eta) = progress.eta_secs() {
            suffix.push_str(&format!(" ({})", format_duration(eta)));
        }
    }

    if !progress.message.is_empty() {
        if suffix.is_empty() {
            suffix.push(' ');
        } else {
            suffix.push_str(" - ");
        }
        suffix.push_str(&progress.message);
    }

    format!("{}{}", bar, suffix)
}

fn format_bar_standard(ratio: f32, width: usize, colors: &ThemeColors) -> String {
    let filled = (ratio * width as f32) as usize;
    let empty = width - filled;

    format!(
        "[{}{}{}{}]",
        colors.success,
        BAR_FILLED.to_string().repeat(filled),
        BAR_EMPTY.to_string().repeat(empty),
        colors.reset
    )
}

fn format_bar_compact(ratio: f32, width: usize, colors: &ThemeColors) -> String {
    let filled = (ratio * width as f32) as usize;
    let partial_idx = ((ratio * width as f32).fract() * (BAR_PARTIAL.len() - 1) as f32) as usize;
    let empty = width.saturating_sub(filled + 1);

    let mut bar = String::new();
    bar.push_str(colors.success);
    bar.push_str(&BAR_FILLED.to_string().repeat(filled));

    if filled < width {
        bar.push(BAR_PARTIAL[partial_idx]);
        bar.push_str(&BAR_EMPTY.to_string().repeat(empty));
    }

    bar.push_str(colors.reset);
    bar
}

fn format_bar_blocks(ratio: f32, width: usize, colors: &ThemeColors) -> String {
    let filled = (ratio * width as f32) as usize;
    let empty = width - filled;

    format!(
        "{}{}{}{}",
        colors.info,
        "▓".repeat(filled),
        "░".repeat(empty),
        colors.reset
    )
}

fn format_duration(secs: f32) -> String {
    if secs < 60.0 {
        format!("{:.0}s", secs)
    } else if secs < 3600.0 {
        format!("{}m {}s", (secs / 60.0) as u32, (secs % 60.0) as u32)
    } else {
        format!(
            "{}h {}m",
            (secs / 3600.0) as u32,
            ((secs % 3600.0) / 60.0) as u32
        )
    }
}

/// Indeterminate progress spinner frames.
pub const SPINNER_BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_DOTS: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
pub const SPINNER_LINE: &[&str] = &["-", "\\", "|", "/"];

/// Format an indeterminate spinner.
pub fn format_spinner(frame: usize, message: &str, theme: Theme) -> String {
    let colors = theme.colors();
    let spinner = SPINNER_BRAILLE[frame % SPINNER_BRAILLE.len()];

    format!(
        "{}{}{} {}",
        colors.processing, spinner, colors.reset, message
    )
}

/// Bouncing bar for indeterminate progress.
pub fn format_bouncing_bar(frame: usize, width: usize, theme: Theme) -> String {
    let colors = theme.colors();
    let bar_width = 4;
    let travel = width.saturating_sub(bar_width);

    // Calculate position (bounces back and forth)
    let cycle = frame % (travel * 2);
    let pos = if cycle < travel {
        cycle
    } else {
        travel * 2 - cycle
    };

    let mut bar = String::new();
    bar.push('[');
    bar.push_str(&" ".repeat(pos));
    bar.push_str(colors.info);
    bar.push_str(&"=".repeat(bar_width));
    bar.push_str(colors.reset);
    bar.push_str(&" ".repeat(travel - pos));
    bar.push(']');

    bar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_new() {
        let p = Progress::new(0.5);
        assert_eq!(p.progress, 0.5);
        assert!(p.message.is_empty());
    }

    #[test]
    fn progress_clamps() {
        let p = Progress::new(1.5);
        assert_eq!(p.progress, 1.0);

        let p = Progress::new(-0.5);
        assert_eq!(p.progress, 0.0);
    }

    #[test]
    fn progress_eta() {
        let mut p = Progress::new(0.5);
        p.elapsed_secs = 10.0;
        let eta = p.eta_secs().unwrap();
        assert!((eta - 10.0).abs() < 0.1); // Should be ~10 seconds remaining
    }

    #[test]
    fn format_progress_bar_zero() {
        let p = Progress::new(0.0);
        let config = ProgressConfig {
            width: 10,
            show_percent: true,
            ..Default::default()
        };
        let bar = format_progress_bar(&p, &config, Theme::None);
        assert!(bar.contains("0%"));
    }

    #[test]
    fn format_progress_bar_full() {
        let p = Progress::new(1.0);
        let config = ProgressConfig {
            width: 10,
            show_percent: true,
            ..Default::default()
        };
        let bar = format_progress_bar(&p, &config, Theme::None);
        assert!(bar.contains("100%"));
    }

    #[test]
    fn format_progress_bar_with_message() {
        let p = Progress::new(0.5).with_message("Downloading...");
        let config = ProgressConfig::default();
        let bar = format_progress_bar(&p, &config, Theme::None);
        assert!(bar.contains("Downloading"));
    }

    #[test]
    fn format_spinner_cycles() {
        let s1 = format_spinner(0, "Loading", Theme::None);
        let s2 = format_spinner(1, "Loading", Theme::None);
        assert_ne!(s1, s2); // Different frames
        assert!(s1.contains("Loading"));
    }

    #[test]
    fn format_bouncing_bar_moves() {
        let b1 = format_bouncing_bar(0, 20, Theme::None);
        let b2 = format_bouncing_bar(5, 20, Theme::None);
        assert_ne!(b1, b2); // Bar should move
    }

    #[test]
    fn spinner_frames_defined() {
        assert!(!SPINNER_BRAILLE.is_empty());
        assert!(!SPINNER_DOTS.is_empty());
        assert!(!SPINNER_LINE.is_empty());
    }
}
