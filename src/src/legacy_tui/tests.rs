//! Regression tests for legacy TUI state transitions and rendering behavior.

#[cfg(test)]
use super::set_logging_for_tests;
use super::state::{CodexApp, OUTPUT_MAX_LINES};
use super::{init_logging, log_debug, log_debug_content};
use crate::codex::{self, CodexEvent, CodexEventKind, CodexJobStats};
use crate::config::AppConfig;
use crate::voice;
use crate::{audio, log_file_path};
use clap::Parser;
use std::env;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

static LOG_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_logging_enabled(action: impl FnOnce()) {
    let _guard = LOG_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("log test lock");
    let log_path = log_file_path();
    let _ = std::fs::remove_file(&log_path);
    set_logging_for_tests(true, false);
    action();
    set_logging_for_tests(false, false);
}

fn with_log_lock(action: impl FnOnce()) {
    let _guard = LOG_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("log test lock");
    action();
}

fn clear_log_env() {
    env::remove_var("VOICETERM_LOGS");
    env::remove_var("VOICETERM_NO_LOGS");
    env::remove_var("VOICETERM_LOG_CONTENT");
}

fn test_config() -> AppConfig {
    let mut config = AppConfig::parse_from(["voiceterm-tests"]);
    config.persistent_codex = false; // Disable PTY in tests
    config
}

#[test]
fn append_output_trims_history() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    let lines = (0..600).map(|i| format!("line {i}")).collect();
    app.append_output(lines);
    assert!(app.output_lines().len() <= OUTPUT_MAX_LINES);
}

#[test]
fn scroll_helpers_update_offset() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    app.append_output(vec!["one".into(), "two".into(), "three".into()]);
    app.page_down();
    assert_eq!(app.get_scroll_offset(), 10);
    app.scroll_to_top();
    assert_eq!(app.get_scroll_offset(), 0);
}

#[test]
fn codex_job_completion_updates_ui() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    app.input = "test prompt".into();
    let (_result, hook_guard) = codex::with_job_hook(
        Box::new(|prompt, _| {
            vec![CodexEventKind::Finished {
                lines: vec![format!("> {prompt}"), String::from("result line")],
                status: "ok".into(),
                stats: test_stats(),
            }]
        }),
        || {
            app.send_current_input().unwrap();
            wait_for_codex_job(&mut app);
        },
    );
    drop(hook_guard);
    let lines = app.output_lines();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], "> test prompt");
    assert_eq!(lines[1], "result line");
    assert_eq!(app.status_text(), "ok");
    assert!(app.sanitized_input_text().is_empty());
}

#[test]
fn codex_job_cancellation_updates_status() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    app.input = "test prompt".into();
    let (_result, hook_guard) = codex::with_job_hook(
        Box::new(|_, cancel| {
            let start = Instant::now();
            while !cancel.is_cancelled() {
                if start.elapsed() > Duration::from_millis(200) {
                    return vec![CodexEventKind::FatalError {
                        phase: "cancel",
                        message: "cancel did not propagate".into(),
                        disable_pty: false,
                    }];
                }
                thread::sleep(Duration::from_millis(10));
            }
            vec![CodexEventKind::Canceled { disable_pty: false }]
        }),
        || {
            app.send_current_input().unwrap();
            assert!(app.cancel_codex_job_if_active());
            wait_for_codex_job(&mut app);
        },
    );
    drop(hook_guard);
    assert_eq!(app.status_text(), "Codex request canceled.");
    assert!(!app.cancel_codex_job_if_active());
}

#[test]
fn input_and_scroll_changes_request_redraw() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    assert!(app.take_redraw_request()); // clear initial draw
    assert!(!app.take_redraw_request());
    app.push_input_char('a');
    assert!(app.take_redraw_request());
    assert!(!app.take_redraw_request());
    app.scroll_down();
    assert!(app.take_redraw_request());
}

#[test]
fn fatal_event_updates_status() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    app.handle_backend_event(CodexEvent {
        job_id: 1,
        kind: CodexEventKind::FatalError {
            phase: "cli",
            message: "boom".into(),
            disable_pty: true,
        },
    });
    assert_eq!(app.status_text(), "Codex failed: boom");
}

#[test]
fn perf_smoke_emits_voice_metrics() {
    with_logging_enabled(|| {
        let log_path = log_file_path();
        let metrics = audio::CaptureMetrics {
            capture_ms: 800,
            transcribe_ms: 0,
            speech_ms: 600,
            silence_tail_ms: 200,
            frames_processed: 5,
            frames_dropped: 0,
            early_stop_reason: audio::StopReason::VadSilence { tail_ms: 200 },
        };
        voice::log_voice_metrics(&metrics);
        let contents =
            std::fs::read_to_string(&log_path).expect("perf smoke log file should exist");
        assert!(
            contents.contains("voice_metrics|"),
            "voice metrics log not found"
        );
    });
}

#[test]
fn logging_disabled_by_default() {
    with_log_lock(|| {
        clear_log_env();
        let log_path = log_file_path();
        let _ = std::fs::remove_file(&log_path);
        let config = AppConfig::parse_from(["voiceterm-tests"]);
        init_logging(&config);
        log_debug("should-not-write");
        assert!(std::fs::metadata(&log_path).is_err());
    });
}

#[test]
fn logging_enabled_writes_log() {
    with_log_lock(|| {
        clear_log_env();
        let log_path = log_file_path();
        let _ = std::fs::remove_file(&log_path);
        let mut config = AppConfig::parse_from(["voiceterm-tests"]);
        config.logs = true;
        init_logging(&config);
        log_debug("log-enabled");
        let contents = std::fs::read_to_string(&log_path).expect("log file should be created");
        assert!(contents.contains("log-enabled"));
    });
}

#[test]
fn log_content_requires_flag() {
    with_log_lock(|| {
        clear_log_env();
        let log_path = log_file_path();
        let _ = std::fs::remove_file(&log_path);
        let mut config = AppConfig::parse_from(["voiceterm-tests"]);
        config.logs = true;
        config.log_content = false;
        init_logging(&config);
        log_debug_content("secret");
        let contents = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(
            !contents.contains("secret"),
            "content should not be logged without --log-content"
        );
    });
}

#[test]
fn memory_guard_backend_threads_drop() {
    let config = test_config();
    let mut app = CodexApp::new(config);
    app.input = "memory-guard".into();
    let (_result, hook_guard) = codex::with_job_hook(
        Box::new(|prompt, _| {
            vec![CodexEventKind::Finished {
                lines: vec![format!("> {prompt}"), String::from("result line")],
                status: "ok".into(),
                stats: test_stats(),
            }]
        }),
        || {
            app.send_current_input().unwrap();
            wait_for_codex_job(&mut app);
        },
    );
    drop(hook_guard);
    assert_eq!(codex::active_backend_threads(), 0);
}

fn wait_for_codex_job(app: &mut CodexApp) {
    for _ in 0..50 {
        app.poll_codex_job().unwrap();
        if app.codex_job.is_none() {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("Codex job did not complete in time");
}

fn test_stats() -> CodexJobStats {
    CodexJobStats {
        backend_type: "cli",
        started_at: Instant::now(),
        first_token_at: None,
        finished_at: Instant::now(),
        tokens_received: 0,
        bytes_transferred: 0,
        pty_attempts: 0,
        cli_fallback_used: false,
        disable_pty: false,
    }
}
