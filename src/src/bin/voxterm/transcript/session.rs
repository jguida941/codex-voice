//! Transcript session abstraction so delivery logic is decoupled from PTY transport.

use anyhow::Result;
use voxterm::pty_session::PtyOverlaySession;

/// Abstraction over a destination that can accept transcript text.
pub(crate) trait TranscriptSession {
    /// Send text without a trailing newline (insert mode).
    fn send_text(&mut self, text: &str) -> Result<()>;
    /// Send text and append a newline (auto-send mode).
    fn send_text_with_newline(&mut self, text: &str) -> Result<()>;
}

impl TranscriptSession for PtyOverlaySession {
    fn send_text(&mut self, text: &str) -> Result<()> {
        self.send_text(text)
    }

    fn send_text_with_newline(&mut self, text: &str) -> Result<()> {
        self.send_text_with_newline(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::Receiver;
    use std::time::{Duration, Instant};

    fn recv_output_contains(rx: &Receiver<Vec<u8>>, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut buffer = String::new();
        while Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
                buffer.push_str(&String::from_utf8_lossy(&chunk));
                if buffer.contains(needle) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn transcript_session_impl_sends_text() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text(&mut session, "ping\n").expect("send text");
        assert!(recv_output_contains(&session.output_rx, "ping"));
    }

    #[test]
    fn transcript_session_impl_sends_text_with_newline() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        TranscriptSession::send_text_with_newline(&mut session, "pong")
            .expect("send text with newline");
        assert!(recv_output_contains(&session.output_rx, "pong"));
    }
}
