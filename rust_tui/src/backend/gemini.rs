//! Gemini CLI backend implementation.

use super::AiBackend;

/// Backend for Gemini CLI.
pub struct GeminiBackend {
    command: Vec<String>,
}

impl Default for GeminiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiBackend {
    /// Create a new Gemini CLI backend with default settings.
    pub fn new() -> Self {
        Self {
            command: vec!["gemini".to_string()],
        }
    }

    /// Create a Gemini CLI backend with additional arguments.
    pub fn with_args(args: Vec<String>) -> Self {
        let mut command = vec!["gemini".to_string()];
        command.extend(args);
        Self { command }
    }
}

impl AiBackend for GeminiBackend {
    fn name(&self) -> &str {
        "gemini"
    }

    fn display_name(&self) -> &str {
        "Gemini CLI"
    }

    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    fn prompt_pattern(&self) -> &str {
        // Gemini CLI prompt pattern
        r"(?i)^(gemini>|>\s*)$"
    }

    fn thinking_pattern(&self) -> Option<&str> {
        Some(r"(?i)(generating|thinking|\.\.\.)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gemini_backend() {
        let backend = GeminiBackend::new();
        assert_eq!(backend.name(), "gemini");
        assert_eq!(backend.display_name(), "Gemini CLI");
        assert_eq!(backend.command(), vec!["gemini"]);
        assert!(!backend.prompt_pattern().is_empty());
        assert!(backend.thinking_pattern().is_some());
    }

    #[test]
    fn test_gemini_with_args() {
        let backend = GeminiBackend::with_args(vec!["--model".to_string(), "pro".to_string()]);
        assert_eq!(backend.command(), vec!["gemini", "--model", "pro"]);
    }
}
