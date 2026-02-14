//! Audio-meter module so mic level feedback stays consistent across HUD views.
//!
//! Provides visual representation of audio levels for mic meter and real-time display.

mod format;
mod measure;
mod recommend;

use anyhow::Result;
use std::io::{self, Write};
use std::time::Duration;
use voiceterm::audio::Recorder;
use voiceterm::config::AppConfig;

use crate::theme::Theme;

#[allow(unused_imports)]
pub use format::{
    format_level_compact, format_level_meter, format_mic_meter_display, format_waveform,
};

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

pub(crate) fn run_mic_meter(config: &AppConfig, theme: Theme) -> Result<()> {
    recommend::validate_sample_ms("ambient", config.mic_meter_ambient_ms)?;
    recommend::validate_sample_ms("speech", config.mic_meter_speech_ms)?;

    let recorder = Recorder::new(config.input_device.as_deref())?;
    println!("Mic meter using input device: {}", recorder.device_name());

    let ambient_ms = config.mic_meter_ambient_ms;
    let speech_ms = config.mic_meter_speech_ms;

    println!(
        "Sampling ambient noise for {:.1}s... stay quiet.",
        ambient_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let ambient = measure::measure(&recorder, Duration::from_millis(ambient_ms))?;

    println!(
        "Sampling speech for {:.1}s... speak normally.",
        speech_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let speech = measure::measure(&recorder, Duration::from_millis(speech_ms))?;

    let (suggested, warning) = recommend::recommend_threshold(ambient.rms_db, speech.rms_db);
    println!();
    println!(
        "{}",
        format_mic_meter_display(ambient, Some(speech), suggested, theme)
    );
    println!("\nSuggested --voice-vad-threshold-db: {suggested:.1}");
    println!("Example: voiceterm --voice-vad-threshold-db {suggested:.1}");
    if let Some(message) = warning {
        println!("Note: {message}");
    }

    Ok(())
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
}
