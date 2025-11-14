//! Codex backend interfaces plus the async worker used by the TUI. The backend
//! exposes a `CodexBackend` trait that emits structured events so the UI can
//! render streaming output, show “thinking…” indicators, and react to
//! recoverable vs fatal failures without duplicating business logic.

use crate::{config::AppConfig, log_debug, pty_session::PtyCodexSession};
use anyhow::{anyhow, Context, Result};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::{
    collections::{HashMap, VecDeque},
    env,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use strip_ansi_escapes::strip;

/// Spinner frames used by the UI when a Codex request is inflight.
pub const CODEX_SPINNER_FRAMES: &[char] = &['-', '\\', '|', '/'];
// Codex is an AI that takes seconds to respond, not milliseconds
// These timeouts must be realistic for AI response times
const PTY_FIRST_BYTE_TIMEOUT_MS: u64 = 10000;  // 10s for first byte (was 150ms)
const PTY_OVERALL_TIMEOUT_MS: u64 = 30000;     // 30s overall (was 500ms)
const PTY_QUIET_GRACE_MS: u64 = 2000;          // 2s quiet period (was 350ms)
const PTY_HEALTHCHECK_TIMEOUT_MS: u64 = 5000;  // 5s health check (was 2000ms)
const BACKEND_EVENT_CAPACITY: usize = 1024;

/// Unique identifier for Codex requests routed through the backend.
pub type JobId = u64;

/// User-facing mode describing how Codex should treat the request.
#[derive(Debug, Clone, Copy)]
pub enum RequestMode {
    Chat,
}

/// Payload variants supported by the backend.
#[derive(Debug, Clone)]
pub enum RequestPayload {
    Chat { prompt: String },
}

/// Structured Codex request routed through the backend.
#[derive(Debug, Clone)]
pub struct CodexRequest {
    pub payload: RequestPayload,
    pub timeout: Option<Duration>,
    pub workspace_files: Vec<PathBuf>,
}

impl CodexRequest {
    pub fn chat(prompt: String) -> Self {
        Self {
            payload: RequestPayload::Chat { prompt },
            timeout: None,
            workspace_files: Vec::new(),
        }
    }
}

/// Telemetry produced for every Codex job so latency regressions can be audited.
#[derive(Debug, Clone)]
pub struct BackendStats {
    pub backend_type: &'static str,
    pub started_at: Instant,
    pub first_token_at: Option<Instant>,
    pub finished_at: Instant,
    pub tokens_received: usize,
    pub bytes_transferred: usize,
    pub pty_attempts: u32,
    pub cli_fallback_used: bool,
    pub disable_pty: bool,
}

impl BackendStats {
    fn new(now: Instant) -> Self {
        Self {
            backend_type: "cli",
            started_at: now,
            first_token_at: None,
            finished_at: now,
            tokens_received: 0,
            bytes_transferred: 0,
            pty_attempts: 0,
            cli_fallback_used: false,
            disable_pty: false,
        }
    }
}

/// Event emitted by the backend describing job progress.
#[derive(Debug, Clone)]
pub struct BackendEvent {
    pub job_id: JobId,
    pub kind: BackendEventKind,
}

/// Classified event payload.
#[derive(Debug, Clone)]
pub enum BackendEventKind {
    Started {
        mode: RequestMode,
    },
    Status {
        message: String,
    },
    Token {
        text: String,
    },
    RecoverableError {
        phase: &'static str,
        message: String,
        retry_available: bool,
    },
    FatalError {
        phase: &'static str,
        message: String,
        disable_pty: bool,
    },
    Finished {
        lines: Vec<String>,
        status: String,
        stats: BackendStats,
    },
    Canceled {
        disable_pty: bool,
    },
}

/// Errors surfaced synchronously when a backend cannot start a job.
#[derive(Debug)]
pub enum BackendError {
    InvalidRequest(&'static str),
    BackendDisabled(String),
}

/// Runtime implementation of the Codex backend interface.
pub trait CodexBackend: Send + Sync {
    fn start(&self, request: CodexRequest) -> Result<BackendJob, BackendError>;
    fn cancel(&self, job_id: JobId);
    fn working_dir(&self) -> &Path;
}

/// Handle to an asynchronous Codex invocation routed through the backend.
pub struct BackendJob {
    pub id: JobId,
    events: Arc<BoundedEventQueue>,
    signal_rx: Receiver<()>,
    handle: Option<JoinHandle<()>>,
    cancel_token: CancelToken,
}

impl BackendJob {
    fn new(
        id: JobId,
        events: Arc<BoundedEventQueue>,
        signal_rx: Receiver<()>,
        handle: JoinHandle<()>,
        cancel_token: CancelToken,
    ) -> Self {
        Self {
            id,
            events,
            signal_rx,
            handle: Some(handle),
            cancel_token,
        }
    }

    /// Request cancellation; the worker best-effort terminates subprocesses and emits a
    /// `BackendEventKind::Canceled` terminal event.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Poll the signal channel without blocking to determine whether new events exist.
    pub fn try_recv_signal(&self) -> Result<(), TryRecvError> {
        self.signal_rx.try_recv().map(|_| ())
    }

    /// Drain any queued backend events.
    pub fn drain_events(&self) -> Vec<BackendEvent> {
        self.events.drain()
    }

    /// Take ownership of the worker handle so the caller can join it once the job finishes.
    pub fn take_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }
}

/// Backing store for all events emitted by a job. Ensures bounded capacity with drop-oldest semantics.
struct BoundedEventQueue {
    capacity: usize,
    inner: Mutex<VecDeque<BackendEvent>>,
}

impl BoundedEventQueue {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn push(&self, event: BackendEvent) -> Result<(), BackendQueueError> {
        let mut queue = self.inner.lock().unwrap();
        if queue.len() >= self.capacity && !Self::drop_non_terminal(&mut queue) {
            return Err(BackendQueueError);
        }
        queue.push_back(event);
        Ok(())
    }

    fn drain(&self) -> Vec<BackendEvent> {
        let mut queue = self.inner.lock().unwrap();
        queue.drain(..).collect()
    }

    fn drop_non_terminal(queue: &mut VecDeque<BackendEvent>) -> bool {
        if let Some(idx) = queue
            .iter()
            .position(|event| matches!(event.kind, BackendEventKind::Token { .. }))
        {
            queue.remove(idx);
            return true;
        }
        if let Some(idx) = queue
            .iter()
            .position(|event| matches!(event.kind, BackendEventKind::Status { .. }))
        {
            queue.remove(idx);
            return true;
        }
        if let Some(idx) = queue.iter().position(|event| {
            matches!(
                event.kind,
                BackendEventKind::RecoverableError { .. } | BackendEventKind::Started { .. }
            )
        }) {
            queue.remove(idx);
            return true;
        }
        false
    }
}

#[derive(Debug)]
struct BackendQueueError;

/// Sender that writes backend events into the bounded queue and notifies the UI.
struct EventSender {
    queue: Arc<BoundedEventQueue>,
    signal_tx: mpsc::Sender<()>,
}

impl EventSender {
    fn new(queue: Arc<BoundedEventQueue>, signal_tx: mpsc::Sender<()>) -> Self {
        Self { queue, signal_tx }
    }

    fn emit(&self, event: BackendEvent) -> Result<(), BackendQueueError> {
        self.queue.push(event)?;
        let _ = self.signal_tx.send(());
        Ok(())
    }
}

/// Default CLI/PTY backend implementation driving the `codex` binary.
pub struct CliBackend {
    config: AppConfig,
    working_dir: PathBuf,
    next_job_id: AtomicU64,
    state: Arc<Mutex<CliBackendState>>,
    cancel_tokens: Arc<Mutex<HashMap<JobId, CancelToken>>>,
}

struct CliBackendState {
    codex_session: Option<PtyCodexSession>,
    pty_disabled: bool,
}

impl CliBackend {
    pub fn new(config: AppConfig) -> Self {
        let working_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let state = CliBackendState {
            codex_session: None,
            pty_disabled: !config.persistent_codex,
        };
        Self {
            config,
            working_dir,
            next_job_id: AtomicU64::new(0),
            state: Arc::new(Mutex::new(state)),
            cancel_tokens: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn take_codex_session_for_job(&self) -> Option<PtyCodexSession> {
        if !self.config.persistent_codex {
            return None;
        }
        let mut state = self.state.lock().unwrap();
        if state.pty_disabled {
            return None;
        }
        if state.codex_session.is_none() {
            if let Err(err) = self.ensure_codex_session(&mut state) {
                log_debug(&format!(
                    "CliBackend: persistent Codex unavailable: {err:#}"
                ));
                state.pty_disabled = true;
                return None;
            }
        }
        state.codex_session.take()
    }

    fn ensure_codex_session(&self, state: &mut CliBackendState) -> Result<()> {
        let working_dir = self.working_dir.clone();
        log_debug(&format!("Attempting to create PTY session with codex_cmd={}, working_dir={}",
            self.config.codex_cmd, working_dir.to_str().unwrap_or(".")));
        match PtyCodexSession::new(
            &self.config.codex_cmd,
            working_dir.to_str().unwrap_or("."),
            &self.config.codex_args,
            &self.config.term_value,
        ) {
            Ok(mut session) => {
                log_debug("PTY session created, checking responsiveness...");
                let timeout = Duration::from_millis(PTY_HEALTHCHECK_TIMEOUT_MS);
                if session.is_responsive(timeout) {
                    state.codex_session = Some(session);
                    log_debug("CliBackend: persistent PTY session ready and responsive");
                    Ok(())
                } else {
                    log_debug("PTY health check failed - session unresponsive");
                    Err(anyhow!("persistent Codex unresponsive"))
                }
            }
            Err(err) => {
                log_debug(&format!("Failed to create PTY session: {:#}", err));
                Err(err.context("failed to start Codex PTY"))
            }
        }
    }
}

impl CodexBackend for CliBackend {
    fn start(&self, request: CodexRequest) -> Result<BackendJob, BackendError> {
        let mode = match &request.payload {
            RequestPayload::Chat { prompt } => {
                if prompt.trim().is_empty() {
                    return Err(BackendError::InvalidRequest("Prompt is empty"));
                }
                RequestMode::Chat
            }
        };

        let job_id = self.next_job_id.fetch_add(1, Ordering::Relaxed) + 1;
        let queue = Arc::new(BoundedEventQueue::new(BACKEND_EVENT_CAPACITY));
        let queue_for_worker = Arc::clone(&queue);
        let (signal_tx, signal_rx) = mpsc::channel();
        let cancel_token = CancelToken::new();
        let cancel_for_worker = cancel_token.clone();
        let mut session_for_job = self.take_codex_session_for_job();
        let config = self.config.clone();
        let working_dir = self.working_dir.clone();
        let state = Arc::clone(&self.state);
        let cancel_registry = Arc::clone(&self.cancel_tokens);

        {
            let mut registry = cancel_registry.lock().unwrap();
            registry.insert(job_id, cancel_token.clone());
        }

        let context = JobContext {
            job_id,
            request,
            mode,
            config,
            working_dir,
        };
        let handle = thread::spawn(move || {
            #[cfg(test)]
            let _thread_guard = BackendThreadGuard::new();
            let sender = EventSender::new(queue_for_worker, signal_tx);
            let outcome =
                run_codex_job(context, session_for_job.take(), cancel_for_worker, &sender);
            CliBackend::cleanup_job(cancel_registry, job_id);
            CliBackend::restore_static_state(state, outcome.codex_session, outcome.disable_pty);
        });

        Ok(BackendJob::new(
            job_id,
            queue,
            signal_rx,
            handle,
            cancel_token,
        ))
    }

    fn cancel(&self, job_id: JobId) {
        let maybe_token = {
            let registry = self.cancel_tokens.lock().unwrap();
            registry.get(&job_id).cloned()
        };
        if let Some(token) = maybe_token {
            token.cancel();
        }
    }

    fn working_dir(&self) -> &Path {
        &self.working_dir
    }
}

impl CliBackend {
    fn cleanup_job(registry: Arc<Mutex<HashMap<JobId, CancelToken>>>, job_id: JobId) {
        let mut registry = registry.lock().unwrap();
        registry.remove(&job_id);
    }

    fn restore_static_state(
        state: Arc<Mutex<CliBackendState>>,
        session: Option<PtyCodexSession>,
        disable_pty: bool,
    ) {
        let mut state = state.lock().unwrap();
        if disable_pty {
            state.pty_disabled = true;
            state.codex_session = None;
            return;
        }
        if let Some(session) = session {
            state.codex_session = Some(session);
        }
    }
}

#[derive(Clone)]
struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
enum CodexCallError {
    Cancelled,
    Failure(anyhow::Error),
}

impl From<anyhow::Error> for CodexCallError {
    fn from(err: anyhow::Error) -> Self {
        Self::Failure(err)
    }
}

impl From<std::io::Error> for CodexCallError {
    fn from(err: std::io::Error) -> Self {
        Self::Failure(err.into())
    }
}

struct CodexRunOutcome {
    codex_session: Option<PtyCodexSession>,
    disable_pty: bool,
}

struct JobContext {
    job_id: JobId,
    request: CodexRequest,
    mode: RequestMode,
    config: AppConfig,
    working_dir: PathBuf,
}

fn run_codex_job(
    context: JobContext,
    codex_session: Option<PtyCodexSession>,
    cancel: CancelToken,
    sender: &EventSender,
) -> CodexRunOutcome {
    let JobContext {
        job_id,
        request,
        mode,
        config,
        working_dir,
    } = context;
    let mut outcome = CodexRunOutcome {
        codex_session,
        disable_pty: false,
    };

    if sender
        .emit(BackendEvent {
            job_id,
            kind: BackendEventKind::Started { mode },
        })
        .is_err()
    {
        log_debug("CodexBackend: failed to emit Started event (queue overflow)");
        return outcome;
    }

    let prompt = match request.payload {
        RequestPayload::Chat { prompt } => prompt,
    };

    #[cfg(test)]
    if let Some(events) = try_job_hook(&prompt, &cancel) {
        for kind in events {
            let _ = sender.emit(BackendEvent { job_id, kind });
        }
        return outcome;
    }

    if prompt.trim().is_empty() {
        let _ = sender.emit(BackendEvent {
            job_id,
            kind: BackendEventKind::FatalError {
                phase: "input_validation",
                message: "Prompt is empty.".into(),
                disable_pty: false,
            },
        });
        return outcome;
    }

    let started_at = Instant::now();
    let mut stats = BackendStats::new(started_at);
    let mut codex_output: Option<String> = None;

    if config.persistent_codex {
        if let Some(mut session) = outcome.codex_session.take() {
            stats.pty_attempts = 1;
            log_debug("CodexBackend: attempting persistent Codex session");
            match call_codex_via_session(&mut session, &prompt, &cancel) {
                Ok(text) => {
                    codex_output = Some(text);
                    outcome.codex_session = Some(session);
                }
                Err(CodexCallError::Cancelled) => {
                    let _ = sender.emit(BackendEvent {
                        job_id,
                        kind: BackendEventKind::Canceled { disable_pty: false },
                    });
                    outcome.codex_session = Some(session);
                    return outcome;
                }
                Err(err) => {
                    outcome.disable_pty = true;
                    let _ = sender.emit(BackendEvent {
                        job_id,
                        kind: BackendEventKind::RecoverableError {
                            phase: "pty_session",
                            message: format!("Persistent Codex failed: {err:?}"),
                            retry_available: true,
                        },
                    });
                }
            }
        }
    }

    if cancel.is_cancelled() {
        let _ = sender.emit(BackendEvent {
            job_id,
            kind: BackendEventKind::Canceled {
                disable_pty: outcome.disable_pty,
            },
        });
        return outcome;
    }

    let output_text = match codex_output {
        Some(text) => text,
        None => match call_codex_cli(&config, &prompt, &working_dir, &cancel) {
            Ok(text) => {
                stats.cli_fallback_used = true;
                text
            }
            Err(CodexCallError::Cancelled) => {
                let _ = sender.emit(BackendEvent {
                    job_id,
                    kind: BackendEventKind::Canceled {
                        disable_pty: outcome.disable_pty,
                    },
                });
                return outcome;
            }
            Err(CodexCallError::Failure(err)) => {
                let _ = sender.emit(BackendEvent {
                    job_id,
                    kind: BackendEventKind::FatalError {
                        phase: "cli",
                        message: format!("{err:#}"),
                        disable_pty: outcome.disable_pty,
                    },
                });
                return outcome;
            }
        },
    };

    stats.finished_at = Instant::now();
    stats.bytes_transferred = output_text.len();
    stats.disable_pty = outcome.disable_pty;

    let sanitized_output = if outcome.codex_session.is_some() && config.persistent_codex {
        output_text
    } else {
        sanitize_pty_output(output_text.as_bytes())
    };
    let sanitized_lines = prepare_for_display(&sanitized_output);
    let mut lines = Vec::with_capacity(sanitized_lines.len() + 4);
    let prompt_line = format!("> {}", prompt.trim());
    lines.push(prompt_line);
    lines.push(String::new());
    lines.extend(sanitized_lines);
    lines.push(String::new());

    let line_count = lines.len();
    if config.log_timings {
        let total_ms = stats
            .finished_at
            .duration_since(stats.started_at)
            .as_secs_f64()
            * 1000.0;
        log_debug(&format!(
            "timing|phase=codex_job|job_id={job_id}|pty_attempts={}|cli_fallback={}|disable_pty={}|total_ms={total_ms:.1}|lines={line_count}",
            stats.pty_attempts, stats.cli_fallback_used, outcome.disable_pty
        ));
    }

    let status = format!("Codex returned {line_count} lines.");
    let _ = sender.emit(BackendEvent {
        job_id,
        kind: BackendEventKind::Finished {
            lines,
            status,
            stats,
        },
    });
    outcome
}

fn call_codex_cli(
    config: &AppConfig,
    prompt: &str,
    working_dir: &Path,
    cancel: &CancelToken,
) -> Result<String, CodexCallError> {
    // Skip broken Python PTY helper - use direct CLI invocation
    // The helper fails because codex checks if stdout is a TTY before we can attach the PTY
    // Persistent PTY (PtyCodexSession) is the proper solution

    // Primary fallback: interactive Codex invocation
    let mut interactive_cmd = Command::new(&config.codex_cmd);
    interactive_cmd
        .args(&config.codex_args)
        .arg("-C")
        .arg(working_dir)
        .env("TERM", &config.term_value)
        .env("CODEX_NONINTERACTIVE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let interactive_output =
        spawn_with_cancel(interactive_cmd, Some(prompt), cancel).map_err(|err| match err {
            CodexCallError::Cancelled => CodexCallError::Cancelled,
            CodexCallError::Failure(e) => CodexCallError::Failure(
                e.context(format!("failed to spawn interactive {}", config.codex_cmd)),
            ),
        })?;

    if interactive_output.status.success() {
        return Ok(String::from_utf8_lossy(&interactive_output.stdout).to_string());
    }

    let interactive_stderr = String::from_utf8_lossy(&interactive_output.stderr).to_string();

    // Last resort: codex exec -
    let mut exec_cmd = Command::new(&config.codex_cmd);
    exec_cmd
        .arg("exec")
        .arg("-C")
        .arg(working_dir)
        .args(&config.codex_args)
        .env("TERM", &config.term_value)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let exec_output =
        spawn_with_cancel(exec_cmd, Some(prompt), cancel).map_err(|err| match err {
            CodexCallError::Cancelled => CodexCallError::Cancelled,
            CodexCallError::Failure(e) => CodexCallError::Failure(
                e.context(format!("failed to spawn {} exec", config.codex_cmd)),
            ),
        })?;

    if exec_output.status.success() {
        return Ok(String::from_utf8_lossy(&exec_output.stdout).to_string());
    }

    Err(CodexCallError::Failure(anyhow!(
        "All codex invocation methods failed:\n\
           PTY Helper: Check if scripts/run_in_pty.py exists\n\
           Interactive: {}\n\
           Exec mode: {}",
        interactive_stderr.trim(),
        String::from_utf8_lossy(&exec_output.stderr).trim()
    )))
}

fn call_codex_via_session(
    session: &mut PtyCodexSession,
    prompt: &str,
    cancel: &CancelToken,
) -> Result<String, CodexCallError> {
    session
        .send(prompt)
        .context("failed to write prompt to persistent Codex session")?;

    let mut combined_raw = Vec::new();
    let start_time = Instant::now();
    let overall_timeout = Duration::from_millis(PTY_OVERALL_TIMEOUT_MS);
    let first_output_deadline = start_time + Duration::from_millis(PTY_FIRST_BYTE_TIMEOUT_MS);
    let quiet_grace = Duration::from_millis(PTY_QUIET_GRACE_MS);
    let mut last_progress = Instant::now();
    let mut last_len = 0usize;

    loop {
        if cancel.is_cancelled() {
            return Err(CodexCallError::Cancelled);
        }

        // Use 50ms polling interval (much smaller than overall timeout)
        let output_chunks = session.read_output_timeout(Duration::from_millis(50));
        for chunk in output_chunks {
            // Removed excessive debug logging - was writing 100k+ lines per request
            combined_raw.extend_from_slice(&chunk);
            last_progress = Instant::now();
        }

        if !combined_raw.is_empty() {
            let sanitized = sanitize_pty_output(&combined_raw);
            if sanitized.len() != last_len {
                last_len = sanitized.len();
                last_progress = Instant::now();
            }
            // Removed excessive debug logging that writes 500k+ lines per request
            let idle = Instant::now().duration_since(last_progress);
            if !sanitized.trim().is_empty() && idle >= quiet_grace {
                return Ok(sanitized);
            }
            if idle >= overall_timeout {
                if sanitized.trim().is_empty() {
                    break;
                }
                return Ok(sanitized);
            }
        } else {
            let now = Instant::now();
            if now >= first_output_deadline {
                log_debug(&format!(
                    "Persistent Codex session produced no output within {PTY_FIRST_BYTE_TIMEOUT_MS}ms; falling back"
                ));
                return Err(CodexCallError::Failure(anyhow!(
                    "persistent Codex session timed out before producing output"
                )));
            }
            if now.duration_since(start_time) >= overall_timeout {
                break;
            }
        }
    }

    log_debug("Persistent Codex session yielded no printable output; falling back");
    Err(CodexCallError::Failure(anyhow!(
        "persistent Codex session returned no text"
    )))
}

/// Outcome of the optional Python PTY helper invocation.
// Python PTY helper removed - we have a proper Rust PTY implementation (PtyCodexSession)
// that handles all PTY requirements without needing external Python scripts

fn spawn_with_cancel(
    mut cmd: Command,
    prompt: Option<&str>,
    cancel: &CancelToken,
) -> Result<Output, CodexCallError> {
    let mut child = cmd.spawn()?;
    if let Some(text) = prompt {
        if let Some(mut stdin) = child.stdin.take() {
            write_prompt_with_newline(&mut stdin, text)?;
        }
    }
    wait_child_with_cancel(child, cancel)
}

fn wait_child_with_cancel(child: Child, cancel: &CancelToken) -> Result<Output, CodexCallError> {
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    let mut cancel_requested_at: Option<Instant> = None;
    let mut sigkill_sent = false;

    loop {
        match rx.try_recv() {
            Ok(result) => {
                return match result {
                    Ok(output) => {
                        if cancel_requested_at.is_some() {
                            Err(CodexCallError::Cancelled)
                        } else {
                            Ok(output)
                        }
                    }
                    Err(err) => Err(CodexCallError::Failure(err.into())),
                };
            }
            Err(TryRecvError::Disconnected) => {
                return Err(CodexCallError::Failure(anyhow!(
                    "Codex child waiter disconnected unexpectedly"
                )));
            }
            Err(TryRecvError::Empty) => {}
        }

        if cancel.is_cancelled() {
            if cancel_requested_at.is_none() {
                log_debug("CodexJob: cancellation requested; sending SIGTERM");
                send_signal(pid, Signal::Term);
                cancel_requested_at = Some(Instant::now());
            } else if !sigkill_sent
                && cancel_requested_at
                    .map(|start| start.elapsed() >= Duration::from_millis(500))
                    .unwrap_or(false)
            {
                log_debug("CodexJob: escalation to SIGKILL");
                send_signal(pid, Signal::Kill);
                sigkill_sent = true;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}

enum Signal {
    Term,
    Kill,
}

fn send_signal(pid: u32, signal: Signal) {
    #[cfg(unix)]
    unsafe {
        let signo = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        if libc::kill(pid as i32, signo) != 0 {
            log_debug(&format!(
                "CodexJob: failed to send signal {signo} to pid {pid}: {}",
                io::Error::last_os_error()
            ));
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = signal;
        log_debug("CodexJob: cancellation requested, but signals unsupported on this platform");
    }
}

/// Normalize CR/LF pairs, strip ANSI, and guarantee clean UTF-8 suitable for the TUI.
pub fn sanitize_pty_output(raw: &[u8]) -> String {
    if raw.is_empty() {
        return String::new();
    }

    let normalized = normalize_control_bytes(raw);
    let ansi_free = strip(&normalized);
    let mut text = String::from_utf8_lossy(&ansi_free).to_string();
    if raw.last() == Some(&b'\n') && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn prepare_for_display(text: &str) -> Vec<String> {
    text.lines().map(|line| line.to_string()).collect()
}

fn normalize_control_bytes(raw: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(raw.len());
    let mut idx = 0;
    let mut line_start = 0usize;

    while idx < raw.len() {
        match raw[idx] {
            b'\r' => {
                output.truncate(line_start);
                idx += 1;
            }
            b'\n' => {
                output.push(b'\n');
                idx += 1;
                line_start = output.len();
            }
            b'\x08' => {
                idx += 1;
                let removed_newline = pop_last_codepoint(&mut output);
                if removed_newline {
                    line_start = current_line_start(&output);
                }
            }
            0 => {
                idx += 1;
            }
            0x1B => {
                if let Some(next) = raw.get(idx + 1) {
                    if *next == b']' {
                        idx = skip_osc_sequence(raw, idx + 2);
                        continue;
                    } else if *next == b'[' {
                        if let Some((end, final_byte)) = find_csi_sequence(raw, idx) {
                            if final_byte == b'm' {
                                idx = end + 1;
                                continue;
                            }
                            output.extend_from_slice(raw.get(idx..=end).unwrap_or(&[]));
                            idx = end + 1;
                            continue;
                        }
                    }
                }
                output.push(raw[idx]);
                idx += 1;
            }
            byte => {
                output.push(byte);
                idx += 1;
            }
        }
        if line_start > output.len() {
            line_start = current_line_start(&output);
        }
    }

    output
}

fn pop_last_codepoint(buf: &mut Vec<u8>) -> bool {
    if buf.is_empty() {
        return false;
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        return true;
    }
    while let Some(byte) = buf.pop() {
        if (byte & 0b1100_0000) != 0b1000_0000 {
            break;
        }
    }
    false
}

fn current_line_start(buf: &[u8]) -> usize {
    buf.iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

fn skip_osc_sequence(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() {
        match bytes[cursor] {
            0x07 => return cursor + 1,
            0x1B if cursor + 1 < bytes.len() && bytes[cursor + 1] == b'\\' => {
                return cursor + 2;
            }
            _ => cursor += 1,
        }
    }
    cursor
}

fn find_csi_sequence(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    if bytes.get(start)? != &0x1B || bytes.get(start + 1)? != &b'[' {
        return None;
    }
    let mut idx = start + 2;
    while idx < bytes.len() {
        let b = bytes[idx];
        if (0x40..=0x7E).contains(&b) {
            return Some((idx, b));
        }
        idx += 1;
    }
    None
}

fn write_prompt_with_newline<W: Write>(writer: &mut W, prompt: &str) -> io::Result<()> {
    writer.write_all(prompt.as_bytes())?;
    if !prompt.ends_with('\n') {
        writer.write_all(b"\n")?;
    }
    Ok(())
}

#[cfg(test)]
use std::sync::{MutexGuard, OnceLock};

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct CancelProbe(CancelToken);

#[cfg(test)]
impl CancelProbe {
    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[cfg(test)]
type CodexJobHook = Box<dyn Fn(&str, CancelProbe) -> Vec<BackendEventKind> + Send + Sync + 'static>;

#[cfg(test)]
static CODEX_JOB_HOOK: OnceLock<Mutex<Option<CodexJobHook>>> = OnceLock::new();
#[cfg(test)]
static JOB_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
#[cfg(test)]
static ACTIVE_BACKEND_THREADS: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn active_backend_threads() -> usize {
    ACTIVE_BACKEND_THREADS.load(Ordering::SeqCst)
}

#[cfg(test)]
struct BackendThreadGuard;

#[cfg(test)]
impl BackendThreadGuard {
    fn new() -> Self {
        ACTIVE_BACKEND_THREADS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

#[cfg(test)]
impl Drop for BackendThreadGuard {
    fn drop(&mut self) {
        ACTIVE_BACKEND_THREADS.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn try_job_hook(prompt: &str, cancel: &CancelToken) -> Option<Vec<BackendEventKind>> {
    let storage = CODEX_JOB_HOOK.get_or_init(|| Mutex::new(None));
    let guard = storage.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(|hook| hook(prompt, CancelProbe(cancel.clone())))
}

#[cfg(test)]
pub(crate) struct JobHookGuard {
    lock: Option<MutexGuard<'static, ()>>,
}

#[cfg(test)]
impl Drop for JobHookGuard {
    fn drop(&mut self) {
        if let Some(storage) = CODEX_JOB_HOOK.get() {
            *storage.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
        self.lock.take();
    }
}

#[cfg(test)]
pub(crate) fn with_job_hook<R>(hook: CodexJobHook, f: impl FnOnce() -> R) -> (R, JobHookGuard) {
    let storage = CODEX_JOB_HOOK.get_or_init(|| Mutex::new(None));
    let lock = JOB_HOOK_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    *storage.lock().unwrap_or_else(|e| e.into_inner()) = Some(hook);
    let result = f();
    (result, JobHookGuard { lock: Some(lock) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn sanitize_handles_backspace() {
        let cleaned = sanitize_pty_output(b"a\x08b\n");
        assert_eq!(cleaned, "b\n");
    }

    #[test]
    fn sanitize_strips_cursor_query_bytes() {
        let cleaned = sanitize_pty_output(b"Hello\x1b[6nWorld\n");
        assert_eq!(cleaned, "HelloWorld\n");
    }

    #[test]
    fn sanitize_preserves_numeric_lines() {
        let sample = "2024\n0;1;2\n";
        let cleaned = sanitize_pty_output(sample.as_bytes());
        assert_eq!(cleaned, sample);
    }

    #[test]
    fn sanitize_keeps_wide_glyphs() {
        let sample = "│> Test 😊 你好\n";
        let cleaned = sanitize_pty_output(sample.as_bytes());
        assert_eq!(cleaned, sample);
    }

    #[test]
    fn sanitize_strips_escape_wrapped_cursor_report() {
        let cleaned = sanitize_pty_output(b"\x1b[>0;0;0uHello");
        assert_eq!(cleaned, "Hello");
    }

    #[test]
    fn bounded_queue_drops_token_before_status() {
        let queue = BoundedEventQueue::new(2);
        queue
            .push(BackendEvent {
                job_id: 1,
                kind: BackendEventKind::Token {
                    text: "token".into(),
                },
            })
            .unwrap();
        queue
            .push(BackendEvent {
                job_id: 1,
                kind: BackendEventKind::Status {
                    message: "status".into(),
                },
            })
            .unwrap();
        // Force overflow; token event should be dropped first.
        queue
            .push(BackendEvent {
                job_id: 1,
                kind: BackendEventKind::RecoverableError {
                    phase: "test",
                    message: "backpressure".into(),
                    retry_available: true,
                },
            })
            .unwrap();
        let events = queue.drain();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, BackendEventKind::Status { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e.kind, BackendEventKind::RecoverableError { .. })));
    }

    #[test]
    fn event_sender_notifies_listener() {
        let queue = Arc::new(BoundedEventQueue::new(8));
        let (tx, rx) = mpsc::channel();
        let sender = EventSender::new(Arc::clone(&queue), tx);
        sender
            .emit(BackendEvent {
                job_id: 7,
                kind: BackendEventKind::Status {
                    message: "hello".into(),
                },
            })
            .unwrap();
        rx.try_recv().expect("signal");
        let events = queue.drain();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].job_id, 7);
    }
}
