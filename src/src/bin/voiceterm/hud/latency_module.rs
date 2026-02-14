//! HUD latency module so recent transcription latency remains visible to users.
//!
//! Shows the last transcription latency: "◷ 1.2s"

use super::{HudModule, HudState};

/// Latency indicator module showing last transcription time.
pub struct LatencyModule;

impl LatencyModule {
    /// Create a new latency module.
    pub fn new() -> Self {
        Self
    }

    fn latency_char(ms: u32) -> char {
        match ms {
            0..=150 => '▁',
            151..=300 => '▂',
            301..=500 => '▄',
            501..=800 => '▆',
            _ => '█',
        }
    }

    fn render_sparkline(history: &[u32], width: usize) -> String {
        if history.is_empty() || width == 0 {
            return String::new();
        }
        let mut out = String::with_capacity(width);
        let start = history.len().saturating_sub(width);
        let slice = &history[start..];
        if slice.len() < width {
            out.push_str(&"▁".repeat(width - slice.len()));
        }
        for sample in slice {
            out.push(Self::latency_char(*sample));
        }
        out
    }
}

impl Default for LatencyModule {
    fn default() -> Self {
        Self::new()
    }
}

impl HudModule for LatencyModule {
    fn id(&self) -> &'static str {
        "latency"
    }

    fn render(&self, state: &HudState, max_width: usize) -> String {
        if max_width < self.min_width() {
            return String::new();
        }

        match state.last_latency_ms {
            Some(ms) => {
                let secs = ms as f32 / 1000.0;
                let trend = if state.latency_history_ms.is_empty() {
                    Self::latency_char(ms).to_string()
                } else {
                    Self::render_sparkline(&state.latency_history_ms, 4)
                };
                let full = if secs >= 10.0 {
                    format!("◷ {:.0}s {trend}", secs)
                } else {
                    format!("◷ {:.1}s {trend}", secs)
                };

                if full.chars().count() <= max_width {
                    full
                } else if max_width >= 7 {
                    // Compact format with decimal precision + single trend marker.
                    format!("◷{:.1}s{}", secs, Self::latency_char(ms))
                } else if max_width >= 6 {
                    // Ultra compact format without decimal + trend marker.
                    format!("◷{:.0}s{}", secs, Self::latency_char(ms))
                } else if max_width >= 4 {
                    format!("◷{:.0}s", secs)
                } else {
                    String::new()
                }
            }
            None => String::new(),
        }
    }

    fn min_width(&self) -> usize {
        // Minimum: "◷ --" = 4 chars
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_module_id() {
        let module = LatencyModule::new();
        assert_eq!(module.id(), "latency");
    }

    #[test]
    fn latency_module_min_width() {
        let module = LatencyModule::new();
        assert_eq!(module.min_width(), 4);
    }

    #[test]
    fn latency_module_tick_interval() {
        let module = LatencyModule::new();
        // Latency is event-driven, no tick
        assert!(module.tick_interval().is_none());
    }

    #[test]
    fn latency_module_render_no_data() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: None,
            ..Default::default()
        };
        let output = module.render(&state, 10);
        assert!(output.is_empty());
    }

    #[test]
    fn latency_module_render_with_data() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(1200),
            latency_history_ms: vec![220, 350, 900, 1200],
            ..Default::default()
        };
        let output = module.render(&state, 10);
        assert!(output.contains("◷"));
        assert!(output.contains("1.2s") || output.contains("1s"));
    }

    #[test]
    fn latency_module_render_fast() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(500),
            latency_history_ms: vec![100, 180, 250, 500],
            ..Default::default()
        };
        let output = module.render(&state, 10);
        assert!(output.contains("0.5s") || output.contains("1s"));
    }

    #[test]
    fn latency_module_render_slow() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(15000),
            latency_history_ms: vec![700, 900, 1200, 15000],
            ..Default::default()
        };
        let output = module.render(&state, 10);
        // Should show whole seconds for 10+
        assert!(output.contains("15s"));
    }

    #[test]
    fn latency_module_render_narrow() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(1200),
            ..Default::default()
        };
        // Too narrow
        let output = module.render(&state, 3);
        assert!(output.is_empty());
    }

    #[test]
    fn latency_module_render_compact() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(1200),
            latency_history_ms: vec![200, 350, 500, 1200],
            ..Default::default()
        };
        // Just enough for compact+trend
        let output = module.render(&state, 6);
        assert!(output.contains("◷"));
        assert!(output.contains("s"));
    }

    #[test]
    fn latency_module_uses_history_sparkline() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(800),
            latency_history_ms: vec![120, 220, 450, 820],
            ..Default::default()
        };
        let output = module.render(&state, 16);
        assert!(output.contains('▁') || output.contains('▂') || output.contains('▄'));
    }
}
