//! Prompt-observation logging so readiness heuristics can be tuned with evidence.

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::OverlayConfig;

const PROMPT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub(crate) fn resolve_prompt_log(config: &OverlayConfig) -> Option<PathBuf> {
    if let Some(path) = &config.prompt_log {
        return Some(path.clone());
    }
    if let Ok(path) = env::var("VOXTERM_PROMPT_LOG") {
        return Some(PathBuf::from(path));
    }
    None
}

pub(crate) struct PromptLogger {
    writer: Option<Mutex<PromptLogWriter>>,
}

struct PromptLogWriter {
    path: PathBuf,
    file: fs::File,
    bytes_written: u64,
}

impl PromptLogWriter {
    fn new(path: PathBuf) -> Option<Self> {
        let mut bytes_written = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if bytes_written > PROMPT_LOG_MAX_BYTES {
            let _ = fs::remove_file(&path);
            bytes_written = 0;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        Some(Self {
            path,
            file,
            bytes_written,
        })
    }

    fn rotate_if_needed(&mut self, next_len: usize) {
        if self.bytes_written.saturating_add(next_len as u64) <= PROMPT_LOG_MAX_BYTES {
            return;
        }
        if let Ok(file) = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
        {
            self.file = file;
            self.bytes_written = 0;
        }
    }

    fn write_line(&mut self, line: &str) {
        self.rotate_if_needed(line.len());
        if self.file.write_all(line.as_bytes()).is_ok() {
            self.bytes_written = self.bytes_written.saturating_add(line.len() as u64);
        }
    }
}

impl PromptLogger {
    pub(crate) fn new(path: Option<PathBuf>) -> Self {
        let writer = path.and_then(PromptLogWriter::new).map(Mutex::new);
        Self { writer }
    }

    pub(crate) fn log(&self, message: &str) {
        let Some(writer) = &self.writer else {
            return;
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("[{timestamp}] {message}\n");
        if let Ok(mut guard) = writer.lock() {
            guard.write_line(&line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OverlayConfig, VoiceSendMode};
    use clap::Parser;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use voxterm::config::AppConfig;

    fn temp_log_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        env::temp_dir().join(format!("{label}_{unique}.log"))
    }

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn resolve_prompt_log_prefers_config() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: Some(PathBuf::from("/tmp/codex_prompt_override.log")),
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        let resolved = resolve_prompt_log(&config);
        assert_eq!(
            resolved,
            Some(PathBuf::from("/tmp/codex_prompt_override.log"))
        );
    }

    #[test]
    fn resolve_prompt_log_uses_env() {
        let _guard = env_lock().lock().unwrap();
        let env_path = PathBuf::from("/tmp/codex_prompt_env.log");
        env::set_var("VOXTERM_PROMPT_LOG", &env_path);
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        let resolved = resolve_prompt_log(&config);
        env::remove_var("VOXTERM_PROMPT_LOG");
        assert_eq!(resolved, Some(env_path));
    }

    #[test]
    fn resolve_prompt_log_defaults_to_none() {
        let _guard = env_lock().lock().unwrap();
        env::remove_var("VOXTERM_PROMPT_LOG");
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            transcript_idle_ms: 250,
            voice_send_mode: VoiceSendMode::Auto,
            theme_name: None,
            no_color: false,
            hud_right_panel: crate::config::HudRightPanel::Ribbon,
            hud_right_panel_recording_only: true,
            hud_style: crate::config::HudStyle::Full,
            minimal_hud: false,
            backend: "codex".to_string(),
            codex: false,
            claude: false,
            gemini: false,
            login: false,
        };
        assert!(resolve_prompt_log(&config).is_none());
    }

    #[test]
    fn prompt_logger_writes_lines() {
        let path = temp_log_path("prompt_logger");
        let logger = PromptLogger::new(Some(path.clone()));
        logger.log("hello");
        let contents = std::fs::read_to_string(&path).expect("log file");
        let _ = std::fs::remove_file(&path);
        assert!(contents.contains("hello"));
    }
}
