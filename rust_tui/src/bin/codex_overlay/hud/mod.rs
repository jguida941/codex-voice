//! HUD module system for the voice overlay.
//!
//! Provides a modular, composable architecture for rendering HUD elements
//! in the status line. Each module can render its own section and declare
//! update intervals.
mod latency_module;
mod meter_module;
mod mode_module;
mod queue_module;

pub use latency_module::LatencyModule;
pub use meter_module::MeterModule;
pub use mode_module::ModeModule;
pub use queue_module::QueueModule;

use std::time::Duration;

/// Voice mode for the HUD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Auto-voice mode (hands-free)
    Auto,
    /// Manual voice mode (push-to-talk)
    Manual,
    /// Insert mode (typing)
    #[default]
    Insert,
}

impl Mode {
    /// Short label for display.
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Manual => "MANUAL",
            Self::Insert => "INSERT",
        }
    }
}

/// State passed to HUD modules for rendering.
#[derive(Debug, Clone, Default)]
pub struct HudState {
    /// Current voice mode.
    pub mode: Mode,
    /// Whether currently recording.
    pub is_recording: bool,
    /// Recording duration in seconds (reserved for duration module).
    #[allow(dead_code)]
    pub recording_duration_secs: f32,
    /// Current audio level in dBFS.
    pub audio_level_db: f32,
    /// Number of pending transcripts in queue.
    pub queue_depth: usize,
    /// Last measured latency in milliseconds.
    pub last_latency_ms: Option<u32>,
    /// Name of the STT backend (reserved for model/status module).
    #[allow(dead_code)]
    pub backend_name: String,
}

/// Trait for HUD modules that render status information.
///
/// Each module is responsible for rendering a fixed-width section of the
/// status line. Modules can specify update intervals for periodic refreshes.
pub trait HudModule: Send + Sync {
    /// Unique identifier for this module.
    fn id(&self) -> &'static str;

    /// Render the module content to a fixed-width string.
    ///
    /// The returned string should fit within `max_width` characters.
    /// Implementations should handle truncation gracefully.
    fn render(&self, state: &HudState, max_width: usize) -> String;

    /// Minimum width this module needs to render meaningfully.
    fn min_width(&self) -> usize;

    /// Update interval for periodic refreshes.
    ///
    /// Returns `None` if the module is event-driven only and doesn't
    /// need periodic updates.
    fn tick_interval(&self) -> Option<Duration> {
        None
    }
}

/// Registry that holds enabled HUD modules and renders them.
pub struct HudRegistry {
    modules: Vec<Box<dyn HudModule>>,
}

impl HudRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
        }
    }

    /// Create a registry with default modules.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(ModeModule::new()));
        registry.register(Box::new(MeterModule::new()));
        registry.register(Box::new(LatencyModule::new()));
        registry.register(Box::new(QueueModule::new()));
        registry
    }

    /// Register a HUD module.
    pub fn register(&mut self, module: Box<dyn HudModule>) {
        self.modules.push(module);
    }

    /// Get a module by ID.
    #[allow(dead_code)]
    pub fn get(&self, id: &str) -> Option<&dyn HudModule> {
        self.modules
            .iter()
            .find(|m| m.id() == id)
            .map(|m| m.as_ref())
    }

    /// Number of registered modules.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    /// Check if registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Render all modules with a separator.
    ///
    /// Returns a string with all module outputs joined by the separator.
    /// Modules that don't fit within the available width are omitted.
    pub fn render_all(&self, state: &HudState, max_width: usize, separator: &str) -> String {
        if self.modules.is_empty() {
            return String::new();
        }

        let sep_width = separator.chars().count();
        let mut parts: Vec<String> = Vec::new();
        let mut remaining_width = max_width;

        for module in &self.modules {
            let min_w = module.min_width();

            // Check if we have room for this module plus separator
            let needed = if parts.is_empty() {
                min_w
            } else {
                min_w + sep_width
            };

            if remaining_width < needed {
                continue;
            }

            // Calculate available width for this module
            let available = if parts.is_empty() {
                remaining_width
            } else {
                remaining_width - sep_width
            };

            let rendered = module.render(state, available);
            let rendered_width = rendered.chars().count();

            if !rendered.is_empty() {
                if !parts.is_empty() {
                    remaining_width -= sep_width;
                }
                remaining_width -= rendered_width;
                parts.push(rendered);
            }
        }

        parts.join(separator)
    }

    /// Get the minimum tick interval across all modules.
    ///
    /// Returns the shortest tick interval, or `None` if no modules
    /// require periodic updates.
    pub fn min_tick_interval(&self) -> Option<Duration> {
        self.modules
            .iter()
            .filter_map(|m| m.tick_interval())
            .min()
    }

    /// Iterate over all registered modules.
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &dyn HudModule> {
        self.modules.iter().map(|m| m.as_ref())
    }
}

impl Default for HudRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_labels() {
        assert_eq!(Mode::Auto.label(), "AUTO");
        assert_eq!(Mode::Manual.label(), "MANUAL");
        assert_eq!(Mode::Insert.label(), "INSERT");
    }

    #[test]
    fn hud_state_default() {
        let state = HudState::default();
        assert!(!state.is_recording);
        assert_eq!(state.queue_depth, 0);
        assert!(state.last_latency_ms.is_none());
    }

    #[test]
    fn registry_new_is_empty() {
        let registry = HudRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn registry_with_defaults_has_modules() {
        let registry = HudRegistry::with_defaults();
        assert!(!registry.is_empty());
        assert!(registry.len() >= 4);
    }

    #[test]
    fn registry_get_module() {
        let registry = HudRegistry::with_defaults();
        assert!(registry.get("mode").is_some());
        assert!(registry.get("meter").is_some());
        assert!(registry.get("latency").is_some());
        assert!(registry.get("queue").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_render_all_empty() {
        let registry = HudRegistry::new();
        let state = HudState::default();
        let output = registry.render_all(&state, 80, " │ ");
        assert!(output.is_empty());
    }

    #[test]
    fn registry_render_all_with_modules() {
        let registry = HudRegistry::with_defaults();
        let state = HudState {
            mode: Mode::Auto,
            is_recording: true,
            audio_level_db: -30.0,
            queue_depth: 2,
            ..Default::default()
        };
        let output = registry.render_all(&state, 80, " │ ");
        assert!(!output.is_empty());
        // Should contain mode indicator
        assert!(output.contains("AUTO") || output.contains("●"));
    }

    #[test]
    fn registry_respects_max_width() {
        let registry = HudRegistry::with_defaults();
        let state = HudState::default();
        // Very narrow width should still produce something
        let output = registry.render_all(&state, 20, " ");
        // Output should be within bounds (accounting for ANSI codes is tricky,
        // so we just check it's not excessively long)
        assert!(output.len() < 200); // Generous for ANSI codes
    }
}
