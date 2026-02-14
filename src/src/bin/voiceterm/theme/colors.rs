//! Theme color tokens so rendering code references semantic colors, not raw escapes.

/// ANSI color codes for a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    /// Color for recording/active states
    pub recording: &'static str,
    /// Color for processing/working states
    pub processing: &'static str,
    /// Color for success states
    pub success: &'static str,
    /// Color for warning states
    pub warning: &'static str,
    /// Color for error states
    pub error: &'static str,
    /// Color for info states
    pub info: &'static str,
    /// Reset code
    pub reset: &'static str,
    /// Dim/muted text for secondary info
    pub dim: &'static str,
    /// Primary background color (for main status area)
    pub bg_primary: &'static str,
    /// Secondary background color (for shortcuts row)
    pub bg_secondary: &'static str,
    /// Border/frame color
    pub border: &'static str,
    /// Border character set
    pub borders: super::BorderSet,
    /// Mode indicator symbol
    pub indicator_rec: &'static str,
    pub indicator_auto: &'static str,
    pub indicator_manual: &'static str,
    pub indicator_idle: &'static str,
}
