//! Claude Code backend implementation.

use super::AiBackend;

/// Backend for Claude Code CLI.
pub struct ClaudeBackend {
    command: Vec<String>,
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl ClaudeBackend {
    /// Create a new Claude Code backend with default settings.
    pub fn new() -> Self {
        Self {
            command: vec!["claude".to_string()],
        }
    }

    /// Create a Claude Code backend with additional arguments.
    pub fn with_args(args: Vec<String>) -> Self {
        let mut command = vec!["claude".to_string()];
        command.extend(args);
        Self { command }
    }
}

impl AiBackend for ClaudeBackend {
    fn name(&self) -> &str {
        "claude"
    }

    fn display_name(&self) -> &str {
        "Claude Code"
    }

    fn command(&self) -> Vec<String> {
        self.command.clone()
    }

    fn prompt_pattern(&self) -> &str {
        // Claude Code shows ">" when ready for input
        r"^>\s*$"
    }

    fn thinking_pattern(&self) -> Option<&str> {
        // Claude Code shows spinner or "Thinking..." indicator
        Some(r"(?i)(thinking|processing|\.\.\.)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_backend() {
        let backend = ClaudeBackend::new();
        assert_eq!(backend.name(), "claude");
        assert_eq!(backend.display_name(), "Claude Code");
        assert_eq!(backend.command(), vec!["claude"]);
        assert!(!backend.prompt_pattern().is_empty());
        assert!(backend.thinking_pattern().is_some());
    }

    #[test]
    fn test_claude_with_args() {
        let backend = ClaudeBackend::with_args(vec!["--model".to_string(), "opus".to_string()]);
        assert_eq!(
            backend.command(),
            vec!["claude", "--model", "opus"]
        );
    }
}
