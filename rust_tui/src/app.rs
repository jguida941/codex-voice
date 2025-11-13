//! Terminal UI shell for the `codex_voice` pipeline. It mirrors the Python
//! prototype but wraps it in a full-screen experience driven by `ratatui`.

use std::{
    env, fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{mpsc::TryRecvError, Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::codex::{self, CodexJob, CodexJobMessage, CODEX_SPINNER_FRAMES};
use crate::config::AppConfig;
use crate::voice::{self, VoiceCaptureTrigger, VoiceJob, VoiceJobMessage};
use crate::{audio, pty_session, stt};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

/// Maximum number of lines to retain in the scrollback buffer.
const OUTPUT_MAX_LINES: usize = 500;
/// Spinner cadence for Codex worker status updates.
const CODEX_SPINNER_INTERVAL: Duration = Duration::from_millis(150);

/// Path to the temp log file we rotate between runs.
pub fn log_file_path() -> PathBuf {
    env::temp_dir().join("codex_voice_tui.log")
}

/// Write debug messages to a temp file so we can troubleshoot without corrupting the TUI.
pub fn log_debug(msg: &str) {
    use std::fs::OpenOptions;

    let log_path = log_file_path();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "[{timestamp}] {msg}");
    }
}

/// Remove the log file if it grows past 5 MB between runs.
pub fn init_debug_log_file() {
    let log_path = log_file_path();
    if let Ok(metadata) = fs::metadata(&log_path) {
        const MAX_BYTES: u64 = 5 * 1024 * 1024;
        if metadata.len() > MAX_BYTES {
            let _ = fs::remove_file(&log_path);
        }
    }
}

/// JSON payload emitted by the Python fallback pipeline; we parse it to reuse the
/// verified Python flow when native capture fails.
#[derive(Debug, Deserialize)]
pub(crate) struct PipelineJsonResult {
    pub(crate) transcript: String,
    #[allow(dead_code)]
    pub(crate) prompt: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) codex_output: Option<String>,
    #[serde(default)]
    pub(crate) metrics: PipelineMetrics,
}

/// Optional timing metadata emitted by the python helper, used for log summaries.
#[derive(Debug, Deserialize, Default, Clone, Copy)]
pub(crate) struct PipelineMetrics {
    #[serde(default)]
    pub(crate) record_s: f64,
    #[serde(default)]
    pub(crate) stt_s: f64,
    #[serde(default)]
    pub(crate) codex_s: f64,
    #[serde(default)]
    pub(crate) total_s: f64,
}

/// Execute the original python pipeline and parse its JSON result for STT fallback.
pub(crate) fn run_python_transcription(config: &AppConfig) -> Result<PipelineJsonResult> {
    let mut cmd = Command::new(&config.python_cmd);
    cmd.arg(&config.pipeline_script);
    cmd.args(["--seconds", &config.seconds.to_string()]);
    cmd.args(["--lang", &config.lang]);
    cmd.args(["--ffmpeg-cmd", &config.ffmpeg_cmd]);
    if let Some(device) = &config.ffmpeg_device {
        cmd.args(["--ffmpeg-device", device]);
    }
    cmd.args(["--whisper-cmd", &config.whisper_cmd]);
    cmd.args(["--whisper-model", &config.whisper_model]);
    if let Some(model_path) = &config.whisper_model_path {
        cmd.args(["--whisper-model-path", model_path]);
    }
    cmd.args(["--codex-cmd", &config.codex_cmd]);
    for arg in &config.codex_args {
        cmd.arg(format!("--codex-arg={arg}"));
    }
    cmd.arg("--no-codex");
    cmd.arg("--emit-json");
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    log_debug("Invoking python fallback for transcription");
    let call_started = Instant::now();
    let output = cmd
        .output()
        .context("failed to run python fallback pipeline")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(anyhow!(
            "python fallback failed with status {}.\nstdout:\n{}\nstderr:\n{}",
            output.status,
            stdout.trim(),
            stderr.trim()
        ));
    }

    let mut parsed: Option<PipelineJsonResult> = None;
    let mut last_parse_error: Option<(String, serde_json::Error)> = None;

    let stdout_trimmed = stdout.trim();
    if !stdout_trimmed.is_empty() {
        match serde_json::from_str::<PipelineJsonResult>(stdout_trimmed) {
            Ok(json) => parsed = Some(json),
            Err(err) => last_parse_error = Some((stdout_trimmed.to_string(), err)),
        }
    }

    if parsed.is_none() {
        for line in stdout.lines().rev() {
            let mut trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("JSON:") {
                trimmed = rest.trim();
            }
            if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
                continue;
            }
            match serde_json::from_str::<PipelineJsonResult>(trimmed) {
                Ok(json) => {
                    parsed = Some(json);
                    break;
                }
                Err(err) => {
                    last_parse_error = Some((trimmed.to_string(), err));
                }
            }
        }
    }

    let parsed = match parsed {
        Some(json) => {
            if let Some((line, err)) = last_parse_error {
                log_debug(&format!(
                    "Python fallback JSON parse warnings (last error: {err} on `{line}`)"
                ));
            }
            json
        }
        None => {
            let mut error = anyhow!(
                "python fallback did not emit JSON.\nstdout:\n{}\nstderr:\n{}",
                stdout.trim(),
                stderr.trim()
            );
            if let Some((line, parse_err)) = last_parse_error {
                error = error.context(format!("last JSON parse failure `{line}`: {parse_err}"));
            }
            return Err(error);
        }
    };
    if config.log_timings {
        let elapsed = call_started.elapsed().as_secs_f64();
        log_debug(&format!(
            "timing|phase=python_pipeline|record_s={:.3}|stt_s={:.3}|codex_s={:.3}|total_s={:.3}|rust_elapsed_s={:.3}",
            parsed.metrics.record_s,
            parsed.metrics.stt_s,
            parsed.metrics.codex_s,
            parsed.metrics.total_s,
            elapsed,
        ));
    }
    Ok(parsed)
}

/// Central application state shared between the event loop, renderer, and voice worker.
pub struct App {
    config: AppConfig,
    input: String,
    output: Vec<String>,
    status: String,
    voice_enabled: bool,
    scroll_offset: u16,
    codex_session: Option<pty_session::PtyCodexSession>,
    codex_job: Option<CodexJob>,
    codex_spinner_index: usize,
    codex_spinner_last_tick: Option<Instant>,
    audio_recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    voice_job: Option<VoiceJob>,
}

impl App {
    /// Create the application state with default status text.
    pub fn new(config: AppConfig) -> Self {
        Self {
            config,
            input: String::new(),
            output: Vec::new(),
            status: "Ready. Press Ctrl+R for voice capture.".into(),
            voice_enabled: false,
            scroll_offset: 0,
            codex_session: None,
            codex_job: None,
            codex_spinner_index: 0,
            codex_spinner_last_tick: None,
            audio_recorder: None,
            transcriber: None,
            voice_job: None,
        }
    }

    /// Create the audio recorder on first use so we only query the OS once.
    fn get_recorder(&mut self) -> Result<Arc<Mutex<audio::Recorder>>> {
        if self.audio_recorder.is_none() {
            let recorder = audio::Recorder::new(self.config.input_device.as_deref())?;
            self.audio_recorder = Some(Arc::new(Mutex::new(recorder)));
        }
        Ok(self
            .audio_recorder
            .as_ref()
            .expect("recorder initialized")
            .clone())
    }

    /// Load the Whisper model lazily because it is heavy and can take seconds.
    fn get_transcriber(&mut self) -> Result<Option<Arc<Mutex<stt::Transcriber>>>> {
        if self.transcriber.is_none() {
            let Some(model_path) = self.config.whisper_model_path.clone() else {
                return Ok(None);
            };
            let transcriber = stt::Transcriber::new(&model_path)?;
            self.transcriber = Some(Arc::new(Mutex::new(transcriber)));
        }
        Ok(self.transcriber.as_ref().cloned())
    }

    /// Ensure a long-lived PTY session exists so repeated prompts avoid Codex cold-start costs.
    fn ensure_codex_session(&mut self) -> Result<()> {
        if !self.config.persistent_codex {
            bail!("persistent Codex disabled via CLI flag");
        }
        if self.codex_session.is_none() {
            self.status = "Starting persistent Codex session...".into();
            let working_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

            match pty_session::PtyCodexSession::new(
                &self.config.codex_cmd,
                working_dir.to_str().unwrap_or("."),
                &self.config.codex_args,
                &self.config.term_value,
            ) {
                Ok(session) => {
                    self.codex_session = Some(session);
                    self.status = "Codex session ready (persistent).".into();
                    log_debug("PTY Codex session started successfully");
                }
                Err(e) => {
                    let msg = format!("Failed to start Codex PTY: {e}");
                    self.status = msg.clone();
                    log_debug(&msg);
                    bail!(msg);
                }
            }
        } else {
            // Check if session is still alive
            if let Some(ref session) = self.codex_session {
                if !session.is_alive() {
                    self.status = "Codex session died, restarting...".into();
                    self.codex_session = None;
                    self.ensure_codex_session()?;
                }
            }
        }
        Ok(())
    }

    fn take_codex_session_for_job(&mut self) -> Option<pty_session::PtyCodexSession> {
        if !self.config.persistent_codex {
            return None;
        }
        if self.codex_session.is_none() {
            if let Err(err) = self.ensure_codex_session() {
                let msg = format!("Persistent Codex unavailable: {err:#}");
                log_debug(&msg);
                self.status = msg;
                return None;
            }
        }
        self.codex_session.take()
    }

    /// Append output lines while trimming scrollback so the terminal stays snappy on long sessions.
    fn append_output(&mut self, lines: Vec<String>) {
        self.output.extend(lines);
        if self.output.len() > OUTPUT_MAX_LINES {
            let excess = self.output.len().saturating_sub(OUTPUT_MAX_LINES);
            self.output.drain(0..excess);
        }
        // Scroll near the bottom when new content arrives.
        let offset = self.output.len().saturating_sub(10).min(u16::MAX as usize);
        self.scroll_offset = offset as u16;
    }

    /// Start the background worker that records speech and pipes it through STT without blocking the UI.
    pub(crate) fn start_voice_capture(&mut self, trigger: VoiceCaptureTrigger) -> Result<bool> {
        if self.voice_job.is_some() {
            if trigger == VoiceCaptureTrigger::Manual {
                self.status = "Voice capture already running; please wait...".into();
            }
            return Ok(false);
        }

        let transcriber = self.get_transcriber()?;
        if transcriber.is_none() {
            log_debug(
                "No native Whisper model configured; using python fallback for voice capture.",
            );
            if self.config.no_python_fallback {
                let msg = "Native Whisper model not configured and --no-python-fallback is set.";
                self.status = msg.into();
                bail!(msg);
            }
        }
        let mut fallback_note: Option<String> = None;
        let recorder = if transcriber.is_some() {
            match self.get_recorder() {
                Ok(recorder) => Some(recorder),
                Err(err) => {
                    if self.config.no_python_fallback {
                        let msg = format!(
                            "Audio recorder unavailable and --no-python-fallback is set: {err:#}"
                        );
                        self.status = msg.clone();
                        bail!(msg);
                    }
                    log_debug(&format!(
                        "Audio recorder unavailable ({err:#}); falling back to python pipeline."
                    ));
                    fallback_note =
                        Some("Recorder unavailable; falling back to python pipeline.".into());
                    None
                }
            }
        } else {
            None
        };
        let using_native = transcriber.is_some() && recorder.is_some();
        let job = voice::start_voice_job(recorder, transcriber.clone(), self.config.clone());
        self.voice_job = Some(job);

        let pipeline_label = if using_native {
            "Rust pipeline"
        } else {
            "Python pipeline"
        };
        self.status = match trigger {
            VoiceCaptureTrigger::Manual => format!(
                "Recording voice for {} seconds... ({pipeline_label})",
                self.config.seconds,
                pipeline_label = pipeline_label
            ),
            VoiceCaptureTrigger::Auto => format!(
                "Recording voice for {} seconds... ({pipeline_label}, auto mode)",
                self.config.seconds,
                pipeline_label = pipeline_label
            ),
        };
        if let Some(note) = fallback_note {
            self.status.push(' ');
            self.status.push_str(&note);
        }
        Ok(true)
    }

    pub(crate) fn toggle_voice_mode(&mut self) -> Result<()> {
        self.voice_enabled = !self.voice_enabled;
        if self.voice_enabled {
            if !self.start_voice_capture(VoiceCaptureTrigger::Auto)? {
                self.voice_enabled = false;
                self.status = "Voice mode failed to start; disabled voice mode.".into();
            }
        } else {
            self.status = "Voice mode disabled.".into();
        }
        Ok(())
    }

    pub(crate) fn send_current_input(&mut self) -> Result<()> {
        if self.codex_job.is_some() {
            self.status = "Codex request already running; press Esc to cancel.".into();
            return Ok(());
        }

        let prompt = self.input.trim().to_string();
        if prompt.is_empty() {
            self.status = "Nothing to send; prompt is empty.".into();
            return Ok(());
        }

        let session = self.take_codex_session_for_job();
        let job = codex::start_codex_job(prompt, self.config.clone(), session);
        self.codex_job = Some(job);
        self.codex_spinner_index = 0;
        self.codex_spinner_last_tick = Some(Instant::now());
        let spinner = CODEX_SPINNER_FRAMES.first().copied().unwrap_or('-');
        self.status = format!("Waiting for Codex {spinner} (Esc/Ctrl+C to cancel)");
        Ok(())
    }

    pub(crate) fn poll_voice_job(&mut self) -> Result<()> {
        // Check the worker channel without blocking the UI thread.
        let mut finished = false;
        let mut message_to_handle: Option<VoiceJobMessage> = None;
        if let Some(job) = self.voice_job.as_mut() {
            match job.receiver.try_recv() {
                Ok(message) => {
                    message_to_handle = Some(message);
                    finished = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.status = "Voice capture worker disconnected unexpectedly.".into();
                    finished = true;
                }
            }
            if finished {
                // Join the worker thread once it signals completion to avoid lingering handles.
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
            }
        }
        if let Some(message) = message_to_handle {
            self.handle_voice_job_message(message);
        }
        if finished {
            self.voice_job = None;
        }
        Ok(())
    }

    pub(crate) fn poll_codex_job(&mut self) -> Result<()> {
        let mut finished = false;
        let mut message_to_handle: Option<CodexJobMessage> = None;

        if let Some(job) = self.codex_job.as_mut() {
            match job.receiver.try_recv() {
                Ok(message) => {
                    message_to_handle = Some(message);
                    finished = true;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.status = "Codex worker disconnected unexpectedly.".into();
                    finished = true;
                }
            }
            if finished {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
            }
        }

        if let Some(message) = message_to_handle {
            self.handle_codex_job_message(message);
        }
        if finished {
            self.codex_job = None;
            self.codex_spinner_last_tick = None;
        }
        Ok(())
    }

    fn handle_codex_job_message(&mut self, message: CodexJobMessage) {
        match message {
            CodexJobMessage::Completed {
                lines,
                status,
                codex_session,
            } => {
                self.append_output(lines);
                self.status = status;
                self.codex_session = codex_session;
                self.input.clear();
                if self.voice_enabled {
                    if let Err(err) = self.start_voice_capture(VoiceCaptureTrigger::Auto) {
                        self.status = format!("Voice capture failed after Codex call: {err:#}");
                    }
                }
            }
            CodexJobMessage::Failed {
                error,
                codex_session,
            } => {
                self.status = format!("Codex failed: {error}");
                self.codex_session = codex_session;
            }
            CodexJobMessage::Canceled { codex_session } => {
                self.status = "Codex request canceled.".into();
                self.codex_session = codex_session;
            }
        }
    }

    pub(crate) fn cancel_codex_job_if_active(&mut self) -> bool {
        if let Some(job) = self.codex_job.as_ref() {
            job.cancel();
            self.status = "Canceling Codex request...".into();
            true
        } else {
            false
        }
    }

    pub(crate) fn update_codex_spinner(&mut self) {
        if self.codex_job.is_none() {
            return;
        }
        let now = Instant::now();
        let last_tick = self
            .codex_spinner_last_tick
            .get_or_insert_with(Instant::now);
        if now.duration_since(*last_tick) < CODEX_SPINNER_INTERVAL {
            return;
        }
        self.codex_spinner_last_tick = Some(now);
        if CODEX_SPINNER_FRAMES.is_empty() {
            return;
        }
        self.codex_spinner_index = (self.codex_spinner_index + 1) % CODEX_SPINNER_FRAMES.len();
        let spinner = CODEX_SPINNER_FRAMES[self.codex_spinner_index];
        self.status = format!("Waiting for Codex {spinner} (Esc/Ctrl+C to cancel)");
    }

    /// Update UI state based on whatever the voice worker reported (transcript, silence, or error).
    fn handle_voice_job_message(&mut self, message: VoiceJobMessage) {
        match message {
            VoiceJobMessage::Transcript { text, source } => {
                log_debug("Voice capture completed successfully");
                self.input = text;
                self.status = format!(
                    "Transcript captured ({}); edit and press Enter.",
                    source.label()
                );
            }
            VoiceJobMessage::Empty { source } => {
                log_debug("Voice capture detected no speech");
                self.input.clear();
                self.status = format!("No speech detected ({}). Try again.", source.label());
            }
            VoiceJobMessage::Error(err) => {
                log_debug(&format!("Voice capture worker error: {err}"));
                self.status = format!("Voice capture failed: {err}");
            }
        }
    }

    pub(crate) fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        }
    }

    pub(crate) fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub(crate) fn page_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(10);
    }

    pub(crate) fn page_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(10);
    }

    pub(crate) fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        let offset = self.output.len().saturating_sub(10).min(u16::MAX as usize);
        self.scroll_offset = offset as u16;
    }

    /// Returns the current input text for rendering in the UI.
    pub(crate) fn sanitized_input_text(&self) -> String {
        self.input.clone()
    }

    pub(crate) fn status_text(&self) -> &str {
        &self.status
    }

    pub(crate) fn output_lines(&self) -> &[String] {
        &self.output
    }

    pub(crate) fn get_scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub(crate) fn has_active_jobs(&self) -> bool {
        self.codex_job.is_some() || self.voice_job.is_some()
    }

    pub(crate) fn push_input_char(&mut self, ch: char) {
        self.input.push(ch);
    }

    pub(crate) fn backspace_input(&mut self) {
        self.input.pop();
    }

    pub(crate) fn clear_input(&mut self) {
        self.input.clear();
    }

    /// Keep the UI in sync with the persistent Codex PTY without blocking.
    pub(crate) fn drain_persistent_output(&mut self) {
        if let Some(ref session) = self.codex_session {
            // Collect PTY bytes and convert them into clean lines.
            let chunks = session.read_output();
            if !chunks.is_empty() {
                let mut raw = Vec::new();
                for chunk in chunks {
                    raw.extend_from_slice(&chunk);
                }
                let sanitized = codex::sanitize_pty_output(&raw);
                if !sanitized.trim().is_empty() {
                    let lines = codex::prepare_for_display(&sanitized);
                    if !lines.is_empty() {
                        self.append_output(lines);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex;
    use clap::Parser;
    use std::thread;
    use std::time::Duration;

    fn test_config() -> AppConfig {
        let mut config = AppConfig::parse_from(["codex-voice-tests"]);
        config.persistent_codex = false;  // Disable PTY in tests
        config
    }

    #[test]
    fn append_output_trims_history() {
        let config = test_config();
        let mut app = App::new(config);
        let lines = (0..600).map(|i| format!("line {i}")).collect();
        app.append_output(lines);
        assert!(app.output_lines().len() <= OUTPUT_MAX_LINES);
    }

    #[test]
    fn scroll_helpers_update_offset() {
        let config = test_config();
        let mut app = App::new(config);
        app.append_output(vec!["one".into(), "two".into(), "three".into()]);
        app.page_down();
        assert_eq!(app.get_scroll_offset(), 10);
        app.scroll_to_top();
        assert_eq!(app.get_scroll_offset(), 0);
    }

    #[test]
    fn codex_job_completion_updates_ui() {
        let config = test_config();
        let mut app = App::new(config);
        app.input = "test prompt".into();
        let (_result, hook_guard) = codex::with_job_hook(
            Box::new(|prompt, _| CodexJobMessage::Completed {
                lines: vec![format!("> {prompt}"), String::from("result line")],
                status: "ok".into(),
                codex_session: None,
            }),
            || {
                app.send_current_input().unwrap();
                wait_for_codex_job(&mut app);
            },
        );
        drop(hook_guard);
        let lines = app.output_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "> test prompt");
        assert_eq!(lines[1], "result line");
        assert_eq!(app.status_text(), "ok");
        assert!(app.sanitized_input_text().is_empty());
    }

    #[test]
    fn codex_job_cancellation_updates_status() {
        let config = test_config();
        let mut app = App::new(config);
        app.input = "test prompt".into();
        let (_result, hook_guard) = codex::with_job_hook(
            Box::new(|_, cancel| {
                while !cancel.is_cancelled() {
                    thread::sleep(Duration::from_millis(10));
                }
                CodexJobMessage::Canceled {
                    codex_session: None,
                }
            }),
            || {
                app.send_current_input().unwrap();
                assert!(app.cancel_codex_job_if_active());
                wait_for_codex_job(&mut app);
            },
        );
        drop(hook_guard);
        assert_eq!(app.status_text(), "Codex request canceled.");
        assert!(!app.cancel_codex_job_if_active());
    }

    fn wait_for_codex_job(app: &mut App) {
        for _ in 0..50 {
            app.poll_codex_job().unwrap();
            if app.codex_job.is_none() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("Codex job did not complete in time");
    }
}
