//! Codex CLI invocation flow with cancellation and non-PTY reliability safeguards.

use super::backend::{CancelToken, CodexCallError};
use crate::{config::AppConfig, log_debug};
use anyhow::{anyhow, Result};
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::{
    io::{self, Write},
    path::Path,
    process::{Child, Command, Output, Stdio},
    sync::mpsc::{self, TryRecvError},
    thread,
    time::{Duration, Instant},
};

pub(super) fn call_codex_cli(
    config: &AppConfig,
    prompt: &str,
    working_dir: &Path,
    cancel: &CancelToken,
) -> Result<String, CodexCallError> {
    // Use codex exec - directly (most reliable non-PTY path)
    // The interactive mode (codex -C ...) always fails with "stdin is not a terminal"
    // so we skip it to reduce latency
    let mut exec_cmd = Command::new(&config.codex_cmd);
    exec_cmd
        .arg("exec")
        .arg("-") // Read from stdin - must be right after "exec"
        .arg("-C")
        .arg(working_dir)
        .args(&config.codex_args)
        .env("TERM", &config.term_value)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let exec_output =
        spawn_with_cancel(exec_cmd, Some(prompt), cancel).map_err(|err| match err {
            CodexCallError::Cancelled => CodexCallError::Cancelled,
            CodexCallError::Failure(e) => CodexCallError::Failure(
                e.context(format!("failed to spawn {} exec", config.codex_cmd)),
            ),
        })?;

    if exec_output.status.success() {
        return Ok(String::from_utf8_lossy(&exec_output.stdout).to_string());
    }

    Err(CodexCallError::Failure(anyhow!(
        "codex exec failed: {}",
        String::from_utf8_lossy(&exec_output.stderr).trim()
    )))
}

fn spawn_with_cancel(
    mut cmd: Command,
    prompt: Option<&str>,
    cancel: &CancelToken,
) -> Result<Output, CodexCallError> {
    let mut child = cmd.spawn()?;
    if let Some(text) = prompt {
        if let Some(mut stdin) = child.stdin.take() {
            write_prompt_with_newline(&mut stdin, text)?;
        }
    }
    wait_child_with_cancel(child, cancel)
}

fn wait_child_with_cancel(child: Child, cancel: &CancelToken) -> Result<Output, CodexCallError> {
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = child.wait_with_output();
        let _ = tx.send(result);
    });

    let mut cancel_requested_at: Option<Instant> = None;
    let mut sigkill_sent = false;

    loop {
        match rx.try_recv() {
            Ok(result) => {
                return match result {
                    Ok(output) => {
                        if cancel_requested_at.is_some() {
                            Err(CodexCallError::Cancelled)
                        } else {
                            Ok(output)
                        }
                    }
                    Err(err) => Err(CodexCallError::Failure(err.into())),
                };
            }
            Err(TryRecvError::Disconnected) => {
                return Err(CodexCallError::Failure(anyhow!(
                    "Codex child waiter disconnected unexpectedly"
                )));
            }
            Err(TryRecvError::Empty) => {}
        }

        if cancel.is_cancelled() {
            if cancel_requested_at.is_none() {
                log_debug("CodexJob: cancellation requested; sending SIGTERM");
                send_signal(pid, Signal::Term);
                cancel_requested_at = Some(Instant::now());
            } else if should_send_sigkill(sigkill_sent, cancel_requested_at, Instant::now()) {
                log_debug("CodexJob: escalation to SIGKILL");
                send_signal(pid, Signal::Kill);
                sigkill_sent = true;
            }
        }

        thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn should_send_sigkill(
    sigkill_sent: bool,
    cancel_requested_at: Option<Instant>,
    now: Instant,
) -> bool {
    if sigkill_sent {
        return false;
    }
    match cancel_requested_at {
        Some(start) => now.duration_since(start) >= Duration::from_millis(500),
        None => false,
    }
}

pub(super) enum Signal {
    Term,
    Kill,
}

#[cfg(test)]
static SEND_SIGNAL_FAILURES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn reset_send_signal_failures() {
    SEND_SIGNAL_FAILURES.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
pub(super) fn send_signal_failures() -> usize {
    SEND_SIGNAL_FAILURES.load(std::sync::atomic::Ordering::SeqCst)
}

pub(super) fn send_signal(pid: u32, signal: Signal) {
    #[cfg(unix)]
    unsafe {
        // SAFETY: libc::kill only needs a valid pid and signal number.
        // The pid comes from the spawned child process, and we map to SIGTERM/SIGKILL.
        let signo = match signal {
            Signal::Term => libc::SIGTERM,
            Signal::Kill => libc::SIGKILL,
        };
        if libc::kill(pid as i32, signo) != 0 {
            #[cfg(test)]
            SEND_SIGNAL_FAILURES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            log_debug(&format!(
                "CodexJob: failed to send signal {signo} to pid {pid}: {}",
                io::Error::last_os_error()
            ));
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = signal;
        log_debug("CodexJob: cancellation requested, but signals unsupported on this platform");
    }
}

pub(super) fn write_prompt_with_newline<W: Write>(writer: &mut W, prompt: &str) -> io::Result<()> {
    writer.write_all(prompt.as_bytes())?;
    if !prompt.ends_with('\n') {
        writer.write_all(b"\n")?;
    }
    Ok(())
}
