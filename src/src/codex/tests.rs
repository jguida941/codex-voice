//! Regression tests for Codex backend execution, cancellation, and event delivery.

use super::backend::{
    BoundedEventQueue, CancelToken, CodexCallError, CodexEvent, CodexEventKind, CodexJob,
    EventSender,
};
use super::cli::{
    call_codex_cli, reset_send_signal_failures, send_signal, send_signal_failures,
    should_send_sigkill, write_prompt_with_newline, Signal,
};
use super::pty_backend::{
    call_codex_via_session, clamp_line_start, compute_deadline, current_line_start, duration_ms,
    find_csi_sequence, first_output_timed_out, init_guard, normalize_control_bytes,
    pop_last_codepoint, should_accept_printable, should_break_overall, should_fail_control_only,
    skip_osc_sequence, step_guard, CodexCliBackendState, CodexSession,
};
use super::{
    prepare_for_display, sanitize_pty_output, CodexCliBackend, CodexJobRunner, CodexRequest,
};
use clap::Parser;
use std::{
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::Command;

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
        .push(CodexEvent {
            job_id: 1,
            kind: CodexEventKind::Token {
                text: "token".into(),
            },
        })
        .unwrap();
    queue
        .push(CodexEvent {
            job_id: 1,
            kind: CodexEventKind::Status {
                message: "status".into(),
            },
        })
        .unwrap();
    // Force overflow; token event should be dropped first.
    queue
        .push(CodexEvent {
            job_id: 1,
            kind: CodexEventKind::RecoverableError {
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
        .any(|e| matches!(e.kind, CodexEventKind::Status { .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e.kind, CodexEventKind::RecoverableError { .. })));
}

#[test]
fn event_sender_notifies_listener() {
    let queue = Arc::new(BoundedEventQueue::new(8));
    let (tx, rx) = mpsc::channel();
    let sender = EventSender::new(Arc::clone(&queue), tx);
    sender
        .emit(CodexEvent {
            job_id: 7,
            kind: CodexEventKind::Status {
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
fn normalize_control_bytes_preserves_crlf_lines() {
    let cleaned = normalize_control_bytes(b"hello\r\nworld\r\n");
    assert_eq!(String::from_utf8_lossy(&cleaned), "hello\nworld\n");
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
    assert!(should_fail_control_only(
        false,
        control_timeout,
        control_timeout
    ));
    assert!(!should_fail_control_only(
        true,
        control_timeout,
        control_timeout
    ));

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
    let mut job = CodexJob::new(1, queue, rx, handle, cancel);
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
    let mut job = CodexJob::new(1, queue, rx, handle, cancel);
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
    let mut job = CodexJob::new(1, queue, rx, handle, cancel);
    assert!(matches!(
        job.try_recv_signal(),
        Err(mpsc::TryRecvError::Empty)
    ));
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
fn new_test_pty_session() -> crate::pty_session::PtyCliSession {
    let args: Vec<String> = Vec::new();
    crate::pty_session::PtyCliSession::new("/bin/cat", ".", &args, "xterm-256color")
        .expect("pty session")
}

#[cfg(unix)]
#[test]
fn call_codex_cli_reports_failure() {
    let script = "#!/bin/sh\ncat >/dev/null\necho \"boom\" 1>&2\nexit 42";
    let path = write_stub_script(script);
    let mut config = crate::config::AppConfig::parse_from(["test"]);
    config.codex_cmd = path.to_string_lossy().into_owned();
    config.codex_args.clear();
    let err = call_codex_cli(&config, "hello", Path::new("."), &CancelToken::new());
    assert!(matches!(err, Err(CodexCallError::Failure(_))));
    let _ = std::fs::remove_file(path);
}

#[cfg(unix)]
#[test]
fn cli_backend_reuses_preloaded_session() {
    let mut config = crate::config::AppConfig::parse_from(["test"]);
    config.persistent_codex = true;
    let backend = CodexCliBackend::new(config);
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
    let mut config = crate::config::AppConfig::parse_from(["test"]);
    config.persistent_codex = true;
    config.codex_cmd = "bad\0cmd".into();
    let backend = CodexCliBackend::new(config);
    let mut state = CodexCliBackendState {
        codex_session: None,
        pty_disabled: false,
    };
    assert!(backend.ensure_codex_session(&mut state).is_err());
    assert!(state.codex_session.is_none());
}

#[test]
fn cli_backend_cancels_and_cleans_registry() {
    let backend = CodexCliBackend::new(crate::config::AppConfig::parse_from(["test"]));
    let token = CancelToken::new();
    {
        let mut registry = backend.cancel_tokens.lock().unwrap();
        registry.insert(7, token.clone());
    }
    backend.cancel(7);
    assert!(token.is_cancelled());
    CodexCliBackend::cleanup_job(Arc::clone(&backend.cancel_tokens), 7);
    let registry = backend.cancel_tokens.lock().unwrap();
    assert!(registry.is_empty());
}

#[cfg(unix)]
#[test]
fn cli_backend_reset_session_clears_cached_session() {
    let mut config = crate::config::AppConfig::parse_from(["test"]);
    config.persistent_codex = true;
    let backend = CodexCliBackend::new(config);
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
    let state = Arc::new(Mutex::new(CodexCliBackendState {
        codex_session: None,
        pty_disabled: false,
    }));
    let session = new_test_pty_session();
    CodexCliBackend::restore_static_state(Arc::clone(&state), Some(session), false);
    let state = state.lock().unwrap();
    assert!(state.codex_session.is_some());
    assert!(!state.pty_disabled);
}

#[cfg(unix)]
#[test]
fn cli_backend_restore_static_state_disables_pty() {
    let state = Arc::new(Mutex::new(CodexCliBackendState {
        codex_session: None,
        pty_disabled: false,
    }));
    CodexCliBackend::restore_static_state(Arc::clone(&state), None, true);
    let state = state.lock().unwrap();
    assert!(state.pty_disabled);
    assert!(state.codex_session.is_none());
}

#[test]
fn backend_job_ids_increment() {
    let backend = CodexCliBackend::new(crate::config::AppConfig::parse_from(["test"]));
    let (mut jobs, _guard) = super::with_job_hook(Box::new(|_, _| Vec::new()), || {
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
    fn send(&mut self, _text: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn read_output_timeout(&self, _timeout: Duration) -> Vec<Vec<u8>> {
        self.outputs.borrow_mut().pop_front().unwrap_or_default()
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
    <crate::pty_session::PtyCliSession as CodexSession>::send(&mut session, "ping\n").unwrap();
    let _ = <crate::pty_session::PtyCliSession as CodexSession>::read_output_timeout(
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
        let err = std::io::Error::last_os_error();
        if res != 0 && err.kind() == std::io::ErrorKind::NotFound {
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
