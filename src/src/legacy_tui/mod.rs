//! Legacy Codex-only TUI shell retained for compatibility and focused testing.
//!
//! This mirrors the Python prototype but wraps it in a full-screen ratatui
//! experience for the legacy Codex CLI path.

mod logging;
mod state;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use logging::set_logging_for_tests;
pub use logging::{
    crash_log_path, init_logging, log_debug, log_debug_content, log_file_path, log_panic,
};
pub use state::CodexApp;
#[allow(unused_imports)]
pub(crate) use state::{run_python_transcription, PipelineJsonResult, PipelineMetrics};
