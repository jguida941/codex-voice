use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use crossbeam_channel::{select, unbounded, Receiver, Sender};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size as terminal_size};
use regex::Regex;
use rust_tui::pty_session::PtyOverlaySession;
use rust_tui::{
    audio, config::AppConfig, init_debug_log_file, log_debug, log_file_path, mic_meter, stt,
    voice, VoiceCaptureSource, VoiceCaptureTrigger, VoiceJobMessage,
};
use std::collections::VecDeque;
use std::env;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use vte::{Parser as VteParser, Perform};

static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);
const MAX_PENDING_TRANSCRIPTS: usize = 5;

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

#[derive(Debug, PartialEq, Eq)]
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

trait TranscriptSession {
    fn send_text(&mut self, text: &str) -> Result<()>;
    fn send_text_with_newline(&mut self, text: &str) -> Result<()>;
}

impl TranscriptSession for PtyOverlaySession {
    fn send_text(&mut self, text: &str) -> Result<()> {
        self.send_text(text)
    }

    fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
        self.send_text_with_newline(text)
    }
}

fn main() -> Result<()> {
    init_debug_log_file();
    let log_path = log_file_path();
    log_debug("=== Codex Voice Overlay Started ===");
    log_debug(&format!("Log file: {log_path:?}"));

    let mut config = OverlayConfig::parse();
    if config.app.list_input_devices {
        list_input_devices()?;
        return Ok(());
    }

    if config.app.mic_meter {
        mic_meter::run_mic_meter(&config.app)?;
        return Ok(());
    }

    config.app.validate()?;

    install_sigwinch_handler()?;

    let working_dir = env::var("CODEX_VOICE_CWD")
        .ok()
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|dir| dir.to_string_lossy().to_string())
        })
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
    let mut last_enter_at: Option<Instant> = None;
    let mut pending_transcripts: VecDeque<PendingTranscript> = VecDeque::new();
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
                            } else {
                                last_enter_at = Some(Instant::now());
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
                        try_flush_pending(
                            &mut pending_transcripts,
                            &prompt_tracker,
                            &mut last_enter_at,
                            &mut session,
                            &writer_tx,
                            &mut status_clear_deadline,
                        );
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
                    let rearm_auto = matches!(
                        message,
                        VoiceJobMessage::Empty { .. } | VoiceJobMessage::Error(_)
                    );
                    match message {
                        VoiceJobMessage::Transcript { text, source } => {
                            if prompt_ready(&prompt_tracker, last_enter_at) {
                                deliver_transcript(
                                    &text,
                                    source,
                                    config.voice_send_mode,
                                    &mut session,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    pending_transcripts.len(),
                                );
                                last_enter_at = Some(now);
                            } else {
                                queue_transcript(
                                    &mut pending_transcripts,
                                    PendingTranscript {
                                        text,
                                        source,
                                        mode: config.voice_send_mode,
                                    },
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                );
                            }
                        }
                        other => {
                            handle_voice_message(
                                other,
                                &config,
                                &mut session,
                                &writer_tx,
                                &mut status_clear_deadline,
                                auto_voice_enabled,
                            );
                        }
                    }
                    if auto_voice_enabled && rearm_auto {
                        // Treat empty/error captures as activity so auto-voice can re-arm after idle.
                        prompt_tracker.note_activity(now);
                    }
                }

                try_flush_pending(
                    &mut pending_transcripts,
                    &prompt_tracker,
                    &mut last_enter_at,
                    &mut session,
                    &writer_tx,
                    &mut status_clear_deadline,
                );

                if auto_voice_enabled
                    && voice_manager.is_idle()
                    && should_auto_trigger(
                        &prompt_tracker,
                        now,
                        idle_timeout,
                        last_auto_trigger_at,
                    )
                {
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

fn queue_transcript(
    pending: &mut VecDeque<PendingTranscript>,
    transcript: PendingTranscript,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
) {
    if pending.len() >= MAX_PENDING_TRANSCRIPTS {
        pending.pop_front();
        log_debug("pending transcript queue full; dropping oldest transcript");
    }
    pending.push_back(transcript);
    let status = format!("Transcript queued ({})", pending.len());
    set_status(writer_tx, status_clear_deadline, &status, None);
}

fn try_flush_pending(
    pending: &mut VecDeque<PendingTranscript>,
    prompt_tracker: &PromptTracker,
    last_enter_at: &mut Option<Instant>,
    session: &mut impl TranscriptSession,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
) {
    if pending.is_empty() {
        return;
    }
    if !prompt_ready(prompt_tracker, *last_enter_at) {
        return;
    }
    if let Some(next) = pending.pop_front() {
        let remaining = pending.len();
        deliver_transcript(
            &next.text,
            next.source,
            next.mode,
            session,
            writer_tx,
            status_clear_deadline,
            remaining,
        );
        *last_enter_at = Some(Instant::now());
    }
}
                        }
                        other => {
                            handle_voice_message(
                                other,
                                &config,
                                &mut session,
                                &writer_tx,
                                &mut status_clear_deadline,
                                auto_voice_enabled,
                            );
                        }
                    }
                    if auto_voice_enabled && rearm_auto {
                        // Treat empty/error captures as activity so auto-voice can re-arm after idle.
                        prompt_tracker.note_activity(now);
                    }
                }

                if auto_voice_enabled
                    && voice_manager.is_idle()
                    && should_auto_trigger(
                        &prompt_tracker,
                        now,
                        idle_timeout,
                        last_auto_trigger_at,
                    )
                {
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
    match audio::Recorder::list_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("No audio input devices detected.");
            } else {
                println!("Available audio input devices:");
                for name in devices {
                    println!("  - {name}");
                }
            }
        }
        Err(err) => {
            eprintln!("Failed to list audio input devices: {err}");
        }
    }
    Ok(())
}

fn install_sigwinch_handler() -> Result<()> {
    unsafe {
        let handler = handle_sigwinch as *const () as libc::sighandler_t;
        if libc::signal(libc::SIGWINCH, handler) == libc::SIG_ERR {
            log_debug("failed to install SIGWINCH handler");
            return Err(anyhow!("failed to install SIGWINCH handler"));
        }
    }
    Ok(())
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

struct InputParser {
    pending: Vec<u8>,
    skip_lf: bool,
    esc_buffer: Option<Vec<u8>>,
}

impl InputParser {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            skip_lf: false,
            esc_buffer: None,
        }
    }

    fn consume_bytes(&mut self, bytes: &[u8], out: &mut Vec<InputEvent>) {
        for &byte in bytes {
            if self.consume_escape(byte) {
                continue;
            }
            if self.skip_lf {
                if byte == 0x0a {
                    self.skip_lf = false;
                    continue;
                }
                self.skip_lf = false;
            }

            match byte {
                0x11 => {
                    self.flush_pending(out);
                    out.push(InputEvent::Exit);
                }
                0x12 => {
                    self.flush_pending(out);
                    out.push(InputEvent::VoiceTrigger);
                }
                0x16 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ToggleAutoVoice);
                }
                0x14 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ToggleSendMode);
                }
                0x1d => {
                    self.flush_pending(out);
                    out.push(InputEvent::IncreaseSensitivity);
                }
                0x1c => {
                    self.flush_pending(out);
                    out.push(InputEvent::DecreaseSensitivity);
                }
                0x0d | 0x0a => {
                    self.flush_pending(out);
                    out.push(InputEvent::EnterKey);
                    if byte == 0x0d {
                        self.skip_lf = true;
                    }
                }
                _ => self.pending.push(byte),
            }
        }
    }

    fn consume_escape(&mut self, byte: u8) -> bool {
        const MAX_CSI_LEN: usize = 32;

        if let Some(ref mut buffer) = self.esc_buffer {
            buffer.push(byte);
            if buffer.len() == 2 && buffer[1] != b'[' {
                self.pending.extend_from_slice(buffer);
                self.esc_buffer = None;
                return true;
            }

            if buffer.len() >= 2 && buffer[1] == b'[' {
                if is_csi_final(byte) {
                    if is_csi_u_numeric(buffer) {
                        self.esc_buffer = None;
                    } else {
                        self.pending.extend_from_slice(buffer);
                        self.esc_buffer = None;
                    }
                    return true;
                }
                if buffer.len() > MAX_CSI_LEN {
                    self.pending.extend_from_slice(buffer);
                    self.esc_buffer = None;
                    return true;
                }
                return true;
            }

            if buffer.len() > MAX_CSI_LEN {
                self.pending.extend_from_slice(buffer);
                self.esc_buffer = None;
            }
            return true;
        }

        if byte == 0x1b {
            self.esc_buffer = Some(vec![byte]);
            return true;
        }
        false
    }

    fn flush_pending(&mut self, out: &mut Vec<InputEvent>) {
        if let Some(buffer) = self.esc_buffer.take() {
            self.pending.extend_from_slice(&buffer);
        }
        if !self.pending.is_empty() {
            out.push(InputEvent::Bytes(std::mem::take(&mut self.pending)));
        }
    }
}

fn is_csi_final(byte: u8) -> bool {
    (0x40..=0x7e).contains(&byte)
}

fn is_csi_u_numeric(buffer: &[u8]) -> bool {
    if buffer.len() < 3 {
        return false;
    }
    if buffer[0] != 0x1b || buffer[1] != b'[' || *buffer.last().unwrap() != b'u' {
        return false;
    }
    buffer[2..buffer.len() - 1]
        .iter()
        .all(|b| b.is_ascii_digit() || *b == b';')
}

fn spawn_input_thread(tx: Sender<InputEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1024];
        let mut parser = InputParser::new();
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    log_debug(&format!("stdin read error: {err}"));
                    break;
                }
            };
            let mut events = Vec::new();
            parser.consume_bytes(&buf[..n], &mut events);
            parser.flush_pending(&mut events);
            for event in events {
                if tx.send(event).is_err() {
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
                    let _ =
                        write_status_line(&mut stdout, status.as_deref().unwrap_or(""), rows, cols);
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

fn write_status_line(stdout: &mut dyn Write, text: &str, rows: u16, cols: u16) -> io::Result<()> {
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

fn clear_status_line(stdout: &mut dyn Write, rows: u16, cols: u16) -> io::Result<()> {
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
        .map(|ch| {
            if ch.is_ascii_graphic() || ch == ' ' {
                ch
            } else {
                ' '
            }
        })
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
    session: &mut impl TranscriptSession,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    auto_voice_enabled: bool,
) {
    match message {
        VoiceJobMessage::Transcript { text, source } => {
            let label = source.label();
            let status = format!("Transcript ready ({label})");
            set_status(
                writer_tx,
                status_clear_deadline,
                &status,
                Some(Duration::from_secs(2)),
            );
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
            if auto_voice_enabled {
                log_debug(&format!("auto voice capture detected no speech ({label})"));
                set_status(writer_tx, status_clear_deadline, "Auto-voice enabled", None);
            } else {
                let status = format!("No speech detected ({label})");
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    &status,
                    Some(Duration::from_secs(2)),
                );
            }
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
    session: &mut impl TranscriptSession,
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

fn deliver_transcript(
    text: &str,
    source: VoiceCaptureSource,
    mode: VoiceSendMode,
    session: &mut impl TranscriptSession,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    queued_remaining: usize,
) {
    let label = source.label();
    let status = if queued_remaining > 0 {
        format!("Transcript ready ({label}) • queued {queued_remaining}")
    } else {
        format!("Transcript ready ({label})")
    };
    set_status(
        writer_tx,
        status_clear_deadline,
        &status,
        Some(Duration::from_secs(2)),
    );
    if let Err(err) = send_transcript(session, text, mode) {
        log_debug(&format!("failed to send transcript: {err:#}"));
        set_status(
            writer_tx,
            status_clear_deadline,
            "Failed to send transcript (see log)",
            Some(Duration::from_secs(2)),
        );
    }
}

fn prompt_ready(prompt_tracker: &PromptTracker, last_enter_at: Option<Instant>) -> bool {
    match (prompt_tracker.last_prompt_seen_at(), last_enter_at) {
        (Some(prompt_at), Some(enter_at)) => prompt_at > enter_at,
        (Some(_), None) => true,
        _ => false,
    }
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

fn using_native_pipeline(has_transcriber: bool, has_recorder: bool) -> bool {
    has_transcriber && has_recorder
}

struct VoiceStartInfo {
    pipeline_label: &'static str,
    fallback_note: Option<String>,
}

struct PendingTranscript {
    text: String,
    source: VoiceCaptureSource,
    mode: VoiceSendMode,
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
        next = next.clamp(MIN_DB, MAX_DB);
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
            log_debug(
                "No native Whisper model configured; using python fallback for voice capture.",
            );
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

        let using_native = using_native_pipeline(transcriber.is_some(), recorder.is_some());
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
        log_debug(&format!(
            "voice capture started ({status}) using {pipeline_label}"
        ));

        Ok(Some(VoiceStartInfo {
            pipeline_label,
            fallback_note,
        }))
    }

    fn poll_message(&mut self) -> Option<VoiceJobMessage> {
        let job = self.job.as_mut()?;
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

    fn note_activity(&mut self, now: Instant) {
        self.last_output_at = now;
        self.has_seen_output = true;
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

    fn temp_log_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        env::temp_dir().join(format!("{label}_{unique}.log"))
    }

    fn parse_events(chunks: &[&[u8]]) -> Vec<InputEvent> {
        let mut parser = InputParser::new();
        let mut events = Vec::new();
        for chunk in chunks {
            parser.consume_bytes(chunk, &mut events);
            parser.flush_pending(&mut events);
        }
        events
    }

    #[derive(Default)]
    struct StubSession {
        sent: Vec<String>,
        sent_with_newline: Vec<String>,
    }

    impl TranscriptSession for StubSession {
        fn send_text(&mut self, text: &str) -> Result<()> {
            self.sent.push(text.to_string());
            Ok(())
        }

        fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
            self.sent_with_newline.push(text.to_string());
            Ok(())
        }
    }

    fn recv_output_contains(rx: &crossbeam_channel::Receiver<Vec<u8>>, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut buffer = String::new();
        while Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if buffer.contains(needle) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn sigwinch_handler_sets_flag() {
        SIGWINCH_RECEIVED.store(false, Ordering::SeqCst);
        handle_sigwinch(0);
        assert!(SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst));
    }

    #[test]
    fn install_sigwinch_handler_installs_handler() {
        SIGWINCH_RECEIVED.store(false, Ordering::SeqCst);
        install_sigwinch_handler().expect("install sigwinch handler");
        unsafe {
            libc::raise(libc::SIGWINCH);
        }
        for _ in 0..20 {
            if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("SIGWINCH was not received");
    }

    #[test]
    fn resolve_prompt_log_prefers_config() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: Some(PathBuf::from("/tmp/codex_prompt_override.log")),
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            voice_send_mode: VoiceSendMode::Auto,
        };
        let resolved = resolve_prompt_log(&config);
        assert_eq!(resolved, PathBuf::from("/tmp/codex_prompt_override.log"));
    }

    #[test]
    fn resolve_prompt_log_uses_env() {
        let env_path = PathBuf::from("/tmp/codex_prompt_env.log");
        env::set_var("CODEX_OVERLAY_PROMPT_LOG", &env_path);
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            voice_send_mode: VoiceSendMode::Auto,
        };
        let resolved = resolve_prompt_log(&config);
        env::remove_var("CODEX_OVERLAY_PROMPT_LOG");
        assert_eq!(resolved, env_path);
    }

    #[test]
    fn resolve_prompt_regex_honors_config() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: Some("^codex> $".to_string()),
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            voice_send_mode: VoiceSendMode::Auto,
        };
        let regex = resolve_prompt_regex(&config).expect("regex should compile");
        assert!(regex.is_some());
    }

    #[test]
    fn resolve_prompt_regex_rejects_invalid() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: Some("[".to_string()),
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            voice_send_mode: VoiceSendMode::Auto,
        };
        assert!(resolve_prompt_regex(&config).is_err());
    }

    #[test]
    fn input_parser_emits_bytes_and_controls() {
        let events = parse_events(&[b"hi\x12there"]);
        assert_eq!(
            events,
            vec![
                InputEvent::Bytes(b"hi".to_vec()),
                InputEvent::VoiceTrigger,
                InputEvent::Bytes(b"there".to_vec())
            ]
        );
    }

    #[test]
    fn input_parser_maps_control_keys() {
        let mappings = vec![
            (0x11, InputEvent::Exit),
            (0x12, InputEvent::VoiceTrigger),
            (0x16, InputEvent::ToggleAutoVoice),
            (0x14, InputEvent::ToggleSendMode),
            (0x1d, InputEvent::IncreaseSensitivity),
            (0x1c, InputEvent::DecreaseSensitivity),
            (0x0a, InputEvent::EnterKey),
        ];

        for (byte, expected) in mappings {
            let events = parse_events(&[&[byte]]);
            assert_eq!(events, vec![expected]);
        }
    }

    #[test]
    fn input_parser_skips_lf_after_cr() {
        let events = parse_events(&[&[0x0d], &[0x0a]]);
        assert_eq!(events, vec![InputEvent::EnterKey]);
    }

    #[test]
    fn input_parser_keeps_non_lf_after_cr() {
        let events = parse_events(&[&[0x0d, b'x']]);
        assert_eq!(
            events,
            vec![InputEvent::EnterKey, InputEvent::Bytes(b"x".to_vec())]
        );
    }

    #[test]
    fn input_parser_drops_csi_u_sequences() {
        let events = parse_events(&[b"\x1b[48;0;0u"]);
        assert!(events.is_empty());
    }

    #[test]
    fn input_parser_preserves_arrow_sequences() {
        let events = parse_events(&[b"\x1b[A"]);
        assert_eq!(events, vec![InputEvent::Bytes(b"\x1b[A".to_vec())]);
    }

    #[test]
    fn status_helpers_sanitize_and_truncate() {
        let sanitized = sanitize_status("ok\tbad\n");
        assert_eq!(sanitized, "ok bad ");
        assert_eq!(truncate_status("hello", 0), "");
        assert_eq!(truncate_status("hello", 2), "he");
    }

    #[test]
    fn write_and_clear_status_line_respect_dimensions() {
        let mut buf = Vec::new();
        write_status_line(&mut buf, "hi", 0, 10).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 0).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 10).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));
        assert!(output.contains("hi"));

        buf.clear();
        clear_status_line(&mut buf, 2, 10).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));

        buf.clear();
        clear_status_line(&mut buf, 2, 0).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn set_status_updates_deadline() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut deadline = None;
        let now = Instant::now();
        set_status(
            &tx,
            &mut deadline,
            "status",
            Some(Duration::from_millis(50)),
        );
        let msg = rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::Status { text } => assert_eq!(text, "status"),
            _ => panic!("unexpected writer message"),
        }
        assert!(deadline.expect("deadline set") > now);

        set_status(&tx, &mut deadline, "steady", None);
        assert!(deadline.is_none());
    }

    #[test]
    fn should_auto_trigger_checks_prompt_and_idle() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_auto"));
        let mut tracker = PromptTracker::new(None, logger);
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
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_control"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.feed_output(b"ab\rde\tf\n");
        assert_eq!(tracker.last_line.as_deref(), Some("de f"));
        assert!(tracker.has_seen_output());
    }

    #[test]
    fn prompt_tracker_idle_ready_on_threshold() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_idle"));
        let mut tracker = PromptTracker::new(None, logger);
        let now = Instant::now();
        tracker.note_activity(now - Duration::from_millis(1000));
        assert!(tracker.idle_ready(now, Duration::from_millis(1000)));
    }

    #[test]
    fn prompt_logger_writes_lines() {
        let path = temp_log_path("prompt_logger");
        let logger = PromptLogger::new(path.clone());
        logger.log("hello");
        let contents = std::fs::read_to_string(&path).expect("log file");
        let _ = std::fs::remove_file(&path);
        assert!(contents.contains("hello"));
    }

    #[test]
    fn voice_manager_clamps_sensitivity() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert_eq!(manager.adjust_sensitivity(1000.0), -10.0);
        assert_eq!(manager.adjust_sensitivity(-1000.0), -80.0);
    }

    #[test]
    fn voice_manager_reports_idle_and_source() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert!(manager.is_idle());
        manager.active_source = Some(VoiceCaptureSource::Python);
        assert_eq!(manager.active_source(), Some(VoiceCaptureSource::Python));
    }

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

    #[test]
    fn prompt_tracker_ignores_non_graphic_bytes() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_non_graphic"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.feed_output(b"hi\xC2\xA0there\n");
        assert_eq!(tracker.last_line.as_deref(), Some("hithere"));
    }

    #[test]
    fn prompt_tracker_on_idle_triggers_on_threshold() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_idle_threshold"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.feed_output(b"codex> ");
        let now = tracker.last_output_at() + Duration::from_millis(1000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_some());
    }

    #[test]
    fn prompt_tracker_on_idle_skips_when_regex_present() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_idle_regex"));
        let regex = Regex::new(r"^codex> $").unwrap();
        let mut tracker = PromptTracker::new(Some(regex), logger);
        tracker.feed_output(b"not a prompt");
        let now = tracker.last_output_at() + Duration::from_millis(1000);
        tracker.on_idle(now, Duration::from_millis(1000));
        assert!(tracker.last_prompt_seen_at().is_none());
    }

    #[test]
    fn prompt_tracker_matches_learned_prompt() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_match"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.learned_prompt = Some("codex> ".to_string());
        assert!(tracker.matches_prompt("codex> "));
    }

    #[test]
    fn prompt_tracker_rejects_mismatched_prompt() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_mismatch"));
        let mut tracker = PromptTracker::new(None, logger);
        tracker.learned_prompt = Some("codex> ".to_string());
        assert!(!tracker.matches_prompt("nope> "));
    }

    #[test]
    fn prompt_tracker_has_seen_output_starts_false() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_seen"));
        let tracker = PromptTracker::new(None, logger);
        assert!(!tracker.has_seen_output());
    }

    #[test]
    fn should_auto_trigger_respects_last_trigger_equal_times() {
        let logger = PromptLogger::new(temp_log_path("prompt_tracker_last_trigger"));
        let mut tracker = PromptTracker::new(None, logger);
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

    #[test]
    fn using_native_pipeline_requires_both_components() {
        assert!(!using_native_pipeline(false, false));
        assert!(!using_native_pipeline(true, false));
        assert!(!using_native_pipeline(false, true));
        assert!(using_native_pipeline(true, true));
    }

    #[test]
    fn voice_manager_is_idle_false_when_job_present() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (_tx, rx) = mpsc::channel();
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });
        assert!(!manager.is_idle());
    }

    #[test]
    fn voice_manager_request_early_stop_sets_flag() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        assert!(!manager.request_early_stop());

        let (_tx, rx) = mpsc::channel();
        let stop_flag = Arc::new(AtomicBool::new(false));
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: stop_flag.clone(),
        });
        assert!(manager.request_early_stop());
        assert!(stop_flag.load(Ordering::Relaxed));
    }

    #[test]
    fn voice_manager_start_capture_errors_without_fallback() {
        let mut config = AppConfig::parse_from(["test"]);
        config.no_python_fallback = true;
        let mut manager = VoiceManager::new(config);
        assert!(manager.start_capture(VoiceCaptureTrigger::Manual).is_err());
    }

    #[test]
    fn voice_manager_get_transcriber_errors_on_missing_model() {
        let mut config = AppConfig::parse_from(["test"]);
        config.whisper_model_path = Some("/no/such/model.bin".to_string());
        let mut manager = VoiceManager::new(config);
        assert!(manager.get_transcriber().is_err());
    }

    #[test]
    fn start_voice_capture_reports_running_job_on_manual_only() {
        let config = AppConfig::parse_from(["test"]);
        let mut manager = VoiceManager::new(config);
        let (_tx, rx) = mpsc::channel();
        manager.job = Some(voice::VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
        let mut deadline = None;
        start_voice_capture(
            &mut manager,
            VoiceCaptureTrigger::Manual,
            &writer_tx,
            &mut deadline,
        )
        .expect("start capture manual");

        let msg = writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::Status { text } => {
                assert!(text.contains("already running"));
            }
            _ => panic!("unexpected writer message"),
        }

        start_voice_capture(
            &mut manager,
            VoiceCaptureTrigger::Auto,
            &writer_tx,
            &mut deadline,
        )
        .expect("start capture auto");

        assert!(writer_rx.try_recv().is_err());
    }

    #[test]
    fn send_transcript_respects_mode_and_trims() {
        let mut session = StubSession::default();
        send_transcript(&mut session, " hello ", VoiceSendMode::Auto).unwrap();
        assert_eq!(session.sent_with_newline, vec!["hello"]);

        send_transcript(&mut session, " hi ", VoiceSendMode::Insert).unwrap();
        assert_eq!(session.sent, vec!["hi"]);

        send_transcript(&mut session, "   ", VoiceSendMode::Insert).unwrap();
        assert_eq!(session.sent.len(), 1);
    }

    #[test]
    fn handle_voice_message_sends_status_and_transcript() {
        let config = OverlayConfig {
            app: AppConfig::parse_from(["test"]),
            prompt_regex: None,
            prompt_log: None,
            auto_voice: false,
            auto_voice_idle_ms: 1200,
            voice_send_mode: VoiceSendMode::Auto,
        };
        let mut session = StubSession::default();
        let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
        let mut deadline = None;

        handle_voice_message(
            VoiceJobMessage::Transcript {
                text: " hello ".to_string(),
                source: VoiceCaptureSource::Native,
            },
            &config,
            &mut session,
            &writer_tx,
            &mut deadline,
            false,
        );

        let msg = writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::Status { text } => {
                assert!(text.contains("Transcript ready"));
            }
            _ => panic!("unexpected writer message"),
        }
        assert_eq!(session.sent_with_newline, vec!["hello"]);
    }

    #[test]
    fn transcript_session_impl_sends_text() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text(&mut session, "ping\n").expect("send text");
        assert!(recv_output_contains(&session.output_rx, "ping"));
    }

    #[test]
    fn transcript_session_impl_sends_text_with_newline() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text_with_newline(&mut session, "pong")
            .expect("send text with newline");
        assert!(recv_output_contains(&session.output_rx, "pong"));
    }
}
