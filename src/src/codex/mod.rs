//! Codex backend interfaces and async worker used by UI and IPC runtimes.
//! exposes a `CodexJobRunner` trait that emits structured events so the UI can
//! render streaming output, show "thinking..." indicators, and react to
//! recoverable vs fatal failures without duplicating business logic.

mod backend;
mod cli;
mod pty_backend;
#[cfg(test)]
mod tests;

/// Spinner frames used by the UI when a Codex request is inflight.
/// Uses Braille pattern characters for a smooth, modern animation.
pub const CODEX_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub use backend::{
    CodexBackendError, CodexEvent, CodexEventKind, CodexJob, CodexJobRunner, CodexJobStats,
    CodexRequest, JobId, RequestMode, RequestPayload,
};

pub use pty_backend::{prepare_for_display, sanitize_pty_output, CodexCliBackend};

#[cfg(test)]
pub(crate) use backend::{build_test_backend_job, TestSignal};
#[cfg(test)]
pub(crate) use pty_backend::{
    active_backend_threads, reset_session_count, reset_session_count_reset, with_job_hook,
};
