//! Multi-backend architecture for AI CLI tools.
//!
//! This module provides a unified interface for different AI CLI backends
//! (Codex CLI, Claude Code, Gemini CLI, Aider, OpenCode, etc.) allowing the voice HUD
//! to work with any supported AI tool.

mod aider;
mod claude;
mod codex;
mod custom;
mod gemini;
mod opencode;

pub use aider::AiderBackend;
pub use claude::ClaudeBackend;
pub use codex::CodexBackend;
pub use custom::CustomBackend;
pub use gemini::GeminiBackend;
pub use opencode::OpenCodeBackend;

/// Trait defining the interface for AI CLI backends.
///
/// Each backend implementation provides the command to launch the AI tool
/// and patterns to detect when the tool is ready for input or thinking.
pub trait AiBackend: Send + Sync {
    /// Internal identifier for this backend (e.g., "claude", "gemini").
    fn name(&self) -> &str;

    /// Human-readable display name (e.g., "Claude Code", "Gemini CLI").
    fn display_name(&self) -> &str;

    /// Command and arguments to launch this backend.
    fn command(&self) -> Vec<String>;

    /// Regex pattern for detecting when the AI is ready for input.
    /// This typically matches the prompt or indicator shown when waiting for user input.
    fn prompt_pattern(&self) -> &str;

    /// Optional regex pattern for detecting when the AI is thinking/processing.
    /// Returns None if the backend doesn't have a distinct thinking indicator.
    fn thinking_pattern(&self) -> Option<&str>;
}

/// Registry for looking up AI backends by name.
pub struct BackendRegistry {
    backends: Vec<Box<dyn AiBackend>>,
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BackendRegistry {
    /// Create a new registry with all built-in backends.
    pub fn new() -> Self {
        Self {
            backends: vec![
                Box::new(CodexBackend::new()),
                Box::new(ClaudeBackend::new()),
                Box::new(GeminiBackend::new()),
                Box::new(AiderBackend::new()),
                Box::new(OpenCodeBackend::new()),
            ],
        }
    }

    /// Look up a backend by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&dyn AiBackend> {
        let name_lower = name.to_lowercase();
        self.backends
            .iter()
            .find(|b| b.name().to_lowercase() == name_lower)
            .map(|b| b.as_ref())
    }

    /// Get the default backend (Codex CLI).
    pub fn default_backend(&self) -> &dyn AiBackend {
        self.get("codex").expect("codex backend always exists")
    }

    /// List all available backend names.
    pub fn available_backends(&self) -> Vec<&str> {
        self.backends.iter().map(|b| b.name()).collect()
    }

    /// Register a custom backend.
    pub fn register(&mut self, backend: Box<dyn AiBackend>) {
        self.backends.push(backend);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_lookup() {
        let registry = BackendRegistry::new();

        assert!(registry.get("codex").is_some());
        assert!(registry.get("claude").is_some());
        assert!(registry.get("Claude").is_some()); // case-insensitive
        assert!(registry.get("gemini").is_some());
        assert!(registry.get("aider").is_some());
        assert!(registry.get("opencode").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_default_backend() {
        let registry = BackendRegistry::new();
        let default = registry.default_backend();
        assert_eq!(default.name(), "codex");
    }

    #[test]
    fn test_available_backends() {
        let registry = BackendRegistry::new();
        let names = registry.available_backends();
        assert!(names.contains(&"codex"));
        assert!(names.contains(&"claude"));
        assert!(names.contains(&"gemini"));
        assert!(names.contains(&"aider"));
        assert!(names.contains(&"opencode"));
    }

    #[test]
    fn test_register_custom() {
        let mut registry = BackendRegistry::new();
        let custom = CustomBackend::new("my-ai".to_string());
        registry.register(Box::new(custom));
        assert!(registry.get("custom").is_some());
    }
}
