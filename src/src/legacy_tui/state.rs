//! Legacy Codex TUI state that coordinates rendering, input, and voice jobs.
//!
//! The multi-backend overlay lives under `src/bin/voiceterm/`.

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::TryRecvError,
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use crate::codex::{
    CodexBackendError, CodexCliBackend, CodexEvent, CodexEventKind, CodexJob, CodexJobRunner,
    CodexRequest, CODEX_SPINNER_FRAMES,
};
use crate::config::AppConfig;
use crate::voice::{self, VoiceCaptureTrigger, VoiceJob, VoiceJobMessage};
use crate::{audio, log_debug, stt};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

/// Maximum number of lines to retain in the scrollback buffer.
pub(super) const OUTPUT_MAX_LINES: usize = 500;
/// Maximum characters retained in the input buffer.
pub(super) const INPUT_MAX_CHARS: usize = 8_000;
/// Spinner cadence for Codex worker status updates.
const CODEX_SPINNER_INTERVAL: Duration = Duration::from_millis(150);

macro_rules! state_change {
    ($self:expr, $field:ident, $value:expr) => {{
        $self.$field = $value;
        $self.request_redraw();
    }};
    ($self:expr, $body:block) => {{
        $body
        $self.request_redraw();
    }};
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
    /// Legacy field from the python fallback pipeline; retained for compatibility.
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
pub(crate) fn run_python_transcription(
    config: &AppConfig,
    stop_flag: Option<Arc<AtomicBool>>,
) -> Result<PipelineJsonResult> {
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
    let (status, stdout_bytes, stderr_bytes) = if let Some(flag) = stop_flag {
        let mut child = cmd
            .spawn()
            .context("failed to run python fallback pipeline")?;
        let mut stdout = child
            .stdout
            .take()
            .context("failed to capture python fallback stdout")?;
        let mut stderr = child
            .stderr
            .take()
            .context("failed to capture python fallback stderr")?;
        loop {
            if flag.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!("python fallback cancelled"));
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let mut out = Vec::new();
                    let mut err = Vec::new();
                    stdout
                        .read_to_end(&mut out)
                        .context("failed to read python fallback stdout")?;
                    stderr
                        .read_to_end(&mut err)
                        .context("failed to read python fallback stderr")?;
                    break (status, out, err);
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(err) => return Err(anyhow!("python fallback wait failed: {err}")),
            }
        }
    } else {
        let output = cmd
            .output()
            .context("failed to run python fallback pipeline")?;
        (output.status, output.stdout, output.stderr)
    };

    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
    if !status.success() {
        return Err(anyhow!(
            "python fallback failed with status {}.\nstdout:\n{}\nstderr:\n{}",
            status,
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
pub struct CodexApp {
    config: AppConfig,
    pub(super) input: String,
    output: Vec<String>,
    status: String,
    voice_enabled: bool,
    scroll_offset: u16,
    codex_backend: Arc<dyn CodexJobRunner>,
    pub(super) codex_job: Option<CodexJob>,
    codex_spinner_index: usize,
    codex_spinner_last_tick: Option<Instant>,
    needs_redraw: bool,
    audio_recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    voice_job: Option<VoiceJob>,
}

impl CodexApp {
    /// Create the application state with default status text.
    pub fn new(config: AppConfig) -> Self {
        let backend: Arc<dyn CodexJobRunner> = Arc::new(CodexCliBackend::new(config.clone()));
        Self {
            config,
            input: String::new(),
            output: Vec::new(),
            status: "Ready. Press Ctrl+R for voice capture.".into(),
            voice_enabled: false,
            scroll_offset: 0,
            codex_backend: backend,
            codex_job: None,
            codex_spinner_index: 0,
            codex_spinner_last_tick: None,
            needs_redraw: true,
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

    /// Append output lines while trimming scrollback so the terminal stays snappy on long sessions.
    pub(super) fn append_output(&mut self, lines: Vec<String>) {
        self.output.extend(lines);
        if self.output.len() > OUTPUT_MAX_LINES {
            let excess = self.output.len().saturating_sub(OUTPUT_MAX_LINES);
            self.output.drain(0..excess);
        }
        // Scroll near the bottom when new content arrives.
        let offset = self.output.len().saturating_sub(10).min(u16::MAX as usize);
        self.scroll_offset = offset as u16;
        self.request_redraw();
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
        let job = voice::start_voice_job(recorder, transcriber.clone(), self.config.clone(), None);
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
        self.request_redraw();
        Ok(true)
    }

    pub(crate) fn toggle_voice_mode(&mut self) -> Result<()> {
        self.voice_enabled = !self.voice_enabled;
        if self.voice_enabled {
            if !self.start_voice_capture(VoiceCaptureTrigger::Auto)? {
                self.voice_enabled = false;
                self.status = "Voice mode failed to start; disabled voice mode.".into();
                self.request_redraw();
            }
        } else {
            self.status = "Voice mode disabled.".into();
            self.request_redraw();
        }
        Ok(())
    }

    pub(crate) fn send_current_input(&mut self) -> Result<()> {
        if self.codex_job.is_some() {
            state_change!(
                self,
                status,
                "Codex request already running; press Esc to cancel.".into()
            );
            return Ok(());
        }

        let prompt = self.input.trim().to_string();
        if prompt.is_empty() {
            state_change!(self, status, "Nothing to send; prompt is empty.".into());
            return Ok(());
        }

        let request = CodexRequest::chat(prompt);
        match self.codex_backend.start(request) {
            Ok(job) => {
                self.codex_job = Some(job);
            }
            Err(err) => {
                let reason = match err {
                    CodexBackendError::InvalidRequest(msg) => msg.to_string(),
                    CodexBackendError::BackendDisabled(msg) => msg,
                };
                state_change!(self, status, format!("Codex unavailable: {reason}"));
                return Ok(());
            }
        }
        self.codex_spinner_index = 0;
        self.codex_spinner_last_tick = Some(Instant::now());
        let spinner = CODEX_SPINNER_FRAMES.first().copied().unwrap_or("⠋");
        state_change!(
            self,
            status,
            format!("Waiting for Codex {spinner} (Esc/Ctrl+C to cancel)")
        );
        state_change!(self, {
            self.input.clear();
        });
        Ok(())
    }

    pub(crate) fn poll_voice_job(&mut self) -> Result<()> {
        // Check the worker channel without blocking the UI thread.
        let mut finished = false;
        let mut message_to_handle: Option<VoiceJobMessage> = None;
        let mut auto_restart = false;
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
            auto_restart = self.handle_voice_job_message(message);
        }
        if finished {
            self.voice_job = None;
            if auto_restart && self.voice_enabled {
                if let Err(err) = self.start_voice_capture(VoiceCaptureTrigger::Auto) {
                    self.status = format!("Voice capture failed to restart: {err:#}");
                    self.request_redraw();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn poll_codex_job(&mut self) -> Result<()> {
        let mut pending_events = Vec::new();
        let mut worker_disconnected = false;
        let mut job_handle: Option<thread::JoinHandle<()>> = None;
        let mut empty_signal_spins = 0usize;

        {
            if let Some(job) = self.codex_job.as_mut() {
                loop {
                    match job.try_recv_signal() {
                        Ok(()) => {
                            let drained = job.drain_events();
                            if drained.is_empty() {
                                empty_signal_spins += 1;
                                if empty_signal_spins > 3 {
                                    break;
                                }
                            } else {
                                empty_signal_spins = 0;
                                pending_events.extend(drained);
                            }
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            worker_disconnected = true;
                            pending_events.extend(job.drain_events());
                            break;
                        }
                    }
                }
                if worker_disconnected {
                    if let Some(handle) = job.take_handle() {
                        job_handle = Some(handle);
                    }
                }
            }
        }

        let mut terminal_event_seen = false;
        for event in pending_events {
            if self.handle_backend_event(event) {
                terminal_event_seen = true;
            }
        }

        let finished = worker_disconnected || terminal_event_seen;
        if finished {
            if worker_disconnected && !terminal_event_seen {
                self.status = "Codex worker disconnected unexpectedly.".into();
            }
            if job_handle.is_none() {
                if let Some(job) = self.codex_job.as_mut() {
                    if let Some(handle) = job.take_handle() {
                        job_handle = Some(handle);
                    }
                }
            }
            self.codex_job = None;
            self.codex_spinner_last_tick = None;
        }

        if let Some(handle) = job_handle {
            let _ = handle.join();
        }

        Ok(())
    }

    pub(super) fn handle_backend_event(&mut self, event: CodexEvent) -> bool {
        match event.kind {
            CodexEventKind::Started { .. } => false,
            CodexEventKind::Status { message } => {
                state_change!(self, status, message);
                false
            }
            CodexEventKind::Token { .. } => {
                // Streaming tokens will be handled in Phase 2. Ignore for now.
                false
            }
            CodexEventKind::RecoverableError { message, .. } => {
                state_change!(self, status, message);
                false
            }
            CodexEventKind::FatalError {
                message,
                disable_pty: _,
                ..
            } => {
                state_change!(self, status, format!("Codex failed: {message}"));
                state_change!(self, {
                    self.input.clear();
                });
                true
            }
            CodexEventKind::Finished { lines, status, .. } => {
                self.append_output(lines);
                state_change!(self, status, status);
                state_change!(self, {
                    self.input.clear();
                });
                if self.voice_enabled {
                    if let Err(err) = self.start_voice_capture(VoiceCaptureTrigger::Auto) {
                        state_change!(
                            self,
                            status,
                            format!("Voice capture failed after Codex call: {err:#}")
                        );
                    }
                }
                true
            }
            CodexEventKind::Canceled { .. } => {
                state_change!(self, status, "Codex request canceled.".into());
                state_change!(self, {
                    self.input.clear();
                });
                true
            }
        }
    }

    pub(crate) fn cancel_codex_job_if_active(&mut self) -> bool {
        if let Some(job) = self.codex_job.as_ref() {
            job.cancel();
            self.status = "Canceling Codex request...".into();
            self.request_redraw();
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
        self.codex_spinner_index = (self.codex_spinner_index + 1) % CODEX_SPINNER_FRAMES.len();
        let spinner = CODEX_SPINNER_FRAMES[self.codex_spinner_index];
        self.status = format!("Waiting for Codex {} (Esc/Ctrl+C to cancel)", spinner);
        self.request_redraw();
    }

    /// Update UI state based on whatever the voice worker reported (transcript, silence, or error).
    fn handle_voice_job_message(&mut self, message: VoiceJobMessage) -> bool {
        let mut auto_restart = false;
        match message {
            VoiceJobMessage::Transcript {
                text,
                source,
                metrics,
            } => {
                log_debug("Voice capture completed successfully");
                let mut input = text;
                let truncated = if input.len() > INPUT_MAX_CHARS {
                    input.truncate(INPUT_MAX_CHARS);
                    true
                } else {
                    false
                };
                self.input = input;
                let drop_note = metrics
                    .as_ref()
                    .filter(|metrics| metrics.frames_dropped > 0)
                    .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
                let drop_suffix = drop_note
                    .as_ref()
                    .map(|note| format!(", {note}"))
                    .unwrap_or_default();
                if truncated {
                    self.status = format!(
                        "Transcript captured ({}{drop_suffix}); truncated to {INPUT_MAX_CHARS} chars.",
                        source.label(),
                        drop_suffix = drop_suffix
                    );
                } else {
                    self.status = format!(
                        "Transcript captured ({}{drop_suffix}); edit and press Enter.",
                        source.label(),
                        drop_suffix = drop_suffix
                    );
                }
            }
            VoiceJobMessage::Empty { source, metrics } => {
                log_debug("Voice capture detected no speech");
                let drop_note = metrics
                    .as_ref()
                    .filter(|metrics| metrics.frames_dropped > 0)
                    .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
                let drop_suffix = drop_note
                    .as_ref()
                    .map(|note| format!(", {note}"))
                    .unwrap_or_default();
                if self.voice_enabled {
                    if let Some(note) = drop_note {
                        self.status = format!("Auto-voice enabled ({note})");
                    } else {
                        self.status = "Auto-voice enabled.".into();
                    }
                    auto_restart = true;
                } else {
                    self.input.clear();
                    self.status = format!(
                        "No speech detected ({}{drop_suffix}). Try again.",
                        source.label(),
                        drop_suffix = drop_suffix
                    );
                }
            }
            VoiceJobMessage::Error(err) => {
                log_debug(&format!("Voice capture worker error: {err}"));
                self.status = format!("Voice capture failed: {err}");
            }
        }
        self.request_redraw();
        auto_restart
    }

    pub(crate) fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            state_change!(self, scroll_offset, self.scroll_offset.saturating_sub(1));
        }
    }

    pub(crate) fn scroll_down(&mut self) {
        state_change!(self, scroll_offset, self.scroll_offset.saturating_add(1));
    }

    pub(crate) fn page_up(&mut self) {
        state_change!(self, scroll_offset, self.scroll_offset.saturating_sub(10));
    }

    pub(crate) fn page_down(&mut self) {
        state_change!(self, scroll_offset, self.scroll_offset.saturating_add(10));
    }

    pub(crate) fn scroll_to_top(&mut self) {
        state_change!(self, scroll_offset, 0);
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        let offset = self.output.len().saturating_sub(10).min(u16::MAX as usize);
        state_change!(self, scroll_offset, offset as u16);
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

    pub(crate) fn request_redraw(&mut self) {
        self.needs_redraw = true;
    }

    pub(crate) fn take_redraw_request(&mut self) -> bool {
        let requested = self.needs_redraw;
        self.needs_redraw = false;
        requested
    }

    pub(crate) fn push_input_char(&mut self, ch: char) {
        if self.input.len() >= INPUT_MAX_CHARS {
            let msg = format!("Input limit reached (max {INPUT_MAX_CHARS} chars).");
            if self.status != msg {
                self.status = msg;
                self.request_redraw();
            }
            return;
        }
        state_change!(self, {
            self.input.push(ch);
        });
    }

    pub(crate) fn backspace_input(&mut self) {
        state_change!(self, {
            self.input.pop();
        });
    }

    pub(crate) fn clear_input(&mut self) {
        state_change!(self, {
            self.input.clear();
        });
    }

    /// Drain any background Codex session output. The backend now owns PTY state so this is a no-op.
    pub(crate) fn drain_persistent_output(&mut self) {
        // Intentionally left blank until the backend exposes a PTY polling hook.
    }
}
