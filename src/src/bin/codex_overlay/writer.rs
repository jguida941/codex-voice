use crossbeam_channel::{Receiver, Sender};
use crossterm::terminal::size as terminal_size;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};
use voxterm::log_debug;

use crate::status_line::{format_status_banner, StatusBanner, StatusLineState};
use crate::status_style::StatusType;
use crate::theme::Theme;

const SAVE_CURSOR: &[u8] = b"\x1b[s\x1b7";
const RESTORE_CURSOR: &[u8] = b"\x1b[u\x1b8";
const WRITER_RECV_TIMEOUT_MS: u64 = 25;

// SGR mouse mode escape sequences
// Enable basic mouse reporting + SGR extended coordinates
const MOUSE_ENABLE: &[u8] = b"\x1b[?1000h\x1b[?1006h";
// Disable mouse reporting
const MOUSE_DISABLE: &[u8] = b"\x1b[?1006l\x1b[?1000l";

#[derive(Debug, Clone)]
struct OverlayPanel {
    content: String,
    height: usize,
}

struct WriterState {
    stdout: io::Stdout,
    status: Option<String>,
    enhanced_status: Option<StatusLineState>,
    pending_status: Option<String>,
    pending_enhanced: Option<StatusLineState>,
    overlay_panel: Option<OverlayPanel>,
    pending_overlay: Option<OverlayPanel>,
    pending_overlay_clear: bool,
    pending_clear: bool,
    needs_redraw: bool,
    rows: u16,
    cols: u16,
    last_output_at: Instant,
    last_status_draw_at: Instant,
    theme: Theme,
    current_banner_height: usize,
    mouse_enabled: bool,
}

impl WriterState {
    fn new() -> Self {
        Self {
            stdout: io::stdout(),
            status: None,
            enhanced_status: None,
            pending_status: None,
            pending_enhanced: None,
            overlay_panel: None,
            pending_overlay: None,
            pending_overlay_clear: false,
            pending_clear: false,
            needs_redraw: false,
            rows: 0,
            cols: 0,
            last_output_at: Instant::now(),
            last_status_draw_at: Instant::now(),
            theme: Theme::default(),
            current_banner_height: 0,
            mouse_enabled: false,
        }
    }

    /// Enable SGR mouse tracking for clickable buttons.
    fn enable_mouse(&mut self) {
        if !self.mouse_enabled {
            if let Err(err) = self.stdout.write_all(MOUSE_ENABLE) {
                log_debug(&format!("mouse enable failed: {err}"));
            }
            let _ = self.stdout.flush();
            self.mouse_enabled = true;
        }
    }

    /// Disable mouse tracking.
    fn disable_mouse(&mut self) {
        if self.mouse_enabled {
            if let Err(err) = self.stdout.write_all(MOUSE_DISABLE) {
                log_debug(&format!("mouse disable failed: {err}"));
            }
            let _ = self.stdout.flush();
            self.mouse_enabled = false;
        }
    }

    fn handle_message(&mut self, message: WriterMessage) -> bool {
        match message {
            WriterMessage::PtyOutput(bytes) => {
                if let Err(err) = self.stdout.write_all(&bytes) {
                    log_debug(&format!("stdout write_all failed: {err}"));
                    return false;
                }
                self.last_output_at = Instant::now();
                if self.status.is_some()
                    || self.enhanced_status.is_some()
                    || self.overlay_panel.is_some()
                {
                    self.needs_redraw = true;
                }
                if let Err(err) = self.stdout.flush() {
                    log_debug(&format!("stdout flush failed: {err}"));
                }
            }
            WriterMessage::Status { text } => {
                self.pending_status = Some(text);
                self.pending_enhanced = None;
                self.pending_clear = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::EnhancedStatus(state) => {
                self.pending_enhanced = Some(state);
                self.pending_status = None;
                self.pending_clear = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ShowOverlay { content, height } => {
                self.pending_overlay = Some(OverlayPanel { content, height });
                self.pending_overlay_clear = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ClearOverlay => {
                self.pending_overlay = None;
                self.pending_overlay_clear = true;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ClearStatus => {
                self.pending_status = None;
                self.pending_enhanced = None;
                self.pending_clear = true;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::Bell { count } => {
                let sequence = vec![0x07; count.max(1) as usize];
                if let Err(err) = self.stdout.write_all(&sequence) {
                    log_debug(&format!("bell write failed: {err}"));
                }
                if let Err(err) = self.stdout.flush() {
                    log_debug(&format!("bell flush failed: {err}"));
                }
            }
            WriterMessage::Resize { rows, cols } => {
                self.rows = rows;
                self.cols = cols;
                if self.status.is_some()
                    || self.enhanced_status.is_some()
                    || self.pending_status.is_some()
                    || self.pending_enhanced.is_some()
                    || self.overlay_panel.is_some()
                    || self.pending_overlay.is_some()
                {
                    self.needs_redraw = true;
                }
                self.maybe_redraw_status();
            }
            WriterMessage::SetTheme(new_theme) => {
                self.theme = new_theme;
                if self.status.is_some()
                    || self.enhanced_status.is_some()
                    || self.overlay_panel.is_some()
                {
                    self.needs_redraw = true;
                }
            }
            WriterMessage::EnableMouse => {
                self.enable_mouse();
            }
            WriterMessage::DisableMouse => {
                self.disable_mouse();
            }
            WriterMessage::Shutdown => {
                // Disable mouse before exiting to restore terminal state
                self.disable_mouse();
                return false;
            }
        }
        true
    }

    fn maybe_redraw_status(&mut self) {
        const STATUS_IDLE_MS: u64 = 50;
        const STATUS_MAX_WAIT_MS: u64 = 500;
        if !self.needs_redraw {
            return;
        }
        let since_output = self.last_output_at.elapsed();
        let since_draw = self.last_status_draw_at.elapsed();
        if since_output < Duration::from_millis(STATUS_IDLE_MS)
            && since_draw < Duration::from_millis(STATUS_MAX_WAIT_MS)
        {
            return;
        }
        if self.rows == 0 || self.cols == 0 {
            if let Ok((c, r)) = terminal_size() {
                self.rows = r;
                self.cols = c;
            }
        }
        if self.pending_clear {
            let current_banner_height = self.current_banner_height;
            if current_banner_height > 1 {
                let _ = clear_status_banner(&mut self.stdout, self.rows, current_banner_height);
            } else {
                let _ = clear_status_line(&mut self.stdout, self.rows, self.cols);
            }
            self.status = None;
            self.enhanced_status = None;
            self.current_banner_height = 0;
            self.pending_clear = false;
        }
        if self.pending_overlay_clear {
            if let Some(panel) = self.overlay_panel.as_ref() {
                let _ = clear_overlay_panel(&mut self.stdout, self.rows, panel.height);
            }
            self.overlay_panel = None;
            self.pending_overlay_clear = false;
        }
        if let Some(panel) = self.pending_overlay.as_ref() {
            if let Some(current) = self.overlay_panel.as_ref() {
                if current.height != panel.height {
                    let _ = clear_overlay_panel(&mut self.stdout, self.rows, current.height);
                }
            }
        }
        if let Some(panel) = self.pending_overlay.take() {
            self.overlay_panel = Some(panel);
        }
        if let Some(state) = self.pending_enhanced.take() {
            self.enhanced_status = Some(state);
            self.status = None;
        }
        if let Some(text) = self.pending_status.take() {
            self.status = Some(text);
            self.enhanced_status = None;
        }

        let flush_error = {
            let rows = self.rows;
            let cols = self.cols;
            let theme = self.theme;
            let (stdout, overlay_panel, enhanced_status, status, current_banner_height) = (
                &mut self.stdout,
                &self.overlay_panel,
                &self.enhanced_status,
                &self.status,
                &mut self.current_banner_height,
            );
            if let Some(panel) = overlay_panel.as_ref() {
                let _ = write_overlay_panel(stdout, panel, rows);
            } else if let Some(state) = enhanced_status.as_ref() {
                let banner = format_status_banner(state, theme, cols as usize);
                let clear_height = (*current_banner_height).max(banner.height);
                if clear_height > 0 {
                    let _ = clear_status_banner(stdout, rows, clear_height);
                }
                *current_banner_height = banner.height;
                let _ = write_status_banner(stdout, &banner, rows);
            } else if let Some(text) = status.as_deref() {
                let _ = write_status_line(stdout, text, rows, cols, theme);
            }
            stdout.flush().err()
        };
        self.needs_redraw = false;
        self.last_status_draw_at = Instant::now();
        if let Some(err) = flush_error {
            log_debug(&format!("status redraw flush failed: {err}"));
        }
    }
}

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
        let mut state = WriterState::new();
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

fn write_status_line(
    stdout: &mut dyn Write,
    text: &str,
    rows: u16,
    cols: u16,
    theme: Theme,
) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let sanitized = sanitize_status(text);
    let status_type = StatusType::from_message(&sanitized);
    let display_width = status_type.prefix_display_width() + sanitized.chars().count();
    let prefix = status_type.prefix_with_theme(theme);
    let formatted = if display_width <= cols as usize {
        format!("{prefix}{sanitized}")
    } else {
        // Truncate the text portion, keeping room for the prefix
        let max_text_len = (cols as usize).saturating_sub(status_type.prefix_display_width());
        let truncated = truncate_status(&sanitized, max_text_len);
        format!("{prefix}{truncated}")
    };
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(formatted.as_bytes());
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

/// Write a multi-line status banner at the bottom of the terminal.
fn write_status_banner(stdout: &mut dyn Write, banner: &StatusBanner, rows: u16) -> io::Result<()> {
    if rows == 0 || banner.height == 0 {
        return Ok(());
    }
    let height = banner.height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR); // Save cursor

    for (idx, line) in banner.lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
        sequence.extend_from_slice(line.as_bytes()); // Write content
    }

    sequence.extend_from_slice(RESTORE_CURSOR); // Restore cursor
    stdout.write_all(&sequence)
}

/// Clear a multi-line status banner.
/// Also clears extra rows above to catch ghost content from terminal scrolling.
fn clear_status_banner(stdout: &mut dyn Write, rows: u16, height: usize) -> io::Result<()> {
    if rows == 0 || height == 0 {
        return Ok(());
    }
    // Only clear the banner rows to avoid erasing PTY content above the HUD.
    let clear_height = height.min(rows as usize);
    let start_row = rows.saturating_sub(clear_height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR); // Save cursor

    for idx in 0..clear_height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
    }

    sequence.extend_from_slice(RESTORE_CURSOR); // Restore cursor
    stdout.write_all(&sequence)
}

fn clear_status_line(stdout: &mut dyn Write, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

fn write_overlay_panel(stdout: &mut dyn Write, panel: &OverlayPanel, rows: u16) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    let lines: Vec<&str> = panel.content.lines().collect();
    let height = panel.height.min(lines.len()).min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    for (idx, line) in lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
        sequence.extend_from_slice(line.as_bytes());
    }
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

fn clear_overlay_panel(stdout: &mut dyn Write, rows: u16, height: usize) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    let height = height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(SAVE_CURSOR);
    for idx in 0..height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
    }
    sequence.extend_from_slice(RESTORE_CURSOR);
    stdout.write_all(&sequence)
}

fn sanitize_status(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_graphic() || ch == ' ' {
                ch
            } else {
                ' '
            }
        })
        .collect()
}

fn truncate_status(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn status_helpers_sanitize_and_truncate() {
        let sanitized = sanitize_status("ok\tbad\n");
        assert_eq!(sanitized, "ok bad ");
        assert_eq!(truncate_status("hello", 0), "");
        assert_eq!(truncate_status("hello", 2), "he");
    }

    #[test]
    fn write_and_clear_status_line_respect_dimensions() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        write_status_line(&mut buf, "hi", 0, 10, theme).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 0, theme).unwrap();
        assert!(buf.is_empty());

        write_status_line(&mut buf, "hi", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));
        assert!(output.contains("hi"));
        // Should contain color codes (info prefix for generic message)
        assert!(output.contains("\u{1b}[94m")); // Blue for info

        buf.clear();
        clear_status_line(&mut buf, 2, 10).unwrap();
        let output = String::from_utf8_lossy(&buf);
        assert!(output.contains("\u{1b}[2;1H"));

        buf.clear();
        clear_status_line(&mut buf, 2, 0).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn write_status_line_includes_colored_prefix() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        write_status_line(&mut buf, "Listening Manual Mode", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Recording status should have red prefix
        assert!(output.contains("\u{1b}[91m")); // Red
        assert!(output.contains("● REC"));

        buf.clear();
        write_status_line(&mut buf, "Processing...", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Processing status should have yellow prefix
        assert!(output.contains("\u{1b}[93m")); // Yellow
        assert!(output.contains("◐"));

        buf.clear();
        write_status_line(&mut buf, "Transcript ready", 2, 80, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Success status should have green prefix
        assert!(output.contains("\u{1b}[92m")); // Green
        assert!(output.contains("✓"));
    }

    #[test]
    fn write_status_line_truncation_preserves_status_type() {
        let theme = Theme::Coral;
        let mut buf = Vec::new();
        let long_msg = "Transcript ready (Rust pipeline with extra detail)";
        // Force truncation so the status keyword would be removed from the visible text.
        write_status_line(&mut buf, long_msg, 2, 12, theme).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Success status should still have green prefix even if text is truncated.
        assert!(output.contains("\u{1b}[92m")); // Green
        assert!(output.contains("✓"));
    }

    #[test]
    fn write_status_line_respects_no_color_theme() {
        let mut buf = Vec::new();
        write_status_line(&mut buf, "Processing...", 2, 80, Theme::None).unwrap();
        let output = String::from_utf8_lossy(&buf);
        // Should have the indicator but no escape codes for color
        assert!(output.contains("◐"));
        assert!(output.contains("Processing..."));
        // The only escape codes should be cursor positioning, not color
        let color_codes = output.matches("\u{1b}[9").count();
        assert_eq!(color_codes, 0, "Should not contain color codes");
    }

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
