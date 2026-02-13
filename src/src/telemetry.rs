//! Optional local telemetry logging used for debugging and performance triage.

use crate::config::AppConfig;
use std::env;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing_subscriber::fmt::time::UtcTime;

static TRACING_INIT: OnceLock<()> = OnceLock::new();

pub(crate) fn tracing_log_path() -> PathBuf {
    env::var("VOXTERM_TRACE_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::temp_dir().join("voxterm_trace.jsonl"))
}

pub(crate) fn init_tracing(config: &AppConfig) {
    let enabled = (config.logs || config.log_timings) && !config.no_logs;
    if !enabled {
        return;
    }

    let _ = TRACING_INIT.get_or_init(|| {
        let path = tracing_log_path();
        let file = match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => file,
            Err(_) => return,
        };
        let subscriber = tracing_subscriber::fmt()
            .json()
            .with_timer(UtcTime::rfc_3339())
            .with_writer(file)
            .with_current_span(false)
            .with_span_list(false)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}
