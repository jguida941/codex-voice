//! Status-line animation frames so recording/processing states feel alive.

use std::time::{SystemTime, UNIX_EPOCH};

const HEARTBEAT_FRAMES: &[char] = &['·', '•', '●', '•'];

/// Pulsing recording indicator frames (cycles every ~400ms at 10fps).
const RECORDING_PULSE_FRAMES: &[&str] = &["●", "◉", "●", "○"];

/// Processing spinner frames (braille dots for smooth animation).
const PROCESSING_SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Get the current animation frame based on system time.
/// Returns a frame index that cycles through the given frame count.
#[inline]
fn get_animation_frame(frame_count: usize, cycle_ms: u64) -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    ((now / cycle_ms) % frame_count as u64) as usize
}

/// Get the pulsing recording indicator.
#[inline]
pub(super) fn get_recording_indicator() -> &'static str {
    let frame = get_animation_frame(RECORDING_PULSE_FRAMES.len(), 250);
    RECORDING_PULSE_FRAMES[frame]
}

/// Get the processing spinner character.
#[inline]
pub(super) fn get_processing_spinner() -> &'static str {
    let frame = get_animation_frame(PROCESSING_SPINNER_FRAMES.len(), 100);
    PROCESSING_SPINNER_FRAMES[frame]
}

#[inline]
pub(super) fn heartbeat_frame_index() -> usize {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (now.as_secs() % HEARTBEAT_FRAMES.len() as u64) as usize
}

pub(super) fn heartbeat_glyph(animate: bool) -> (char, bool) {
    let frame_idx = if animate { heartbeat_frame_index() } else { 0 };
    let glyph = HEARTBEAT_FRAMES.get(frame_idx).copied().unwrap_or('·');
    (glyph, frame_idx == 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_indicator_in_range() {
        let indicator = get_recording_indicator();
        assert!(RECORDING_PULSE_FRAMES.contains(&indicator));
    }

    #[test]
    fn processing_spinner_in_range() {
        let indicator = get_processing_spinner();
        assert!(PROCESSING_SPINNER_FRAMES.contains(&indicator));
    }

    #[test]
    fn heartbeat_frame_index_in_range() {
        let idx = heartbeat_frame_index();
        assert!(idx < HEARTBEAT_FRAMES.len());
    }
}
