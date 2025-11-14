//! Minimal PTY wrapper used to host Codex in a real terminal so persistent sessions
//! can keep state (tools, environment) between prompts.

use crate::log_debug;
use anyhow::{anyhow, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::ffi::CString;
use std::io::{self, ErrorKind};
use std::mem;
use std::os::unix::io::RawFd;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

/// Uses PTY to run Codex in a proper terminal environment
pub struct PtyCodexSession {
    master_fd: RawFd,
    child_pid: i32,
    output_rx: Receiver<Vec<u8>>,
    _output_thread: thread::JoinHandle<()>,
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

            thread::sleep(Duration::from_millis(500));

            Ok(Self {
                master_fd,
                child_pid,
                output_rx: rx,
                _output_thread: output_thread,
            })
        }
    }

    /// Write text to the PTY, automatically ensuring prompts end with a newline.
    pub fn send(&mut self, text: &str) -> Result<()> {
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
        let mut output = Vec::new();
        let start = Instant::now();
        let mut last_chunk: Option<Instant> = None;

        while start.elapsed() < timeout {
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
                        if last.elapsed() < Duration::from_millis(300) {
                            continue;
                        }
                    }
                    break;
                }
            }
        }
        output
    }

    /// Probe whether the PTY echoes back within a tight timeout.
    /// Drains all stale output first to avoid false positives from buffered data.
    pub fn is_responsive(&mut self, timeout: Duration) -> bool {
        // Aggressively drain all stale bytes to avoid false positives
        for _ in 0..5 {
            if self.read_output().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        // Send newline and check for ANY response
        if self.send("\n").is_err() {
            return false;
        }

        // Any output within timeout indicates responsiveness
        !self.read_output_timeout(timeout).is_empty()
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
                    let mut status = 0;
                    if libc::waitpid(self.child_pid, &mut status, 0) < 0 {
                        log_debug(&format!(
                            "waitpid after SIGKILL failed: {}",
                            io::Error::last_os_error()
                        ));
                    }
                }
            }
            close_fd(self.master_fd);
        }
    }
}

/// Fork and exec the Codex binary under a newly allocated PTY pair.
unsafe fn spawn_codex_child(
    argv: &[CString],
    working_dir: &CString,
    term_value: &CString,
) -> Result<(RawFd, i32)> {
    let mut master_fd: RawFd = -1;
    let mut slave_fd: RawFd = -1;
    if libc::openpty(
        &mut master_fd,
        &mut slave_fd,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
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

/// Child-side setup: hook the PTY to stdin/stdout/stderr and exec Codex.
unsafe fn child_exec(
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
unsafe fn set_nonblocking(fd: RawFd) -> Result<()> {
    let flags = libc::fcntl(fd, libc::F_GETFL, 0);
    if flags < 0 {
        return Err(errno_error("fcntl(F_GETFL) failed"));
    }
    if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
        return Err(errno_error("fcntl(F_SETFL) failed"));
    }
    Ok(())
}

/// Continuously read from the PTY and forward chunks to the main thread.
fn spawn_reader_thread(master_fd: RawFd, tx: Sender<Vec<u8>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            let n = unsafe {
                libc::read(
                    master_fd,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if n > 0 {
                let mut data = buffer.get(..n as usize).unwrap_or(&[]).to_vec();
                // Answer simple terminal capability queries so Codex doesn't hang waiting.
                respond_to_terminal_queries(&mut data, master_fd);
                if data.is_empty() {
                    continue;
                }
                if tx.send(data).is_err() {
                    break;
                }
                continue;
            }
            if n == 0 {
                break;
            }
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted || err.kind() == ErrorKind::WouldBlock {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            log_debug(&format!("PTY read error: {err}"));
            break;
        }
    })
}

/// Helper that formats OS errors with additional context.
fn errno_error(context: &str) -> anyhow::Error {
    anyhow!("{context}: {}", io::Error::last_os_error())
}

/// Close a file descriptor while ignoring errors.
unsafe fn close_fd(fd: RawFd) {
    if fd >= 0 {
        let _ = libc::close(fd);
    }
}

/// Write the entire buffer to the PTY master, retrying short writes.
fn write_all(fd: RawFd, mut data: &[u8]) -> Result<()> {
    while !data.is_empty() {
        let written = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, data.len()) };
        if written < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == ErrorKind::Interrupted {
                continue;
            }
            return Err(anyhow!("write to PTY failed: {err}"));
        }
        let written = written as usize;
        data = if written <= data.len() {
            &data[written..]
        } else {
            &[]
        };
    }
    Ok(())
}

/// Wait for the child process to terminate, but bail out after a short timeout.
fn wait_for_exit(child_pid: i32, timeout: Duration) -> bool {
    let start = Instant::now();
    let mut status = 0;
    while start.elapsed() < timeout {
        let result = unsafe { libc::waitpid(child_pid, &mut status, libc::WNOHANG) };
        if result > 0 {
            return true;
        }
        if result < 0 {
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

/// Codex occasionally probes terminal capabilities; strip those control sequences
/// and apply inline edits (CR/BS) before rendering so the UI only sees printable text.
fn respond_to_terminal_queries(buffer: &mut Vec<u8>, master_fd: RawFd) {
    let (rows, cols) = current_terminal_size(master_fd);
    let mut idx = 0;
    while idx < buffer.len() {
        if buffer[idx] != 0x1B {
            idx += 1;
            continue;
        }
        if idx + 1 >= buffer.len() {
            break;
        }
        match buffer[idx + 1] {
            b'[' => {
                match find_csi_sequence(buffer, idx + 2) {
                    Some((params_end, final_byte)) => {
                        let seq_end = params_end + 1;
                        let params_start = idx + 2;
                        let params: Vec<u8> =
                            buffer.get(params_start..params_end).unwrap_or(&[]).to_vec();
                        if should_strip_without_reply(&params, final_byte) {
                            buffer.drain(idx..seq_end);
                            continue;
                        }
                        if let Some(reply) = csi_reply(&params, final_byte, rows, cols) {
                            buffer.drain(idx..seq_end);
                            if let Err(err) = write_all(master_fd, &reply) {
                                log_debug(&format!(
                                    "Failed to answer terminal query (CSI {}{}): {err:#}",
                                    String::from_utf8_lossy(&params),
                                    final_byte as char
                                ));
                            }
                            continue;
                        }
                        idx = seq_end;
                        continue;
                    }
                    None => break, // Incomplete CSI sequence; wait for more data
                }
            }
            b']' => {
                if let Some(end) = find_osc_terminator(buffer, idx + 2) {
                    buffer.drain(idx..end);
                    continue;
                } else {
                    // Unterminated OSC; drop the rest to avoid leaking into output
                    buffer.drain(idx..);
                    break;
                }
            }
            _ => {}
        }
        idx += 1;
    }

    apply_control_edits(buffer);
}

fn should_strip_without_reply(params: &[u8], final_byte: u8) -> bool {
    final_byte == b'u' && (params.starts_with(b"?") || params.starts_with(b">"))
}

fn csi_reply(params: &[u8], final_b: u8, rows: u16, cols: u16) -> Option<Vec<u8>> {
    // Normalize params: drop leading '?', '>' and spaces.
    let p: Vec<u8> = params
        .iter()
        .copied()
        .filter(|b| *b != b' ')
        .skip_while(|b| *b == b'?' || *b == b'>')
        .collect();

    match final_b {
        // DSR: status report → ESC[0n
        b'n' if p == b"5" => Some(b"\x1b[0n".to_vec()),

        // DSR: cursor position request → ESC[row;colR
        b'n' if p == b"6" => {
            let (r, c) = (rows.max(1), cols.max(1));
            Some(format!("\x1b[{r};{c}R").into_bytes())
        }

        // DA: primary device attributes → safe VT220-ish reply
        b'c' => Some(b"\x1b[?1;2c".to_vec()),

        _ => None,
    }
}

fn current_terminal_size(master_fd: RawFd) -> (u16, u16) {
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    unsafe {
        if libc::ioctl(master_fd, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 && ws.ws_col > 0
        {
            (ws.ws_row, ws.ws_col)
        } else {
            (24, 80)
        }
    }
}

fn find_csi_sequence(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    let mut idx = start;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if (0x40..=0x7E).contains(&byte) {
            return Some((idx, byte));
        }
        idx += 1;
    }
    None
}

fn find_osc_terminator(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    while cursor < bytes.len() {
        match bytes[cursor] {
            0x07 => return Some(cursor + 1),
            0x1B if cursor + 1 < bytes.len() && bytes[cursor + 1] == b'\\' => {
                return Some(cursor + 2)
            }
            _ => cursor += 1,
        }
    }
    None
}

fn apply_control_edits(buffer: &mut Vec<u8>) {
    let mut output = Vec::with_capacity(buffer.len());
    let mut line_start = 0usize;
    let mut idx = 0;

    while idx < buffer.len() {
        match buffer[idx] {
            b'\r' => {
                output.truncate(line_start);
                idx += 1;
            }
            b'\n' => {
                output.push(b'\n');
                idx += 1;
                line_start = output.len();
            }
            b'\x08' => {
                pop_last_codepoint(&mut output);
                idx += 1;
                if line_start > output.len() {
                    line_start = current_line_start(&output);
                }
            }
            0x1B => {
                if idx + 1 < buffer.len() && buffer[idx + 1] == b']' {
                    if let Some(end) = find_osc_terminator(buffer, idx + 2) {
                        idx = end;
                        continue;
                    } else {
                        break;
                    }
                }
                output.push(buffer[idx]);
                idx += 1;
            }
            byte => {
                output.push(byte);
                idx += 1;
            }
        }
    }

    buffer.clear();
    buffer.extend_from_slice(&output);
}

fn pop_last_codepoint(buf: &mut Vec<u8>) {
    if buf.is_empty() {
        return;
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
        return;
    }
    while let Some(byte) = buf.pop() {
        if (byte & 0b1100_0000) != 0b1000_0000 {
            break;
        }
    }
}

fn current_line_start(buf: &[u8]) -> usize {
    buf.iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::io::RawFd;

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
        let mut buf = [0u8; 64];
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        assert!(n >= 0);
        buf[..n as usize].to_vec()
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
    fn apply_control_edits_handles_cr_and_backspace() {
        let mut data = b"foo\rbar\x08z\nnext\x08!".to_vec();
        apply_control_edits(&mut data);
        assert_eq!(data, b"baz\nnex!");
    }

    fn reply_contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
