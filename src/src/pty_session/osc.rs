//! Terminal query interception so PTY control probes do not leak into UI output.

use crate::log_debug;
use std::mem;
use std::os::unix::io::RawFd;
#[cfg(any(test, feature = "mutants"))]
use std::time::Instant;

use super::counters::terminal_size_override;
#[cfg(any(test, feature = "mutants"))]
use super::counters::{
    guard_loop, record_apply_linestart_recalc, record_apply_osc_start, record_respond_osc_start,
};
use super::io::write_all;

/// Backend CLIs occasionally probe terminal capabilities; strip those control sequences
/// and apply inline edits (CR/BS) before rendering so the UI only sees printable text.
pub(super) fn respond_to_terminal_queries(buffer: &mut Vec<u8>, master_fd: RawFd) {
    let (rows, cols) = current_terminal_size(master_fd);
    let mut idx = 0;
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while idx < buffer.len() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(
                guard_start,
                guard_iters,
                buffer.len().saturating_mul(4).max(64),
                "respond_to_terminal_queries",
            );
        }
        if buffer[idx] != 0x1B {
            idx += 1;
            continue;
        }
        if idx + 1 >= buffer.len() {
            break;
        }
        match buffer[idx + 1] {
            b'[' => match find_csi_sequence(buffer, idx + 2) {
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
            },
            b']' => {
                #[cfg(any(test, feature = "mutants"))]
                record_respond_osc_start(idx + 2);
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

/// Answer terminal queries but keep all other ANSI sequences intact.
pub(super) fn respond_to_terminal_queries_passthrough(buffer: &mut Vec<u8>, master_fd: RawFd) {
    let (rows, cols) = current_terminal_size(master_fd);
    let mut idx = 0;
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while idx < buffer.len() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(
                guard_start,
                guard_iters,
                buffer.len().saturating_mul(4).max(64),
                "respond_to_terminal_queries_passthrough",
            );
        }
        if buffer[idx] != 0x1B {
            idx += 1;
            continue;
        }
        if idx + 1 >= buffer.len() {
            break;
        }
        if buffer[idx + 1] == b'[' {
            match find_csi_sequence(buffer, idx + 2) {
                Some((params_end, final_byte)) => {
                    let seq_end = params_end + 1;
                    let params_start = idx + 2;
                    let params: Vec<u8> =
                        buffer.get(params_start..params_end).unwrap_or(&[]).to_vec();
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
                None => break,
            }
        }
        idx += 1;
    }
}

pub(super) fn should_strip_without_reply(params: &[u8], final_byte: u8) -> bool {
    // Strip keyboard protocol queries (Kitty keyboard protocol)
    if final_byte == b'u' && (params.starts_with(b"?") || params.starts_with(b">")) {
        return true;
    }
    // Strip mode set/reset sequences (h/l) - these don't need responses
    // Examples: ?2004h (bracketed paste), ?1004h (focus events), ?25h (cursor visible)
    if (final_byte == b'h' || final_byte == b'l') && params.starts_with(b"?") {
        return true;
    }
    // Strip cursor movement and styling sequences
    if matches!(
        final_byte,
        b'A' | b'B' | b'C' | b'D' | b'H' | b'J' | b'K' | b'm' | b'r'
    ) {
        return true;
    }
    false
}

pub(super) fn csi_reply(params: &[u8], final_b: u8, rows: u16, cols: u16) -> Option<Vec<u8>> {
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

pub(super) fn current_terminal_size(master_fd: RawFd) -> (u16, u16) {
    if let Some((ok, row, col)) = terminal_size_override() {
        return if ok && row > 0 && col > 0 {
            (row, col)
        } else {
            (24, 80)
        };
    }
    // SAFETY: libc::winsize is a plain C struct; zeroed is a valid baseline.
    let mut ws: libc::winsize = unsafe { mem::zeroed() };
    unsafe {
        // SAFETY: ioctl reads from master_fd and writes into ws, which is initialized.
        if libc::ioctl(master_fd, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 && ws.ws_col > 0
        {
            (ws.ws_row, ws.ws_col)
        } else {
            (24, 80)
        }
    }
}

pub(super) fn find_csi_sequence(bytes: &[u8], start: usize) -> Option<(usize, u8)> {
    let mut idx = start;
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while idx < bytes.len() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(
                guard_start,
                guard_iters,
                bytes.len().saturating_add(16),
                "find_csi_sequence",
            );
        }
        let byte = bytes[idx];
        if (0x40..=0x7E).contains(&byte) {
            return Some((idx, byte));
        }
        idx += 1;
    }
    None
}

pub(super) fn find_osc_terminator(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;
    while cursor < bytes.len() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(
                guard_start,
                guard_iters,
                bytes.len().saturating_add(16),
                "find_osc_terminator",
            );
        }
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

pub(super) fn apply_control_edits(buffer: &mut Vec<u8>) {
    let mut output = Vec::with_capacity(buffer.len());
    let mut line_start = 0usize;
    let mut idx = 0;
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    #[cfg(any(test, feature = "mutants"))]
    let mut guard_iters: usize = 0;

    while idx < buffer.len() {
        #[cfg(any(test, feature = "mutants"))]
        {
            let prev = guard_iters;
            guard_iters += 1;
            assert!(guard_iters > prev);
            guard_loop(
                guard_start,
                guard_iters,
                buffer.len().saturating_mul(4).max(64),
                "apply_control_edits",
            );
        }
        match buffer[idx] {
            b'\r' => {
                if buffer.get(idx + 1) == Some(&b'\n') {
                    output.push(b'\n');
                    idx += 2;
                    line_start = output.len();
                    continue;
                }
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
                    #[cfg(any(test, feature = "mutants"))]
                    record_apply_linestart_recalc();
                    line_start = current_line_start(&output);
                }
            }
            0x1B => {
                if idx + 1 < buffer.len() && buffer[idx + 1] == b']' {
                    #[cfg(any(test, feature = "mutants"))]
                    record_apply_osc_start(idx + 2);
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

pub(super) fn pop_last_codepoint(buf: &mut Vec<u8>) {
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

pub(super) fn current_line_start(buf: &[u8]) -> usize {
    buf.iter()
        .rposition(|&b| b == b'\n')
        .map(|pos| pos + 1)
        .unwrap_or(0)
}
