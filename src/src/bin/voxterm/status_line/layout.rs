//! Responsive status-line layout breakpoints so HUD content degrades gracefully.

use crate::config::HudStyle;

/// Terminal width breakpoints for responsive layout.
pub(super) mod breakpoints {
    /// Full layout with all sections
    pub const FULL: usize = 80;
    /// Medium layout - shorter shortcuts
    pub const MEDIUM: usize = 60;
    /// Compact layout - minimal left section
    pub const COMPACT: usize = 40;
    /// Minimal layout - message only
    pub const MINIMAL: usize = 25;
}

/// Return the number of rows used by the status banner for a given width and HUD style.
#[must_use]
pub fn status_banner_height(width: usize, hud_style: HudStyle) -> usize {
    match hud_style {
        HudStyle::Hidden => 1,  // Single-row launcher/indicator
        HudStyle::Minimal => 1, // Single line
        HudStyle::Full => {
            if width < breakpoints::COMPACT {
                1
            } else {
                4
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_banner_height_respects_hud_style() {
        // Full mode: 4 rows for wide terminals
        assert_eq!(status_banner_height(80, HudStyle::Full), 4);
        // Full mode: 1 row for narrow terminals
        assert_eq!(status_banner_height(30, HudStyle::Full), 1);

        // Minimal mode: always 1 row
        assert_eq!(status_banner_height(80, HudStyle::Minimal), 1);
        assert_eq!(status_banner_height(30, HudStyle::Minimal), 1);

        // Hidden mode: always 1 row (launcher when idle)
        assert_eq!(status_banner_height(80, HudStyle::Hidden), 1);
        assert_eq!(status_banner_height(30, HudStyle::Hidden), 1);
    }
}
