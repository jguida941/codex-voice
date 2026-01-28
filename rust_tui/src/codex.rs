//! Codex backend interfaces plus the async worker used by the TUI. The backend
//! exposes a `CodexBackend` trait that emits structured events so the UI can
//! render streaming output, show “thinking…” indicators, and react to
//! recoverable vs fatal failures without duplicating business logic.

use crate::{config::AppConfig, log_debug, pty_session::PtyCodexSession};
use anyhow::{anyhow, Context, Result};
#[cfg(test)]
use std::cell::Cell;
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
#[cfg(test)]
const PTY_FIRST_BYTE_TIMEOUT_MS: u64 = 500;
#[cfg(not(test))]
const PTY_FIRST_BYTE_TIMEOUT_MS: u64 = 3000; // 3s for first byte - fail fast if PTY not working
#[cfg(test)]
const PTY_OVERALL_TIMEOUT_MS: u64 = 5000;
#[cfg(not(test))]
const PTY_OVERALL_TIMEOUT_MS: u64 = 60000; // 60s overall for long responses
#[cfg(test)]
const PTY_QUIET_GRACE_MS: u64 = 200;
#[cfg(not(test))]
const PTY_QUIET_GRACE_MS: u64 = 2000; // 2s quiet period (was 350ms)
const PTY_HEALTHCHECK_TIMEOUT_MS: u64 = 5000; // 5s health check (was 2000ms)
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

#[cfg(test)]
impl BackendJob {
    fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum TestSignal {
    Ready,
    Disconnected,
    Empty,
}

#[cfg(test)]
pub(crate) fn build_test_backend_job(
    events: Vec<BackendEvent>,
    signal: TestSignal,
) -> BackendJob {
    let queue = Arc::new(BoundedEventQueue::new(32));
    for event in events {
        let _ = queue.push(event);
    }
    let (tx, rx) = mpsc::channel();
    match signal {
        TestSignal::Ready => {
            let _ = tx.send(());
        }
        TestSignal::Disconnected => {
            drop(tx);
        }
        TestSignal::Empty => {
            let _ = tx;
        }
    }
    let handle = thread::spawn(|| {});
    BackendJob::new(0, queue, rx, handle, CancelToken::new())
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

#[cfg(test)]
thread_local! {
    static RESET_SESSION_COUNT: Cell<usize> = Cell::new(0);
}

#[cfg(test)]
pub(crate) fn reset_session_count() -> usize {
    RESET_SESSION_COUNT.with(|count| count.get())
}

#[cfg(test)]
pub(crate) fn reset_session_count_reset() {
    RESET_SESSION_COUNT.with(|count| count.set(0));
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
        let wd_str = working_dir.to_str().unwrap_or(".");
        log_debug(&format!(
            "Attempting to create PTY session with codex_cmd={}, working_dir={}",
            self.config.codex_cmd, wd_str
        ));

        // Build args with -C flag for working directory
        let mut pty_args = vec!["-C".to_string(), wd_str.to_string()];
        pty_args.extend(self.config.codex_args.clone());

        match PtyCodexSession::new(
            &self.config.codex_cmd,
            wd_str,
            &pty_args,
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
                log_debug(&format!("Failed to create PTY session: {err:#}"));
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

    /// Drop the cached PTY session so the next request can re-establish state.
    pub fn reset_session(&self) {
        #[cfg(test)]
        RESET_SESSION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        let mut state = self.state.lock().unwrap();
        state.codex_session = None;
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

    // Always sanitize output - PTY has control chars, CLI is already clean (sanitize is no-op)
    let sanitized_output = sanitize_pty_output(output_text.as_bytes());
    let sanitized_lines = prepare_for_display(&sanitized_output);
    let mut lines = Vec::with_capacity(sanitized_lines.len() + 4);
    let prompt_line = format!("> {}", prompt.trim());
    lines.push(prompt_line);
    lines.push(String::new());
    lines.extend(sanitized_lines);
    lines.push(String::new());

    let line_count = lines.len();
    if config.log_timings {
        let total_ms = duration_ms(stats.finished_at.duration_since(stats.started_at));
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
    // Use codex exec - directly (most reliable non-PTY path)
    // The interactive mode (codex -C ...) always fails with "stdin is not a terminal"
    // so we skip it to reduce latency
    let mut exec_cmd = Command::new(&config.codex_cmd);
    exec_cmd
        .arg("exec")
        .arg("-") // Read from stdin - must be right after "exec"
        .arg("-C")
        .arg(working_dir)
        .args(&config.codex_args)
        .env("TERM", &config.term_value)
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
        "codex exec failed: {}",
        String::from_utf8_lossy(&exec_output.stderr).trim()
    )))
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn compute_deadline(start: Instant, timeout: Duration) -> Instant {
    start + timeout
}

fn should_accept_printable(
    has_printable: bool,
    idle_since_raw: Duration,
    quiet_grace: Duration,
) -> bool {
    has_printable && idle_since_raw >= quiet_grace
}

fn should_fail_control_only(
    has_printable: bool,
    idle_since_printable: Duration,
    control_only_timeout: Duration,
) -> bool {
    !has_printable && idle_since_printable >= control_only_timeout
}

fn should_break_overall(elapsed: Duration, overall_timeout: Duration) -> bool {
    elapsed >= overall_timeout
}

fn first_output_timed_out(now: Instant, deadline: Instant) -> bool {
    now >= deadline
}

trait CodexSession {
    fn send(&mut self, text: &str) -> Result<()>;
    fn read_output_timeout(&self, timeout: Duration) -> Vec<Vec<u8>>;
}

impl CodexSession for PtyCodexSession {
    fn send(&mut self, text: &str) -> Result<()> {
        PtyCodexSession::send(self, text)
    }

    fn read_output_timeout(&self, timeout: Duration) -> Vec<Vec<u8>> {
        PtyCodexSession::read_output_timeout(self, timeout)
    }
}

fn call_codex_via_session<S: CodexSession>(
    session: &mut S,
    prompt: &str,
    cancel: &CancelToken,
) -> Result<String, CodexCallError> {
    session
        .send(prompt)
        .context("failed to write prompt to persistent Codex session")?;

    let mut combined_raw = Vec::new();
    let start_time = Instant::now();
    let overall_timeout = Duration::from_millis(PTY_OVERALL_TIMEOUT_MS);
    let first_output_deadline =
        compute_deadline(start_time, Duration::from_millis(PTY_FIRST_BYTE_TIMEOUT_MS));
    let quiet_grace = Duration::from_millis(PTY_QUIET_GRACE_MS);
    let control_only_timeout = Duration::from_millis(5000); // 5s max if only control sequences
    let mut last_printable_output = start_time;
    let mut first_raw_output: Option<Instant> = None;
    let mut last_raw_output = start_time;

    loop {
        if cancel.is_cancelled() {
            return Err(CodexCallError::Cancelled);
        }

        // Use 50ms polling interval
        let output_chunks = session.read_output_timeout(Duration::from_millis(50));
        for chunk in output_chunks {
            combined_raw.extend_from_slice(&chunk);
            last_raw_output = Instant::now();
            if first_raw_output.is_none() {
                first_raw_output = Some(last_raw_output);
            }
        }

        let now = Instant::now();

        if !combined_raw.is_empty() {
            let sanitized = sanitize_pty_output(&combined_raw);
            let has_printable = !sanitized.trim().is_empty();

            if has_printable {
                last_printable_output = now;
            }

            let idle_since_raw = now.duration_since(last_raw_output);
            let idle_since_printable = now.duration_since(last_printable_output);

            // Success: got printable output and no new data for quiet_grace period
            if should_accept_printable(has_printable, idle_since_raw, quiet_grace) {
                return Ok(sanitized);
            }

            // Fail fast: raw output but no printable content for control_only_timeout
            if should_fail_control_only(has_printable, idle_since_printable, control_only_timeout) {
                log_debug("Persistent Codex session produced only control sequences; falling back");
                return Err(CodexCallError::Failure(anyhow!(
                    "persistent Codex session produced no printable output"
                )));
            }

            // Overall timeout with output
            if should_break_overall(now.duration_since(start_time), overall_timeout) {
                if has_printable {
                    return Ok(sanitized);
                }
                break;
            }
        } else {
            // No output yet - check first byte timeout
            if first_output_timed_out(now, first_output_deadline) {
                log_debug(&format!(
                    "Persistent Codex session produced no output within {PTY_FIRST_BYTE_TIMEOUT_MS}ms; falling back"
                ));
                return Err(CodexCallError::Failure(anyhow!(
                    "persistent Codex session timed out before producing output"
                )));
            }
            if should_break_overall(now.duration_since(start_time), overall_timeout) {
                break;
            }
        }
    }

    log_debug("Persistent Codex session yielded no printable output; falling back");
    Err(CodexCallError::Failure(anyhow!(
        "persistent Codex session returned no text"
    )))
}

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
            } else if should_send_sigkill(sigkill_sent, cancel_requested_at, Instant::now()) {
                log_debug("CodexJob: escalation to SIGKILL");
                send_signal(pid, Signal::Kill);
                sigkill_sent = true;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}

fn should_send_sigkill(
    sigkill_sent: bool,
    cancel_requested_at: Option<Instant>,
    now: Instant,
) -> bool {
    if sigkill_sent {
        return false;
    }
    match cancel_requested_at {
        Some(start) => now.duration_since(start) >= Duration::from_millis(500),
        None => false,
    }
}

enum Signal {
    Term,
    Kill,
}

#[cfg(test)]
static SEND_SIGNAL_FAILURES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
fn reset_send_signal_failures() {
    SEND_SIGNAL_FAILURES.store(0, Ordering::SeqCst);
}

#[cfg(test)]
fn send_signal_failures() -> usize {
    SEND_SIGNAL_FAILURES.load(Ordering::SeqCst)
}

fn send_signal(pid: u32, signal: Signal) {
    #[cfg(unix)]
    unsafe {
        let signo = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        if libc::kill(pid as i32, signo) != 0 {
            #[cfg(test)]
            SEND_SIGNAL_FAILURES.fetch_add(1, Ordering::SeqCst);
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
    let mut guard = init_guard(raw.len());

    while idx < raw.len() {
        if !step_guard(&mut guard) {
            break;
        }
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
                        // Skip OSC sequences entirely
                        idx = skip_osc_sequence(raw, idx + 2);
                        continue;
                    } else if *next == b'[' {
                        // Skip ALL CSI sequences - don't preserve any
                        if let Some((end, _final_byte)) = find_csi_sequence(raw, idx) {
                            idx = end + 1;
                            continue;
                        }
                    } else if *next == b'(' || *next == b')' {
                        // Skip character set designation sequences (ESC ( or ESC ))
                        idx += 3;
                        continue;
                    } else if *next == b'>' || *next == b'=' {
                        // Skip keypad mode sequences
                        idx += 2;
                        continue;
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
        line_start = clamp_line_start(line_start, &output);
    }

    output
}

fn init_guard(len: usize) -> usize {
    len.saturating_mul(4).max(16)
}

fn step_guard(guard: &mut usize) -> bool {
    if *guard == 0 {
        return false;
    }
    *guard -= 1;
    true
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

fn clamp_line_start(line_start: usize, buf: &[u8]) -> usize {
    if line_start > buf.len() {
        current_line_start(buf)
    } else {
        line_start
    }
}

fn skip_osc_sequence(bytes: &[u8], mut cursor: usize) -> usize {
    for idx in cursor..bytes.len() {
        match bytes[idx] {
            0x07 => return idx + 1,
            0x1B if idx + 1 < bytes.len() && bytes[idx + 1] == b'\\' => return idx + 2,
            _ => {}
        }
        cursor = idx + 1;
    }
    cursor
}

fn find_csi_sequence(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    if bytes.get(start)? != &0x1B || bytes.get(start + 1)? != &b'[' {
        return None;
    }
    for idx in (start + 2)..bytes.len() {
        let b = bytes[idx];
        if (0x40..=0x7E).contains(&b) {
            return Some((idx, b));
        }
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
    use clap::Parser;
    use std::sync::Arc;
    use std::thread;

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

    #[test]
    fn prepare_for_display_preserves_empty_lines() {
        let input = "alpha\n\nbeta\n";
        let lines = prepare_for_display(input);
        assert_eq!(lines, vec!["alpha", "", "beta"]);
    }

    #[test]
    fn normalize_control_bytes_handles_cr_and_backspace() {
        let cleaned = normalize_control_bytes(b"hello\rworld\n");
        assert_eq!(String::from_utf8_lossy(&cleaned), "world\n");

        let cleaned = normalize_control_bytes(b"a\n\x08b");
        assert_eq!(String::from_utf8_lossy(&cleaned), "ab");
    }

    #[test]
    fn normalize_control_bytes_skips_osc_and_csi_sequences() {
        let cleaned = normalize_control_bytes(b"hi\x1b]0;title\x07there");
        assert_eq!(String::from_utf8_lossy(&cleaned), "hithere");

        let cleaned = normalize_control_bytes(b"hi\x1b]0;title\x1b\\there");
        assert_eq!(String::from_utf8_lossy(&cleaned), "hithere");

        let cleaned = normalize_control_bytes(b"hi\x1b[31mred");
        assert_eq!(String::from_utf8_lossy(&cleaned), "hired");
    }

    #[test]
    fn normalize_control_bytes_skips_charset_and_keypad_sequences() {
        let cleaned = normalize_control_bytes(b"A\x1b(BZ\x1b>Q");
        assert_eq!(String::from_utf8_lossy(&cleaned), "AZQ");
    }

    #[test]
    fn normalize_control_bytes_drops_nul_and_keeps_unknown_escape() {
        let cleaned = normalize_control_bytes(b"a\0b\x1bZ");
        assert_eq!(cleaned, b"ab\x1bZ");
    }

    #[test]
    fn normalize_control_bytes_handles_osc_after_bel() {
        let cleaned = normalize_control_bytes(b"\x07A\x1b]0;title\x07Z");
        assert_eq!(cleaned, b"\x07AZ");
    }

    #[test]
    fn normalize_control_bytes_skips_osc_with_long_prefix() {
        let cleaned = normalize_control_bytes(b"12345678\x1b]x\x07Z");
        assert_eq!(String::from_utf8_lossy(&cleaned), "12345678Z");
    }

    #[test]
    fn pop_last_codepoint_handles_utf8_and_newline() {
        let mut buf = "a😊".as_bytes().to_vec();
        assert!(!pop_last_codepoint(&mut buf));
        assert_eq!(buf, b"a");

        let mut buf = b"hi\n".to_vec();
        assert!(pop_last_codepoint(&mut buf));
        assert_eq!(buf, b"hi");
    }

    #[test]
    fn current_line_start_finds_last_newline() {
        assert_eq!(current_line_start(b""), 0);
        assert_eq!(current_line_start(b"alpha"), 0);
        assert_eq!(current_line_start(b"alpha\nbeta"), 6);
    }

    #[test]
    fn clamp_line_start_keeps_valid_and_clamps_overflow() {
        let buf = b"hi\nthere";
        assert_eq!(clamp_line_start(buf.len(), buf), buf.len());
        assert_eq!(clamp_line_start(42, buf), current_line_start(buf));
    }

    #[test]
    fn guard_helpers_enforce_limits() {
        assert_eq!(init_guard(0), 16);
        assert_eq!(init_guard(4), 16);
        assert_eq!(init_guard(5), 20);

        let mut guard = 2;
        assert!(step_guard(&mut guard));
        assert_eq!(guard, 1);
        assert!(step_guard(&mut guard));
        assert_eq!(guard, 0);
        assert!(!step_guard(&mut guard));
    }

    #[test]
    fn duration_ms_converts_to_millis() {
        let duration = Duration::from_millis(1500);
        assert!((duration_ms(duration) - 1500.0).abs() < 0.01);
    }

    #[test]
    fn compute_deadline_moves_forward() {
        let start = Instant::now();
        let deadline = compute_deadline(start, Duration::from_millis(10));
        assert!(deadline >= start);
    }

    #[test]
    fn session_timeout_helpers_handle_boundaries() {
        let quiet = Duration::from_millis(5);
        assert!(should_accept_printable(true, quiet, quiet));
        assert!(!should_accept_printable(false, quiet, quiet));

        let control_timeout = Duration::from_millis(7);
        assert!(should_fail_control_only(false, control_timeout, control_timeout));
        assert!(!should_fail_control_only(true, control_timeout, control_timeout));

        let overall = Duration::from_millis(3);
        assert!(should_break_overall(overall, overall));
        assert!(!should_break_overall(Duration::from_millis(2), overall));

        let now = Instant::now();
        assert!(first_output_timed_out(now, now));
        let later = now + Duration::from_millis(50);
        assert!(!first_output_timed_out(now, later));
    }

    #[test]
    fn should_send_sigkill_after_delay() {
        let start = Instant::now();
        assert!(!should_send_sigkill(false, Some(start), start));
        let later = start + Duration::from_millis(500);
        assert!(should_send_sigkill(false, Some(start), later));
        assert!(!should_send_sigkill(true, Some(start), later));
    }

    #[test]
    fn skip_osc_sequence_stops_on_bel_or_st() {
        let bytes = b"\x1b]0;title\x07rest";
        let end = skip_osc_sequence(bytes, 2);
        assert_eq!(end, 2 + b"0;title\x07".len());

        let bytes = b"\x1b]0;title\x1b\\rest";
        let end = skip_osc_sequence(bytes, 2);
        assert_eq!(end, 2 + b"0;title\x1b\\".len());

        let bytes = b"\x1b]unterminated";
        let end = skip_osc_sequence(bytes, 2);
        assert_eq!(end, bytes.len());
    }

    #[test]
    fn skip_osc_sequence_handles_trailing_escape() {
        let bytes = b"\x1b]title\x1b";
        let end = skip_osc_sequence(bytes, bytes.len() - 1);
        assert_eq!(end, bytes.len());
    }

    #[test]
    fn skip_osc_sequence_handles_immediate_st() {
        let bytes = b"\x1b\\";
        let end = skip_osc_sequence(bytes, 0);
        assert_eq!(end, bytes.len());
    }

    #[test]
    fn skip_osc_sequence_ignores_escape_without_st() {
        let bytes = b"\x1b]title\x1bXrest";
        let end = skip_osc_sequence(bytes, 2);
        assert_eq!(end, bytes.len());
    }

    #[test]
    fn find_csi_sequence_detects_final_byte() {
        let bytes = b"\x1b[31m";
        assert_eq!(find_csi_sequence(bytes, 0), Some((4, b'm')));
        assert_eq!(find_csi_sequence(bytes, 1), None);
    }

    #[test]
    fn find_csi_sequence_returns_none_for_non_csi() {
        let bytes = b"\x1bX123";
        assert_eq!(find_csi_sequence(bytes, 0), None);
    }

    #[test]
    fn find_csi_sequence_rejects_escape_without_bracket() {
        let bytes = b"\x1bXm";
        assert_eq!(find_csi_sequence(bytes, 0), None);
    }

    #[test]
    fn write_prompt_with_newline_appends_once() {
        let mut buf = Vec::new();
        write_prompt_with_newline(&mut buf, "hello").unwrap();
        assert_eq!(buf, b"hello\n");

        let mut buf = Vec::new();
        write_prompt_with_newline(&mut buf, "hello\n").unwrap();
        assert_eq!(buf, b"hello\n");
    }

    #[test]
    fn backend_job_take_handle_consumes_once() {
        let queue = Arc::new(BoundedEventQueue::new(1));
        let (_tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let handle = thread::spawn(|| {});
        let mut job = BackendJob::new(1, queue, rx, handle, cancel);
        let handle = job.take_handle().expect("handle");
        handle.join().unwrap();
        assert!(job.take_handle().is_none());
    }

    #[test]
    fn backend_job_cancel_sets_flag() {
        let queue = Arc::new(BoundedEventQueue::new(1));
        let (_tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let handle = thread::spawn(|| {});
        let mut job = BackendJob::new(1, queue, rx, handle, cancel);
        job.cancel();
        assert!(job.is_cancelled());
        let handle = job.take_handle().expect("handle");
        handle.join().unwrap();
    }

    #[test]
    fn backend_job_try_recv_signal_reports_empty_then_ready() {
        let queue = Arc::new(BoundedEventQueue::new(1));
        let (tx, rx) = mpsc::channel();
        let cancel = CancelToken::new();
        let handle = thread::spawn(|| {});
        let mut job = BackendJob::new(1, queue, rx, handle, cancel);
        assert!(matches!(job.try_recv_signal(), Err(TryRecvError::Empty)));
        tx.send(()).unwrap();
        assert!(job.try_recv_signal().is_ok());
        let handle = job.take_handle().expect("handle");
        handle.join().unwrap();
    }

    #[test]
    fn cancel_token_flips_state() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[cfg(unix)]
    fn write_stub_script(contents: &str) -> std::path::PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("codex_stub_{nanos}.sh"));
        fs::write(&path, contents).expect("write stub");
        let mut perms = fs::metadata(&path).expect("stat stub").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod stub");
        path
    }

    #[cfg(unix)]
    fn new_test_pty_session() -> PtyCodexSession {
        let args: Vec<String> = Vec::new();
        PtyCodexSession::new("/bin/cat", ".", &args, "xterm-256color")
            .expect("pty session")
    }

    #[cfg(unix)]
    #[test]
    fn call_codex_cli_reports_failure() {
        let script = "#!/bin/sh\ncat >/dev/null\necho \"boom\" 1>&2\nexit 42";
        let path = write_stub_script(script);
        let mut config = AppConfig::parse_from(["test"]);
        config.codex_cmd = path.to_string_lossy().into_owned();
        config.codex_args.clear();
        let err = call_codex_cli(&config, "hello", Path::new("."), &CancelToken::new());
        assert!(matches!(err, Err(CodexCallError::Failure(_))));
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    fn cli_backend_reuses_preloaded_session() {
        let mut config = AppConfig::parse_from(["test"]);
        config.persistent_codex = true;
        let backend = CliBackend::new(config);
        let session = new_test_pty_session();
        {
            let mut state = backend.state.lock().unwrap();
            state.pty_disabled = false;
            state.codex_session = Some(session);
        }
        let session = backend.take_codex_session_for_job();
        assert!(session.is_some());
        let state = backend.state.lock().unwrap();
        assert!(state.codex_session.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn cli_backend_ensure_codex_session_fails_for_bad_command() {
        let mut config = AppConfig::parse_from(["test"]);
        config.persistent_codex = true;
        config.codex_cmd = "bad\0cmd".into();
        let backend = CliBackend::new(config);
        let mut state = CliBackendState {
            codex_session: None,
            pty_disabled: false,
        };
        assert!(backend.ensure_codex_session(&mut state).is_err());
        assert!(state.codex_session.is_none());
    }

    #[test]
    fn cli_backend_cancels_and_cleans_registry() {
        let backend = CliBackend::new(AppConfig::parse_from(["test"]));
        let token = CancelToken::new();
        {
            let mut registry = backend.cancel_tokens.lock().unwrap();
            registry.insert(7, token.clone());
        }
        backend.cancel(7);
        assert!(token.is_cancelled());
        CliBackend::cleanup_job(Arc::clone(&backend.cancel_tokens), 7);
        let registry = backend.cancel_tokens.lock().unwrap();
        assert!(registry.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn cli_backend_reset_session_clears_cached_session() {
        let mut config = AppConfig::parse_from(["test"]);
        config.persistent_codex = true;
        let backend = CliBackend::new(config);
        let session = new_test_pty_session();
        {
            let mut state = backend.state.lock().unwrap();
            state.pty_disabled = false;
            state.codex_session = Some(session);
        }
        backend.reset_session();
        let state = backend.state.lock().unwrap();
        assert!(state.codex_session.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn cli_backend_restore_static_state_sets_session() {
        let state = Arc::new(Mutex::new(CliBackendState {
            codex_session: None,
            pty_disabled: false,
        }));
        let session = new_test_pty_session();
        CliBackend::restore_static_state(Arc::clone(&state), Some(session), false);
        let state = state.lock().unwrap();
        assert!(state.codex_session.is_some());
        assert!(!state.pty_disabled);
    }

    #[cfg(unix)]
    #[test]
    fn cli_backend_restore_static_state_disables_pty() {
        let state = Arc::new(Mutex::new(CliBackendState {
            codex_session: None,
            pty_disabled: false,
        }));
        CliBackend::restore_static_state(Arc::clone(&state), None, true);
        let state = state.lock().unwrap();
        assert!(state.pty_disabled);
        assert!(state.codex_session.is_none());
    }

    #[test]
    fn backend_job_ids_increment() {
        let backend = CliBackend::new(AppConfig::parse_from(["test"]));
        let (mut jobs, _guard) = with_job_hook(Box::new(|_, _| Vec::new()), || {
            let job1 = backend
                .start(CodexRequest::chat("one".into()))
                .expect("job1");
            let job2 = backend
                .start(CodexRequest::chat("two".into()))
                .expect("job2");
            (job1, job2)
        });
        assert!(jobs.0.id > 0);
        assert_eq!(jobs.0.id + 1, jobs.1.id);
        let handle = jobs.0.take_handle().expect("handle");
        handle.join().unwrap();
        let handle = jobs.1.take_handle().expect("handle");
        handle.join().unwrap();
    }

    struct StubSession {
        outputs: std::cell::RefCell<std::collections::VecDeque<Vec<Vec<u8>>>>,
    }

    impl StubSession {
        fn new(outputs: Vec<Vec<Vec<u8>>>) -> Self {
            Self {
                outputs: std::cell::RefCell::new(outputs.into()),
            }
        }
    }

    impl CodexSession for StubSession {
        fn send(&mut self, _text: &str) -> Result<()> {
            Ok(())
        }

        fn read_output_timeout(&self, _timeout: Duration) -> Vec<Vec<u8>> {
            self.outputs
                .borrow_mut()
                .pop_front()
                .unwrap_or_default()
        }
    }

    #[test]
    fn call_codex_via_session_returns_output() {
        let mut session = StubSession::new(vec![vec![b"ping\n".to_vec()]]);
        let output = call_codex_via_session(&mut session, "ping", &CancelToken::new())
            .expect("call_codex_via_session");
        assert!(output.contains("ping"));
    }

    #[cfg(unix)]
    #[test]
    fn pty_session_send_and_read_output() {
        crate::pty_session::reset_pty_session_counters();
        let mut session = new_test_pty_session();
        <PtyCodexSession as CodexSession>::send(&mut session, "ping\n").unwrap();
        let _ = <PtyCodexSession as CodexSession>::read_output_timeout(
            &session,
            Duration::from_millis(50),
        );
        assert!(crate::pty_session::pty_session_send_count() >= 1);
        assert!(crate::pty_session::pty_session_read_count() >= 1);
    }

    #[cfg(unix)]
    #[test]
    fn send_signal_terminates_child() {
        let mut child = Command::new("sleep").arg("5").spawn().expect("spawn sleep");
        let pid = child.id();
        send_signal(pid, Signal::Term);
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            if let Some(_status) = child.try_wait().expect("try_wait") {
                break;
            }
            if Instant::now() >= deadline {
                send_signal(pid, Signal::Kill);
                panic!("child did not exit after SIGTERM");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(unix)]
    #[test]
    fn send_signal_tracks_failure_on_invalid_pid() {
        reset_send_signal_failures();
        let current_pid = unsafe { libc::getpid() } as i32;
        let mut candidate = current_pid + 10000;
        for _ in 0..1000 {
            let res = unsafe { libc::kill(candidate, 0) };
            let err = io::Error::last_os_error();
            if res != 0 && err.kind() == io::ErrorKind::NotFound {
                break;
            }
            candidate += 1;
        }
        send_signal(candidate as u32, Signal::Term);
        assert!(send_signal_failures() >= 1);
    }

    #[test]
    fn call_codex_via_session_times_out_without_output() {
        let mut session = StubSession::new(Vec::new());
        let err = call_codex_via_session(&mut session, "ping", &CancelToken::new());
        assert!(matches!(err, Err(CodexCallError::Failure(_))));
    }
}
