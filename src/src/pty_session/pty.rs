//! Pseudo-terminal (PTY) session management.
//!
//! Spawns backend CLIs in a PTY so they behave as if
//! running in an interactive terminal. Handles I/O forwarding, window resize
//! signals, and graceful process termination.

use crate::log_debug;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver};
use std::ffi::CString;
use std::io::{self};
use std::mem;
use std::os::unix::io::RawFd;
use std::os::unix::process::ExitStatusExt;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(any(test, feature = "mutants"))]
use super::counters::{
    guard_loop, record_pty_read, record_pty_send, record_wait_for_exit_error,
    record_wait_for_exit_poll, record_wait_for_exit_reap,
};
use super::counters::{read_output_elapsed, read_output_grace_elapsed, wait_for_exit_elapsed};
use super::io::{spawn_passthrough_reader_thread, spawn_reader_thread, write_all};

/// Uses PTY to run a backend CLI in a proper terminal environment.
pub struct PtyCliSession {
    pub(super) master_fd: RawFd,
    pub(super) child_pid: i32,
    /// Stream of raw PTY output chunks from the child process.
    pub output_rx: Receiver<Vec<u8>>,
    pub(super) _output_thread: thread::JoinHandle<()>,
}

impl PtyCliSession {
    /// Start a backend CLI under a pseudo-terminal so it behaves like an interactive shell.
    pub fn new(
        cli_cmd: &str,
        working_dir: &str,
        args: &[String],
        term_value: &str,
    ) -> Result<Self> {
        let cwd = CString::new(working_dir)
            .with_context(|| format!("working directory contains NUL byte: {working_dir}"))?;
        let term_value_cstr = CString::new(term_value).unwrap_or_else(|_| {
            CString::new("xterm-256color").expect("static TERM fallback should be valid")
        });
        let mut argv: Vec<CString> = Vec::with_capacity(args.len() + 1);
        argv.push(
            CString::new(cli_cmd)
                .with_context(|| format!("cli_cmd contains NUL byte: {cli_cmd}"))?,
        );
        for arg in args {
            argv.push(
                CString::new(arg.as_str())
                    .with_context(|| format!("cli arg contains NUL byte: {arg}"))?,
            );
        }

        // SAFETY: argv/cwd/TERM are valid CStrings; spawn_pty_child returns a valid master fd.
        // set_nonblocking only touches the returned master fd.
        unsafe {
            let (master_fd, child_pid) = spawn_pty_child(&argv, &cwd, &term_value_cstr)?;
            set_nonblocking(master_fd)?;

            let (tx, rx) = bounded(100);
            let output_thread = spawn_reader_thread(master_fd, tx);

            let session = Self {
                master_fd,
                child_pid,
                output_rx: rx,
                _output_thread: output_thread,
            };

            Ok(session)
        }
    }

    /// Write text to the PTY, automatically ensuring prompts end with a newline.
    pub fn send(&mut self, text: &str) -> Result<()> {
        #[cfg(any(test, feature = "mutants"))]
        record_pty_send();
        write_all(self.master_fd, text.as_bytes())?;
        if !text.ends_with('\n') {
            write_all(self.master_fd, b"\n")?;
        }
        Ok(())
    }

    /// Drain any waiting output without blocking.
    pub fn read_output(&self) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        while let Ok(line) = self.output_rx.try_recv() {
            output.push(line);
        }
        output
    }

    /// Block for a short window until output arrives or the timeout expires.
    pub fn read_output_timeout(&self, timeout: Duration) -> Vec<Vec<u8>> {
        #[cfg(any(test, feature = "mutants"))]
        record_pty_read();
        let mut output = Vec::new();
        let start = Instant::now();
        let mut last_chunk: Option<Instant> = None;

        while read_output_elapsed(start) < timeout {
            match self.output_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    last_chunk = Some(Instant::now());
                    output.push(line);
                }
                Err(_) => {
                    if output.is_empty() {
                        break;
                    }
                    if let Some(last) = last_chunk {
                        let elapsed = read_output_grace_elapsed(last);
                        if elapsed < Duration::from_millis(300) {
                            continue;
                        }
                    }
                    break;
                }
            }
        }
        output
    }

    /// Check if the PTY session is responsive by verifying the process is alive.
    /// Note: some backends don't output anything until you send a prompt, so we
    /// can't require output for the health check - just verify the process started.
    pub fn is_responsive(&mut self, _timeout: Duration) -> bool {
        // Drain any startup output (banner, prompts, etc.) without blocking
        let _ = self.read_output();

        if !self.is_alive() {
            log_debug("PTY health check: process not alive");
            return false;
        }

        log_debug("PTY health check: process alive, assuming responsive");
        true
    }

    /// Wait up to `timeout` for at least one output chunk, then drain any remaining bytes.
    pub fn wait_for_output(&self, timeout: Duration) -> Vec<Vec<u8>> {
        let mut output = Vec::new();
        if let Ok(chunk) = self.output_rx.recv_timeout(timeout) {
            output.push(chunk);
            output.extend(self.read_output());
        }
        output
    }

    /// Peek whether the child is still running (without reaping it).
    pub fn is_alive(&self) -> bool {
        if self.child_pid < 0 {
            return false;
        }
        unsafe {
            // SAFETY: child_pid is owned by this session; waitpid with WNOHANG only inspects state.
            let mut status = 0;
            let ret = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            ret == 0 // 0 means still running
        }
    }

    /// Non-blocking check for child exit; reaps the child on completion.
    pub fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        if self.child_pid < 0 {
            return None;
        }
        unsafe {
            let mut status = 0;
            let ret = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            if ret <= 0 {
                None
            } else {
                self.child_pid = -1;
                Some(std::process::ExitStatus::from_raw(status))
            }
        }
    }
}

impl Drop for PtyCliSession {
    fn drop(&mut self) {
        unsafe {
            if self.child_pid < 0 {
                close_fd(self.master_fd);
                return;
            }
            // SAFETY: child_pid/master_fd come from spawn_pty_child; cleanup uses best-effort signals
            // and closes the fd if still open.
            if let Err(err) = self.send("exit\n") {
                log_debug(&format!("failed to send PTY exit command: {err:#}"));
            }
            if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                if libc::kill(self.child_pid, libc::SIGTERM) != 0 {
                    log_debug(&format!(
                        "SIGTERM to PTY session failed: {}",
                        io::Error::last_os_error()
                    ));
                }
                if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                    if libc::kill(self.child_pid, libc::SIGKILL) != 0 {
                        log_debug(&format!(
                            "SIGKILL to PTY session failed: {}",
                            io::Error::last_os_error()
                        ));
                    }
                    #[cfg(any(test, feature = "mutants"))]
                    {
                        let mut status = 0;
                        let _ = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
                    }
                    #[cfg(not(any(test, feature = "mutants")))]
                    {
                        let mut status = 0;
                        let ret = libc::waitpid(self.child_pid, &mut status, 0);
                        if waitpid_failed(ret) {
                            log_debug(&format!(
                                "waitpid after SIGKILL failed: {}",
                                io::Error::last_os_error()
                            ));
                        }
                    }
                }
            }
            close_fd(self.master_fd);
        }
    }
}

/// PTY session that forwards raw output (ANSI intact) while answering terminal queries.
pub struct PtyOverlaySession {
    pub(super) master_fd: RawFd,
    pub(super) child_pid: i32,
    /// Stream of raw PTY output chunks from the child process.
    pub output_rx: Receiver<Vec<u8>>,
    pub(super) _output_thread: thread::JoinHandle<()>,
}

impl PtyOverlaySession {
    /// Start a backend CLI under a pseudo-terminal but keep output raw for overlay rendering.
    pub fn new(
        cli_cmd: &str,
        working_dir: &str,
        args: &[String],
        term_value: &str,
    ) -> Result<Self> {
        let cwd = CString::new(working_dir)
            .with_context(|| format!("working directory contains NUL byte: {working_dir}"))?;
        let term_value_cstr = CString::new(term_value).unwrap_or_else(|_| {
            CString::new("xterm-256color").expect("static TERM fallback should be valid")
        });
        let mut argv: Vec<CString> = Vec::with_capacity(args.len() + 1);
        argv.push(
            CString::new(cli_cmd)
                .with_context(|| format!("cli_cmd contains NUL byte: {cli_cmd}"))?,
        );
        for arg in args {
            argv.push(
                CString::new(arg.as_str())
                    .with_context(|| format!("cli arg contains NUL byte: {arg}"))?,
            );
        }

        // SAFETY: argv/cwd/TERM are valid CStrings; spawn_pty_child returns a valid master fd.
        // set_nonblocking only touches the returned master fd.
        unsafe {
            let (master_fd, child_pid) = spawn_pty_child(&argv, &cwd, &term_value_cstr)?;
            set_nonblocking(master_fd)?;

            let (tx, rx) = bounded(100);
            let output_thread = spawn_passthrough_reader_thread(master_fd, tx);

            Ok(Self {
                master_fd,
                child_pid,
                output_rx: rx,
                _output_thread: output_thread,
            })
        }
    }

    /// Write raw bytes to the PTY master.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        write_all(self.master_fd, bytes)
    }

    /// Write text to the PTY master.
    pub fn send_text(&mut self, text: &str) -> Result<()> {
        write_all(self.master_fd, text.as_bytes())
    }

    /// Write text to the PTY master and ensure it ends with a newline.
    pub fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
        write_all(self.master_fd, text.as_bytes())?;
        if !text.ends_with('\n') {
            write_all(self.master_fd, b"\n")?;
        }
        Ok(())
    }

    /// Update the PTY window size and notify the child.
    pub fn set_winsize(&self, rows: u16, cols: u16) -> Result<()> {
        // SAFETY: libc::winsize is a plain C struct; zeroed is a valid baseline.
        let mut ws: libc::winsize = unsafe { mem::zeroed() };
        ws.ws_row = rows.max(1);
        ws.ws_col = cols.max(1);
        ws.ws_xpixel = 0;
        ws.ws_ypixel = 0;
        // SAFETY: ioctl writes to ws and reads master_fd; ws is initialized.
        let result = unsafe { libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &ws) };
        if result != 0 {
            return Err(errno_error("ioctl(TIOCSWINSZ) failed"));
        }
        // SAFETY: SIGWINCH is sent to the child pid owned by this session.
        let _ = unsafe { libc::kill(self.child_pid, libc::SIGWINCH) };
        Ok(())
    }

    /// Peek whether the child is still running (without reaping it).
    pub fn is_alive(&self) -> bool {
        unsafe {
            // SAFETY: child_pid is owned by this session; waitpid with WNOHANG only inspects state.
            let mut status = 0;
            let ret = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            ret == 0 // 0 means still running
        }
    }
}

#[cfg(any(test, feature = "mutants"))]
impl PtyOverlaySession {
    pub fn test_winsize(&self) -> (u16, u16) {
        super::osc::current_terminal_size(self.master_fd)
    }
}

#[cfg(any(test, feature = "mutants"))]
#[cfg_attr(any(test, feature = "mutants"), allow(dead_code))]
pub(crate) fn test_pty_session(
    master_fd: RawFd,
    child_pid: i32,
    output_rx: Receiver<Vec<u8>>,
) -> PtyCliSession {
    let handle = thread::spawn(|| {});
    PtyCliSession {
        master_fd,
        child_pid,
        output_rx,
        _output_thread: handle,
    }
}

impl Drop for PtyOverlaySession {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: child_pid/master_fd come from spawn_pty_child; cleanup uses best-effort signals
            // and closes the fd if still open.
            if let Err(err) = self.send_text_with_newline("exit") {
                log_debug(&format!("failed to send PTY exit command: {err:#}"));
            }
            if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                if libc::kill(self.child_pid, libc::SIGTERM) != 0 {
                    log_debug(&format!(
                        "SIGTERM to PTY session failed: {}",
                        io::Error::last_os_error()
                    ));
                }
                if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                    if libc::kill(self.child_pid, libc::SIGKILL) != 0 {
                        log_debug(&format!(
                            "SIGKILL to PTY session failed: {}",
                            io::Error::last_os_error()
                        ));
                    }
                    #[cfg(any(test, feature = "mutants"))]
                    {
                        let mut status = 0;
                        let _ = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
                    }
                    #[cfg(not(any(test, feature = "mutants")))]
                    {
                        let mut status = 0;
                        let ret = libc::waitpid(self.child_pid, &mut status, 0);
                        if waitpid_failed(ret) {
                            log_debug(&format!(
                                "waitpid after SIGKILL failed: {}",
                                io::Error::last_os_error()
                            ));
                        }
                    }
                }
            }
            close_fd(self.master_fd);
        }
    }
}

/// Forks and execs a child process under a new PTY.
///
/// # Safety
///
/// This function performs low-level PTY allocation and process forking.
/// The caller must ensure:
/// - `argv` contains valid null-terminated C strings
/// - `working_dir` is a valid directory path
/// - The returned file descriptor is eventually closed
///
/// The child process calls `_exit(1)` on any setup failure to avoid
/// undefined behavior from returning after `fork()`.
pub(super) unsafe fn spawn_pty_child(
    argv: &[CString],
    working_dir: &CString,
    term_value: &CString,
) -> Result<(RawFd, i32)> {
    let mut master_fd: RawFd = -1;
    let mut slave_fd: RawFd = -1;

    // Set a proper terminal size - some backends check this for terminal detection
    // SAFETY: libc::winsize is a plain C struct; zeroed is a valid baseline.
    let mut winsize: libc::winsize = mem::zeroed();
    winsize.ws_row = 24;
    winsize.ws_col = 80;
    winsize.ws_xpixel = 0;
    winsize.ws_ypixel = 0;

    #[allow(clippy::unnecessary_mut_passed)]
    // SAFETY: openpty expects valid pointers for master/slave/winsize; we pass stack locals.
    if libc::openpty(
        &mut master_fd,
        &mut slave_fd,
        ptr::null_mut(),
        ptr::null_mut(),
        &mut winsize,
    ) != 0
    {
        return Err(errno_error("openpty failed"));
    }

    // SAFETY: fork is called before any unsafe Rust invariants are relied on.
    let pid = libc::fork();
    if pid < 0 {
        close_fd(master_fd);
        close_fd(slave_fd);
        return Err(errno_error("fork failed"));
    }

    if pid == 0 {
        child_exec(slave_fd, argv, working_dir, term_value);
    }

    close_fd(slave_fd);
    Ok((master_fd, pid))
}

/// Child process setup after fork: configures PTY and execs the target binary.
///
/// # Safety
///
/// Must only be called in the child process after `fork()`. This function
/// never returns - it either calls `execvp()` to replace the process image
/// or `_exit(1)` on failure.
///
/// The `-> !` return type indicates this function diverges (never returns).
pub(super) unsafe fn child_exec(
    slave_fd: RawFd,
    argv: &[CString],
    working_dir: &CString,
    term_value: &CString,
) -> ! {
    let fail = |context: &str| -> ! {
        let err = io::Error::last_os_error();
        let msg = format!("child_exec {context} failed: {err}\n");
        // SAFETY: write is async-signal-safe and stderr is a valid fd in the child.
        let _ = libc::write(
            libc::STDERR_FILENO,
            msg.as_ptr() as *const libc::c_void,
            msg.len(),
        );
        libc::_exit(1);
    };

    if libc::setsid() == -1 {
        fail("setsid");
    }
    if libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0) == -1 {
        fail("ioctl(TIOCSCTTY)");
    }
    if libc::dup2(slave_fd, libc::STDIN_FILENO) < 0
        || libc::dup2(slave_fd, libc::STDOUT_FILENO) < 0
        || libc::dup2(slave_fd, libc::STDERR_FILENO) < 0
    {
        fail("dup2");
    }
    close_fd(slave_fd);

    if libc::chdir(working_dir.as_ptr()) != 0 {
        fail("chdir");
    }

    let term_key = CString::new("TERM").expect("TERM constant is valid");
    if libc::setenv(term_key.as_ptr(), term_value.as_ptr(), 1) != 0 {
        fail("setenv(TERM)");
    }

    let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|s| s.as_ptr()).collect();
    argv_ptrs.push(ptr::null());

    libc::execvp(argv_ptrs[0], argv_ptrs.as_ptr());
    fail("execvp");
}

/// Configure the PTY master for non-blocking reads.
///
/// # Safety
///
/// `fd` must be a valid, open file descriptor.
pub(super) unsafe fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = libc::fcntl(fd, libc::F_GETFL, 0);
    if flags < 0 {
        return Err(errno_error("fcntl(F_GETFL) failed"));
    }
    if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
        return Err(errno_error("fcntl(F_SETFL) failed"));
    }
    Ok(())
}

/// Helper that formats OS errors with additional context.
pub(super) fn errno_error(context: &str) -> anyhow::Error {
    anyhow!("{context}: {}", io::Error::last_os_error())
}

/// Close a file descriptor while ignoring errors.
///
/// # Safety
///
/// `fd` must be a valid, open file descriptor (or -1 to ignore).
pub(super) unsafe fn close_fd(fd: RawFd) {
    if fd >= 0 {
        let _ = libc::close(fd);
    }
}

#[cfg_attr(any(test, feature = "mutants"), allow(dead_code))]
pub(super) fn waitpid_failed(ret: i32) -> bool {
    ret < 0
}

/// Wait for the child process to terminate, but bail out after a short timeout.
pub(super) fn wait_for_exit(child_pid: i32, timeout: Duration) -> bool {
    if timeout.is_zero() {
        return false;
    }
    let start = Instant::now();
    let mut status = 0;
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while wait_for_exit_elapsed(start) < timeout {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(start, guard_iters, 10_000, "wait_for_exit");
        }
        #[cfg(any(test, feature = "mutants"))]
        record_wait_for_exit_poll();
        // SAFETY: child_pid is owned by this session; waitpid with WNOHANG only inspects state.
        let result = unsafe { libc::waitpid(child_pid, &mut status, libc::WNOHANG) };
        if result > 0 {
            #[cfg(any(test, feature = "mutants"))]
            record_wait_for_exit_reap();
            return true;
        }
        if result < 0 {
            #[cfg(any(test, feature = "mutants"))]
            record_wait_for_exit_error();
            log_debug(&format!(
                "waitpid({}) failed: {}",
                child_pid,
                io::Error::last_os_error()
            ));
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}
