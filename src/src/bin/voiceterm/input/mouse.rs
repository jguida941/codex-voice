//! Mouse-sequence parsing so clicks work across SGR, URXVT, and X10 protocols.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseEventKind {
    Press,
    Release,
}

#[inline]
fn decode_button_code(button: u16) -> u16 {
    if button >= 32 {
        button - 32
    } else {
        button
    }
}

#[inline]
fn map_left_click_kind(button: u16, explicit_release: bool) -> Option<MouseEventKind> {
    if explicit_release || button == 3 {
        if button == 0 || button == 3 {
            Some(MouseEventKind::Release)
        } else {
            None
        }
    } else if button == 0 {
        Some(MouseEventKind::Press)
    } else {
        None
    }
}

#[inline]
pub(crate) fn is_sgr_mouse_sequence(buffer: &[u8]) -> bool {
    if buffer.len() < 6 {
        return false;
    }
    if buffer[0] != 0x1b || buffer[1] != b'[' || buffer[2] != b'<' {
        return false;
    }
    matches!(buffer[buffer.len() - 1], b'M' | b'm')
}

#[inline]
pub(crate) fn is_urxvt_mouse_sequence(buffer: &[u8]) -> bool {
    if buffer.len() < 8 {
        return false;
    }
    if buffer[0] != 0x1b || buffer[1] != b'[' || buffer[2] == b'<' {
        return false;
    }
    matches!(buffer[buffer.len() - 1], b'M' | b'm')
}

#[inline]
pub(crate) fn is_x10_mouse_prefix(buffer: &[u8]) -> bool {
    buffer.len() >= 3 && buffer[0] == 0x1b && buffer[1] == b'[' && buffer[2] == b'M'
}

/// Parse SGR mouse event: ESC [ < button ; x ; y M (press) or m (release)
/// Only handles left-click press (button 0) and release (button 0 or 3).
#[inline]
pub(crate) fn parse_sgr_mouse(buffer: &[u8]) -> Option<(MouseEventKind, u16, u16)> {
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
    if final_char != b'M' && final_char != b'm' {
        return None;
    }
    // Parse: button ; x ; y
    let params = &buffer[3..buffer.len() - 1];
    let params_str = std::str::from_utf8(params).ok()?;
    let mut parts = params_str.split(';');

    let button = decode_button_code(parts.next()?.parse().ok()?);
    let x: u16 = parts.next()?.parse().ok()?;
    let y: u16 = parts.next()?.parse().ok()?;
    let explicit_release = final_char == b'm';
    let kind = map_left_click_kind(button, explicit_release)?;

    Some((kind, x, y))
}

/// Parse URXVT/1015 mouse event: ESC [ Cb ; Cx ; Cy M
/// Also accepts lowercase `m` as explicit release.
#[inline]
pub(crate) fn parse_urxvt_mouse(buffer: &[u8]) -> Option<(MouseEventKind, u16, u16)> {
    if !is_urxvt_mouse_sequence(buffer) {
        return None;
    }
    let final_char = buffer[buffer.len() - 1];
    let params = &buffer[2..buffer.len() - 1];
    let params_str = std::str::from_utf8(params).ok()?;
    let mut parts = params_str.split(';');
    let button = decode_button_code(parts.next()?.parse().ok()?);
    let x: u16 = parts.next()?.parse().ok()?;
    let y: u16 = parts.next()?.parse().ok()?;
    let explicit_release = final_char == b'm';
    let kind = map_left_click_kind(button, explicit_release)?;
    Some((kind, x, y))
}

/// Parse X10 mouse event: ESC [ M Cb Cx Cy
/// Coordinates and button values are encoded as value+32.
#[inline]
pub(crate) fn parse_x10_mouse(buffer: &[u8]) -> Option<(MouseEventKind, u16, u16)> {
    if buffer.len() != 6 || !is_x10_mouse_prefix(buffer) {
        return None;
    }
    let button = (*buffer.get(3)?).checked_sub(32)? as u16;
    let x = (*buffer.get(4)?).checked_sub(32)? as u16;
    let y = (*buffer.get(5)?).checked_sub(32)? as u16;
    let kind = map_left_click_kind(button & 0b11, false)?;
    Some((kind, x, y))
}

#[inline]
pub(crate) fn parse_mouse_event(buffer: &[u8]) -> Option<(MouseEventKind, u16, u16)> {
    parse_sgr_mouse(buffer)
        .or_else(|| parse_urxvt_mouse(buffer))
        .or_else(|| parse_x10_mouse(buffer))
}

#[inline]
pub(crate) fn is_mouse_sequence(buffer: &[u8]) -> bool {
    is_sgr_mouse_sequence(buffer) || is_urxvt_mouse_sequence(buffer) || is_x10_mouse_prefix(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sgr_mouse_left_click() {
        let press = parse_sgr_mouse(b"\x1b[<0;10;5M");
        assert_eq!(press, Some((MouseEventKind::Press, 10, 5)));
        let release = parse_sgr_mouse(b"\x1b[<0;10;5m");
        assert_eq!(release, Some((MouseEventKind::Release, 10, 5)));
    }

    #[test]
    fn parse_urxvt_mouse_left_click() {
        let press = parse_urxvt_mouse(b"\x1b[32;10;5M");
        assert_eq!(press, Some((MouseEventKind::Press, 10, 5)));
        let release = parse_urxvt_mouse(b"\x1b[35;10;5M");
        assert_eq!(release, Some((MouseEventKind::Release, 10, 5)));
    }

    #[test]
    fn parse_x10_mouse_left_click() {
        let press = parse_x10_mouse(&[0x1b, b'[', b'M', 32, 42, 37]);
        assert_eq!(press, Some((MouseEventKind::Press, 10, 5)));
        let release = parse_x10_mouse(&[0x1b, b'[', b'M', 35, 42, 37]);
        assert_eq!(release, Some((MouseEventKind::Release, 10, 5)));
    }

    #[test]
    fn parse_mouse_event_accepts_all_supported_protocols() {
        assert!(parse_mouse_event(b"\x1b[<0;10;5M").is_some());
        assert!(parse_mouse_event(b"\x1b[32;10;5M").is_some());
        assert!(parse_mouse_event(&[0x1b, b'[', b'M', 32, 40, 35]).is_some());
    }
}
