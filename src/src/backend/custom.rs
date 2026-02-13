//! User-defined backend adapter so arbitrary CLIs can reuse overlay orchestration.

use super::AiBackend;

/// Backend for custom AI CLI commands.
///
/// This allows users to specify their own command to use as an AI backend,
/// with configurable prompt and thinking patterns.
pub struct CustomBackend {
    command_str: String,
    prompt_pattern: String,
    thinking_pattern: Option<String>,
}

impl CustomBackend {
    /// Create a new custom backend with the given command string.
    ///
    /// The command string is parsed as a shell command (split on whitespace).
    /// Uses default patterns that work with most CLI tools.
    pub fn new(command_str: String) -> Self {
        Self {
            command_str,
            prompt_pattern: r">\s*$".to_string(),
            thinking_pattern: Some(r"(?i)(thinking|processing|\.\.\.)".to_string()),
        }
    }

    /// Create a custom backend with full configuration.
    pub fn with_patterns(
        command_str: String,
        prompt_pattern: String,
        thinking_pattern: Option<String>,
    ) -> Self {
        Self {
            command_str,
            prompt_pattern,
            thinking_pattern,
        }
    }
}

impl AiBackend for CustomBackend {
    fn name(&self) -> &str {
        "custom"
    }

    fn display_name(&self) -> &str {
        "Custom"
    }

    fn command(&self) -> Vec<String> {
        // Simple whitespace splitting for the command; quoting is not parsed.
        // Use a wrapper script if your command requires complex arguments.
        self.command_str
            .split_whitespace()
            .map(String::from)
            .collect()
    }

    fn prompt_pattern(&self) -> &str {
        &self.prompt_pattern
    }

    fn thinking_pattern(&self) -> Option<&str> {
        self.thinking_pattern.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_custom_backend() {
        let backend = CustomBackend::new("my-ai --interactive".to_string());
        assert_eq!(backend.name(), "custom");
        assert_eq!(backend.display_name(), "Custom");
        assert_eq!(backend.command(), vec!["my-ai", "--interactive"]);
        assert!(!backend.prompt_pattern().is_empty());
        assert!(backend.thinking_pattern().is_some());
    }

    #[test]
    fn test_custom_with_patterns() {
        let backend = CustomBackend::with_patterns(
            "my-ai".to_string(),
            r"my-ai>".to_string(),
            Some(r"working".to_string()),
        );
        assert_eq!(backend.prompt_pattern(), "my-ai>");
        assert_eq!(backend.thinking_pattern(), Some("working"));
    }

    #[test]
    fn test_custom_no_thinking_pattern() {
        let backend = CustomBackend::with_patterns("simple-ai".to_string(), r">".to_string(), None);
        assert!(backend.thinking_pattern().is_none());
    }
}
