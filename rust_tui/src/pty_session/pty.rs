//! Pseudo-terminal (PTY) session management.
//!
//! Spawns child processes (like Codex CLI) in a PTY so they behave as if
//! running in an interactive terminal. Handles I/O forwarding, window resize
//! signals, and graceful process termination.

use crate::log_debug;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver};
use std::ffi::CString;
use std::io::{self};
use std::mem;
use std::os::unix::io::RawFd;
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

/// Uses PTY to run Codex in a proper terminal environment
pub struct PtyCodexSession {
    pub(super) master_fd: RawFd,
    pub(super) child_pid: i32,
    pub output_rx: Receiver<Vec<u8>>,
    pub(super) _output_thread: thread::JoinHandle<()>,
}

impl PtyCodexSession {
    /// Start Codex under a pseudo-terminal so it behaves like an interactive shell.
    pub fn new(
        codex_cmd: &str,
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
            CString::new(codex_cmd)
                .with_context(|| format!("codex_cmd contains NUL byte: {codex_cmd}"))?,
        );
        for arg in args {
            argv.push(
                CString::new(arg.as_str())
                    .with_context(|| format!("codex arg contains NUL byte: {arg}"))?,
            );
        }

        unsafe {
            let (master_fd, child_pid) = spawn_codex_child(&argv, &cwd, &term_value_cstr)?;
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
    /// Note: Codex doesn't output anything until you send a prompt, so we can't
    /// require output for health check - just verify the process started.
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
        unsafe {
            let mut status = 0;
            let ret = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            ret == 0 // 0 means still running
        }
    }
}

impl Drop for PtyCodexSession {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = self.send("exit\n") {
                log_debug(&format!("failed to send PTY exit command: {err:#}"));
            }
            if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                if libc::kill(self.child_pid, libc::SIGTERM) != 0 {
                    log_debug(&format!(
                        "SIGTERM to Codex session failed: {}",
                        io::Error::last_os_error()
                    ));
                }
                if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                    if libc::kill(self.child_pid, libc::SIGKILL) != 0 {
                        log_debug(&format!(
                            "SIGKILL to Codex session failed: {}",
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
    pub output_rx: Receiver<Vec<u8>>,
    pub(super) _output_thread: thread::JoinHandle<()>,
}

impl PtyOverlaySession {
    /// Start Codex under a pseudo-terminal but keep output raw for overlay rendering.
    pub fn new(
        codex_cmd: &str,
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
            CString::new(codex_cmd)
                .with_context(|| format!("codex_cmd contains NUL byte: {codex_cmd}"))?,
        );
        for arg in args {
            argv.push(
                CString::new(arg.as_str())
                    .with_context(|| format!("codex arg contains NUL byte: {arg}"))?,
            );
        }

        unsafe {
            let (master_fd, child_pid) = spawn_codex_child(&argv, &cwd, &term_value_cstr)?;
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
        let mut ws: libc::winsize = unsafe { mem::zeroed() };
        ws.ws_row = rows.max(1);
        ws.ws_col = cols.max(1);
        ws.ws_xpixel = 0;
        ws.ws_ypixel = 0;
        let result = unsafe { libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &ws) };
        if result != 0 {
            return Err(errno_error("ioctl(TIOCSWINSZ) failed"));
        }
        let _ = unsafe { libc::kill(self.child_pid, libc::SIGWINCH) };
        Ok(())
    }

    /// Peek whether the child is still running (without reaping it).
    pub fn is_alive(&self) -> bool {
        unsafe {
            let mut status = 0;
            let ret = libc::waitpid(self.child_pid, &mut status, libc::WNOHANG);
            ret == 0 // 0 means still running
        }
    }
}

impl Drop for PtyOverlaySession {
    fn drop(&mut self) {
        unsafe {
            if let Err(err) = self.send_text_with_newline("exit") {
                log_debug(&format!("failed to send PTY exit command: {err:#}"));
            }
            if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                if libc::kill(self.child_pid, libc::SIGTERM) != 0 {
                    log_debug(&format!(
                        "SIGTERM to Codex session failed: {}",
                        io::Error::last_os_error()
                    ));
                }
                if !wait_for_exit(self.child_pid, Duration::from_millis(500)) {
                    if libc::kill(self.child_pid, libc::SIGKILL) != 0 {
                        log_debug(&format!(
                            "SIGKILL to Codex session failed: {}",
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
pub(super) unsafe fn spawn_codex_child(
    argv: &[CString],
    working_dir: &CString,
    term_value: &CString,
) -> Result<(RawFd, i32)> {
    let mut master_fd: RawFd = -1;
    let mut slave_fd: RawFd = -1;

    // Set a proper terminal size - codex checks this for terminal detection
    let mut winsize: libc::winsize = mem::zeroed();
    winsize.ws_row = 24;
    winsize.ws_col = 80;
    winsize.ws_xpixel = 0;
    winsize.ws_ypixel = 0;

    #[allow(clippy::unnecessary_mut_passed)]
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
    let fail = || -> ! {
        libc::_exit(1);
    };

    if libc::setsid() == -1 {
        fail();
    }
    if libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0) == -1 {
        fail();
    }
    if libc::dup2(slave_fd, libc::STDIN_FILENO) < 0
        || libc::dup2(slave_fd, libc::STDOUT_FILENO) < 0
        || libc::dup2(slave_fd, libc::STDERR_FILENO) < 0
    {
        fail();
    }
    close_fd(slave_fd);

    if libc::chdir(working_dir.as_ptr()) != 0 {
        fail();
    }

    let term_key = CString::new("TERM").expect("TERM constant is valid");
    if libc::setenv(term_key.as_ptr(), term_value.as_ptr(), 1) != 0 {
        fail();
    }

    let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|s| s.as_ptr()).collect();
    argv_ptrs.push(ptr::null());

    libc::execvp(argv_ptrs[0], argv_ptrs.as_ptr());
    fail();
}

/// Configure the PTY master for non-blocking reads.
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
