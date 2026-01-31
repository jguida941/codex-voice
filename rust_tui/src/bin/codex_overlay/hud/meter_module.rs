//! Audio meter HUD module.
//!
//! Shows the current audio level with a visual waveform: "-40dB ▁▂▃▅▆"

use super::{HudModule, HudState};
use std::time::Duration;

/// Waveform characters for visualization.
const WAVEFORM_CHARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Audio meter module showing current audio level.
pub struct MeterModule {
    /// Number of waveform bars to show.
    bar_count: usize,
}

impl MeterModule {
    /// Create a new meter module with default settings.
    pub fn new() -> Self {
        Self { bar_count: 6 }
    }

    /// Create a meter module with a specific bar count.
    #[allow(dead_code)]
    pub fn with_bar_count(bar_count: usize) -> Self {
        Self {
            bar_count: bar_count.max(1),
        }
    }

    /// Convert a dB level to a waveform character.
    fn db_to_char(db: f32) -> char {
        // Map -60dB to 0dB to waveform characters
        let normalized = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        let idx = (normalized * (WAVEFORM_CHARS.len() - 1) as f32) as usize;
        WAVEFORM_CHARS[idx]
    }
}

impl Default for MeterModule {
    fn default() -> Self {
        Self::new()
    }
}

impl HudModule for MeterModule {
    fn id(&self) -> &'static str {
        "meter"
    }

    fn render(&self, state: &HudState, max_width: usize) -> String {
        if max_width < self.min_width() {
            return String::new();
        }

        // Only show meter when recording
        if !state.is_recording {
            return String::new();
        }

        let db = state.audio_level_db;
        let db_str = format!("{:>3.0}dB", db);

        // Generate waveform bars
        let waveform_char = Self::db_to_char(db);
        let waveform: String = std::iter::repeat_n(waveform_char, self.bar_count).collect();

        let full = format!("{} {}", db_str, waveform);
        if full.chars().count() <= max_width {
            full
        } else if max_width >= 5 {
            // Just the dB value
            db_str
        } else {
            String::new()
        }
    }

    fn min_width(&self) -> usize {
        // Minimum: "-40dB" = 5 chars
        5
    }

    fn tick_interval(&self) -> Option<Duration> {
        // Update meter at ~12fps for smooth animation
        Some(Duration::from_millis(80))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_module_id() {
        let module = MeterModule::new();
        assert_eq!(module.id(), "meter");
    }

    #[test]
    fn meter_module_min_width() {
        let module = MeterModule::new();
        assert_eq!(module.min_width(), 5);
    }

    #[test]
    fn meter_module_tick_interval() {
        let module = MeterModule::new();
        assert!(module.tick_interval().is_some());
        assert_eq!(module.tick_interval(), Some(Duration::from_millis(80)));
    }

    #[test]
    fn meter_module_render_not_recording() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: false,
            audio_level_db: -30.0,
            ..Default::default()
        };
        let output = module.render(&state, 20);
        assert!(output.is_empty());
    }

    #[test]
    fn meter_module_render_recording() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: true,
            audio_level_db: -30.0,
            ..Default::default()
        };
        let output = module.render(&state, 20);
        assert!(output.contains("-30dB") || output.contains("30dB"));
        // Should have waveform chars
        let has_waveform = WAVEFORM_CHARS.iter().any(|&c| output.contains(c));
        assert!(has_waveform);
    }

    #[test]
    fn meter_module_render_loud() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: true,
            audio_level_db: -10.0,
            ..Default::default()
        };
        let output = module.render(&state, 20);
        // Loud signal should show higher bars
        assert!(output.contains('█') || output.contains('▇') || output.contains('▆'));
    }

    #[test]
    fn meter_module_render_quiet() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: true,
            audio_level_db: -55.0,
            ..Default::default()
        };
        let output = module.render(&state, 20);
        // Quiet signal should show lower bars
        assert!(output.contains('▁') || output.contains('▂'));
    }

    #[test]
    fn meter_module_render_narrow() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: true,
            audio_level_db: -30.0,
            ..Default::default()
        };
        // Too narrow
        let output = module.render(&state, 4);
        assert!(output.is_empty());
    }

    #[test]
    fn meter_module_render_just_db() {
        let module = MeterModule::new();
        let state = HudState {
            is_recording: true,
            audio_level_db: -30.0,
            ..Default::default()
        };
        // Just enough for dB
        let output = module.render(&state, 5);
        assert!(output.contains("dB"));
    }

    #[test]
    fn db_to_char_range() {
        // Test the full range
        assert_eq!(MeterModule::db_to_char(-60.0), '▁');
        assert_eq!(MeterModule::db_to_char(0.0), '█');
        // Middle value
        let mid = MeterModule::db_to_char(-30.0);
        assert!(WAVEFORM_CHARS.contains(&mid));
    }

    #[test]
    fn custom_bar_count() {
        let module = MeterModule::with_bar_count(10);
        let state = HudState {
            is_recording: true,
            audio_level_db: -30.0,
            ..Default::default()
        };
        let output = module.render(&state, 30);
        // Count waveform chars (excluding dB label)
        let waveform_count = output.chars().filter(|c| WAVEFORM_CHARS.contains(c)).count();
        assert_eq!(waveform_count, 10);
    }
}
