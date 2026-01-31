//! Codex CLI backend implementation.

use super::AiBackend;

/// Backend for Codex CLI.
pub struct CodexBackend {
    command: Vec<String>,
}

impl Default for CodexBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexBackend {
    /// Create a new Codex backend with default settings.
    pub fn new() -> Self {
        Self {
            command: vec!["codex".to_string()],
        }
    }

    /// Create a Codex backend with additional arguments.
    pub fn with_args(args: Vec<String>) -> Self {
        let mut command = vec!["codex".to_string()];
        command.extend(args);
        Self { command }
    }
}

impl AiBackend for CodexBackend {
    fn name(&self) -> &str {
        "codex"
    }

    fn display_name(&self) -> &str {
        "Codex"
    }

    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    fn prompt_pattern(&self) -> &str {
        // Codex prompt is learned dynamically by default.
        ""
    }

    fn thinking_pattern(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codex_backend() {
        let backend = CodexBackend::new();
        assert_eq!(backend.name(), "codex");
        assert_eq!(backend.display_name(), "Codex");
        assert_eq!(backend.command(), vec!["codex"]);
        assert_eq!(backend.prompt_pattern(), "");
        assert!(backend.thinking_pattern().is_none());
    }

    #[test]
    fn test_codex_with_args() {
        let backend = CodexBackend::with_args(vec!["--foo".to_string()]);
        assert_eq!(backend.command(), vec!["codex", "--foo"]);
    }
}
