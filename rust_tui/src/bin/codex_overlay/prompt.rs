use anyhow::{Context, Result};
use regex::Regex;
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use vte::{Parser as VteParser, Perform};

use crate::config::OverlayConfig;

const PROMPT_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub(crate) fn resolve_prompt_log(config: &OverlayConfig) -> Option<PathBuf> {
    if let Some(path) = &config.prompt_log {
        return Some(path.clone());
    }
    if let Ok(path) = env::var("CODEX_VOICE_PROMPT_LOG") {
        return Some(PathBuf::from(path));
    }
    None
}

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
        .or_else(|| env::var("CODEX_VOICE_PROMPT_REGEX").ok());
    if let Some(raw) = user_override {
        let regex = Regex::new(&raw).with_context(|| format!("invalid prompt regex: {raw}"))?;
        return Ok(PromptRegexConfig {
            regex: Some(regex),
            allow_auto_learn: false,
        });
    }

    if let Some(raw) = backend_fallback.map(str::trim).filter(|pattern| !pattern.is_empty()) {
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

pub(crate) struct PromptTracker {
    regex: Option<Regex>,
    learned_prompt: Option<String>,
    allow_auto_learn: bool,
    last_prompt_seen_at: Option<Instant>,
    last_output_at: Instant,
    has_seen_output: bool,
    current_line: Vec<u8>,
    last_line: Option<String>,
    prompt_logger: PromptLogger,
}

fn strip_ansi_preserve_controls(bytes: &[u8]) -> Vec<u8> {
    struct ControlStripper {
        output: Vec<u8>,
    }

    impl Perform for ControlStripper {
        fn print(&mut self, c: char) {
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            self.output.extend_from_slice(encoded.as_bytes());
        }

        fn execute(&mut self, byte: u8) {
            match byte {
                b'\n' | b'\r' | b'\t' => self.output.push(byte),
                _ => {}
            }
        }
    }

    let mut parser = VteParser::new();
    let mut stripper = ControlStripper {
        output: Vec::with_capacity(bytes.len()),
    };
    parser.advance(&mut stripper, bytes);
    stripper.output
}

impl PromptTracker {
    pub(crate) fn new(
        regex: Option<Regex>,
        allow_auto_learn: bool,
        prompt_logger: PromptLogger,
    ) -> Self {
        Self {
            regex,
            learned_prompt: None,
            allow_auto_learn,
            last_prompt_seen_at: None,
            last_output_at: Instant::now(),
            has_seen_output: false,
            current_line: Vec::new(),
            last_line: None,
            prompt_logger,
        }
    }

    pub(crate) fn feed_output(&mut self, bytes: &[u8]) {
        self.last_output_at = Instant::now();
        self.has_seen_output = true;

        let cleaned = strip_ansi_preserve_controls(bytes);
        for byte in cleaned {
            match byte {
                b'\n' => {
                    self.flush_line("line_complete");
                }
                b'\r' => {
                    self.current_line.clear();
                }
                b'\t' => {
                    self.current_line.push(b' ');
                }
                byte if byte.is_ascii_graphic() || byte == b' ' => {
                    self.current_line.push(byte);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn on_idle(&mut self, now: Instant, idle_timeout: Duration) {
        if !self.has_seen_output {
            return;
        }
        if now.duration_since(self.last_output_at) < idle_timeout {
            return;
        }
        let candidate = if !self.current_line.is_empty() {
            self.current_line_as_string()
        } else {
            self.last_line.clone().unwrap_or_default()
        };
        if candidate.trim().is_empty() {
            return;
        }
        if self.allow_auto_learn
            && self.learned_prompt.is_none()
            && !self.matches_prompt(&candidate)
        {
            if !looks_like_prompt(&candidate) {
                return;
            }
            self.learned_prompt = Some(candidate.clone());
            self.last_prompt_seen_at = Some(now);
            self.prompt_logger
                .log(&format!("prompt_learned|line={candidate}"));
            return;
        }
        if self.matches_prompt(&candidate) {
            self.update_prompt_seen(now, &candidate, "idle_match");
        }
    }

    fn flush_line(&mut self, reason: &str) {
        let line = self.current_line_as_string();
        self.current_line.clear();
        if line.trim().is_empty() {
            return;
        }
        self.last_line = Some(line.clone());
        if self.matches_prompt(&line) {
            self.update_prompt_seen(Instant::now(), &line, reason);
        }
    }

    fn matches_prompt(&self, line: &str) -> bool {
        let mut matches = false;
        if let Some(regex) = &self.regex {
            matches |= regex.is_match(line);
        }
        if let Some(prompt) = &self.learned_prompt {
            matches |= line.trim_end() == prompt.trim_end();
        }
        matches
    }

    fn update_prompt_seen(&mut self, now: Instant, line: &str, reason: &str) {
        self.last_prompt_seen_at = Some(now);
        self.prompt_logger
            .log(&format!("prompt_detected|reason={reason}|line={line}"));
    }

    fn current_line_as_string(&self) -> String {
        String::from_utf8_lossy(&self.current_line).to_string()
    }

    pub(crate) fn last_prompt_seen_at(&self) -> Option<Instant> {
        self.last_prompt_seen_at
    }

    pub(crate) fn last_output_at(&self) -> Instant {
        self.last_output_at
    }

    pub(crate) fn note_activity(&mut self, now: Instant) {
        self.last_output_at = now;
        self.has_seen_output = true;
    }

    pub(crate) fn idle_ready(&self, now: Instant, idle_timeout: Duration) -> bool {
        now.duration_since(self.last_output_at) >= idle_timeout
    }

    pub(crate) fn has_seen_output(&self) -> bool {
        self.has_seen_output
    }
}

fn looks_like_prompt(line: &str) -> bool {
    let trimmed = line.trim_end();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return false;
    }
    matches!(
        trimmed.chars().last(),
        Some('>') | Some('›') | Some('❯') | Some('$') | Some('#')
    )
}

pub(crate) fn should_auto_trigger(
    prompt_tracker: &PromptTracker,
    now: Instant,
    idle_timeout: Duration,
    last_trigger_at: Option<Instant>,
) -> bool {
    if !prompt_tracker.has_seen_output() {
        return last_trigger_at.is_none() && prompt_tracker.idle_ready(now, idle_timeout);
    }
    if let Some(prompt_at) = prompt_tracker.last_prompt_seen_at() {
        if last_trigger_at.is_none_or(|last| prompt_at > last) {
            return true;
        }
    }
    if prompt_tracker.idle_ready(now, idle_timeout)
        && last_trigger_at.is_none_or(|last| prompt_tracker.last_output_at() > last)
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{OverlayConfig, VoiceSendMode};
    use clap::Parser;
    use rust_tui::config::AppConfig;
    use std::env;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

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
            backend: "codex".to_string(),
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
        env::set_var("CODEX_VOICE_PROMPT_LOG", &env_path);
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
            backend: "codex".to_string(),
        };
        let resolved = resolve_prompt_log(&config);
        env::remove_var("CODEX_VOICE_PROMPT_LOG");
        assert_eq!(resolved, Some(env_path));
    }

    #[test]
    fn resolve_prompt_log_defaults_to_none() {
        let _guard = env_lock().lock().unwrap();
        env::remove_var("CODEX_VOICE_PROMPT_LOG");
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
            backend: "codex".to_string(),
        };
        assert!(resolve_prompt_log(&config).is_none());
    }

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
            backend: "codex".to_string(),
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
            backend: "codex".to_string(),
        };
        assert!(resolve_prompt_regex(&config, None).is_err());
    }

    #[test]
    fn should_auto_trigger_checks_prompt_and_idle() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_auto")));
        let mut tracker = PromptTracker::new(None, true, logger);
        let now = Instant::now();
        tracker.has_seen_output = true;
        tracker.last_output_at = now - Duration::from_millis(2000);
        tracker.last_prompt_seen_at = Some(now - Duration::from_millis(1500));

        assert!(should_auto_trigger(
            &tracker,
            now,
            Duration::from_millis(1000),
            Some(now - Duration::from_millis(2000))
        ));
        assert!(!should_auto_trigger(
            &tracker,
            now,
            Duration::from_millis(1000),
            Some(now - Duration::from_millis(1000))
        ));

        tracker.last_prompt_seen_at = None;
        tracker.last_output_at = now - Duration::from_millis(1200);
        assert!(should_auto_trigger(
            &tracker,
            now,
            Duration::from_millis(1000),
            Some(now - Duration::from_millis(2000))
        ));
        tracker.last_output_at = now - Duration::from_millis(500);
        assert!(!should_auto_trigger(
            &tracker,
            now,
            Duration::from_millis(1000),
            Some(now - Duration::from_millis(2000))
        ));
    }

    #[test]
    fn prompt_tracker_feed_output_handles_control_bytes() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_control")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.feed_output(b"ab\rde\tf\n");
        assert_eq!(tracker.last_line.as_deref(), Some("de f"));
        assert!(tracker.has_seen_output());
    }

    #[test]
    fn prompt_tracker_idle_ready_on_threshold() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_idle")));
        let mut tracker = PromptTracker::new(None, true, logger);
        let now = Instant::now();
        tracker.note_activity(now - Duration::from_millis(1000));
        assert!(tracker.idle_ready(now, Duration::from_millis(1000)));
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

    #[test]
    fn prompt_tracker_learns_prompt_on_idle() {
        let logger = PromptLogger::new(Some(env::temp_dir().join("codex_voice_prompt_test.log")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(2000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_matches_regex() {
        let logger = PromptLogger::new(Some(env::temp_dir().join("codex_voice_prompt_test.log")));
        let regex = Regex::new(r"^codex> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), false, logger);
        tracker.feed_output(b"codex> \n");
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_ignores_non_graphic_bytes() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_non_graphic")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.feed_output(b"hi\xC2\xA0there\n");
        assert_eq!(tracker.last_line.as_deref(), Some("hithere"));
    }

    #[test]
    fn prompt_tracker_on_idle_triggers_on_threshold() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_idle_threshold")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(1000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_on_idle_skips_when_regex_present() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_idle_regex")));
        let regex = Regex::new(r"^codex> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), false, logger);
        tracker.feed_output(b"not a prompt");
        let now = tracker.last_output_at() + Duration::from_millis(1000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_none());
    }

    #[test]
    fn prompt_tracker_on_idle_learns_when_auto_learn_enabled() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_idle_fallback")));
        let regex = Regex::new(r"^>$").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), true, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(1000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_matches_learned_prompt() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_match")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.learned_prompt = Some("codex> ".to_string());
        assert!(tracker.matches_prompt("codex> "));
    }

    #[test]
    fn prompt_tracker_rejects_mismatched_prompt() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_mismatch")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.learned_prompt = Some("codex> ".to_string());
        assert!(!tracker.matches_prompt("nope> "));
    }

    #[test]
    fn prompt_tracker_has_seen_output_starts_false() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_seen")));
        let tracker = PromptTracker::new(None, true, logger);
        assert!(!tracker.has_seen_output());
    }

    #[test]
    fn should_auto_trigger_respects_last_trigger_equal_times() {
        let logger = PromptLogger::new(Some(temp_log_path("prompt_tracker_last_trigger")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.has_seen_output = true;
        let now = Instant::now();
        tracker.last_prompt_seen_at = Some(now);
        tracker.last_output_at = now;
        assert!(!should_auto_trigger(
            &tracker,
            now,
            Duration::from_millis(0),
            Some(now)
        ));
    }
}
