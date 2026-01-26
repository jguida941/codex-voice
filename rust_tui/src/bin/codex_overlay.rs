use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use regex::Regex;
use rust_tui::pty_session::PtyOverlaySession;
use rust_tui::{
    audio, config::AppConfig, init_debug_log_file, log_debug, log_file_path, stt, voice,
    VoiceCaptureSource, VoiceCaptureTrigger, VoiceJobMessage,
};
use std::env;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use strip_ansi_escapes::strip;

static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_sigwinch(_: libc::c_int) {
    SIGWINCH_RECEIVED.store(true, Ordering::SeqCst);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum VoiceSendMode {
    Auto,
    Insert,
}

#[derive(Debug, Parser, Clone)]
#[command(about = "Codex Voice overlay mode", author, version)]
struct OverlayConfig {
    #[command(flatten)]
    app: AppConfig,

    /// Regex used to detect the Codex prompt line (overrides auto-detection)
    #[arg(long = "prompt-regex")]
    prompt_regex: Option<String>,

    /// Log file path for prompt detection diagnostics
    #[arg(long = "prompt-log")]
    prompt_log: Option<PathBuf>,

    /// Start in auto-voice mode
    #[arg(long = "auto-voice", default_value_t = false)]
    auto_voice: bool,

    /// Idle time before auto-voice triggers when prompt detection is unknown (ms)
    #[arg(long = "auto-voice-idle-ms", default_value_t = 1200)]
    auto_voice_idle_ms: u64,

    /// Voice transcript handling (auto = send newline, insert = leave for editing)
    #[arg(long = "voice-send-mode", value_enum, default_value_t = VoiceSendMode::Auto)]
    voice_send_mode: VoiceSendMode,
}

#[derive(Debug)]
enum InputEvent {
    Bytes(Vec<u8>),
    VoiceTrigger,
    ToggleAutoVoice,
    ToggleSendMode,
    IncreaseSensitivity,
    DecreaseSensitivity,
    EnterKey,
    Exit,
}

#[derive(Debug)]
enum WriterMessage {
    PtyOutput(Vec<u8>),
    Status { text: String },
    ClearStatus,
    Resize { rows: u16, cols: u16 },
    Shutdown,
}

fn main() -> Result<()> {
    init_debug_log_file();
    let log_path = log_file_path();
    log_debug("=== Codex Voice Overlay Started ===");
    log_debug(&format!("Log file: {log_path:?}"));

    let mut config = OverlayConfig::parse();
    config.app.validate()?;

    if config.app.list_input_devices {
        list_input_devices()?;
        return Ok(());
    }

    install_sigwinch_handler();

    let working_dir = env::var("CODEX_VOICE_CWD")
        .ok()
        .or_else(|| env::current_dir().ok().map(|dir| dir.to_string_lossy().to_string()))
        .unwrap_or_else(|| ".".to_string());

    let prompt_logger = PromptLogger::new(resolve_prompt_log(&config));
    let prompt_regex = resolve_prompt_regex(&config)?;
    let mut prompt_tracker = PromptTracker::new(prompt_regex, prompt_logger);

    let mut session = PtyOverlaySession::new(
        &config.app.codex_cmd,
        &working_dir,
        &config.app.codex_args,
        &config.app.term_value,
    )?;

    enable_raw_mode()?;

    let (writer_tx, writer_rx) = unbounded();
    let _writer_handle = spawn_writer_thread(writer_rx);

    if let Ok((cols, rows)) = terminal_size() {
        let _ = session.set_winsize(rows, cols);
        let _ = writer_tx.send(WriterMessage::Resize { rows, cols });
    }

    let (input_tx, input_rx) = unbounded();
    let _input_handle = spawn_input_thread(input_tx);

    let idle_timeout = Duration::from_millis(config.auto_voice_idle_ms.max(100));
    let mut voice_manager = VoiceManager::new(config.app.clone());
    let mut auto_voice_enabled = config.auto_voice;
    let mut last_auto_trigger_at: Option<Instant> = None;
    let mut status_clear_deadline: Option<Instant> = None;

    if auto_voice_enabled {
        set_status(
            &writer_tx,
            &mut status_clear_deadline,
            "Auto-voice enabled",
            Some(Duration::from_secs(2)),
        );
    }

    let mut running = true;
    while running {
        select! {
            recv(input_rx) -> event => {
                match event {
                    Ok(InputEvent::Bytes(bytes)) => {
                        if let Err(err) = session.send_bytes(&bytes) {
                            log_debug(&format!("failed to write to PTY: {err:#}"));
                            running = false;
                        }
                    }
                    Ok(InputEvent::VoiceTrigger) => {
                        if let Err(err) = start_voice_capture(
                            &mut voice_manager,
                            VoiceCaptureTrigger::Manual,
                            &writer_tx,
                            &mut status_clear_deadline,
                        ) {
                            set_status(
                                &writer_tx,
                                &mut status_clear_deadline,
                                "Voice capture failed (see log)",
                                Some(Duration::from_secs(2)),
                            );
                            log_debug(&format!("voice capture failed: {err:#}"));
                        }
                    }
                    Ok(InputEvent::ToggleAutoVoice) => {
                        auto_voice_enabled = !auto_voice_enabled;
                        let msg = if auto_voice_enabled {
                            "Auto-voice enabled"
                        } else {
                            // Cancel any running capture when disabling auto-voice
                            if voice_manager.cancel_capture() {
                                "Auto-voice disabled (capture cancelled)"
                            } else {
                                "Auto-voice disabled"
                            }
                        };
                        set_status(
                            &writer_tx,
                            &mut status_clear_deadline,
                            msg,
                            Some(Duration::from_secs(2)),
                        );
                    }
                    Ok(InputEvent::ToggleSendMode) => {
                        config.voice_send_mode = match config.voice_send_mode {
                            VoiceSendMode::Auto => VoiceSendMode::Insert,
                            VoiceSendMode::Insert => VoiceSendMode::Auto,
                        };
                        let msg = match config.voice_send_mode {
                            VoiceSendMode::Auto => "Send mode: auto (sends Enter)",
                            VoiceSendMode::Insert => "Send mode: insert (press Enter to send)",
                        };
                        set_status(
                            &writer_tx,
                            &mut status_clear_deadline,
                            msg,
                            Some(Duration::from_secs(3)),
                        );
                    }
                    Ok(InputEvent::IncreaseSensitivity) => {
                        let threshold_db = voice_manager.adjust_sensitivity(5.0);
                        let msg = format!("Mic sensitivity: {threshold_db:.0} dB (less sensitive)");
                        set_status(
                            &writer_tx,
                            &mut status_clear_deadline,
                            &msg,
                            Some(Duration::from_secs(3)),
                        );
                    }
                    Ok(InputEvent::DecreaseSensitivity) => {
                        let threshold_db = voice_manager.adjust_sensitivity(-5.0);
                        let msg = format!("Mic sensitivity: {threshold_db:.0} dB (more sensitive)");
                        set_status(
                            &writer_tx,
                            &mut status_clear_deadline,
                            &msg,
                            Some(Duration::from_secs(3)),
                        );
                    }
                    Ok(InputEvent::EnterKey) => {
                        // In insert mode, Enter stops capture early and sends what was recorded
                        if config.voice_send_mode == VoiceSendMode::Insert && !voice_manager.is_idle() {
                            if voice_manager.active_source() == Some(VoiceCaptureSource::Python) {
                                let _ = voice_manager.cancel_capture();
                                set_status(
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    "Capture cancelled (python fallback cannot stop early)",
                                    Some(Duration::from_secs(3)),
                                );
                            } else {
                                voice_manager.request_early_stop();
                                set_status(
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    "Processing...",
                                    None,
                                );
                            }
                        } else {
                            // Forward Enter to PTY
                            if let Err(err) = session.send_bytes(&[0x0d]) {
                                log_debug(&format!("failed to write Enter to PTY: {err:#}"));
                                running = false;
                            }
                        }
                    }
                    Ok(InputEvent::Exit) => {
                        running = false;
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            recv(session.output_rx) -> chunk => {
                match chunk {
                    Ok(data) => {
                        prompt_tracker.feed_output(&data);
                        if writer_tx.send(WriterMessage::PtyOutput(data)).is_err() {
                            running = false;
                        }
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            default(Duration::from_millis(50)) => {
                if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                    if let Ok((cols, rows)) = terminal_size() {
                        let _ = session.set_winsize(rows, cols);
                        let _ = writer_tx.send(WriterMessage::Resize { rows, cols });
                    }
                }

                let now = Instant::now();
                prompt_tracker.on_idle(now, idle_timeout);

                if let Some(message) = voice_manager.poll_message() {
                    handle_voice_message(
                        message,
                        &config,
                        &mut session,
                        &writer_tx,
                        &mut status_clear_deadline,
                    );
                }

                if auto_voice_enabled && voice_manager.is_idle() {
                    if should_auto_trigger(
                        &prompt_tracker,
                        now,
                        idle_timeout,
                        last_auto_trigger_at,
                    ) {
                        if let Err(err) = start_voice_capture(
                            &mut voice_manager,
                            VoiceCaptureTrigger::Auto,
                            &writer_tx,
                            &mut status_clear_deadline,
                        ) {
                            log_debug(&format!("auto voice capture failed: {err:#}"));
                        } else {
                            last_auto_trigger_at = Some(now);
                        }
                    }
                }

                if let Some(deadline) = status_clear_deadline {
                    if now >= deadline {
                        let _ = writer_tx.send(WriterMessage::ClearStatus);
                        status_clear_deadline = None;
                    }
                }
            }
        }
    }

    let _ = writer_tx.send(WriterMessage::ClearStatus);
    let _ = writer_tx.send(WriterMessage::Shutdown);
    disable_raw_mode()?;
    log_debug("=== Codex Voice Overlay Exiting ===");
    Ok(())
}

fn list_input_devices() -> Result<()> {
    let devices = audio::Recorder::list_devices()?;
    if devices.is_empty() {
        println!("No audio input devices detected.");
    } else {
        println!("Available audio input devices:");
        for name in devices {
            println!("  - {name}");
        }
    }
    Ok(())
}

fn install_sigwinch_handler() {
    unsafe {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let handler = handle_sigwinch as libc::sighandler_t;
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let handler = Some(handle_sigwinch);
        if libc::signal(libc::SIGWINCH, handler) == libc::SIG_ERR {
            log_debug("failed to install SIGWINCH handler");
        }
    }
}

fn resolve_prompt_log(config: &OverlayConfig) -> PathBuf {
    if let Some(path) = &config.prompt_log {
        return path.clone();
    }
    if let Ok(path) = env::var("CODEX_OVERLAY_PROMPT_LOG") {
        return PathBuf::from(path);
    }
    env::temp_dir().join("codex_overlay_prompt.log")
}

fn resolve_prompt_regex(config: &OverlayConfig) -> Result<Option<Regex>> {
    let raw = config
        .prompt_regex
        .clone()
        .or_else(|| env::var("CODEX_OVERLAY_PROMPT_REGEX").ok());
    let Some(pattern) = raw else {
        return Ok(None);
    };
    let regex = Regex::new(&pattern).with_context(|| format!("invalid prompt regex: {pattern}"))?;
    Ok(Some(regex))
}

fn spawn_input_thread(tx: Sender<InputEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1024];
        let mut skip_lf = false;
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    log_debug(&format!("stdin read error: {err}"));
                    break;
                }
            };
            let mut pending = Vec::new();
            for &byte in &buf[..n] {
                if skip_lf {
                    if byte == 0x0a {
                        skip_lf = false;
                        continue;
                    }
                    skip_lf = false;
                }
                match byte {
                    0x11 => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::Exit).is_err() {
                            return;
                        }
                    }
                    0x12 => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::VoiceTrigger).is_err() {
                            return;
                        }
                    }
                    0x16 => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::ToggleAutoVoice).is_err() {
                            return;
                        }
                    }
                    0x14 => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::ToggleSendMode).is_err() {
                            return;
                        }
                    }
                    0x1d => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::IncreaseSensitivity).is_err() {
                            return;
                        }
                    }
                    0x1f => {
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::DecreaseSensitivity).is_err() {
                            return;
                        }
                    }
                    0x0d | 0x0a => {
                        // Enter key - send as separate event so main loop can intercept it
                        if !pending.is_empty() {
                            if tx.send(InputEvent::Bytes(pending)).is_err() {
                                return;
                            }
                            pending = Vec::new();
                        }
                        if tx.send(InputEvent::EnterKey).is_err() {
                            return;
                        }
                        if byte == 0x0d {
                            skip_lf = true;
                        }
                    }
                    _ => pending.push(byte),
                }
            }
            if !pending.is_empty() {
                if tx.send(InputEvent::Bytes(pending)).is_err() {
                    return;
                }
            }
        }
    })
}

fn spawn_writer_thread(rx: Receiver<WriterMessage>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut status: Option<String> = None;
        let mut rows = 0u16;
        let mut cols = 0u16;

        loop {
            match rx.recv() {
                Ok(WriterMessage::PtyOutput(bytes)) => {
                    if stdout.write_all(&bytes).is_err() {
                        break;
                    }
                    if let Some(text) = status.as_deref() {
                        let _ = write_status_line(&mut stdout, text, rows, cols);
                    }
                    let _ = stdout.flush();
                }
                Ok(WriterMessage::Status { text }) => {
                    status = Some(text);
                    let _ = write_status_line(&mut stdout, status.as_deref().unwrap_or(""), rows, cols);
                    let _ = stdout.flush();
                }
                Ok(WriterMessage::ClearStatus) => {
                    status = None;
                    let _ = clear_status_line(&mut stdout, rows, cols);
                    let _ = stdout.flush();
                }
                Ok(WriterMessage::Resize { rows: r, cols: c }) => {
                    rows = r;
                    cols = c;
                    if let Some(text) = status.as_deref() {
                        let _ = write_status_line(&mut stdout, text, rows, cols);
                        let _ = stdout.flush();
                    }
                }
                Ok(WriterMessage::Shutdown) | Err(_) => {
                    break;
                }
            }
        }
    })
}

fn write_status_line(stdout: &mut io::Stdout, text: &str, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let sanitized = sanitize_status(text);
    let trimmed = truncate_status(&sanitized, cols as usize);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7");
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(trimmed.as_bytes());
    sequence.extend_from_slice(b"\x1b8");
    stdout.write_all(&sequence)
}

fn clear_status_line(stdout: &mut io::Stdout, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7");
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(b"\x1b8");
    stdout.write_all(&sequence)
}

fn sanitize_status(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_ascii_graphic() || ch == ' ' { ch } else { ' ' })
        .collect()
}

fn truncate_status(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    text.chars().take(max).collect()
}

fn set_status(
    writer_tx: &Sender<WriterMessage>,
    clear_deadline: &mut Option<Instant>,
    text: &str,
    clear_after: Option<Duration>,
) {
    let _ = writer_tx.send(WriterMessage::Status {
        text: text.to_string(),
    });
    *clear_deadline = clear_after.map(|duration| Instant::now() + duration);
}

fn start_voice_capture(
    voice_manager: &mut VoiceManager,
    trigger: VoiceCaptureTrigger,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
) -> Result<()> {
    match voice_manager.start_capture(trigger)? {
        Some(info) => {
            let mut status = format!("Listening... ({})", info.pipeline_label);
            if let Some(note) = info.fallback_note {
                status.push(' ');
                status.push_str(&note);
            }
            set_status(writer_tx, status_clear_deadline, &status, None);
            Ok(())
        }
        None => {
            if trigger == VoiceCaptureTrigger::Manual {
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    "Voice capture already running",
                    Some(Duration::from_secs(2)),
                );
            }
            Ok(())
        }
    }
}

fn handle_voice_message(
    message: VoiceJobMessage,
    config: &OverlayConfig,
    session: &mut PtyOverlaySession,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
) {
    match message {
        VoiceJobMessage::Transcript { text, source } => {
            let label = source.label();
            let status = format!("Transcript ready ({label})");
            set_status(writer_tx, status_clear_deadline, &status, Some(Duration::from_secs(2)));
            if let Err(err) = send_transcript(session, &text, config.voice_send_mode) {
                log_debug(&format!("failed to send transcript: {err:#}"));
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    "Failed to send transcript (see log)",
                    Some(Duration::from_secs(2)),
                );
            }
        }
        VoiceJobMessage::Empty { source } => {
            let label = source.label();
            let status = format!("No speech detected ({label})");
            set_status(writer_tx, status_clear_deadline, &status, Some(Duration::from_secs(2)));
        }
        VoiceJobMessage::Error(message) => {
            set_status(
                writer_tx,
                status_clear_deadline,
                "Voice capture error (see log)",
                Some(Duration::from_secs(2)),
            );
            log_debug(&format!("voice capture error: {message}"));
        }
    }
}

fn send_transcript(
    session: &mut PtyOverlaySession,
    text: &str,
    mode: VoiceSendMode,
) -> Result<()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    match mode {
        VoiceSendMode::Auto => session.send_text_with_newline(trimmed)?,
        VoiceSendMode::Insert => session.send_text(trimmed)?,
    }
    Ok(())
}

fn should_auto_trigger(
    prompt_tracker: &PromptTracker,
    now: Instant,
    idle_timeout: Duration,
    last_trigger_at: Option<Instant>,
) -> bool {
    if !prompt_tracker.has_seen_output() {
        return false;
    }
    if let Some(prompt_at) = prompt_tracker.last_prompt_seen_at() {
        if last_trigger_at.map_or(true, |last| prompt_at > last) {
            return true;
        }
    }
    if prompt_tracker.idle_ready(now, idle_timeout) {
        if last_trigger_at.map_or(true, |last| prompt_tracker.last_output_at() > last) {
            return true;
        }
    }
    false
}

struct VoiceStartInfo {
    pipeline_label: &'static str,
    fallback_note: Option<String>,
}

struct VoiceManager {
    config: AppConfig,
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    job: Option<voice::VoiceJob>,
    cancel_pending: bool,
    active_source: Option<VoiceCaptureSource>,
}

impl VoiceManager {
    fn new(config: AppConfig) -> Self {
        Self {
            config,
            recorder: None,
            transcriber: None,
            job: None,
            cancel_pending: false,
            active_source: None,
        }
    }

    fn adjust_sensitivity(&mut self, delta_db: f32) -> f32 {
        const MIN_DB: f32 = -80.0;
        const MAX_DB: f32 = -10.0;
        let mut next = self.config.voice_vad_threshold_db + delta_db;
        if next < MIN_DB {
            next = MIN_DB;
        } else if next > MAX_DB {
            next = MAX_DB;
        }
        self.config.voice_vad_threshold_db = next;
        next
    }

    fn is_idle(&self) -> bool {
        self.job.is_none()
    }

    fn active_source(&self) -> Option<VoiceCaptureSource> {
        self.active_source
    }

    /// Cancel any running voice capture. Returns true if a capture was cancelled.
    fn cancel_capture(&mut self) -> bool {
        if let Some(ref job) = self.job {
            job.request_stop();
            self.cancel_pending = true;
            log_debug("voice capture cancel requested");
            true
        } else {
            false
        }
    }

    /// Request early stop of voice capture (stop recording and process what was captured).
    /// Returns true if a capture was running and will be stopped.
    fn request_early_stop(&mut self) -> bool {
        if let Some(ref job) = self.job {
            job.request_stop();
            log_debug("voice capture early stop requested");
            true
        } else {
            false
        }
    }

    fn start_capture(&mut self, trigger: VoiceCaptureTrigger) -> Result<Option<VoiceStartInfo>> {
        if self.job.is_some() {
            return Ok(None);
        }

        let transcriber = self.get_transcriber()?;
        if transcriber.is_none() {
            log_debug("No native Whisper model configured; using python fallback for voice capture.");
            if self.config.no_python_fallback {
                return Err(anyhow!(
                    "Native Whisper model not configured and --no-python-fallback is set."
                ));
            }
        }

        let mut fallback_note: Option<String> = None;
        let recorder = if transcriber.is_some() {
            match self.get_recorder() {
                Ok(recorder) => Some(recorder),
                Err(err) => {
                    if self.config.no_python_fallback {
                        return Err(anyhow!(
                            "Audio recorder unavailable and --no-python-fallback is set: {err:#}"
                        ));
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
        self.job = Some(job);
        self.cancel_pending = false;
        self.active_source = Some(if using_native {
            VoiceCaptureSource::Native
        } else {
            VoiceCaptureSource::Python
        });

        let pipeline_label = if using_native {
            "Rust pipeline"
        } else {
            "Python pipeline"
        };

        let status = match trigger {
            VoiceCaptureTrigger::Manual => "manual",
            VoiceCaptureTrigger::Auto => "auto",
        };
        log_debug(&format!("voice capture started ({status}) using {pipeline_label}"));

        Ok(Some(VoiceStartInfo {
            pipeline_label,
            fallback_note,
        }))
    }

    fn poll_message(&mut self) -> Option<VoiceJobMessage> {
        let Some(job) = self.job.as_mut() else {
            return None;
        };
        match job.receiver.try_recv() {
            Ok(message) => {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
                self.job = None;
                self.active_source = None;
                if self.cancel_pending {
                    self.cancel_pending = false;
                    log_debug("voice capture cancelled; dropping message");
                    None
                } else {
                    Some(message)
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                if let Some(handle) = job.handle.take() {
                    let _ = handle.join();
                }
                self.job = None;
                self.active_source = None;
                let was_cancelled = self.cancel_pending;
                self.cancel_pending = false;
                if was_cancelled {
                    log_debug("voice capture cancelled; worker disconnected");
                    None
                } else {
                    Some(VoiceJobMessage::Error(
                        "voice capture worker disconnected unexpectedly".to_string(),
                    ))
                }
            }
        }
    }

    fn get_recorder(&mut self) -> Result<Arc<Mutex<audio::Recorder>>> {
        if self.recorder.is_none() {
            let recorder = audio::Recorder::new(self.config.input_device.as_deref())?;
            self.recorder = Some(Arc::new(Mutex::new(recorder)));
        }
        Ok(self
            .recorder
            .as_ref()
            .expect("recorder initialized")
            .clone())
    }

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
}

struct PromptLogger {
    path: PathBuf,
}

impl PromptLogger {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn log(&self, message: &str) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "[{timestamp}] {message}");
        }
    }
}

struct PromptTracker {
    regex: Option<Regex>,
    learned_prompt: Option<String>,
    last_prompt_seen_at: Option<Instant>,
    last_output_at: Instant,
    has_seen_output: bool,
    current_line: Vec<u8>,
    last_line: Option<String>,
    prompt_logger: PromptLogger,
}

impl PromptTracker {
    fn new(regex: Option<Regex>, prompt_logger: PromptLogger) -> Self {
        Self {
            regex,
            learned_prompt: None,
            last_prompt_seen_at: None,
            last_output_at: Instant::now(),
            has_seen_output: false,
            current_line: Vec::new(),
            last_line: None,
            prompt_logger,
        }
    }

    fn feed_output(&mut self, bytes: &[u8]) {
        self.last_output_at = Instant::now();
        self.has_seen_output = true;

        let cleaned = strip(bytes);
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

    fn on_idle(&mut self, now: Instant, idle_timeout: Duration) {
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
        if self.learned_prompt.is_none() && self.regex.is_none() {
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
        if let Some(regex) = &self.regex {
            return regex.is_match(line);
        }
        if let Some(prompt) = &self.learned_prompt {
            return line.trim_end() == prompt.trim_end();
        }
        false
    }

    fn update_prompt_seen(&mut self, now: Instant, line: &str, reason: &str) {
        self.last_prompt_seen_at = Some(now);
        self.prompt_logger
            .log(&format!("prompt_detected|reason={reason}|line={line}"));
    }

    fn current_line_as_string(&self) -> String {
        String::from_utf8_lossy(&self.current_line).to_string()
    }

    fn last_prompt_seen_at(&self) -> Option<Instant> {
        self.last_prompt_seen_at
    }

    fn last_output_at(&self) -> Instant {
        self.last_output_at
    }

    fn idle_ready(&self, now: Instant, idle_timeout: Duration) -> bool {
        now.duration_since(self.last_output_at) >= idle_timeout
    }

    fn has_seen_output(&self) -> bool {
        self.has_seen_output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn prompt_tracker_learns_prompt_on_idle() {
        let logger = PromptLogger::new(env::temp_dir().join("codex_overlay_prompt_test.log"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(2000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_matches_regex() {
        let logger = PromptLogger::new(env::temp_dir().join("codex_overlay_prompt_test.log"));
        let regex = Regex::new(r"^codex> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), logger);
        tracker.feed_output(b"codex> \n");
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn cancel_capture_suppresses_voice_message() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (tx, rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_worker = stop_flag.clone();
        let handle = thread::spawn(move || {
            while !stop_flag_worker.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(5));
            }
            let _ = tx.send(VoiceJobMessage::Empty {
                source: VoiceCaptureSource::Native,
            });
        });
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: Some(handle),
            stop_flag: stop_flag.clone(),
        });
        manager.active_source = Some(VoiceCaptureSource::Native);

        assert!(manager.cancel_capture());
        assert!(stop_flag.load(Ordering::Relaxed));

        for _ in 0..50 {
            manager.poll_message();
            if manager.job.is_none() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(manager.job.is_none());
        assert!(!manager.cancel_pending);
    }
}
