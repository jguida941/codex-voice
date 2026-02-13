//! Mouse-tracking escape-sequence controls so HUD click handling is explicit.

use std::io::Write;
use voxterm::log_debug;

// SGR mouse mode escape sequences
// Enable basic mouse reporting + SGR extended coordinates
const MOUSE_ENABLE: &[u8] = b"\x1b[?1000h\x1b[?1006h";
// Disable mouse reporting
const MOUSE_DISABLE: &[u8] = b"\x1b[?1006l\x1b[?1000l";

/// Enable SGR mouse tracking for clickable buttons.
pub(super) fn enable_mouse(stdout: &mut dyn Write, mouse_enabled: &mut bool) {
    if !*mouse_enabled {
        if let Err(err) = stdout.write_all(MOUSE_ENABLE) {
            log_debug(&format!("mouse enable failed: {err}"));
        }
        let _ = stdout.flush();
        *mouse_enabled = true;
    }
}

/// Disable mouse tracking.
pub(super) fn disable_mouse(stdout: &mut dyn Write, mouse_enabled: &mut bool) {
    if *mouse_enabled {
        if let Err(err) = stdout.write_all(MOUSE_DISABLE) {
            log_debug(&format!("mouse disable failed: {err}"));
        }
        let _ = stdout.flush();
        *mouse_enabled = false;
    }
}
