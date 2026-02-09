#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseEventKind {
    Press,
    Release,
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
