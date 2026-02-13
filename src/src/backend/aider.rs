//! Aider backend definition so the shared registry can launch Aider consistently.

use super::AiBackend;

/// Backend for Aider CLI.
pub struct AiderBackend {
    command: Vec<String>,
}

impl Default for AiderBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AiderBackend {
    /// Create a new Aider backend with default settings.
    pub fn new() -> Self {
        Self {
            command: vec!["aider".to_string()],
        }
    }

    /// Create an Aider backend with additional arguments.
    pub fn with_args(args: Vec<String>) -> Self {
        let mut command = vec!["aider".to_string()];
        command.extend(args);
        Self { command }
    }
}

impl AiBackend for AiderBackend {
    fn name(&self) -> &str {
        "aider"
    }

    fn display_name(&self) -> &str {
        "Aider"
    }

    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    fn prompt_pattern(&self) -> &str {
        // Aider shows a prompt like "aider>" or just ">"
        r"(?i)^(aider>|>\s*)$"
    }

    fn thinking_pattern(&self) -> Option<&str> {
        // Aider shows progress indicators when working
        Some(r"(?i)(thinking|working|sending|\.\.\.)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aider_backend() {
        let backend = AiderBackend::new();
        assert_eq!(backend.name(), "aider");
        assert_eq!(backend.display_name(), "Aider");
        assert_eq!(backend.command(), vec!["aider"]);
        assert!(!backend.prompt_pattern().is_empty());
        assert!(backend.thinking_pattern().is_some());
    }

    #[test]
    fn test_aider_with_args() {
        let backend = AiderBackend::with_args(vec!["--model".to_string(), "gpt-4".to_string()]);
        assert_eq!(backend.command(), vec!["aider", "--model", "gpt-4"]);
    }
}
