//! PTY-backed Codex runner so interactive sessions can stream structured events.

use super::backend::{
    BoundedEventQueue, CancelToken, CodexBackendError, CodexCallError, CodexEvent, CodexEventKind,
    CodexJob, CodexJobRunner, CodexJobStats, CodexRequest, EventSender, JobId, RequestMode,
    RequestPayload, BACKEND_EVENT_CAPACITY,
};
use super::cli::call_codex_cli;
use crate::{config::AppConfig, lock_or_recover, log_debug, pty_session::PtyCliSession};
use anyhow::{anyhow, Context, Result};
#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use strip_ansi_escapes::strip;

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
const PTY_MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

/// Default CLI/PTY backend implementation driving the `codex` binary.
pub struct CodexCliBackend {
    config: AppConfig,
    working_dir: PathBuf,
    next_job_id: AtomicU64,
    pub(super) state: Arc<Mutex<CodexCliBackendState>>,
    pub(super) cancel_tokens: Arc<Mutex<HashMap<JobId, CancelToken>>>,
}

pub(super) struct CodexCliBackendState {
    pub(super) codex_session: Option<PtyCliSession>,
    pub(super) pty_disabled: bool,
}

#[cfg(test)]
thread_local! {
    static RESET_SESSION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_session_count() -> usize {
    RESET_SESSION_COUNT.with(|count| count.get())
}

#[cfg(test)]
pub(crate) fn reset_session_count_reset() {
    RESET_SESSION_COUNT.with(|count| count.set(0));
}

impl CodexCliBackend {
    /// Build a backend instance using the provided validated app config.
    pub fn new(config: AppConfig) -> Self {
        let working_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let state = CodexCliBackendState {
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

    pub(super) fn take_codex_session_for_job(&self) -> Option<PtyCliSession> {
        if !self.config.persistent_codex {
            return None;
        }
        let mut state = lock_or_recover(&self.state, "CodexCliBackend::take_codex_session_for_job");
        if state.pty_disabled {
            return None;
        }
        if state.codex_session.is_none() {
            if let Err(err) = self.ensure_codex_session(&mut state) {
                log_debug(&format!(
                    "CodexCliBackend: persistent Codex unavailable: {err:#}"
                ));
                state.pty_disabled = true;
                return None;
            }
        }
        state.codex_session.take()
    }

    pub(super) fn ensure_codex_session(&self, state: &mut CodexCliBackendState) -> Result<()> {
        let working_dir = self.working_dir.clone();
        let wd_str = working_dir.to_str().unwrap_or(".");
        log_debug(&format!(
            "Attempting to create PTY session with codex_cmd={}, working_dir={}",
            self.config.codex_cmd, wd_str
        ));

        // Build args with -C flag for working directory
        let mut pty_args = vec!["-C".to_string(), wd_str.to_string()];
        pty_args.extend(self.config.codex_args.clone());

        match PtyCliSession::new(
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
                    log_debug("CodexCliBackend: persistent PTY session ready and responsive");
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

impl CodexJobRunner for CodexCliBackend {
    fn start(&self, request: CodexRequest) -> Result<CodexJob, CodexBackendError> {
        let mode = match &request.payload {
            RequestPayload::Chat { prompt } => {
                if prompt.trim().is_empty() {
                    return Err(CodexBackendError::InvalidRequest("Prompt is empty"));
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
            let mut registry = lock_or_recover(&cancel_registry, "CodexCliBackend::start");
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
            CodexCliBackend::cleanup_job(cancel_registry, job_id);
            CodexCliBackend::restore_static_state(
                state,
                outcome.codex_session,
                outcome.disable_pty,
            );
        });

        Ok(CodexJob::new(
            job_id,
            queue,
            signal_rx,
            handle,
            cancel_token,
        ))
    }

    fn cancel(&self, job_id: JobId) {
        let maybe_token = {
            let registry = lock_or_recover(&self.cancel_tokens, "CodexCliBackend::cancel");
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

impl CodexCliBackend {
    pub(super) fn cleanup_job(registry: Arc<Mutex<HashMap<JobId, CancelToken>>>, job_id: JobId) {
        let mut registry = lock_or_recover(&registry, "CodexCliBackend::cleanup_job");
        registry.remove(&job_id);
    }

    /// Drop the cached PTY session so the next request can re-establish state.
    pub fn reset_session(&self) {
        #[cfg(test)]
        RESET_SESSION_COUNT.with(|count| count.set(count.get().saturating_add(1)));
        let mut state = lock_or_recover(&self.state, "CodexCliBackend::reset_session");
        state.codex_session = None;
    }

    pub(super) fn restore_static_state(
        state: Arc<Mutex<CodexCliBackendState>>,
        session: Option<PtyCliSession>,
        disable_pty: bool,
    ) {
        let mut state = lock_or_recover(&state, "CodexCliBackend::restore_static_state");
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

struct CodexRunOutcome {
    codex_session: Option<PtyCliSession>,
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
    codex_session: Option<PtyCliSession>,
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
        .emit(CodexEvent {
            job_id,
            kind: CodexEventKind::Started { mode },
        })
        .is_err()
    {
        log_debug("CodexJobRunner: failed to emit Started event (queue overflow)");
        return outcome;
    }

    let prompt = match request.payload {
        RequestPayload::Chat { prompt } => prompt,
    };

    #[cfg(test)]
    if let Some(events) = try_job_hook(&prompt, &cancel) {
        for kind in events {
            let _ = sender.emit(CodexEvent { job_id, kind });
        }
        return outcome;
    }

    if prompt.trim().is_empty() {
        let _ = sender.emit(CodexEvent {
            job_id,
            kind: CodexEventKind::FatalError {
                phase: "input_validation",
                message: "Prompt is empty.".into(),
                disable_pty: false,
            },
        });
        return outcome;
    }

    let started_at = Instant::now();
    let mut stats = CodexJobStats::new(started_at);
    let mut codex_output: Option<String> = None;

    if config.persistent_codex {
        if let Some(mut session) = outcome.codex_session.take() {
            stats.pty_attempts = 1;
            log_debug("CodexJobRunner: attempting persistent Codex session");
            match call_codex_via_session(&mut session, &prompt, &cancel) {
                Ok(text) => {
                    codex_output = Some(text);
                    outcome.codex_session = Some(session);
                }
                Err(CodexCallError::Cancelled) => {
                    let _ = sender.emit(CodexEvent {
                        job_id,
                        kind: CodexEventKind::Canceled { disable_pty: false },
                    });
                    outcome.codex_session = Some(session);
                    return outcome;
                }
                Err(err) => {
                    outcome.disable_pty = true;
                    let _ = sender.emit(CodexEvent {
                        job_id,
                        kind: CodexEventKind::RecoverableError {
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
        let _ = sender.emit(CodexEvent {
            job_id,
            kind: CodexEventKind::Canceled {
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
                let _ = sender.emit(CodexEvent {
                    job_id,
                    kind: CodexEventKind::Canceled {
                        disable_pty: outcome.disable_pty,
                    },
                });
                return outcome;
            }
            Err(CodexCallError::Failure(err)) => {
                let _ = sender.emit(CodexEvent {
                    job_id,
                    kind: CodexEventKind::FatalError {
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
    let _ = sender.emit(CodexEvent {
        job_id,
        kind: CodexEventKind::Finished {
            lines,
            status,
            stats,
        },
    });
    outcome
}

pub(super) fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

pub(super) fn compute_deadline(start: Instant, timeout: Duration) -> Instant {
    start + timeout
}

pub(super) fn should_accept_printable(
    has_printable: bool,
    idle_since_raw: Duration,
    quiet_grace: Duration,
) -> bool {
    has_printable && idle_since_raw >= quiet_grace
}

pub(super) fn should_fail_control_only(
    has_printable: bool,
    idle_since_printable: Duration,
    control_only_timeout: Duration,
) -> bool {
    !has_printable && idle_since_printable >= control_only_timeout
}

pub(super) fn should_break_overall(elapsed: Duration, overall_timeout: Duration) -> bool {
    elapsed >= overall_timeout
}

pub(super) fn first_output_timed_out(now: Instant, deadline: Instant) -> bool {
    now >= deadline
}

pub(super) trait CodexSession {
    fn send(&mut self, text: &str) -> Result<()>;
    fn read_output_timeout(&self, timeout: Duration) -> Vec<Vec<u8>>;
}

impl CodexSession for PtyCliSession {
    fn send(&mut self, text: &str) -> Result<()> {
        PtyCliSession::send(self, text)
    }

    fn read_output_timeout(&self, timeout: Duration) -> Vec<Vec<u8>> {
        PtyCliSession::read_output_timeout(self, timeout)
    }
}

pub(super) fn call_codex_via_session<S: CodexSession>(
    session: &mut S,
    prompt: &str,
    cancel: &CancelToken,
) -> Result<String, CodexCallError> {
    session
        .send(prompt)
        .context("failed to write prompt to persistent Codex session")?;

    let mut combined_raw = Vec::new();
    let mut truncated_output = false;
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
            if combined_raw.len() > PTY_MAX_OUTPUT_BYTES {
                let excess = combined_raw.len() - PTY_MAX_OUTPUT_BYTES;
                combined_raw = combined_raw.split_off(excess);
                if !truncated_output {
                    log_debug("Persistent Codex session output exceeded cap; truncating");
                    truncated_output = true;
                }
            }
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

/// Split normalized text into UI-ready lines for terminal rendering.
pub fn prepare_for_display(text: &str) -> Vec<String> {
    text.lines().map(|line| line.to_string()).collect()
}

pub(super) fn normalize_control_bytes(raw: &[u8]) -> Vec<u8> {
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
                if raw.get(idx + 1) == Some(&b'\n') {
                    output.push(b'\n');
                    idx += 2;
                    line_start = output.len();
                    continue;
                }
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

pub(super) fn init_guard(len: usize) -> usize {
    len.saturating_mul(4).max(16)
}

pub(super) fn step_guard(guard: &mut usize) -> bool {
    if *guard == 0 {
        return false;
    }
    *guard -= 1;
    true
}

pub(super) fn pop_last_codepoint(buf: &mut Vec<u8>) -> bool {
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

pub(super) fn current_line_start(buf: &[u8]) -> usize {
    buf.iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

pub(super) fn clamp_line_start(line_start: usize, buf: &[u8]) -> usize {
    if line_start > buf.len() {
        current_line_start(buf)
    } else {
        line_start
    }
}

pub(super) fn skip_osc_sequence(bytes: &[u8], mut cursor: usize) -> usize {
    while cursor < bytes.len() {
        match bytes[cursor] {
            0x07 => return cursor + 1,
            0x1B if cursor + 1 < bytes.len() && bytes[cursor + 1] == b'\\' => {
                return cursor + 2;
            }
            _ => {}
        }
        cursor += 1;
    }
    cursor
}

pub(super) fn find_csi_sequence(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    if bytes.get(start)? != &0x1B || bytes.get(start + 1)? != &b'[' {
        return None;
    }
    for (idx, b) in bytes.iter().enumerate().skip(start + 2) {
        if (0x40..=0x7E).contains(b) {
            return Some((idx, *b));
        }
    }
    None
}

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
type CodexJobHook = Box<dyn Fn(&str, CancelProbe) -> Vec<CodexEventKind> + Send + Sync + 'static>;

#[cfg(test)]
use std::sync::{MutexGuard, OnceLock};

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
fn try_job_hook(prompt: &str, cancel: &CancelToken) -> Option<Vec<CodexEventKind>> {
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
