//! PTY read/write loops that keep stream forwarding robust under partial escapes.

use crate::log_debug;
use anyhow::{anyhow, Result};
use crossbeam_channel::Sender;
use std::io::{self, ErrorKind};
use std::os::unix::io::RawFd;
use std::thread;
use std::time::Duration;
#[cfg(any(test, feature = "mutants"))]
use std::time::Instant;

#[cfg(any(test, feature = "mutants"))]
use super::counters::guard_loop;
use super::counters::write_all_limit;
use super::osc::{
    find_csi_sequence, find_osc_terminator, respond_to_terminal_queries,
    respond_to_terminal_queries_passthrough,
};

pub(super) fn should_retry_read_error(err: &io::Error) -> bool {
    err.kind() == ErrorKind::Interrupted || err.kind() == ErrorKind::WouldBlock
}

pub(super) fn split_incomplete_escape(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let esc_idx = buffer.iter().rposition(|b| *b == 0x1b)?;
    if esc_idx + 1 >= buffer.len() {
        return Some(buffer.split_off(esc_idx));
    }
    match buffer[esc_idx + 1] {
        b'[' => {
            if find_csi_sequence(buffer, esc_idx + 2).is_none() {
                return Some(buffer.split_off(esc_idx));
            }
        }
        b']' => {
            if find_osc_terminator(buffer, esc_idx + 2).is_none() {
                return Some(buffer.split_off(esc_idx));
            }
        }
        b'(' | b')' => {
            if esc_idx + 2 >= buffer.len() {
                return Some(buffer.split_off(esc_idx));
            }
        }
        _ => {}
    }
    None
}

/// Continuously read from the PTY and forward chunks to the main thread.
pub(super) fn spawn_reader_thread(master_fd: RawFd, tx: Sender<Vec<u8>>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut pending: Vec<u8> = Vec::new();
        #[cfg(any(test, feature = "mutants"))]
        let guard_start = Instant::now();
        #[cfg(any(test, feature = "mutants"))]
        let mut guard_iters: usize = 0;
        loop {
            #[cfg(any(test, feature = "mutants"))]
            {
                let prev = guard_iters;
                guard_iters += 1;
                assert!(guard_iters > prev);
                guard_loop(guard_start, guard_iters, 10_000, "spawn_reader_thread");
            }
            // SAFETY: master_fd is a valid PTY fd owned by this thread, and buffer is writable.
            let n = unsafe {
                libc::read(
                    master_fd,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if n > 0 {
                let mut data = if pending.is_empty() {
                    buffer.get(..n as usize).unwrap_or(&[]).to_vec()
                } else {
                    let mut merged = pending;
                    merged.extend_from_slice(buffer.get(..n as usize).unwrap_or(&[]));
                    pending = Vec::new();
                    merged
                };
                if let Some(tail) = split_incomplete_escape(&mut data) {
                    pending = tail;
                }
                // Answer simple terminal capability queries so the backend CLI doesn't hang waiting.
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
            if should_retry_read_error(&err) {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            log_debug(&format!("PTY read error: {err}"));
            break;
        }
    })
}

/// Continuously read from the PTY and forward raw chunks to the main thread.
pub(super) fn spawn_passthrough_reader_thread(
    master_fd: RawFd,
    tx: Sender<Vec<u8>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut pending: Vec<u8> = Vec::new();
        #[cfg(any(test, feature = "mutants"))]
        let guard_start = Instant::now();
        #[cfg(any(test, feature = "mutants"))]
        let mut guard_iters: usize = 0;
        loop {
            #[cfg(any(test, feature = "mutants"))]
            {
                let prev = guard_iters;
                guard_iters += 1;
                assert!(guard_iters > prev);
                guard_loop(
                    guard_start,
                    guard_iters,
                    10_000,
                    "spawn_passthrough_reader_thread",
                );
            }
            // SAFETY: master_fd is a valid PTY fd owned by this thread, and buffer is writable.
            let n = unsafe {
                libc::read(
                    master_fd,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };
            if n > 0 {
                let mut data = if pending.is_empty() {
                    buffer.get(..n as usize).unwrap_or(&[]).to_vec()
                } else {
                    let mut merged = pending;
                    merged.extend_from_slice(buffer.get(..n as usize).unwrap_or(&[]));
                    pending = Vec::new();
                    merged
                };
                if let Some(tail) = split_incomplete_escape(&mut data) {
                    pending = tail;
                }
                respond_to_terminal_queries_passthrough(&mut data, master_fd);
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
            if should_retry_read_error(&err) {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            log_debug(&format!("PTY read error: {err}"));
            break;
        }
    })
}

/// Attempt to write a single chunk to the PTY master without retry loops.
pub(super) fn try_write(fd: RawFd, data: &[u8]) -> io::Result<usize> {
    if data.is_empty() {
        return Ok(0);
    }
    let write_len = write_all_limit(data.len());
    // SAFETY: fd is the PTY master, data is a live slice, and write_len <= data.len().
    let written = unsafe { libc::write(fd, data.as_ptr() as *const libc::c_void, write_len) };
    if written < 0 {
        return Err(io::Error::last_os_error());
    }
    if written == 0 {
        return Err(io::Error::new(ErrorKind::WriteZero, "PTY write returned 0"));
    }
    Ok(written as usize)
}

/// Write the entire buffer to the PTY master, retrying short writes.
pub(super) fn write_all(fd: RawFd, mut data: &[u8]) -> Result<()> {
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while !data.is_empty() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(guard_start, guard_iters, 10_000, "write_all");
        }
        let written = match try_write(fd, data) {
            Ok(written) => written,
            Err(err) => {
                if err.kind() == ErrorKind::Interrupted || err.kind() == ErrorKind::WouldBlock {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                if err.kind() == ErrorKind::WriteZero {
                    return Err(anyhow!("PTY write returned 0"));
                }
                return Err(anyhow!("PTY write failed: {err}"));
            }
        };
        data = if written <= data.len() {
            &data[written..]
        } else {
            &[]
        };
    }
    Ok(())
}
