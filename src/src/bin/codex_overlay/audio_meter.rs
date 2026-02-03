//! Visual audio level meter with ASCII bars.
//!
//! Provides visual representation of audio levels for mic meter and real-time display.

use anyhow::{anyhow, Result};
use std::io::{self, Write};
use std::time::Duration;
use voxterm::audio::Recorder;
use voxterm::config::{AppConfig, MAX_MIC_METER_SAMPLE_MS, MIN_MIC_METER_SAMPLE_MS};

use crate::theme::{Theme, ThemeColors};

/// Audio level in decibels.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioLevel {
    /// RMS level in dBFS
    pub rms_db: f32,
    /// Peak level in dBFS
    pub peak_db: f32,
}

const RECOMMENDED_FLOOR_DB: f32 = -80.0;
const RECOMMENDED_CEILING_DB: f32 = -10.0;

/// Meter display configuration.
#[derive(Debug, Clone, Copy)]
pub struct MeterConfig {
    /// Minimum dB value (left edge)
    pub min_db: f32,
    /// Maximum dB value (right edge)
    pub max_db: f32,
    /// Width in characters
    pub width: usize,
    /// Show peak indicator
    pub show_peak: bool,
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            min_db: -60.0,
            max_db: 0.0,
            width: 30,
            show_peak: true,
        }
    }
}

/// Characters for the meter bar.
const BAR_FULL: char = '█';
#[allow(dead_code)]
const BAR_HALF: char = '▌';
const BAR_EMPTY: char = '░';
const PEAK_MARKER: char = '│';

#[inline]
fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return RECOMMENDED_FLOOR_DB;
    }
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = energy.sqrt().max(1e-6);
    20.0 * rms.log10()
}

#[inline]
fn peak_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return RECOMMENDED_FLOOR_DB;
    }
    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    20.0 * peak.log10()
}

fn measure(recorder: &Recorder, duration: Duration) -> Result<AudioLevel> {
    let samples = recorder.record_for(duration)?;
    Ok(AudioLevel {
        rms_db: rms_db(&samples),
        peak_db: peak_db(&samples),
    })
}

fn recommend_threshold(ambient_db: f32, speech_db: f32) -> (f32, Option<&'static str>) {
    if speech_db <= ambient_db {
        let suggested = (ambient_db + 1.0).clamp(RECOMMENDED_FLOOR_DB, RECOMMENDED_CEILING_DB);
        return (
            suggested,
            Some("Speech is not louder than ambient noise; results may be unreliable."),
        );
    }

    let margin = speech_db - ambient_db;
    let guard = if margin >= 12.0 {
        6.0
    } else if margin >= 6.0 {
        3.0
    } else {
        1.5
    };

    let mut suggested = ambient_db + guard;
    if suggested > speech_db - 1.0 {
        suggested = (ambient_db + speech_db) / 2.0;
    }

    let warning = if margin < 6.0 {
        Some("Speech is close to ambient noise; consider a quieter room or closer mic.")
    } else {
        None
    };

    (
        suggested.clamp(RECOMMENDED_FLOOR_DB, RECOMMENDED_CEILING_DB),
        warning,
    )
}

fn validate_sample_ms(label: &str, value: u64) -> Result<()> {
    if !(MIN_MIC_METER_SAMPLE_MS..=MAX_MIC_METER_SAMPLE_MS).contains(&value) {
        return Err(anyhow!(
            "--mic-meter-{label}-ms must be between {MIN_MIC_METER_SAMPLE_MS} and {MAX_MIC_METER_SAMPLE_MS} ms"
        ));
    }
    Ok(())
}

pub(crate) fn run_mic_meter(config: &AppConfig, theme: Theme) -> Result<()> {
    validate_sample_ms("ambient", config.mic_meter_ambient_ms)?;
    validate_sample_ms("speech", config.mic_meter_speech_ms)?;

    let recorder = Recorder::new(config.input_device.as_deref())?;
    println!("Mic meter using input device: {}", recorder.device_name());

    let ambient_ms = config.mic_meter_ambient_ms;
    let speech_ms = config.mic_meter_speech_ms;

    println!(
        "Sampling ambient noise for {:.1}s... stay quiet.",
        ambient_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let ambient = measure(&recorder, Duration::from_millis(ambient_ms))?;

    println!(
        "Sampling speech for {:.1}s... speak normally.",
        speech_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let speech = measure(&recorder, Duration::from_millis(speech_ms))?;

    let (suggested, warning) = recommend_threshold(ambient.rms_db, speech.rms_db);
    println!();
    println!(
        "{}",
        format_mic_meter_display(ambient, Some(speech), suggested, theme)
    );
    println!("\nSuggested --voice-vad-threshold-db: {suggested:.1}");
    println!("Example: voxterm --voice-vad-threshold-db {suggested:.1}");
    if let Some(message) = warning {
        println!("Note: {message}");
    }

    Ok(())
}

/// Format a horizontal audio level meter.
#[must_use]
pub fn format_level_meter(level: AudioLevel, config: &MeterConfig, theme: Theme) -> String {
    let colors = theme.colors();
    let range = config.max_db - config.min_db;

    // Calculate bar position (0.0 to 1.0)
    let rms_pos = ((level.rms_db - config.min_db) / range).clamp(0.0, 1.0);
    let peak_pos = ((level.peak_db - config.min_db) / range).clamp(0.0, 1.0);

    // Convert to character positions
    let rms_chars = (rms_pos * config.width as f32) as usize;
    let peak_char = (peak_pos * config.width as f32) as usize;

    let mut bar = String::new();

    for i in 0..config.width {
        if i < rms_chars {
            // Filled portion - color based on level
            let color = level_color(i, config.width, &colors);
            bar.push_str(color);
            bar.push(BAR_FULL);
            bar.push_str(colors.reset);
        } else if config.show_peak && i == peak_char && peak_char > rms_chars {
            // Peak marker
            bar.push_str(colors.warning);
            bar.push(PEAK_MARKER);
            bar.push_str(colors.reset);
        } else {
            // Empty portion
            bar.push(BAR_EMPTY);
        }
    }

    bar
}

/// Get color for a position in the meter (green -> yellow -> red).
#[inline]
fn level_color(pos: usize, width: usize, colors: &ThemeColors) -> &str {
    let ratio = pos as f32 / width as f32;
    if ratio < 0.6 {
        colors.success // Green - safe level
    } else if ratio < 0.85 {
        colors.warning // Yellow - getting loud
    } else {
        colors.error // Red - too loud / clipping
    }
}

/// Format a compact level display with dB value.
#[allow(dead_code)]
pub fn format_level_compact(level: AudioLevel, theme: Theme) -> String {
    let colors = theme.colors();
    let config = MeterConfig {
        width: 15,
        ..Default::default()
    };
    let bar = format_level_meter(level, &config, theme);
    format!(
        "{} {}{:>5.0}dB{}",
        bar, colors.info, level.rms_db, colors.reset
    )
}

/// Format the mic meter calibration display.
#[must_use]
pub fn format_mic_meter_display(
    ambient: AudioLevel,
    speech: Option<AudioLevel>,
    suggested_threshold: f32,
    theme: Theme,
) -> String {
    let colors = theme.colors();
    let config = MeterConfig::default();
    let mut lines = Vec::new();

    lines.push(format!(
        "{}Microphone Calibration{}",
        colors.info, colors.reset
    ));
    lines.push(String::new());

    // Ambient level
    let ambient_bar = format_level_meter(ambient, &config, theme);
    lines.push(format!(
        "Ambient  {} {:>5.1}dB",
        ambient_bar, ambient.rms_db
    ));

    // Speech level (if available)
    if let Some(speech) = speech {
        let speech_bar = format_level_meter(speech, &config, theme);
        lines.push(format!("Speech   {} {:>5.1}dB", speech_bar, speech.rms_db));
    }

    lines.push(String::new());

    // Threshold indicator
    let threshold_pos =
        ((suggested_threshold - config.min_db) / (config.max_db - config.min_db)).clamp(0.0, 1.0);
    let threshold_char = (threshold_pos * config.width as f32) as usize;
    let mut threshold_line = " ".repeat(9); // "Ambient  " width
    threshold_line.push_str(&" ".repeat(threshold_char));
    threshold_line.push_str(&format!("{}▲{}", colors.info, colors.reset));
    lines.push(threshold_line);

    lines.push(format!(
        "{}Suggested threshold: {:.0}dB{}",
        colors.success, suggested_threshold, colors.reset
    ));

    // Scale
    lines.push(String::new());
    let scale_start = format!("{:.0}dB", config.min_db);
    let scale_end = format!("{:.0}dB", config.max_db);
    let scale_padding = config.width + 9 - scale_start.len() - scale_end.len();
    lines.push(format!(
        "{}{}{}{}",
        " ".repeat(9),
        scale_start,
        " ".repeat(scale_padding),
        scale_end
    ));

    lines.join("\n")
}

/// Waveform characters for real-time display.
#[allow(dead_code)]
const WAVEFORM_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Format a mini waveform from recent audio levels.
/// Uses iterator chains to avoid Vec allocations in the hot path.
#[allow(dead_code)]
#[must_use]
pub fn format_waveform(levels: &[f32], width: usize, theme: Theme) -> String {
    let colors = theme.colors();

    if levels.is_empty() {
        return " ".repeat(width);
    }

    let mut result = String::new();

    // Take the last `width` samples or pad with leading zeros
    // Uses iterator chain to avoid Vec allocation
    let start = levels.len().saturating_sub(width);
    let pad_count = width.saturating_sub(levels.len());
    let samples_iter =
        std::iter::repeat_n(0.0_f32, pad_count).chain(levels[start..].iter().copied());

    for level in samples_iter {
        // Convert dB to waveform character (assuming -60 to 0 range)
        let normalized = ((level + 60.0) / 60.0).clamp(0.0, 1.0);
        let char_idx = (normalized * (WAVEFORM_CHARS.len() - 1) as f32) as usize;
        let ch = WAVEFORM_CHARS[char_idx];

        // Color based on level
        let color = if normalized < 0.6 {
            colors.success
        } else if normalized < 0.85 {
            colors.warning
        } else {
            colors.error
        };

        result.push_str(color);
        result.push(ch);
        result.push_str(colors.reset);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_level_default() {
        let level = AudioLevel::default();
        assert_eq!(level.rms_db, 0.0);
        assert_eq!(level.peak_db, 0.0);
    }

    #[test]
    fn meter_config_default() {
        let config = MeterConfig::default();
        assert_eq!(config.min_db, -60.0);
        assert_eq!(config.max_db, 0.0);
        assert_eq!(config.width, 30);
    }

    #[test]
    fn format_level_meter_silent() {
        let level = AudioLevel {
            rms_db: -60.0,
            peak_db: -60.0,
        };
        let config = MeterConfig {
            width: 10,
            show_peak: false,
            ..Default::default()
        };
        let meter = format_level_meter(level, &config, Theme::None);
        // Should be all empty bars
        assert!(meter.contains(BAR_EMPTY));
        assert!(!meter.contains(BAR_FULL));
    }

    #[test]
    fn format_level_meter_loud() {
        let level = AudioLevel {
            rms_db: -10.0,
            peak_db: -5.0,
        };
        let config = MeterConfig {
            width: 10,
            show_peak: true,
            ..Default::default()
        };
        let meter = format_level_meter(level, &config, Theme::None);
        // Should have filled bars
        assert!(meter.contains(BAR_FULL));
    }

    #[test]
    fn format_level_compact_includes_db() {
        let level = AudioLevel {
            rms_db: -30.0,
            peak_db: -25.0,
        };
        let output = format_level_compact(level, Theme::None);
        assert!(output.contains("-30dB") || output.contains("-30"));
    }

    #[test]
    fn format_waveform_empty() {
        let waveform = format_waveform(&[], 5, Theme::None);
        assert_eq!(waveform.len(), 5);
    }

    #[test]
    fn format_waveform_with_levels() {
        let levels = vec![-40.0, -30.0, -20.0, -10.0];
        let waveform = format_waveform(&levels, 4, Theme::None);
        // Should contain waveform characters
        let has_waveform = WAVEFORM_CHARS.iter().any(|&c| waveform.contains(c));
        assert!(has_waveform);
    }

    #[test]
    fn format_mic_meter_display_basic() {
        let ambient = AudioLevel {
            rms_db: -45.0,
            peak_db: -38.0,
        };
        let output = format_mic_meter_display(ambient, None, -35.0, Theme::Coral);
        assert!(output.contains("Microphone Calibration"));
        assert!(output.contains("Ambient"));
        assert!(output.contains("-35"));
    }
}
