use super::counters::*;
use super::io::*;
use super::osc::*;
use super::pty::*;
use crate::set_logging_for_tests;
use crossbeam_channel::{bounded, RecvTimeoutError};
use std::fs;
use std::io::{self, ErrorKind};
use std::mem;
use std::mem::ManuallyDrop;
use std::os::unix::io::RawFd;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

fn pipe_pair() -> (RawFd, RawFd) {
    let mut fds = [0; 2];
    let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
    assert_eq!(
        result,
        0,
        "pipe() failed with errno {}",
        io::Error::last_os_error()
    );
    (fds[0], fds[1])
}

fn close_fd_pair(read_fd: RawFd, write_fd: RawFd) {
    unsafe {
        libc::close(read_fd);
        libc::close(write_fd);
    }
}

fn read_all(fd: RawFd) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 64];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n <= 0 {
            break;
        }
        out.extend_from_slice(&buf[..n as usize]);
    }
    out
}

fn set_nonblocking_fd(fd: RawFd) {
    unsafe { set_nonblocking(fd).unwrap() };
}

fn open_pty_pair() -> (RawFd, RawFd) {
    let mut master = -1;
    let mut slave = -1;
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut ws,
        )
    };
    assert_eq!(
        result,
        0,
        "openpty() failed with errno {}",
        io::Error::last_os_error()
    );
    (master, slave)
}

#[cfg(unix)]
fn spawn_zombie_child() -> i32 {
    unsafe {
        let pid = libc::fork();
        assert!(pid >= 0, "fork failed");
        if pid == 0 {
            libc::_exit(0);
        }
        thread::sleep(Duration::from_millis(10));
        pid
    }
}

fn log_lock() -> &'static Mutex<()> {
    static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOG_LOCK.get_or_init(|| Mutex::new(()))
}

fn read_output_timeout_lock() -> &'static Mutex<()> {
    static READ_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    READ_LOCK.get_or_init(|| Mutex::new(()))
}

fn terminal_size_override_lock() -> &'static Mutex<()> {
    static SIZE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    SIZE_LOCK.get_or_init(|| Mutex::new(()))
}

fn wait_for_exit_lock() -> &'static Mutex<()> {
    static EXIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    EXIT_LOCK.get_or_init(|| Mutex::new(()))
}

fn capture_new_log<F: FnOnce()>(f: F) -> String {
    let _guard = log_lock().lock().unwrap();
    let log_path = crate::log_file_path();
    let _ = fs::remove_file(&log_path);
    set_logging_for_tests(true, false);
    let before = fs::read(&log_path).unwrap_or_default();
    f();
    let after = fs::read(&log_path).unwrap_or_default();
    set_logging_for_tests(false, false);
    let new_bytes = after.get(before.len()..).unwrap_or(&[]);
    String::from_utf8_lossy(new_bytes).to_string()
}

#[test]
fn guard_loop_enforces_iteration_limit() {
    let start = Instant::now();
    let ok = std::panic::catch_unwind(|| guard_loop(start, 10, 10, "guard"));
    assert!(ok.is_ok());
    let err = std::panic::catch_unwind(|| guard_loop(start, 11, 10, "guard"));
    assert!(err.is_err());
}

#[test]
fn guard_loop_enforces_elapsed_limit() {
    let start = Instant::now() - Duration::from_secs(3);
    let err = std::panic::catch_unwind(|| guard_loop(start, 0, 10, "guard"));
    assert!(err.is_err());
}

#[test]
fn guard_elapsed_exceeded_allows_exact_limit() {
    assert!(!guard_elapsed_exceeded(Duration::from_secs(2), 0, 10));
    assert!(guard_elapsed_exceeded(Duration::from_secs(3), 0, 10));
}

#[test]
fn respond_to_terminal_queries_handles_multiple_sequences() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"start\x1b[6nmid\x1b[cend".to_vec();

    respond_to_terminal_queries(&mut buffer, write_fd);

    assert_eq!(buffer, b"startmidend".to_vec());
    let reply = read_reply(read_fd);
    assert!(
        reply_contains(&reply, b"\x1b[24;80R"),
        "reply missing cursor report: {reply:?}"
    );
    assert!(
        reply_contains(&reply, b"\x1b[?1;2c"),
        "reply missing device attributes: {reply:?}"
    );
    close_fd_pair(read_fd, write_fd);
}

fn read_reply(fd: RawFd) -> Vec<u8> {
    set_nonblocking_fd(fd);
    let start = Instant::now();
    let mut buf = [0u8; 64];
    loop {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if n > 0 {
            return buf[..n as usize].to_vec();
        }
        if n == 0 {
            return Vec::new();
        }
        let err = io::Error::last_os_error();
        if err.kind() == ErrorKind::Interrupted {
            continue;
        }
        if err.kind() == ErrorKind::WouldBlock {
            if start.elapsed() > Duration::from_millis(200) {
                return Vec::new();
            }
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        return Vec::new();
    }
}

#[test]
fn respond_to_terminal_queries_replies_to_cursor_request() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"Hello\x1b[6nWorld".to_vec();

    respond_to_terminal_queries(&mut buffer, write_fd);

    assert_eq!(buffer, b"HelloWorld".to_vec());
    let reply = read_reply(read_fd);
    assert!(reply_contains(&reply, b"\x1b[24;80R"));
    close_fd_pair(read_fd, write_fd);
}

#[test]
fn respond_to_terminal_queries_replies_to_status_request() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"\x1b[5nOK".to_vec();

    respond_to_terminal_queries(&mut buffer, write_fd);

    assert_eq!(buffer, b"OK".to_vec());
    let reply = read_reply(read_fd);
    assert_eq!(reply, b"\x1b[0n");
    close_fd_pair(read_fd, write_fd);
}

#[test]
fn respond_to_terminal_queries_replies_to_extended_device_attr() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"\x1b[?1;0c".to_vec();

    respond_to_terminal_queries(&mut buffer, write_fd);

    assert!(buffer.is_empty());
    let reply = read_reply(read_fd);
    assert_eq!(reply, b"\x1b[?1;2c");
    close_fd_pair(read_fd, write_fd);
}

#[test]
fn respond_to_terminal_queries_passthrough_keeps_ansi() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"\x1b[31mred\x1b[0m\x1b[6n".to_vec();

    respond_to_terminal_queries_passthrough(&mut buffer, write_fd);

    assert_eq!(buffer, b"\x1b[31mred\x1b[0m".to_vec());
    let reply = read_reply(read_fd);
    assert!(reply_contains(&reply, b"\x1b[24;80R"));
    close_fd_pair(read_fd, write_fd);
}

#[test]
fn apply_control_edits_handles_cr_and_backspace() {
    let mut data = b"foo\rbar\x08z\nnext\x08!".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"baz\nnex!");
}

#[test]
fn apply_control_edits_preserves_crlf_lines() {
    let mut data = b"hello\r\nworld\r\n".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"hello\nworld\n");
}

#[test]
fn split_incomplete_escape_buffers_trailing_csi() {
    let mut data = b"hello\x1b[<64;119;".to_vec();
    let pending = split_incomplete_escape(&mut data).expect("expected pending escape");
    assert_eq!(data, b"hello");
    assert_eq!(pending, b"\x1b[<64;119;");
    let mut merged = pending;
    merged.extend_from_slice(b"29Mtail");
    assert!(split_incomplete_escape(&mut merged).is_none());
}

#[test]
fn write_all_writes_bytes() {
    let (read_fd, write_fd) = pipe_pair();
    write_all(write_fd, b"hello").unwrap();
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"hello");
    unsafe { libc::close(read_fd) };
}

#[test]
fn try_write_writes_bytes() {
    let (read_fd, write_fd) = pipe_pair();
    let written = try_write(write_fd, b"hello").unwrap();
    assert_eq!(written, 5);
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"hello");
    unsafe { libc::close(read_fd) };
}

#[test]
fn try_write_handles_short_writes() {
    struct LimitReset;
    impl Drop for LimitReset {
        fn drop(&mut self) {
            set_write_all_limit(None);
        }
    }

    set_write_all_limit(Some(2));
    let _reset = LimitReset;

    let (read_fd, write_fd) = pipe_pair();
    let written = try_write(write_fd, b"hello").unwrap();
    assert_eq!(written, 2);
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"he");
    unsafe { libc::close(read_fd) };
}

#[test]
fn write_all_handles_short_writes() {
    struct LimitReset;
    impl Drop for LimitReset {
        fn drop(&mut self) {
            set_write_all_limit(None);
        }
    }

    set_write_all_limit(Some(1));
    let _reset = LimitReset;

    let (read_fd, write_fd) = pipe_pair();
    write_all(write_fd, b"hello").unwrap();
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"hello");
    unsafe { libc::close(read_fd) };
}

#[cfg(unix)]
#[test]
fn try_write_reports_wouldblock() {
    let (read_fd, write_fd) = pipe_pair();
    set_nonblocking_fd(write_fd);

    let fill = [0u8; 1024];
    loop {
        let written =
            unsafe { libc::write(write_fd, fill.as_ptr() as *const libc::c_void, fill.len()) };
        if written < 0 {
            let err = io::Error::last_os_error();
            assert_eq!(err.kind(), ErrorKind::WouldBlock);
            break;
        }
    }

    let err = try_write(write_fd, b"ping").unwrap_err();
    assert_eq!(err.kind(), ErrorKind::WouldBlock);
    close_fd_pair(read_fd, write_fd);
}

#[test]
fn write_all_reports_zero_write() {
    struct LimitReset;
    impl Drop for LimitReset {
        fn drop(&mut self) {
            set_write_all_limit(None);
        }
    }

    set_write_all_limit(Some(0));
    let _reset = LimitReset;

    let (read_fd, write_fd) = pipe_pair();
    let err = write_all(write_fd, b"hi").unwrap_err();
    unsafe {
        libc::close(write_fd);
        libc::close(read_fd);
    }
    assert!(err.to_string().contains("returned 0"));
}

#[cfg(unix)]
#[test]
fn write_all_retries_on_wouldblock() {
    let (read_fd, write_fd) = pipe_pair();
    set_nonblocking_fd(write_fd);

    let fill = [0u8; 1024];
    loop {
        let written =
            unsafe { libc::write(write_fd, fill.as_ptr() as *const libc::c_void, fill.len()) };
        if written < 0 {
            let err = io::Error::last_os_error();
            assert_eq!(err.kind(), ErrorKind::WouldBlock);
            break;
        }
    }

    let read_fd_dup = unsafe { libc::dup(read_fd) };
    assert!(read_fd_dup >= 0);
    let reader = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        let mut buf = [0u8; 4096];
        let _ = unsafe { libc::read(read_fd_dup, buf.as_mut_ptr() as *mut _, buf.len()) };
        unsafe { libc::close(read_fd_dup) };
    });

    write_all(write_fd, b"ping").unwrap();
    unsafe { libc::close(write_fd) };
    reader.join().unwrap();
    let output = read_all(read_fd);
    assert!(reply_contains(&output, b"ping"));
    unsafe { libc::close(read_fd) };
}

#[cfg(unix)]
#[test]
fn wait_for_exit_returns_true_for_exited_child() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(None);
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    assert!(wait_for_exit(pid, Duration::from_secs(1)));
    let _ = child.wait();
    assert_eq!(wait_for_exit_error_count(), 0);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_returns_false_when_timeout() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(None);
    let mut child = std::process::Command::new("sleep")
        .arg("1")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    assert!(!wait_for_exit(pid, Duration::from_millis(10)));
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(wait_for_exit_reap_count(), 0);
    assert_eq!(wait_for_exit_error_count(), 0);
    assert!(wait_for_exit_poll_count() > 0);
}

#[test]
fn find_csi_sequence_finds_final_byte() {
    let bytes = b"1;2R";
    assert_eq!(find_csi_sequence(bytes, 0), Some((3, b'R')));
    assert_eq!(find_csi_sequence(bytes, 4), None);
}

#[test]
fn find_osc_terminator_handles_bel_and_st() {
    let bel = b"0;title\x07rest";
    assert_eq!(find_osc_terminator(bel, 0), Some(b"0;title\x07".len()));
    let st = b"0;title\x1b\\rest";
    assert_eq!(find_osc_terminator(st, 0), Some(b"0;title\x1b\\".len()));
}

#[test]
fn should_strip_without_reply_matches_expected_sequences() {
    assert!(should_strip_without_reply(b"?2004", b'h'));
    assert!(should_strip_without_reply(b">1", b'u'));
    assert!(should_strip_without_reply(b"", b'm'));
    assert!(!should_strip_without_reply(b"6", b'n'));
}

#[test]
fn csi_reply_returns_expected_responses() {
    assert_eq!(csi_reply(b"5", b'n', 24, 80), Some(b"\x1b[0n".to_vec()));
    assert_eq!(csi_reply(b"6", b'n', 2, 3), Some(b"\x1b[2;3R".to_vec()));
    assert_eq!(csi_reply(b"", b'c', 24, 80), Some(b"\x1b[?1;2c".to_vec()));
    assert_eq!(csi_reply(b"1", b'n', 24, 80), None);
}

#[test]
fn current_terminal_size_falls_back_on_invalid_fd() {
    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(None);
    assert_eq!(current_terminal_size(-1), (24, 80));
}

#[test]
fn pty_cli_session_send_appends_newline() {
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyCliSession {
        master_fd: write_fd,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    session.send("hello").unwrap();
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"hello\n");
    unsafe { libc::close(read_fd) };
}

#[test]
fn pty_overlay_session_send_text_with_newline_appends() {
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyOverlaySession {
        master_fd: write_fd,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    session.send_text_with_newline("overlay").unwrap();
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"overlay\n");
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_reader_thread_forwards_output() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, rx) = bounded(2);
    let handle = spawn_reader_thread(read_fd, tx);
    unsafe {
        libc::write(write_fd, b"hello".as_ptr() as *const libc::c_void, 5);
        libc::close(write_fd);
    }
    let data = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(data, b"hello");
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_passthrough_reader_thread_forwards_output() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, rx) = bounded(2);
    let handle = spawn_passthrough_reader_thread(read_fd, tx);
    unsafe {
        libc::write(write_fd, b"hello".as_ptr() as *const libc::c_void, 5);
        libc::close(write_fd);
    }
    let data = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(data, b"hello");
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn should_retry_read_error_reports_expected_kinds() {
    assert!(should_retry_read_error(&io::Error::from(
        ErrorKind::Interrupted
    )));
    assert!(should_retry_read_error(&io::Error::from(
        ErrorKind::WouldBlock
    )));
    assert!(!should_retry_read_error(&io::Error::from(ErrorKind::Other)));
}

#[test]
fn pty_session_counters_track_send_and_read() {
    reset_pty_session_counters();
    assert_eq!(pty_session_send_count(), 0);
    assert_eq!(pty_session_read_count(), 0);

    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyCliSession {
        master_fd: write_fd,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    session.send("ping").unwrap();
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
    assert_eq!(pty_session_send_count(), 1);

    let _guard = read_output_timeout_lock().lock().unwrap();
    let (_tx2, rx2) = bounded(1);
    let handle2 = thread::spawn(|| {});
    let session2 = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx2,
        _output_thread: handle2,
    });
    let _ = session2.read_output_timeout(Duration::from_millis(0));
    assert_eq!(pty_session_read_count(), 1);

    reset_pty_session_counters();
    assert_eq!(pty_session_send_count(), 0);
    assert_eq!(pty_session_read_count(), 0);
}

#[test]
fn current_terminal_size_uses_override_values() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_terminal_size_override(None);
        }
    }

    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(Some((true, 10, 20)));
    let _reset = OverrideReset;
    assert_eq!(current_terminal_size(-1), (10, 20));
}

#[test]
fn current_terminal_size_falls_back_when_override_disabled() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_terminal_size_override(None);
        }
    }

    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(Some((false, 10, 20)));
    let _reset = OverrideReset;
    assert_eq!(current_terminal_size(-1), (24, 80));
}

#[test]
fn current_terminal_size_falls_back_when_ioctl_dimensions_zero() {
    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(None);
    let (master, slave) = open_pty_pair();
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_row = 0;
    ws.ws_col = 0;
    unsafe {
        assert_eq!(libc::ioctl(master, libc::TIOCSWINSZ, &ws), 0);
    }
    assert_eq!(current_terminal_size(master), (24, 80));
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

#[test]
fn current_terminal_size_falls_back_when_ioctl_row_zero() {
    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(None);
    let (master, slave) = open_pty_pair();
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_row = 0;
    ws.ws_col = 80;
    unsafe {
        assert_eq!(libc::ioctl(master, libc::TIOCSWINSZ, &ws), 0);
    }
    assert_eq!(current_terminal_size(master), (24, 80));
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

#[test]
fn current_terminal_size_falls_back_when_ioctl_col_zero() {
    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(None);
    let (master, slave) = open_pty_pair();
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_row = 40;
    ws.ws_col = 0;
    unsafe {
        assert_eq!(libc::ioctl(master, libc::TIOCSWINSZ, &ws), 0);
    }
    assert_eq!(current_terminal_size(master), (24, 80));
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

#[test]
fn respond_to_terminal_queries_handles_trailing_escape() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"end\x1b".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(buffer, b"end\x1b".to_vec());
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_handles_unknown_escape() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"pre\x1bXpost".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(buffer, b"pre\x1bXpost".to_vec());
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_osc_preserves_trailing_bytes() {
    reset_respond_osc_counters();
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"hello!\x1b]\x07X".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(buffer, b"hello!X".to_vec());
    assert_eq!(respond_osc_hits(), 1);
    assert_eq!(respond_osc_start(), 8);
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_osc_with_prior_bel() {
    reset_respond_osc_counters();
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"A\x07B\x1b]0\x07Z".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(buffer, b"A\x07BZ".to_vec());
    assert_eq!(respond_osc_hits(), 1);
    assert_eq!(respond_osc_start(), 5);
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_osc_counters_reset_clears_hits() {
    reset_respond_osc_counters();
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"\x1b]0\x07".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(respond_osc_hits(), 1);
    reset_respond_osc_counters();
    assert_eq!(respond_osc_hits(), 0);
    assert_eq!(respond_osc_start(), usize::MAX);
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_osc_hits_zero_without_osc() {
    reset_respond_osc_counters();
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"plain text".to_vec();
    respond_to_terminal_queries(&mut buffer, write_fd);
    assert_eq!(respond_osc_hits(), 0);
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_passthrough_handles_trailing_escape() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"tail\x1b".to_vec();
    respond_to_terminal_queries_passthrough(&mut buffer, write_fd);
    assert_eq!(buffer, b"tail\x1b".to_vec());
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_passthrough_handles_unknown_escape() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"pre\x1bXpost".to_vec();
    respond_to_terminal_queries_passthrough(&mut buffer, write_fd);
    assert_eq!(buffer, b"pre\x1bXpost".to_vec());
    unsafe { libc::close(write_fd) };
    let _ = read_all(read_fd);
    unsafe { libc::close(read_fd) };
}

#[test]
fn apply_control_edits_backspace_then_carriage_return_resets_line_start() {
    let mut data = b"abc\n\x08\rZ".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"Z");
}

#[test]
fn apply_control_edits_preserves_non_osc_escape() {
    reset_apply_osc_counters();
    let mut data = b"\x1b[31mred".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"\x1b[31mred".to_vec());
    assert_eq!(apply_osc_hits(), 0);
}

#[test]
fn apply_control_edits_handles_trailing_escape() {
    let mut data = b"hi\x1b".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"hi\x1b".to_vec());
}

#[test]
fn apply_control_edits_records_osc_start() {
    reset_apply_osc_counters();
    let mut data = b"pre\x1b]0\x07post".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"prepost".to_vec());
    assert_eq!(apply_osc_hits(), 1);
    assert_eq!(apply_osc_start(), 5);
}

#[test]
fn apply_osc_counters_reset_clears_hits() {
    reset_apply_osc_counters();
    let mut data = b"\x1b]0\x07".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(apply_osc_hits(), 1);
    reset_apply_osc_counters();
    assert_eq!(apply_osc_hits(), 0);
    assert_eq!(apply_osc_start(), usize::MAX);
}

#[test]
fn apply_control_edits_handles_osc_near_end_preserves_trailing() {
    reset_apply_osc_counters();
    let mut data = b"hello!\x1b]\x07X".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"hello!X".to_vec());
    assert_eq!(apply_osc_hits(), 1);
    assert_eq!(apply_osc_start(), 8);
}

#[test]
fn apply_control_edits_handles_osc_with_prior_bel() {
    reset_apply_osc_counters();
    let mut data = b"A\x07B\x1b]0\x07Z".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"A\x07BZ".to_vec());
    assert_eq!(apply_osc_hits(), 1);
    assert_eq!(apply_osc_start(), 5);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_does_not_poll_when_elapsed_equals_timeout_nonzero() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_wait_for_exit_elapsed_override(None);
        }
    }

    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(Some(50));
    let _reset = OverrideReset;
    assert!(!wait_for_exit(99999, Duration::from_millis(50)));
    assert_eq!(wait_for_exit_poll_count(), 0);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_does_not_poll_when_elapsed_equals_timeout() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_wait_for_exit_elapsed_override(None);
        }
    }

    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(Some(0));
    let _reset = OverrideReset;
    assert!(!wait_for_exit(99999, Duration::from_millis(0)));
    assert_eq!(wait_for_exit_poll_count(), 0);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_elapsed_override_skips_polling() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_wait_for_exit_elapsed_override(None);
        }
    }

    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(Some(10_000));
    let _reset = OverrideReset;
    assert!(!wait_for_exit(99999, Duration::from_millis(100)));
    assert_eq!(wait_for_exit_poll_count(), 0);
    assert_eq!(wait_for_exit_error_count(), 0);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_records_reap_count() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(None);
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    assert!(wait_for_exit(pid, Duration::from_secs(1)));
    let _ = child.wait();
    assert!(wait_for_exit_reap_count() >= 1);
}

#[cfg(unix)]
#[test]
fn wait_for_exit_reports_error_for_invalid_pid() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    reset_wait_for_exit_counters();
    set_wait_for_exit_elapsed_override(None);
    assert!(wait_for_exit(99999, Duration::from_millis(10)));
    assert_eq!(wait_for_exit_error_count(), 1);
}

#[test]
fn waitpid_failed_flags_negative() {
    assert!(waitpid_failed(-1));
    assert!(!waitpid_failed(0));
    assert!(!waitpid_failed(1));
}

#[test]
fn respond_to_terminal_queries_strips_osc_and_modes() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"hello\x1b]0;title\x07world\x1b[?2004h!".to_vec();

    respond_to_terminal_queries(&mut buffer, write_fd);

    assert_eq!(buffer, b"helloworld!".to_vec());
    unsafe { libc::close(write_fd) };
    let reply = read_all(read_fd);
    assert!(reply.is_empty());
    unsafe { libc::close(read_fd) };
}

#[test]
fn respond_to_terminal_queries_passthrough_handles_multiple_queries() {
    let (read_fd, write_fd) = pipe_pair();
    let mut buffer = b"\x1b[31mred\x1b[6nmid\x1b[5n\x1b[0m".to_vec();

    respond_to_terminal_queries_passthrough(&mut buffer, write_fd);

    assert_eq!(buffer, b"\x1b[31mredmid\x1b[0m".to_vec());
    unsafe { libc::close(write_fd) };
    let reply = read_all(read_fd);
    assert!(reply_contains(&reply, b"\x1b[24;80R"));
    assert!(reply_contains(&reply, b"\x1b[0n"));
    unsafe { libc::close(read_fd) };
}

#[test]
fn apply_control_edits_strips_osc_sequences() {
    let mut data = b"hi\x1b]0;title\x07there".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"hithere");
}

#[test]
fn apply_control_edits_backspace_before_line_start_rewinds() {
    reset_apply_linestart_recalc_count();
    let mut data = b"abc\n\x08Z".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"abcZ");
    assert_eq!(apply_linestart_recalc_count(), 1);
}

#[test]
fn apply_control_edits_backspace_at_line_end_does_not_recalc() {
    reset_apply_linestart_recalc_count();
    let mut data = b"ab\nc\x08".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(data, b"ab\n".to_vec());
    assert_eq!(apply_linestart_recalc_count(), 0);
}

#[test]
fn reset_apply_linestart_recalc_count_clears() {
    reset_apply_linestart_recalc_count();
    let mut data = b"abc\n\x08Z".to_vec();
    apply_control_edits(&mut data);
    assert_eq!(apply_linestart_recalc_count(), 1);
    reset_apply_linestart_recalc_count();
    assert_eq!(apply_linestart_recalc_count(), 0);
}

#[test]
fn pop_last_codepoint_handles_multibyte() {
    let mut data = b"a\xC3\xA9".to_vec();
    pop_last_codepoint(&mut data);
    assert_eq!(data, b"a");
}

#[test]
fn pop_last_codepoint_removes_trailing_newline() {
    let mut data = b"line\n".to_vec();
    pop_last_codepoint(&mut data);
    assert_eq!(data, b"line");
}

#[test]
fn current_line_start_handles_lines() {
    assert_eq!(current_line_start(b"abc"), 0);
    assert_eq!(current_line_start(b"abc\n"), 4);
    assert_eq!(current_line_start(b"abc\nxyz"), 4);
}

#[test]
fn find_csi_sequence_returns_none_for_incomplete_sequence() {
    assert_eq!(find_csi_sequence(b"12;", 0), None);
}

#[test]
fn find_osc_terminator_ignores_incomplete_escape() {
    assert_eq!(find_osc_terminator(b"no-term\x1b", 0), None);
    let with_bel = b"hi\x1bXmore\x07";
    assert_eq!(find_osc_terminator(with_bel, 0), Some(with_bel.len()));
}

#[test]
fn should_strip_without_reply_additional_cases() {
    assert!(should_strip_without_reply(b"?25", b'h'));
    assert!(should_strip_without_reply(b"?1", b'u'));
    assert!(!should_strip_without_reply(b"2004", b'h'));
    assert!(!should_strip_without_reply(b">1", b'n'));
}

#[test]
fn csi_reply_normalizes_leading_markers() {
    assert_eq!(csi_reply(b"?6", b'n', 4, 5), Some(b"\x1b[4;5R".to_vec()));
    assert_eq!(csi_reply(b"> 6", b'n', 3, 2), Some(b"\x1b[3;2R".to_vec()));
}

#[test]
fn current_terminal_size_reads_winsize() {
    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(None);
    let (master, slave) = open_pty_pair();
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    ws.ws_row = 40;
    ws.ws_col = 120;
    unsafe {
        assert_eq!(libc::ioctl(master, libc::TIOCSWINSZ, &ws), 0);
    }
    assert_eq!(current_terminal_size(master), (40, 120));
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

#[test]
fn current_terminal_size_falls_back_when_dimension_zero() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_terminal_size_override(None);
        }
    }

    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(Some((true, 0, 80)));
    let _reset = OverrideReset;
    assert_eq!(current_terminal_size(0), (24, 80));
}

#[test]
fn current_terminal_size_falls_back_when_override_col_zero() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_terminal_size_override(None);
        }
    }

    let _guard = terminal_size_override_lock().lock().unwrap();
    set_terminal_size_override(Some((true, 10, 0)));
    let _reset = OverrideReset;
    assert_eq!(current_terminal_size(0), (24, 80));
}

#[test]
fn write_all_errors_on_invalid_fd() {
    assert!(write_all(-1, b"oops").is_err());
}

#[test]
fn pty_cli_session_read_output_drains_channel() {
    let (tx, rx) = bounded(4);
    tx.send(b"one".to_vec()).unwrap();
    tx.send(b"two".to_vec()).unwrap();
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let output = session.read_output();
    assert_eq!(output, vec![b"one".to_vec(), b"two".to_vec()]);
    assert!(session.read_output().is_empty());
}

#[test]
fn pty_cli_session_read_output_timeout_elapsed_override_prevents_loop() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_read_output_elapsed_override(None);
        }
    }

    let _guard = read_output_timeout_lock().lock().unwrap();
    set_read_output_elapsed_override(Some(5_000));
    let _reset = OverrideReset;

    let (tx, rx) = bounded(1);
    tx.send(b"ready".to_vec()).unwrap();
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let output = session.read_output_timeout(Duration::from_millis(10));
    assert!(output.is_empty());
}

#[test]
fn pty_cli_session_read_output_timeout_respects_zero_timeout() {
    let _guard = read_output_timeout_lock().lock().unwrap();
    set_read_output_grace_override(None);
    let (tx, rx) = bounded(1);
    tx.send(b"ready".to_vec()).unwrap();
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let output = session.read_output_timeout(Duration::from_millis(0));
    assert!(output.is_empty());
}

#[test]
fn pty_cli_session_read_output_timeout_elapsed_boundary() {
    struct OverrideReset;
    impl Drop for OverrideReset {
        fn drop(&mut self) {
            set_read_output_elapsed_override(None);
        }
    }

    let _guard = read_output_timeout_lock().lock().unwrap();
    set_read_output_elapsed_override(Some(0));
    let _reset = OverrideReset;

    let (tx, rx) = bounded(1);
    tx.send(b"ready".to_vec()).unwrap();
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let output = session.read_output_timeout(Duration::from_millis(0));
    assert!(output.is_empty());
}

#[test]
fn pty_cli_session_read_output_timeout_collects_recent_chunks() {
    let _guard = read_output_timeout_lock().lock().unwrap();
    set_read_output_grace_override(None);
    let (tx, rx) = bounded(4);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let sender = thread::spawn(move || {
        tx.send(b"first".to_vec()).unwrap();
        thread::sleep(Duration::from_millis(150));
        tx.send(b"second".to_vec()).unwrap();
    });
    let output = session.read_output_timeout(Duration::from_millis(800));
    sender.join().unwrap();
    assert_eq!(output, vec![b"first".to_vec(), b"second".to_vec()]);
}

#[test]
fn pty_cli_session_read_output_timeout_breaks_on_grace_boundary() {
    struct GraceReset;
    impl Drop for GraceReset {
        fn drop(&mut self) {
            set_read_output_grace_override(None);
        }
    }

    let _guard = read_output_timeout_lock().lock().unwrap();
    set_read_output_grace_override(Some(300));
    let _reset = GraceReset;

    let (tx, rx) = bounded(4);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let sender = thread::spawn(move || {
        tx.send(b"first".to_vec()).unwrap();
        thread::sleep(Duration::from_millis(200));
        tx.send(b"second".to_vec()).unwrap();
    });
    let output = session.read_output_timeout(Duration::from_millis(700));
    sender.join().unwrap();
    assert_eq!(output, vec![b"first".to_vec()]);
}

#[test]
fn pty_cli_session_wait_for_output_collects_and_drains() {
    let (tx, rx) = bounded(4);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    let sender = thread::spawn(move || {
        tx.send(b"first".to_vec()).unwrap();
        tx.send(b"second".to_vec()).unwrap();
    });
    let output = session.wait_for_output(Duration::from_millis(200));
    sender.join().unwrap();
    assert_eq!(output, vec![b"first".to_vec(), b"second".to_vec()]);
}

#[test]
fn pty_cli_session_is_responsive_tracks_liveness() {
    let mut child = std::process::Command::new("sleep")
        .arg("1")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: pid,
        output_rx: rx,
        _output_thread: handle,
    });
    assert!(session.is_responsive(Duration::from_millis(10)));
    let _ = child.kill();
    let _ = child.wait();
    assert!(!session.is_responsive(Duration::from_millis(10)));
}

#[test]
fn pty_cli_session_is_alive_with_negative_pid_returns_false() {
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    assert!(!session.is_alive());
}

#[cfg(unix)]
#[test]
fn pty_cli_session_try_wait_reaps_exited_child() {
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: pid,
        output_rx: rx,
        _output_thread: handle,
    });
    let start = Instant::now();
    let mut status = None;
    while start.elapsed() < Duration::from_millis(200) {
        if let Some(s) = session.try_wait() {
            status = Some(s);
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(status.is_some());
    assert_eq!(session.child_pid, -1);
    let _ = child.wait();
}

#[test]
fn pty_cli_session_is_alive_reflects_child() {
    let mut child = std::process::Command::new("sleep")
        .arg("1")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyCliSession {
        master_fd: -1,
        child_pid: pid,
        output_rx: rx,
        _output_thread: handle,
    });
    assert!(session.is_alive());
    let _ = child.kill();
    let _ = child.wait();
    assert!(!session.is_alive());
}

#[test]
fn pty_overlay_session_send_bytes_writes() {
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let mut session = ManuallyDrop::new(PtyOverlaySession {
        master_fd: write_fd,
        child_pid: -1,
        output_rx: rx,
        _output_thread: handle,
    });
    session.send_bytes(b"bytes").unwrap();
    unsafe { libc::close(write_fd) };
    let output = read_all(read_fd);
    assert_eq!(output, b"bytes");
    unsafe { libc::close(read_fd) };
}

#[test]
fn pty_overlay_session_set_winsize_updates_and_minimums() {
    let (master, slave) = open_pty_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyOverlaySession {
        master_fd: master,
        child_pid: unsafe { libc::getpid() },
        output_rx: rx,
        _output_thread: handle,
    });
    session.set_winsize(0, 0).unwrap();
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    unsafe {
        assert_eq!(libc::ioctl(master, libc::TIOCGWINSZ, &mut ws), 0);
    }
    assert_eq!(ws.ws_row, 1);
    assert_eq!(ws.ws_col, 1);
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

#[test]
fn pty_overlay_session_set_winsize_errors_on_invalid_fd() {
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyOverlaySession {
        master_fd: -1,
        child_pid: unsafe { libc::getpid() },
        output_rx: rx,
        _output_thread: handle,
    });
    assert!(session.set_winsize(10, 10).is_err());
}

#[test]
fn pty_overlay_session_is_alive_reflects_child() {
    let mut child = std::process::Command::new("sleep")
        .arg("1")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = ManuallyDrop::new(PtyOverlaySession {
        master_fd: -1,
        child_pid: pid,
        output_rx: rx,
        _output_thread: handle,
    });
    assert!(session.is_alive());
    let _ = child.kill();
    let _ = child.wait();
    assert!(!session.is_alive());
}

#[test]
fn spawn_reader_thread_recovers_from_wouldblock() {
    let (read_fd, write_fd) = pipe_pair();
    set_nonblocking_fd(read_fd);
    let (tx, rx) = bounded(2);
    let handle = spawn_reader_thread(read_fd, tx);
    thread::sleep(Duration::from_millis(20));
    unsafe {
        libc::write(write_fd, b"ping".as_ptr() as *const libc::c_void, 4);
        libc::close(write_fd);
    }
    let data = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(data, b"ping");
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_reader_thread_closes_channel_on_eof() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, rx) = bounded(1);
    let handle = spawn_reader_thread(read_fd, tx);
    unsafe {
        libc::close(write_fd);
    }
    let err = rx.recv_timeout(Duration::from_millis(200)).unwrap_err();
    assert!(matches!(err, RecvTimeoutError::Disconnected));
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_reader_thread_does_not_log_on_eof() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, _rx) = bounded(1);
    let handle = spawn_reader_thread(read_fd, tx);
    let log = capture_new_log(|| unsafe {
        libc::close(write_fd);
        handle.join().unwrap();
    });
    assert!(!log.contains("PTY read error"));
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_passthrough_reader_thread_recovers_from_wouldblock() {
    let (read_fd, write_fd) = pipe_pair();
    set_nonblocking_fd(read_fd);
    let (tx, rx) = bounded(2);
    let handle = spawn_passthrough_reader_thread(read_fd, tx);
    thread::sleep(Duration::from_millis(20));
    unsafe {
        libc::write(write_fd, b"pong".as_ptr() as *const libc::c_void, 4);
        libc::close(write_fd);
    }
    let data = rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert_eq!(data, b"pong");
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_passthrough_reader_thread_closes_channel_on_eof() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, rx) = bounded(1);
    let handle = spawn_passthrough_reader_thread(read_fd, tx);
    unsafe {
        libc::close(write_fd);
    }
    let err = rx.recv_timeout(Duration::from_millis(200)).unwrap_err();
    assert!(matches!(err, RecvTimeoutError::Disconnected));
    handle.join().unwrap();
    unsafe { libc::close(read_fd) };
}

#[test]
fn spawn_passthrough_reader_thread_does_not_log_on_eof() {
    let (read_fd, write_fd) = pipe_pair();
    let (tx, _rx) = bounded(1);
    let handle = spawn_passthrough_reader_thread(read_fd, tx);
    let log = capture_new_log(|| unsafe {
        libc::close(write_fd);
        handle.join().unwrap();
    });
    assert!(!log.contains("PTY read error"));
    unsafe { libc::close(read_fd) };
}

#[cfg(unix)]
#[test]
fn wait_for_exit_with_zero_timeout_does_not_poll() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    set_wait_for_exit_elapsed_override(None);
    let mut child = std::process::Command::new("true")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    assert!(!wait_for_exit(pid, Duration::from_millis(0)));
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn wait_for_exit_reaps_forked_child() {
    let _guard = wait_for_exit_lock().lock().unwrap();
    set_wait_for_exit_elapsed_override(None);
    unsafe {
        let pid = libc::fork();
        assert!(pid >= 0);
        if pid == 0 {
            libc::_exit(0);
        }
        let result = wait_for_exit(pid, Duration::from_secs(1));
        let mut status = 0;
        let _ = libc::waitpid(pid, &mut status, 0);
        assert!(result);
    }
}

#[cfg(unix)]
#[test]
fn pty_cli_session_drop_terminates_child() {
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let log = capture_new_log(|| {
        let session = PtyCliSession {
            master_fd: write_fd,
            child_pid: pid,
            output_rx: rx,
            _output_thread: handle,
        };
        drop(session);
    });
    assert!(!log.contains("SIGTERM to PTY session failed"));
    assert!(!log.contains("SIGKILL to PTY session failed"));
    unsafe { libc::close(read_fd) };
    let mut status = 0;
    let start = Instant::now();
    let mut alive = true;
    while start.elapsed() < Duration::from_millis(200) {
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        alive = false;
        break;
    }
    if alive {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            let _ = libc::waitpid(pid, &mut status, 0);
        }
        panic!("child still alive after PtyCliSession drop");
    }
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn pty_cli_session_drop_writes_exit_for_zero_pid() {
    let zombie_pid = spawn_zombie_child();
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let session = PtyCliSession {
        master_fd: write_fd,
        child_pid: 0,
        output_rx: rx,
        _output_thread: handle,
    };
    drop(session);
    let output = read_all(read_fd);
    assert!(reply_contains(&output, b"exit\n"));
    unsafe { libc::close(read_fd) };
    let mut status = 0;
    unsafe {
        let _ = libc::waitpid(zombie_pid, &mut status, libc::WNOHANG);
    }
}

#[cfg(unix)]
#[test]
fn pty_cli_session_drop_sigkill_for_ignored_sigterm() {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; sleep 5")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let log = capture_new_log(|| {
        let session = PtyCliSession {
            master_fd: write_fd,
            child_pid: pid,
            output_rx: rx,
            _output_thread: handle,
        };
        drop(session);
    });
    assert!(!log.contains("SIGKILL to PTY session failed"));
    unsafe { libc::close(read_fd) };
    let mut status = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if result == 0 {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            let result = libc::waitpid(pid, &mut status, 0);
            assert!(
                result > 0 || result == -1,
                "child still alive after SIGKILL in PtyCliSession drop"
            );
        }
    }
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn pty_overlay_session_drop_terminates_child() {
    let mut child = std::process::Command::new("sleep")
        .arg("5")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let log = capture_new_log(|| {
        let session = PtyOverlaySession {
            master_fd: write_fd,
            child_pid: pid,
            output_rx: rx,
            _output_thread: handle,
        };
        drop(session);
    });
    assert!(!log.contains("SIGTERM to PTY session failed"));
    assert!(!log.contains("SIGKILL to PTY session failed"));
    unsafe { libc::close(read_fd) };
    let mut status = 0;
    let start = Instant::now();
    let mut alive = true;
    while start.elapsed() < Duration::from_millis(200) {
        let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
        if result == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        alive = false;
        break;
    }
    if alive {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            let _ = libc::waitpid(pid, &mut status, 0);
        }
        panic!("child still alive after PtyOverlaySession drop");
    }
    let _ = child.wait();
}

#[cfg(unix)]
#[test]
fn pty_overlay_session_drop_sigkill_for_ignored_sigterm() {
    let mut child = std::process::Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM; sleep 5")
        .spawn()
        .expect("spawned child");
    let pid = child.id() as i32;
    let (read_fd, write_fd) = pipe_pair();
    let (_tx, rx) = bounded(1);
    let handle = thread::spawn(|| {});
    let log = capture_new_log(|| {
        let session = PtyOverlaySession {
            master_fd: write_fd,
            child_pid: pid,
            output_rx: rx,
            _output_thread: handle,
        };
        drop(session);
    });
    assert!(!log.contains("SIGKILL to PTY session failed"));
    unsafe { libc::close(read_fd) };
    let mut status = 0;
    let result = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if result == 0 {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            let result = libc::waitpid(pid, &mut status, 0);
            assert!(
                result > 0 || result == -1,
                "child still alive after SIGKILL in PtyOverlaySession drop"
            );
        }
    }
    let _ = child.wait();
}

fn reply_contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
