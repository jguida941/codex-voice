//! Input-thread bootstrap so stdin capture stays isolated from render/event logic.

use crossbeam_channel::Sender;
use std::io::{self, Read};
use std::thread;
use voiceterm::log_debug;

use crate::arrow_keys::is_arrow_escape_noise;
use crate::input::event::InputEvent;
use crate::input::parser::InputParser;

const INPUT_DEBUG_ENV: &str = "VOICETERM_DEBUG_INPUT";
const INPUT_DEBUG_MAX_BYTES: usize = 64;
fn input_debug_enabled() -> bool {
    std::env::var(INPUT_DEBUG_ENV).is_ok()
}

fn format_debug_bytes(bytes: &[u8]) -> String {
    let sample_len = bytes.len().min(INPUT_DEBUG_MAX_BYTES);
    let mut out = String::new();
    for (idx, byte) in bytes.iter().take(sample_len).enumerate() {
        if idx > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    if bytes.len() > sample_len {
        out.push_str(" ...");
    }
    out
}

pub(crate) fn spawn_input_thread(tx: Sender<InputEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1024];
        let mut parser = InputParser::new();
        let debug_input = input_debug_enabled();
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    log_debug(&format!("stdin read error: {err}"));
                    break;
                }
            };
            if debug_input {
                log_debug(&format!(
                    "input bytes ({}): {}",
                    n,
                    format_debug_bytes(&buf[..n])
                ));
            }
            let mut events = Vec::new();
            parser.consume_bytes(&buf[..n], &mut events);
            parser.flush_pending(&mut events);
            if debug_input && !events.is_empty() {
                log_debug(&format!("input events: {events:?}"));
            }
            for event in events {
                if debug_input {
                    if let InputEvent::Bytes(bytes) = &event {
                        if is_arrow_escape_noise(bytes) {
                            log_debug(&format!(
                                "startup escape candidate: {}",
                                format_debug_bytes(bytes)
                            ));
                        }
                    }
                }
                if tx.send(event).is_err() {
                    return;
                }
            }
        }
    })
}
