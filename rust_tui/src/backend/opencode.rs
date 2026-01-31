//! OpenCode backend implementation.

use super::AiBackend;

/// Backend for OpenCode CLI.
pub struct OpenCodeBackend {
    command: Vec<String>,
}

impl Default for OpenCodeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenCodeBackend {
    /// Create a new OpenCode backend with default settings.
    pub fn new() -> Self {
        Self {
            command: vec!["opencode".to_string()],
        }
    }

    /// Create an OpenCode backend with additional arguments.
    pub fn with_args(args: Vec<String>) -> Self {
        let mut command = vec!["opencode".to_string()];
        command.extend(args);
        Self { command }
    }
}

impl AiBackend for OpenCodeBackend {
    fn name(&self) -> &str {
        "opencode"
    }

    fn display_name(&self) -> &str {
        "OpenCode"
    }

    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    fn prompt_pattern(&self) -> &str {
        // OpenCode prompt pattern
        r"(?i)^(opencode>|>\s*)$"
    }

    fn thinking_pattern(&self) -> Option<&str> {
        Some(r"(?i)(thinking|processing|\.\.\.)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_backend() {
        let backend = OpenCodeBackend::new();
        assert_eq!(backend.name(), "opencode");
        assert_eq!(backend.display_name(), "OpenCode");
        assert_eq!(backend.command(), vec!["opencode"]);
        assert!(!backend.prompt_pattern().is_empty());
        assert!(backend.thinking_pattern().is_some());
    }

    #[test]
    fn test_opencode_with_args() {
        let backend = OpenCodeBackend::with_args(vec!["--verbose".to_string()]);
        assert_eq!(backend.command(), vec!["opencode", "--verbose"]);
    }
}
