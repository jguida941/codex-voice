use crossbeam_channel::Sender;
use std::io::{self, Read};
use std::thread;
use voxterm::log_debug;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputEvent {
    Bytes(Vec<u8>),
    VoiceTrigger,
    ToggleAutoVoice,
    ToggleSendMode,
    IncreaseSensitivity,
    DecreaseSensitivity,
    HelpToggle,
    ThemePicker,
    SettingsToggle,
    ToggleHudStyle,
    EnterKey,
    Exit,
    /// Mouse click at (x, y) coordinates (1-based, like terminal reports)
    MouseClick {
        x: u16,
        y: u16,
    },
}

pub(crate) fn spawn_input_thread(tx: Sender<InputEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 1024];
        let mut parser = InputParser::new();
        loop {
            let n = match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    log_debug(&format!("stdin read error: {err}"));
                    break;
                }
            };
            let mut events = Vec::new();
            parser.consume_bytes(&buf[..n], &mut events);
            parser.flush_pending(&mut events);
            for event in events {
                if tx.send(event).is_err() {
                    return;
                }
            }
        }
    })
}

struct InputParser {
    pending: Vec<u8>,
    skip_lf: bool,
    esc_buffer: Option<Vec<u8>>,
    mouse_press_seen: bool,
}

impl InputParser {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            skip_lf: false,
            esc_buffer: None,
            mouse_press_seen: false,
        }
    }

    fn consume_bytes(&mut self, bytes: &[u8], out: &mut Vec<InputEvent>) {
        for &byte in bytes {
            if self.consume_escape(byte, out) {
                continue;
            }
            if self.skip_lf {
                if byte == 0x0a {
                    self.skip_lf = false;
                    continue;
                }
                self.skip_lf = false;
            }

            match byte {
                0x11 => {
                    self.flush_pending(out);
                    out.push(InputEvent::Exit);
                }
                0x12 => {
                    self.flush_pending(out);
                    out.push(InputEvent::VoiceTrigger);
                }
                0x16 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ToggleAutoVoice);
                }
                0x14 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ToggleSendMode);
                }
                0x1d => {
                    self.flush_pending(out);
                    out.push(InputEvent::IncreaseSensitivity);
                }
                0x1c => {
                    self.flush_pending(out);
                    out.push(InputEvent::DecreaseSensitivity);
                }
                0x1f => {
                    self.flush_pending(out);
                    out.push(InputEvent::DecreaseSensitivity);
                }
                0x19 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ThemePicker);
                }
                0x0f => {
                    self.flush_pending(out);
                    out.push(InputEvent::SettingsToggle);
                }
                0x15 => {
                    self.flush_pending(out);
                    out.push(InputEvent::ToggleHudStyle);
                }
                b'?' => {
                    self.flush_pending(out);
                    out.push(InputEvent::HelpToggle);
                }
                0x0d | 0x0a => {
                    self.flush_pending(out);
                    out.push(InputEvent::EnterKey);
                    if byte == 0x0d {
                        self.skip_lf = true;
                    }
                }
                _ => self.pending.push(byte),
            }
        }
    }

    fn consume_escape(&mut self, byte: u8, out: &mut Vec<InputEvent>) -> bool {
        const MAX_CSI_LEN: usize = 32;

        if let Some(ref mut buffer) = self.esc_buffer {
            buffer.push(byte);
            if buffer.len() == 2 && buffer[1] != b'[' {
                self.pending.extend_from_slice(buffer);
                self.esc_buffer = None;
                return true;
            }

            if buffer.len() >= 2 && buffer[1] == b'[' {
                if buffer.len() >= 3 && is_csi_final(byte) {
                    let (is_csi_u, event) = {
                        let is_csi_u = is_csi_u_numeric(buffer);
                        let event = if is_csi_u {
                            parse_csi_u_event(buffer)
                        } else {
                            None
                        };
                        (is_csi_u, event)
                    };
                    if is_csi_u {
                        self.esc_buffer = None;
                        if let Some(event) = event {
                            self.flush_pending(out);
                            out.push(event);
                        }
                    } else if let Some((kind, x, y)) = parse_sgr_mouse(buffer) {
                        // SGR mouse click: ESC [ < btn ; x ; y M (press) or m (release)
                        self.esc_buffer = None;
                        self.flush_pending(out);
                        match kind {
                            MouseEventKind::Press => {
                                self.mouse_press_seen = true;
                                out.push(InputEvent::MouseClick { x, y });
                            }
                            MouseEventKind::Release => {
                                if self.mouse_press_seen {
                                    self.mouse_press_seen = false;
                                } else {
                                    out.push(InputEvent::MouseClick { x, y });
                                }
                            }
                        }
                    } else {
                        self.pending.extend_from_slice(buffer);
                        self.esc_buffer = None;
                    }
                    return true;
                }
                if buffer.len() > MAX_CSI_LEN {
                    self.pending.extend_from_slice(buffer);
                    self.esc_buffer = None;
                    return true;
                }
                return true;
            }

            if buffer.len() > MAX_CSI_LEN {
                self.pending.extend_from_slice(buffer);
                self.esc_buffer = None;
            }
            return true;
        }

        if byte == 0x1b {
            self.esc_buffer = Some(vec![byte]);
            return true;
        }
        false
    }

    fn flush_pending(&mut self, out: &mut Vec<InputEvent>) {
        if let Some(buffer) = self.esc_buffer.take() {
            self.pending.extend_from_slice(&buffer);
        }
        if !self.pending.is_empty() {
            out.push(InputEvent::Bytes(std::mem::take(&mut self.pending)));
        }
    }
}

/// Check if byte is a CSI final character (0x40-0x7e).
#[inline]
fn is_csi_final(byte: u8) -> bool {
    (0x40..=0x7e).contains(&byte)
}

/// Check if buffer is a valid CSI-u numeric sequence (e.g., ESC [ 114 ; 5 u).
/// Requires at least 4 bytes: ESC, '[', at least one digit, and 'u'.
#[inline]
fn is_csi_u_numeric(buffer: &[u8]) -> bool {
    // Minimum valid: ESC [ <digit> u = 4 bytes
    if buffer.len() < 4 {
        return false;
    }
    // Safe indexing: we've verified length >= 4
    if buffer[0] != 0x1b || buffer[1] != b'[' || buffer[buffer.len() - 1] != b'u' {
        return false;
    }
    buffer[2..buffer.len() - 1]
        .iter()
        .all(|b| b.is_ascii_digit() || *b == b';')
}

/// Parse a CSI-u keyboard event (e.g., ESC [ 114 ; 5 u for Ctrl+R).
#[inline]
fn parse_csi_u_event(buffer: &[u8]) -> Option<InputEvent> {
    // Minimum valid: ESC [ <digit> u = 4 bytes
    if buffer.len() < 4
        || buffer[0] != 0x1b
        || buffer[1] != b'['
        || buffer[buffer.len() - 1] != b'u'
    {
        return None;
    }
    let params = &buffer[2..buffer.len() - 1];
    let mut parts = params.split(|b| *b == b';');
    let code = parts.next().and_then(parse_csi_u_number)?;
    let modifiers = parts.next().and_then(parse_csi_u_number).unwrap_or(0);

    // Kitty/CSI-u modifier mask uses bit 2^2 (4) for Ctrl.
    if modifiers & 4 == 0 {
        return None;
    }

    let ch = std::char::from_u32(code)?;
    let key = ch.to_ascii_lowercase();
    match key {
        'r' => Some(InputEvent::VoiceTrigger),
        'v' => Some(InputEvent::ToggleAutoVoice),
        't' => Some(InputEvent::ToggleSendMode),
        'y' => Some(InputEvent::ThemePicker),
        'o' => Some(InputEvent::SettingsToggle),
        'u' => Some(InputEvent::ToggleHudStyle),
        '?' => Some(InputEvent::HelpToggle),
        'q' => Some(InputEvent::Exit),
        _ => None,
    }
}

fn parse_csi_u_number(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: u32 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value.saturating_mul(10).saturating_add((b - b'0') as u32);
    }
    Some(value)
}

enum MouseEventKind {
    Press,
    Release,
}

/// Parse SGR mouse event: ESC [ < button ; x ; y M (press) or m (release)
/// Only handles left-click press (button 0) and release (button 0 or 3).
#[inline]
fn parse_sgr_mouse(buffer: &[u8]) -> Option<(MouseEventKind, u16, u16)> {
    // Minimum: ESC [ < 0 ; 1 ; 1 M = 10 bytes
    if buffer.len() < 10 {
        return None;
    }
    // Check prefix: ESC [ <
    if buffer[0] != 0x1b || buffer[1] != b'[' || buffer[2] != b'<' {
        return None;
    }
    // Check final character is 'M' (press) or 'm' (release)
    let final_char = buffer[buffer.len() - 1];
    let kind = match final_char {
        b'M' => MouseEventKind::Press,
        b'm' => MouseEventKind::Release,
        _ => return None,
    };
    // Parse: button ; x ; y
    let params = &buffer[3..buffer.len() - 1];
    let params_str = std::str::from_utf8(params).ok()?;
    let mut parts = params_str.split(';');

    let button: u16 = parts.next()?.parse().ok()?;
    let x: u16 = parts.next()?.parse().ok()?;
    let y: u16 = parts.next()?.parse().ok()?;

    // Only handle left-click press (button 0) or release (button 0 or 3).
    match kind {
        MouseEventKind::Press => {
            if button != 0 {
                return None;
            }
        }
        MouseEventKind::Release => {
            if button != 0 && button != 3 {
                return None;
            }
        }
    }

    Some((kind, x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_parser_emits_bytes_and_controls() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"hi\x12there", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], InputEvent::Bytes(b"hi".to_vec()));
        assert_eq!(out[1], InputEvent::VoiceTrigger);
        assert_eq!(out[2], InputEvent::Bytes(b"there".to_vec()));
    }

    #[test]
    fn input_parser_maps_control_keys() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(&[0x11, 0x16, 0x14, 0x1d, 0x1c, 0x1f, 0x0f], &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(
            out,
            vec![
                InputEvent::Exit,
                InputEvent::ToggleAutoVoice,
                InputEvent::ToggleSendMode,
                InputEvent::IncreaseSensitivity,
                InputEvent::DecreaseSensitivity,
                InputEvent::DecreaseSensitivity,
                InputEvent::SettingsToggle,
            ]
        );
    }

    #[test]
    fn input_parser_maps_theme_picker() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(&[0x19], &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out, vec![InputEvent::ThemePicker]);
    }

    #[test]
    fn input_parser_maps_help_toggle() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"?", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out, vec![InputEvent::HelpToggle]);
    }

    #[test]
    fn input_parser_skips_lf_after_cr() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"a\r\n", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(
            out,
            vec![InputEvent::Bytes(b"a".to_vec()), InputEvent::EnterKey]
        );
    }

    #[test]
    fn input_parser_keeps_non_lf_after_cr() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"a\rb", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(
            out,
            vec![
                InputEvent::Bytes(b"a".to_vec()),
                InputEvent::EnterKey,
                InputEvent::Bytes(b"b".to_vec())
            ]
        );
    }

    #[test]
    fn input_parser_drops_csi_u_sequences() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"\x1b[48;0;0u", &mut out);
        parser.flush_pending(&mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn input_parser_preserves_arrow_sequences() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"\x1b[A", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], InputEvent::Bytes(b"\x1b[A".to_vec()));
    }

    #[test]
    fn input_parser_maps_csi_u_ctrl_sequences() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        // Ctrl+R (kitty/CSI-u: ESC [ 114 ; 5 u)
        parser.consume_bytes(b"\x1b[114;5u", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out, vec![InputEvent::VoiceTrigger]);
    }

    #[test]
    fn input_parser_mouse_release_without_press_emits_click() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"\x1b[<0;10;5m", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out, vec![InputEvent::MouseClick { x: 10, y: 5 }]);
    }

    #[test]
    fn input_parser_mouse_press_then_release_emits_single_click() {
        let mut parser = InputParser::new();
        let mut out = Vec::new();
        parser.consume_bytes(b"\x1b[<0;10;5M\x1b[<0;10;5m", &mut out);
        parser.flush_pending(&mut out);
        assert_eq!(out, vec![InputEvent::MouseClick { x: 10, y: 5 }]);
    }
}
