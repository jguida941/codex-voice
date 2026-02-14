//! Prompt-regex resolution so custom and backend defaults validate consistently.

use anyhow::{Context, Result};
use regex::Regex;
use std::env;

use crate::config::OverlayConfig;

pub(crate) struct PromptRegexConfig {
    pub(crate) regex: Option<Regex>,
    pub(crate) allow_auto_learn: bool,
}

pub(crate) fn resolve_prompt_regex(
    config: &OverlayConfig,
    backend_fallback: Option<&str>,
) -> Result<PromptRegexConfig> {
    let user_override = config
        .prompt_regex
        .clone()
        .or_else(|| env::var("VOICETERM_PROMPT_REGEX").ok());
    if let Some(raw) = user_override {
        let regex = Regex::new(&raw).with_context(|| format!("invalid prompt regex: {raw}"))?;
        return Ok(PromptRegexConfig {
            regex: Some(regex),
            allow_auto_learn: false,
        });
    }

    if let Some(raw) = backend_fallback
        .map(str::trim)
        .filter(|pattern| !pattern.is_empty())
    {
        let regex = Regex::new(raw).with_context(|| format!("invalid prompt regex: {raw}"))?;
        return Ok(PromptRegexConfig {
            regex: Some(regex),
            allow_auto_learn: true,
        });
    }

    Ok(PromptRegexConfig {
        regex: None,
        allow_auto_learn: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OverlayConfig, VoiceSendMode};
    use clap::Parser;
    use voiceterm::config::AppConfig;

    #[test]
    fn resolve_prompt_regex_honors_config() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: Some("^codex> $".to_string()),
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_border_style: crate::config::HudBorderStyle::Theme,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            latency_display: crate::config::LatencyDisplayMode::Short,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        let resolved = resolve_prompt_regex(&config, None).expect("regex should compile");
        assert!(resolved.regex.is_some());
        assert!(!resolved.allow_auto_learn);
    }

    #[test]
    fn resolve_prompt_regex_rejects_invalid() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: Some("[".to_string()),
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_border_style: crate::config::HudBorderStyle::Theme,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            latency_display: crate::config::LatencyDisplayMode::Short,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        assert!(resolve_prompt_regex(&config, None).is_err());
    }
}
