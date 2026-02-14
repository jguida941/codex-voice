//! Writer message protocol and bootstrap so terminal output stays serialized.

mod mouse;
mod render;
mod sanitize;
mod state;

use crossbeam_channel::{Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::status_line::StatusLineState;
use crate::theme::Theme;

const WRITER_RECV_TIMEOUT_MS: u64 = 25;

#[derive(Debug, Clone)]
pub(crate) enum WriterMessage {
    PtyOutput(Vec<u8>),
    /// Simple status message (legacy format with auto-styled prefix)
    #[allow(dead_code)]
    Status {
        text: String,
    },
    /// Enhanced status line with full state
    EnhancedStatus(StatusLineState),
    /// Overlay panel content (multi-line box)
    ShowOverlay {
        content: String,
        height: usize,
    },
    /// Clear overlay panel
    ClearOverlay,
    ClearStatus,
    /// Emit terminal bell sound (optional)
    Bell {
        count: u8,
    },
    Resize {
        rows: u16,
        cols: u16,
    },
    SetTheme(Theme),
    /// Enable mouse tracking for clickable HUD buttons
    EnableMouse,
    /// Disable mouse tracking
    DisableMouse,
    Shutdown,
}

pub(crate) fn spawn_writer_thread(rx: Receiver<WriterMessage>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut state = state::WriterState::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(WRITER_RECV_TIMEOUT_MS)) {
                Ok(message) => {
                    if !state.handle_message(message) {
                        break;
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    state.maybe_redraw_status();
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    })
}

pub(crate) fn set_status(
    writer_tx: &Sender<WriterMessage>,
    clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    text: &str,
    clear_after: Option<Duration>,
) {
    let same_text = current_status.as_deref() == Some(text);
    status_state.message = text.to_string();
    if !same_text {
        *current_status = Some(status_state.message.clone());
    }
    let _ = writer_tx.send(WriterMessage::EnhancedStatus(status_state.clone()));
    *clear_deadline = clear_after.map(|duration| Instant::now() + duration);
}

pub(crate) fn send_enhanced_status(
    writer_tx: &Sender<WriterMessage>,
    status_state: &StatusLineState,
) {
    let _ = writer_tx.send(WriterMessage::EnhancedStatus(status_state.clone()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn set_status_updates_deadline() {
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let now = Instant::now();
        set_status(
            &tx,
            &mut deadline,
            &mut current_status,
            &mut status_state,
            "status",
            Some(Duration::from_millis(50)),
        );
        let msg = rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message");
        match msg {
            WriterMessage::EnhancedStatus(state) => assert_eq!(state.message, "status"),
            _ => panic!("unexpected writer message"),
        }
        assert!(deadline.expect("deadline set") > now);

        set_status(
            &tx,
            &mut deadline,
            &mut current_status,
            &mut status_state,
            "steady",
            None,
        );
        assert!(deadline.is_none());
    }
}
