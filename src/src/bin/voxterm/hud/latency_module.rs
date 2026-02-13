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
                let full = if secs >= 10.0 {
                    format!("◷ {:.0}s", secs)
                } else {
                    format!("◷ {:.1}s", secs)
                };

                if full.chars().count() <= max_width {
                    full
                } else if max_width >= 4 {
                    // Compact format without decimal
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
            ..Default::default()
        };
        let output = module.render(&state, 10);
        assert!(output.contains("◷"));
        assert!(output.contains("1.2s"));
    }

    #[test]
    fn latency_module_render_fast() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(500),
            ..Default::default()
        };
        let output = module.render(&state, 10);
        assert!(output.contains("0.5s"));
    }

    #[test]
    fn latency_module_render_slow() {
        let module = LatencyModule::new();
        let state = HudState {
            last_latency_ms: Some(15000),
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
            ..Default::default()
        };
        // Just enough for compact
        let output = module.render(&state, 4);
        assert!(output.contains("◷"));
        assert!(output.contains("s"));
    }
}
