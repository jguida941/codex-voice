//! Backend command resolution so provider selection is deterministic at startup.

use voxterm::backend::BackendRegistry;

use crate::config::cli::OverlayConfig;
use crate::config::util::{extract_binary_label, is_path_like, split_backend_command};

#[must_use = "ResolvedBackend contains the command to execute"]
pub(crate) struct ResolvedBackend {
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) label: String,
    pub(crate) prompt_pattern: Option<String>,
}

impl OverlayConfig {
    /// Resolve the backend command, arguments, and prompt patterns.
    #[must_use = "backend resolution affects command execution"]
    pub(crate) fn resolve_backend(&self) -> ResolvedBackend {
        // Check shorthand flags first
        let backend_raw = if self.claude {
            "claude"
        } else if self.gemini {
            "gemini"
        } else if self.codex {
            "codex"
        } else {
            self.backend.trim()
        };
        let (primary, extra_args) = split_backend_command(backend_raw);
        let primary_label = extract_binary_label(&primary);

        if primary.is_empty() || primary_label.eq_ignore_ascii_case("codex") {
            let mut args = self.app.codex_args.clone();
            args.extend(extra_args);
            let command = if is_path_like(&primary) {
                primary
            } else {
                self.app.codex_cmd.clone()
            };
            return ResolvedBackend {
                command,
                args,
                label: "codex".to_string(),
                prompt_pattern: None,
            };
        }

        let registry = BackendRegistry::new();
        if let Some(backend) = registry.get(&primary_label) {
            let mut command_parts = backend.command();
            let default_cmd = command_parts
                .first()
                .cloned()
                .unwrap_or_else(|| primary.clone());
            let mut args: Vec<String> = if command_parts.len() > 1 {
                command_parts.drain(1..).collect()
            } else {
                Vec::new()
            };
            args.extend(extra_args);
            let command = if is_path_like(&primary) {
                primary
            } else {
                default_cmd
            };
            let prompt_pattern = if backend.prompt_pattern().trim().is_empty() {
                None
            } else {
                Some(backend.prompt_pattern().to_string())
            };
            return ResolvedBackend {
                command,
                args,
                label: backend.name().to_string(),
                prompt_pattern,
            };
        }

        ResolvedBackend {
            command: primary.clone(),
            args: extra_args,
            label: primary_label.to_lowercase(),
            prompt_pattern: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::cli::{HudRightPanel, HudStyle, VoiceSendMode};
    use clap::Parser;
    use voxterm::config::AppConfig;

    fn make_config_with_backend(backend: &str) -> OverlayConfig {
        OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: HudRightPanel::Ribbon,
            hud_right_panel_recording_only: true,
            hud_style: HudStyle::Full,
            minimal_hud: false,
            backend: backend.to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        }
    }

    #[test]
    fn resolve_backend_preset_codex_uses_app_config() {
        let mut config = make_config_with_backend("codex");
        config.app.codex_cmd = "codex-bin".to_string();
        config.app.codex_args = vec!["--flag".to_string()];
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "codex-bin");
        assert_eq!(resolved.args, vec!["--flag"]);
        assert_eq!(resolved.label, "codex");
        assert!(resolved.prompt_pattern.is_none());
    }

    #[test]
    fn resolve_backend_codex_includes_extra_args() {
        let mut config = make_config_with_backend("codex --extra");
        config.app.codex_args = vec!["--flag".to_string()];
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "codex");
        assert_eq!(resolved.args, vec!["--flag", "--extra"]);
    }

    #[test]
    fn resolve_backend_preset_claude() {
        let config = make_config_with_backend("claude");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "claude");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "claude");
        assert!(resolved.prompt_pattern.is_some());
    }

    #[test]
    fn resolve_backend_preset_gemini() {
        let config = make_config_with_backend("gemini");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "gemini");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "gemini");
        assert!(resolved.prompt_pattern.is_some());
    }

    #[test]
    fn resolve_backend_preset_aider() {
        let config = make_config_with_backend("aider");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "aider");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "aider");
        assert!(resolved.prompt_pattern.is_some());
    }

    #[test]
    fn resolve_backend_preset_opencode() {
        let config = make_config_with_backend("opencode");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "opencode");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "opencode");
        assert!(resolved.prompt_pattern.is_some());
    }

    #[test]
    fn resolve_backend_preset_case_insensitive() {
        let config = make_config_with_backend("CLAUDE");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "claude");
        assert!(resolved.args.is_empty());

        let config = make_config_with_backend("Gemini");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "gemini");
        assert!(resolved.args.is_empty());
    }

    #[test]
    fn resolve_backend_custom_command() {
        let config = make_config_with_backend("my-tool");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "my-tool");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "my-tool");
        assert!(resolved.prompt_pattern.is_none());
    }

    #[test]
    fn resolve_backend_custom_command_with_args() {
        let config = make_config_with_backend("my-tool --flag value");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "my-tool");
        assert_eq!(resolved.args, vec!["--flag", "value"]);
    }

    #[test]
    fn resolve_backend_custom_command_with_quoted_args() {
        let config = make_config_with_backend("my-tool --flag \"value with spaces\"");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "my-tool");
        assert_eq!(resolved.args, vec!["--flag", "value with spaces"]);
    }

    #[test]
    fn resolve_backend_custom_path() {
        let config = make_config_with_backend("/usr/local/bin/my-tool --verbose");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "/usr/local/bin/my-tool");
        assert_eq!(resolved.args, vec!["--verbose"]);
        assert_eq!(resolved.label, "my-tool");
    }

    #[test]
    fn resolve_backend_empty_fallback() {
        let config = make_config_with_backend("   ");
        let resolved = config.resolve_backend();
        assert_eq!(resolved.command, "codex");
        assert!(resolved.args.is_empty());
        assert_eq!(resolved.label, "codex");
    }
}
