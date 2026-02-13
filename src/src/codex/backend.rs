//! Core Codex request/event model that enforces bounded queues under load.

use crate::lock_or_recover;
use anyhow::{Error, Result};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[cfg(test)]
use std::thread;

/// Unique identifier for Codex requests routed through the backend.
pub type JobId = u64;

/// User-facing mode describing how Codex should treat the request.
#[derive(Debug, Clone, Copy)]
pub enum RequestMode {
    /// Standard conversational prompt/response request.
    Chat,
}

/// Payload variants supported by the backend.
#[derive(Debug, Clone)]
pub enum RequestPayload {
    /// Free-form chat prompt sent to Codex.
    Chat {
        /// Prompt text forwarded to the active backend invocation.
        prompt: String,
    },
}

/// Structured Codex request routed through the backend.
#[derive(Debug, Clone)]
pub struct CodexRequest {
    /// Operation payload (currently chat-only).
    pub payload: RequestPayload,
    /// Optional overall timeout for backend execution.
    pub timeout: Option<Duration>,
    /// Extra workspace file paths to include in request context.
    pub workspace_files: Vec<PathBuf>,
}

impl CodexRequest {
    /// Build a chat request with default timeout and no extra workspace files.
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
pub struct CodexJobStats {
    /// Backend strategy label used for telemetry grouping.
    pub backend_type: &'static str,
    /// Timestamp when job execution started.
    pub started_at: Instant,
    /// Timestamp when first token arrived, if any.
    pub first_token_at: Option<Instant>,
    /// Timestamp when job finished or failed.
    pub finished_at: Instant,
    /// Number of tokens streamed during job execution.
    pub tokens_received: usize,
    /// Raw bytes transferred from backend output.
    pub bytes_transferred: usize,
    /// Number of PTY start attempts made before completion/fallback.
    pub pty_attempts: u32,
    /// Whether non-PTY CLI fallback path was used.
    pub cli_fallback_used: bool,
    /// Whether PTY mode is currently disabled due to fatal failures.
    pub disable_pty: bool,
}

impl CodexJobStats {
    pub(super) fn new(now: Instant) -> Self {
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
pub struct CodexEvent {
    /// Job identifier associated with this event.
    pub job_id: JobId,
    /// Event payload describing job progress/state change.
    pub kind: CodexEventKind,
}

/// Classified event payload.
#[derive(Debug, Clone)]
pub enum CodexEventKind {
    /// Job accepted and execution began.
    Started {
        /// Mode used for this invocation.
        mode: RequestMode,
    },
    /// Informational status update from backend orchestration.
    Status {
        /// Human-readable status message.
        message: String,
    },
    /// Streaming token/output chunk from provider.
    Token {
        /// Token or output text fragment.
        text: String,
    },
    /// Non-fatal error where retry/fallback may continue job progress.
    RecoverableError {
        /// Stage in which the error occurred.
        phase: &'static str,
        /// Human-readable error details.
        message: String,
        /// Whether a retry path is available to caller.
        retry_available: bool,
    },
    /// Fatal error that ends the current job.
    FatalError {
        /// Stage in which the fatal error occurred.
        phase: &'static str,
        /// Human-readable fatal error details.
        message: String,
        /// Whether PTY mode was disabled as part of this failure.
        disable_pty: bool,
    },
    /// Terminal success event containing final output summary.
    Finished {
        /// Final output lines prepared for UI rendering.
        lines: Vec<String>,
        /// High-level completion status string.
        status: String,
        /// Telemetry snapshot for this completed job.
        stats: CodexJobStats,
    },
    /// Terminal cancellation event.
    Canceled {
        /// Whether PTY mode is disabled after cancellation handling.
        disable_pty: bool,
    },
}

/// Errors surfaced synchronously when a backend cannot start a job.
#[derive(Debug)]
pub enum CodexBackendError {
    /// Request shape/contents were invalid before execution.
    InvalidRequest(&'static str),
    /// Backend is unavailable/disabled at runtime.
    BackendDisabled(String),
}

/// Runtime implementation of the Codex backend interface.
pub trait CodexJobRunner: Send + Sync {
    /// Start a new asynchronous Codex job.
    fn start(&self, request: CodexRequest) -> Result<CodexJob, CodexBackendError>;
    /// Request cancellation for a running job id.
    fn cancel(&self, job_id: JobId);
    /// Working directory used for backend command execution.
    fn working_dir(&self) -> &Path;
}

/// Handle to an asynchronous Codex invocation routed through the backend.
pub struct CodexJob {
    /// Stable job id used to correlate streamed events.
    pub id: JobId,
    events: Arc<BoundedEventQueue>,
    signal_rx: Receiver<()>,
    handle: Option<JoinHandle<()>>,
    cancel_token: CancelToken,
}

impl CodexJob {
    pub(super) fn new(
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
    /// `CodexEventKind::Canceled` terminal event.
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Poll the signal channel without blocking to determine whether new events exist.
    pub fn try_recv_signal(&self) -> Result<(), TryRecvError> {
        self.signal_rx.try_recv().map(|_| ())
    }

    /// Drain any queued backend events.
    pub fn drain_events(&self) -> Vec<CodexEvent> {
        self.events.drain()
    }

    /// Take ownership of the worker handle so the caller can join it once the job finishes.
    pub fn take_handle(&mut self) -> Option<JoinHandle<()>> {
        self.handle.take()
    }
}

#[cfg(test)]
impl CodexJob {
    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) enum TestSignal {
    Ready,
    Disconnected,
    Empty,
}

#[cfg(test)]
pub(crate) fn build_test_backend_job(events: Vec<CodexEvent>, signal: TestSignal) -> CodexJob {
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
    CodexJob::new(0, queue, rx, handle, CancelToken::new())
}

pub(super) const BACKEND_EVENT_CAPACITY: usize = 1024;

/// Backing store for all events emitted by a job. Ensures bounded capacity with drop-oldest semantics.
pub(super) struct BoundedEventQueue {
    capacity: usize,
    inner: Mutex<VecDeque<CodexEvent>>,
}

impl BoundedEventQueue {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            inner: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    pub(super) fn push(&self, event: CodexEvent) -> Result<(), BackendQueueError> {
        let mut queue = lock_or_recover(&self.inner, "codex::BoundedEventQueue::push");
        if queue.len() >= self.capacity && !Self::drop_non_terminal(&mut queue) {
            return Err(BackendQueueError);
        }
        queue.push_back(event);
        Ok(())
    }

    pub(super) fn drain(&self) -> Vec<CodexEvent> {
        let mut queue = lock_or_recover(&self.inner, "codex::BoundedEventQueue::drain");
        queue.drain(..).collect()
    }

    fn drop_non_terminal(queue: &mut VecDeque<CodexEvent>) -> bool {
        if let Some(idx) = queue
            .iter()
            .position(|event| matches!(event.kind, CodexEventKind::Token { .. }))
        {
            queue.remove(idx);
            return true;
        }
        if let Some(idx) = queue
            .iter()
            .position(|event| matches!(event.kind, CodexEventKind::Status { .. }))
        {
            queue.remove(idx);
            return true;
        }
        if let Some(idx) = queue.iter().position(|event| {
            matches!(
                event.kind,
                CodexEventKind::RecoverableError { .. } | CodexEventKind::Started { .. }
            )
        }) {
            queue.remove(idx);
            return true;
        }
        false
    }
}

#[derive(Debug)]
pub(super) struct BackendQueueError;

/// Sender that writes backend events into the bounded queue and notifies the UI.
pub(super) struct EventSender {
    queue: Arc<BoundedEventQueue>,
    signal_tx: mpsc::Sender<()>,
}

impl EventSender {
    pub(super) fn new(queue: Arc<BoundedEventQueue>, signal_tx: mpsc::Sender<()>) -> Self {
        Self { queue, signal_tx }
    }

    pub(super) fn emit(&self, event: CodexEvent) -> Result<(), BackendQueueError> {
        self.queue.push(event)?;
        let _ = self.signal_tx.send(());
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    pub(super) fn new() -> Self {
        Self {
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(super) fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
pub(super) enum CodexCallError {
    Cancelled,
    Failure(Error),
}

impl From<Error> for CodexCallError {
    fn from(err: Error) -> Self {
        Self::Failure(err)
    }
}

impl From<std::io::Error> for CodexCallError {
    fn from(err: std::io::Error) -> Self {
        Self::Failure(err.into())
    }
}
