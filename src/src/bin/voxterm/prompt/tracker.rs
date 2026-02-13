//! Prompt-detection state machine so auto-voice and auto-send trigger safely.

use regex::Regex;
use std::time::{Duration, Instant};

use super::logger::PromptLogger;
use super::strip::strip_ansi_preserve_controls;

/// Tracks prompt detection state from PTY output to drive auto-voice behavior.
pub(crate) struct PromptTracker {
    /// Optional user-supplied prompt regex override.
    regex: Option<Regex>,
    /// Auto-learned prompt string from recent output.
    learned_prompt: Option<String>,
    /// Whether auto-learning is permitted.
    allow_auto_learn: bool,
    /// Last time a prompt was detected.
    last_prompt_seen_at: Option<Instant>,
    /// Last time any output (PTY or overlay) was seen.
    last_output_at: Instant,
    /// Last time PTY output was observed.
    last_pty_output_at: Option<Instant>,
    /// Whether any output has been seen yet (startup heuristic).
    has_seen_output: bool,
    /// Current line buffer (ANSI-stripped) for prompt matching.
    current_line: Vec<u8>,
    /// Last completed line (ANSI-stripped).
    last_line: Option<String>,
    /// Optional prompt logging sink.
    prompt_logger: PromptLogger,
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
            last_pty_output_at: None,
            has_seen_output: false,
            current_line: Vec::new(),
            last_line: None,
            prompt_logger,
        }
    }

    pub(crate) fn feed_output(&mut self, bytes: &[u8]) {
        let now = Instant::now();
        self.last_output_at = now;
        self.last_pty_output_at = Some(now);
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

    pub(crate) fn last_pty_output_at(&self) -> Option<Instant> {
        self.last_pty_output_at
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
    use crate::prompt::PromptLogger;
    use regex::Regex;
    use std::env;
    use std::time::SystemTime;

    fn temp_log_path(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        env::temp_dir().join(format!("{label}_{unique}.log"))
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
    fn prompt_tracker_learns_prompt_on_idle() {
        let logger = PromptLogger::new(Some(env::temp_dir().join("voxterm_prompt_test.log")));
        let mut tracker = PromptTracker::new(None, true, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(2000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_matches_regex() {
        let logger = PromptLogger::new(Some(env::temp_dir().join("voxterm_prompt_test.log")));
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
