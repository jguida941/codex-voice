//! IPC session lifecycle orchestration for long-lived backend command execution.

use crate::auth;
use crate::codex::{sanitize_pty_output, CodexCliBackend, CodexEvent, CodexEventKind, CodexJob};
use crate::config::AppConfig;
use crate::pty_session::PtyCliSession;
use crate::voice::{VoiceJob, VoiceJobMessage};
use crate::{audio, log_debug, log_debug_content, stt};
use anyhow::Result;
use std::env;
use std::io::{self, BufRead, Write};
#[cfg(any(test, feature = "mutants"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
#[cfg(any(test, feature = "mutants"))]
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::protocol::{IpcCommand, IpcEvent, Provider};
use super::router::{
    handle_auth_command, handle_cancel, handle_send_prompt, handle_set_provider, handle_start_voice,
};

// ============================================================================
// PTY toggle for tests/mutants so IPC paths can run without PTY dependencies.
// ============================================================================
#[cfg(any(test, feature = "mutants"))]
const USE_PTY: bool = false;
#[cfg(not(any(test, feature = "mutants")))]
const USE_PTY: bool = true;

// ============================================================================
// IPC state owned by the event loop.
// ============================================================================

pub(super) struct IpcState {
    pub(super) config: AppConfig,
    pub(super) active_provider: Provider,
    pub(super) codex_cli_backend: Arc<CodexCliBackend>,
    pub(super) claude_cmd: String,
    pub(super) recorder: Option<Arc<Mutex<audio::Recorder>>>,
    pub(super) transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    pub(super) current_job: Option<ActiveJob>,
    pub(super) current_voice_job: Option<VoiceJob>,
    pub(super) current_auth_job: Option<AuthJob>,
    pub(super) session_id: String,
    pub(super) cancelled: bool,
}

pub(super) enum ActiveJob {
    Codex(CodexJob),
    Claude(ClaudeJob),
}

pub(super) enum ClaudeJobOutput {
    Piped {
        child: std::process::Child,
        stdout_rx: Receiver<String>,
    },
    Pty {
        session: PtyCliSession,
    },
}

pub(super) struct ClaudeJob {
    pub(super) output: ClaudeJobOutput,
    #[allow(dead_code)]
    pub(super) started_at: Instant,
    pub(super) pending_exit: Option<std::process::ExitStatus>,
}

pub(super) type AuthResult = auth::AuthResult;

pub(super) struct AuthJob {
    pub(super) provider: Provider,
    pub(super) receiver: Receiver<AuthResult>,
    #[allow(dead_code)]
    pub(super) started_at: Instant,
}

impl IpcState {
    pub(super) fn new(mut config: AppConfig) -> Self {
        // Keep test/mutant runs deterministic by disabling PTY when toggled off.
        if !USE_PTY {
            config.persistent_codex = false;
            log_debug("PTY disabled via USE_PTY toggle");
        }

        // Session id is emitted in capabilities so clients can correlate events.
        let session_id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        // Use already validated command values from config parsing.
        let claude_cmd = config.claude_cmd.clone();

        // Backend is shared so concurrent jobs can be cancelled from multiple paths.
        let codex_cli_backend = Arc::new(CodexCliBackend::new(config.clone()));

        // Allow env override so wrappers can pin provider without extra flags.
        let default_provider = env::var("VOICETERM_PROVIDER")
            .ok()
            .and_then(|s| Provider::from_str(&s))
            .unwrap_or(Provider::Codex);

        // Recorder/transcriber are optional so IPC still works without voice dependencies.
        let recorder = match audio::Recorder::new(config.input_device.as_deref()) {
            Ok(r) => {
                log_debug("Audio recorder initialized");
                Some(Arc::new(Mutex::new(r)))
            }
            Err(e) => {
                log_debug(&format!("Audio recorder not available: {e}"));
                None
            }
        };

        // Load STT lazily from config path; failures remain recoverable.
        let transcriber = if let Some(model_path) = &config.whisper_model_path {
            match stt::Transcriber::new(model_path) {
                Ok(t) => {
                    log_debug("Whisper transcriber initialized");
                    Some(Arc::new(Mutex::new(t)))
                }
                Err(e) => {
                    log_debug(&format!("Whisper not available: {e}"));
                    None
                }
            }
        } else {
            log_debug("No whisper model path configured");
            None
        };

        Self {
            config,
            active_provider: default_provider,
            codex_cli_backend,
            claude_cmd,
            recorder,
            transcriber,
            current_job: None,
            current_voice_job: None,
            current_auth_job: None,
            session_id,
            cancelled: false,
        }
    }

    pub(super) fn emit_capabilities(&self) {
        let providers = vec!["codex".to_string(), "claude".to_string()];

        // Device name is included in capabilities for client-side diagnostics.
        let input_device = self.recorder.as_ref().map(|r| {
            r.lock()
                .map(|recorder| recorder.device_name())
                .unwrap_or_else(|_| "Unknown Device".to_string())
        });

        send_event(&IpcEvent::Capabilities {
            session_id: self.session_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            mic_available: self.recorder.is_some(),
            input_device,
            whisper_model_loaded: self.transcriber.is_some(),
            whisper_model_path: self.config.whisper_model_path.clone(),
            python_fallback_allowed: !self.config.no_python_fallback,
            providers_available: providers,
            active_provider: self.active_provider.as_str().to_string(),
            working_dir: env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            codex_cmd: self.config.codex_cmd.clone(),
            claude_cmd: self.claude_cmd.clone(),
        });
    }
}

// ============================================================================
// Event sending
// ============================================================================

pub(super) fn send_event(event: &IpcEvent) {
    #[cfg(any(test, feature = "mutants"))]
    if capture_test_event(event) {
        return;
    }
    if let Ok(json) = serde_json::to_string(event) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{json}");
        let _ = stdout.flush();
    }
}

#[cfg(any(test, feature = "mutants"))]
static EVENT_SINK: OnceLock<Mutex<Vec<IpcEvent>>> = OnceLock::new();
#[cfg(any(test, feature = "mutants"))]
pub(super) static IPC_LOOP_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(any(test, feature = "mutants"))]
fn capture_test_event(event: &IpcEvent) -> bool {
    if let Some(sink) = EVENT_SINK.get() {
        if let Ok(mut events) = sink.lock() {
            events.push(event.clone());
            return true;
        }
    }
    false
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn init_event_sink() {
    let _ = EVENT_SINK.get_or_init(|| Mutex::new(Vec::new()));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn ipc_loop_count_reset() {
    IPC_LOOP_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn ipc_loop_count() -> u64 {
    IPC_LOOP_COUNT.load(Ordering::SeqCst)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn event_snapshot() -> usize {
    init_event_sink();
    EVENT_SINK
        .get()
        .and_then(|sink| sink.lock().ok().map(|events| events.len()))
        .unwrap_or(0)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn events_since(start: usize) -> Vec<IpcEvent> {
    EVENT_SINK
        .get()
        .and_then(|sink| {
            sink.lock()
                .ok()
                .map(|events| events.iter().skip(start).cloned().collect())
        })
        .unwrap_or_default()
}

impl ClaudeJob {
    pub(super) fn cancel(&mut self) {
        match &mut self.output {
            ClaudeJobOutput::Piped { child, .. } => {
                terminate_piped_child(child);
            }
            ClaudeJobOutput::Pty { session } => {
                let _ = session.send("\u{3}");
            }
        }
    }
}

// ============================================================================
// Stdin Reader Thread
// ============================================================================

#[cfg_attr(any(test, feature = "mutants"), allow(dead_code))]
fn spawn_stdin_reader(tx: Sender<IpcCommand>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let stdin = io::stdin();
        let stdin_lock = stdin.lock();

        for line in stdin_lock.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<IpcCommand>(trimmed) {
                Ok(cmd) => {
                    if tx.send(cmd).is_err() {
                        break; // Main thread has exited
                    }
                }
                Err(e) => {
                    send_event(&IpcEvent::Error {
                        message: format!("Invalid command: {e}"),
                        recoverable: true,
                    });
                }
            }
        }

        log_debug("Stdin reader thread exiting");
    })
}

// ============================================================================
// Claude Backend
// ============================================================================

#[cfg(unix)]
fn signal_process_group_or_pid(pid: i32, signal: i32) -> io::Result<()> {
    if pid <= 0 {
        return Ok(());
    }
    unsafe {
        if libc::kill(-pid, signal) == 0 {
            return Ok(());
        }
        let group_err = io::Error::last_os_error();

        if libc::kill(pid, signal) == 0 {
            return Ok(());
        }
        let pid_err = io::Error::last_os_error();
        if matches!(group_err.raw_os_error(), Some(code) if code == libc::ESRCH)
            || matches!(pid_err.raw_os_error(), Some(code) if code == libc::ESRCH)
        {
            return Ok(());
        }

        Err(io::Error::new(
            pid_err.kind(),
            format!(
                "group(-{pid}) signal failed: {group_err}; pid({pid}) signal failed: {pid_err}"
            ),
        ))
    }
}

fn terminate_piped_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;

        let _ = signal_process_group_or_pid(pid, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(150);
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        let _ = signal_process_group_or_pid(pid, libc::SIGKILL);
    }

    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }

    let _ = child.wait();
}

fn start_claude_job_with_mode(
    claude_cmd: &str,
    prompt: &str,
    skip_permissions: bool,
    term_value: &str,
    use_pty: bool,
) -> Result<ClaudeJob, String> {
    use std::process::{Command, Stdio};

    log_debug_content(&format!(
        "Starting Claude job with prompt: {}...",
        &prompt[..prompt.len().min(30)]
    ));

    // Use --print with --dangerously-skip-permissions for non-interactive operation.
    // Prefer PTY when enabled so thinking/tool call output streams in real time.
    let mut args = vec!["--print".to_string()];
    if skip_permissions {
        args.push("--dangerously-skip-permissions".to_string());
    }
    args.push(prompt.to_string());

    if use_pty {
        let working_dir = env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| ".".to_string());
        match PtyCliSession::new(claude_cmd, &working_dir, &args, term_value) {
            Ok(session) => {
                log_debug("Claude job started (PTY)");
                return Ok(ClaudeJob {
                    output: ClaudeJobOutput::Pty { session },
                    started_at: Instant::now(),
                    pending_exit: None,
                });
            }
            Err(err) => {
                log_debug(&format!(
                    "Claude PTY failed, falling back to pipes: {err:#}"
                ));
            }
        }
    }

    let mut command = Command::new(claude_cmd);
    command.args(&args);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start claude: {e}"))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();

    // Read stdout
    thread::spawn(move || {
        let reader = io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // Read stderr
    thread::spawn(move || {
        let reader = io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            // Only show non-empty stderr lines
            if !line.trim().is_empty() && tx_err.send(format!("[info] {line}")).is_err() {
                break;
            }
        }
    });

    log_debug("Claude job started");
    Ok(ClaudeJob {
        output: ClaudeJobOutput::Piped {
            child,
            stdout_rx: rx,
        },
        started_at: Instant::now(),
        pending_exit: None,
    })
}

pub(super) fn start_claude_job(
    claude_cmd: &str,
    prompt: &str,
    skip_permissions: bool,
    term_value: &str,
) -> Result<ClaudeJob, String> {
    start_claude_job_with_mode(claude_cmd, prompt, skip_permissions, term_value, USE_PTY)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn start_claude_job_with_pty(
    claude_cmd: &str,
    prompt: &str,
    skip_permissions: bool,
    term_value: &str,
) -> Result<ClaudeJob, String> {
    start_claude_job_with_mode(claude_cmd, prompt, skip_permissions, term_value, true)
}

// ============================================================================
// Auth Backend
// ============================================================================

#[cfg(any(test, feature = "mutants"))]
pub(super) type AuthFlowHook =
    Box<dyn Fn(Provider, &str, &str) -> AuthResult + Send + Sync + 'static>;

#[cfg(any(test, feature = "mutants"))]
static AUTH_FLOW_HOOK: OnceLock<Mutex<Option<AuthFlowHook>>> = OnceLock::new();

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(super) fn set_auth_flow_hook(hook: Option<AuthFlowHook>) {
    let storage = AUTH_FLOW_HOOK.get_or_init(|| Mutex::new(None));
    *storage.lock().unwrap_or_else(|e| e.into_inner()) = hook;
}

pub(super) fn run_auth_flow(provider: Provider, codex_cmd: &str, claude_cmd: &str) -> AuthResult {
    #[cfg(any(test, feature = "mutants"))]
    if let Some(storage) = AUTH_FLOW_HOOK.get() {
        if let Ok(guard) = storage.lock() {
            if let Some(hook) = guard.as_ref() {
                return hook(provider, codex_cmd, claude_cmd);
            }
        }
    }
    let command = match provider {
        Provider::Codex => codex_cmd,
        Provider::Claude => claude_cmd,
    };
    auth::run_login_command(command)
        .map_err(|err| format!("{} auth failed: {}", provider.as_str(), err))
}

// ============================================================================
// Main Event Loop
// ============================================================================

/// Run newline-delimited JSON IPC mode until stdin closes or loop exits.
pub fn run_ipc_mode(config: AppConfig) -> Result<()> {
    log_debug("Starting JSON IPC mode (non-blocking)");

    let mut state = IpcState::new(config);

    // Emit capabilities on startup
    state.emit_capabilities();

    // Start stdin reader thread
    let (cmd_tx, cmd_rx) = mpsc::channel();
    #[cfg(any(test, feature = "mutants"))]
    {
        drop(cmd_tx);
        run_ipc_loop(&mut state, &cmd_rx, Some(10))
    }
    #[cfg(not(any(test, feature = "mutants")))]
    {
        let _stdin_handle = spawn_stdin_reader(cmd_tx);
        run_ipc_loop(&mut state, &cmd_rx, None)
    }
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn ipc_guard_tripped(elapsed: Duration) -> bool {
    elapsed > Duration::from_secs(2)
}

pub(super) fn run_ipc_loop(
    state: &mut IpcState,
    cmd_rx: &Receiver<IpcCommand>,
    max_loops: Option<u64>,
) -> Result<()> {
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    let mut loop_count: u64 = 0;
    loop {
        #[cfg(any(test, feature = "mutants"))]
        if ipc_guard_tripped(guard_start.elapsed()) {
            panic!("IPC loop guard exceeded");
        }
        loop_count += 1;
        #[cfg(any(test, feature = "mutants"))]
        IPC_LOOP_COUNT.store(loop_count, Ordering::SeqCst);
        if loop_count.is_multiple_of(1000) {
            log_debug(&format!(
                "IPC loop iteration {}, job active: {}",
                loop_count,
                state.current_job.is_some()
            ));
        }

        if let Some(limit) = max_loops {
            if loop_count >= limit {
                log_debug("IPC loop reached test limit, exiting");
                break;
            }
        }

        // Check for new commands (non-blocking)
        match cmd_rx.try_recv() {
            Ok(cmd) => {
                log_debug_content(&format!("IPC command received: {cmd:?}"));
                state.cancelled = false;

                match cmd {
                    IpcCommand::SendPrompt { prompt, provider } => {
                        handle_send_prompt(state, &prompt, provider);
                    }
                    IpcCommand::StartVoice => {
                        handle_start_voice(state);
                    }
                    IpcCommand::Cancel => {
                        handle_cancel(state);
                    }
                    IpcCommand::SetProvider { provider } => {
                        handle_set_provider(state, &provider);
                    }
                    IpcCommand::Auth { provider } => {
                        handle_auth_command(state, provider);
                    }
                    IpcCommand::GetCapabilities => {
                        state.emit_capabilities();
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                log_debug("Command channel disconnected, exiting");
                break;
            }
        }

        // Process active job events
        if let Some(job) = &mut state.current_job {
            match job {
                ActiveJob::Codex(codex_job) => {
                    if process_codex_events(codex_job, state.cancelled) {
                        state.current_job = None;
                    }
                }
                ActiveJob::Claude(claude_job) => {
                    if process_claude_events(claude_job, state.cancelled) {
                        state.current_job = None;
                    }
                }
            }
        }

        // Process voice job events
        if let Some(voice_job) = &state.current_voice_job {
            if process_voice_events(voice_job, state.cancelled) {
                state.current_voice_job = None;
            }
        }

        if process_auth_events(state) {
            state.current_auth_job = None;
        }

        // Small sleep to prevent busy-waiting
        thread::sleep(Duration::from_millis(5));
    }

    log_debug("IPC mode exiting");
    Ok(())
}

// ============================================================================
// Event Processing (Non-blocking, returns true when job complete)
// ============================================================================

pub(super) fn process_codex_events(job: &mut CodexJob, cancelled: bool) -> bool {
    if cancelled {
        return true;
    }

    let handle_event = |event: CodexEvent| -> bool {
        match event.kind {
            CodexEventKind::Token { text } => {
                send_event(&IpcEvent::Token { text });
                false
            }
            CodexEventKind::Status { message } => {
                send_event(&IpcEvent::Status { message });
                false
            }
            CodexEventKind::Started { .. } => {
                send_event(&IpcEvent::Status {
                    message: "Processing...".to_string(),
                });
                false
            }
            CodexEventKind::Finished { lines, .. } => {
                for line in lines {
                    send_event(&IpcEvent::Token {
                        text: format!("{line}\n"),
                    });
                }
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: true,
                    error: None,
                });
                true
            }
            CodexEventKind::FatalError { message, .. } => {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some(message),
                });
                true
            }
            CodexEventKind::RecoverableError { message, .. } => {
                send_event(&IpcEvent::Status {
                    message: format!("Retrying: {message}"),
                });
                false
            }
            CodexEventKind::Canceled { .. } => {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
                true
            }
        }
    };

    // Check for new events via signal channel
    match job.try_recv_signal() {
        Ok(()) => {
            // Signal received, drain events
            for event in job.drain_events() {
                if handle_event(event) {
                    return true;
                }
            }
            false
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            // Worker finished, drain any remaining events
            let mut completed = false;
            for event in job.drain_events() {
                if handle_event(event) {
                    completed = true;
                    break;
                }
            }
            if !completed {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: true,
                    error: None,
                });
            }
            true
        }
    }
}

pub(super) fn process_claude_events(job: &mut ClaudeJob, cancelled: bool) -> bool {
    if cancelled {
        log_debug("Claude job: cancelled");
        job.cancel();
        return true;
    }

    match &mut job.output {
        ClaudeJobOutput::Piped { child, stdout_rx } => match stdout_rx.try_recv() {
            Ok(line) => {
                log_debug_content(&format!(
                    "Claude job: got line: {}",
                    &line[..line.len().min(50)]
                ));
                send_event(&IpcEvent::Token {
                    text: format!("{line}\n"),
                });
                false
            }
            Err(TryRecvError::Empty) => match child.try_wait() {
                Ok(Some(status)) => {
                    log_debug(&format!(
                        "Claude job: process exited with status {status:?}"
                    ));
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: status.success(),
                        error: if status.success() {
                            None
                        } else {
                            Some(format!("Exit code: {:?}", status.code()))
                        },
                    });
                    true
                }
                Ok(None) => false,
                Err(e) => {
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: false,
                        error: Some(format!("Process error: {e}")),
                    });
                    true
                }
            },
            Err(TryRecvError::Disconnected) => {
                log_debug("Claude job: stdout disconnected");
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log_debug(&format!(
                            "Claude job: process already exited with {status:?}"
                        ));
                        send_event(&IpcEvent::JobEnd {
                            provider: "claude".to_string(),
                            success: status.success(),
                            error: None,
                        });
                        true
                    }
                    Ok(None) => {
                        log_debug("Claude job: process still running, killing it");
                        terminate_piped_child(child);
                        send_event(&IpcEvent::JobEnd {
                            provider: "claude".to_string(),
                            success: true,
                            error: None,
                        });
                        true
                    }
                    Err(e) => {
                        send_event(&IpcEvent::JobEnd {
                            provider: "claude".to_string(),
                            success: false,
                            error: Some(format!("Wait error: {e}")),
                        });
                        true
                    }
                }
            }
        },
        ClaudeJobOutput::Pty { session } => {
            for chunk in session.read_output() {
                let text = sanitize_pty_output(&chunk);
                if !text.is_empty() {
                    send_event(&IpcEvent::Token { text });
                }
            }
            if let Some(status) = job.pending_exit.take() {
                let (success, error) = if status.success() {
                    (true, None)
                } else {
                    let msg = status
                        .code()
                        .map(|code| format!("Exit code: {code}"))
                        .unwrap_or_else(|| "Exited by signal".to_string());
                    (false, Some(msg))
                };
                send_event(&IpcEvent::JobEnd {
                    provider: "claude".to_string(),
                    success,
                    error,
                });
                return true;
            }
            if let Some(status) = session.try_wait() {
                let trailing = session.read_output_timeout(Duration::from_millis(50));
                if !trailing.is_empty() {
                    for chunk in trailing {
                        let text = sanitize_pty_output(&chunk);
                        if !text.is_empty() {
                            send_event(&IpcEvent::Token { text });
                        }
                    }
                    job.pending_exit = Some(status);
                    return false;
                }
                let (success, error) = if status.success() {
                    (true, None)
                } else {
                    let msg = status
                        .code()
                        .map(|code| format!("Exit code: {code}"))
                        .unwrap_or_else(|| "Exited by signal".to_string());
                    (false, Some(msg))
                };
                send_event(&IpcEvent::JobEnd {
                    provider: "claude".to_string(),
                    success,
                    error,
                });
                return true;
            }
            false
        }
    }
}

pub(super) fn process_voice_events(job: &VoiceJob, cancelled: bool) -> bool {
    if cancelled {
        return true;
    }

    match job.receiver.try_recv() {
        Ok(msg) => {
            match msg {
                VoiceJobMessage::Transcript {
                    text,
                    source,
                    metrics,
                } => {
                    let duration_ms = metrics.as_ref().map(|m| m.capture_ms).unwrap_or(0);
                    send_event(&IpcEvent::VoiceEnd { error: None });
                    send_event(&IpcEvent::Transcript { text, duration_ms });
                    log_debug(&format!("Voice transcript via {}", source.label()));
                }
                VoiceJobMessage::Empty { source, metrics: _ } => {
                    send_event(&IpcEvent::VoiceEnd {
                        error: Some("No speech detected".to_string()),
                    });
                    log_debug(&format!("Voice empty via {}", source.label()));
                }
                VoiceJobMessage::Error(message) => {
                    send_event(&IpcEvent::VoiceEnd {
                        error: Some(message),
                    });
                }
            }
            true
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            send_event(&IpcEvent::VoiceEnd {
                error: Some("Voice worker disconnected".to_string()),
            });
            true
        }
    }
}

pub(super) fn process_auth_events(state: &mut IpcState) -> bool {
    let job = match state.current_auth_job.as_mut() {
        Some(job) => job,
        None => return false,
    };

    match job.receiver.try_recv() {
        Ok(result) => {
            let provider = job.provider;
            let (success, error) = match result {
                Ok(()) => (true, None),
                Err(err) => (false, Some(err)),
            };

            if success && provider == Provider::Codex {
                state.codex_cli_backend.reset_session();
            }

            send_event(&IpcEvent::AuthEnd {
                provider: provider.as_str().to_string(),
                success,
                error,
            });
            state.emit_capabilities();
            true
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            let provider = job.provider;
            send_event(&IpcEvent::AuthEnd {
                provider: provider.as_str().to_string(),
                success: false,
                error: Some("Auth worker disconnected".to_string()),
            });
            state.emit_capabilities();
            true
        }
    }
}
