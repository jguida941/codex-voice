//! Status-line animation frames so recording/processing states feel alive.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HEARTBEAT_FRAMES: &[char] = &['·', '•', '●', '•'];
const TRANSITION_PULSE_MARKERS: &[&str] = &["✦", "•"];
#[allow(dead_code)]
const STATE_TRANSITION_DURATION: Duration = Duration::from_millis(360);

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

#[allow(dead_code)]
pub(crate) fn state_transition_progress(started_at: Option<Instant>, now: Instant) -> f32 {
    let Some(started_at) = started_at else {
        return 0.0;
    };
    let elapsed = now.saturating_duration_since(started_at);
    if elapsed >= STATE_TRANSITION_DURATION {
        return 0.0;
    }
    let t = elapsed.as_secs_f32() / STATE_TRANSITION_DURATION.as_secs_f32();
    // Ease-out for a quick initial pulse that decays smoothly.
    let eased = 1.0 - (1.0 - t).powf(3.0);
    (1.0 - eased).clamp(0.0, 1.0)
}

pub(super) fn transition_marker(progress: f32) -> &'static str {
    if progress <= 0.0 {
        ""
    } else if progress > 0.66 {
        TRANSITION_PULSE_MARKERS[0]
    } else if progress > 0.33 {
        TRANSITION_PULSE_MARKERS[1]
    } else {
        ""
    }
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

    #[test]
    fn transition_progress_is_bounded() {
        let now = Instant::now();
        let p_start = state_transition_progress(Some(now), now);
        let p_mid = state_transition_progress(Some(now), now + Duration::from_millis(160));
        let p_end = state_transition_progress(Some(now), now + Duration::from_millis(720));
        assert!((0.9..=1.0).contains(&p_start));
        assert!(p_mid > 0.0 && p_mid < 1.0);
        assert_eq!(p_end, 0.0);
    }

    #[test]
    fn transition_marker_steps_down() {
        assert_eq!(transition_marker(0.9), "✦");
        assert_eq!(transition_marker(0.5), "•");
        assert_eq!(transition_marker(0.1), "");
        assert_eq!(transition_marker(0.0), "");
    }
}
