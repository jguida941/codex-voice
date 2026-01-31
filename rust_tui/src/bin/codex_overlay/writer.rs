use crossbeam_channel::{Receiver, Sender};
use crossterm::terminal::size as terminal_size;
use std::io::{self, Write};
use std::thread;
use std::time::{Duration, Instant};

use crate::status_line::{format_status_banner, StatusBanner, StatusLineState};
use crate::status_style::StatusType;
use crate::theme::Theme;

#[derive(Debug, Clone)]
struct OverlayPanel {
    content: String,
    height: usize,
}

#[derive(Debug, Clone)]
pub(crate) enum WriterMessage {
    PtyOutput(Vec<u8>),
    /// Simple status message (legacy format with auto-styled prefix)
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
    Shutdown,
}

pub(crate) fn spawn_writer_thread(rx: Receiver<WriterMessage>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut stdout = io::stdout();
        let mut status: Option<String> = None;
        let mut enhanced_status: Option<StatusLineState> = None;
        let mut pending_status: Option<String> = None;
        let mut pending_enhanced: Option<StatusLineState> = None;
        let mut overlay_panel: Option<OverlayPanel> = None;
        let mut pending_overlay: Option<OverlayPanel> = None;
        let mut pending_overlay_clear = false;
        let mut pending_clear = false;
        let mut needs_redraw = false;
        let mut rows = 0u16;
        let mut cols = 0u16;
        let mut last_output_at = Instant::now();
        let mut last_status_draw_at = Instant::now();
        let mut theme = Theme::default();
        let mut current_banner_height: usize = 0; // Track banner height for clearing

        loop {
            match rx.recv_timeout(Duration::from_millis(25)) {
                Ok(WriterMessage::PtyOutput(bytes)) => {
                    if stdout.write_all(&bytes).is_err() {
                        break;
                    }
                    last_output_at = Instant::now();
                    if status.is_some() || enhanced_status.is_some() || overlay_panel.is_some() {
                        needs_redraw = true;
                    }
                    let _ = stdout.flush();
                }
                Ok(WriterMessage::Status { text }) => {
                    pending_status = Some(text);
                    pending_enhanced = None;
                    pending_clear = false;
                    needs_redraw = true;
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::EnhancedStatus(state)) => {
                    pending_enhanced = Some(state);
                    pending_status = None;
                    pending_clear = false;
                    needs_redraw = true;
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::ShowOverlay { content, height }) => {
                    pending_overlay = Some(OverlayPanel { content, height });
                    pending_overlay_clear = false;
                    needs_redraw = true;
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::ClearOverlay) => {
                    pending_overlay = None;
                    pending_overlay_clear = true;
                    needs_redraw = true;
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::ClearStatus) => {
                    pending_status = None;
                    pending_enhanced = None;
                    pending_clear = true;
                    needs_redraw = true;
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::Bell { count }) => {
                    let sequence = vec![0x07; count.max(1) as usize];
                    let _ = stdout.write_all(&sequence);
                    let _ = stdout.flush();
                }
                Ok(WriterMessage::Resize { rows: r, cols: c }) => {
                    rows = r;
                    cols = c;
                    if status.is_some()
                        || enhanced_status.is_some()
                        || pending_status.is_some()
                        || pending_enhanced.is_some()
                        || overlay_panel.is_some()
                        || pending_overlay.is_some()
                    {
                        needs_redraw = true;
                    }
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
                }
                Ok(WriterMessage::SetTheme(new_theme)) => {
                    theme = new_theme;
                    if status.is_some() || enhanced_status.is_some() || overlay_panel.is_some() {
                        needs_redraw = true;
                    }
                }
                Ok(WriterMessage::Shutdown) => break,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    maybe_redraw_status(StatusRedraw {
                        stdout: &mut stdout,
                        rows: &mut rows,
                        cols: &mut cols,
                        status: &mut status,
                        enhanced_status: &mut enhanced_status,
                        pending_status: &mut pending_status,
                        pending_enhanced: &mut pending_enhanced,
                        overlay_panel: &mut overlay_panel,
                        pending_overlay: &mut pending_overlay,
                        pending_overlay_clear: &mut pending_overlay_clear,
                        pending_clear: &mut pending_clear,
                        needs_redraw: &mut needs_redraw,
                        last_output_at,
                        last_status_draw_at: &mut last_status_draw_at,
                        theme,
                        current_banner_height: &mut current_banner_height,
                    });
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
        let _ = writer_tx.send(WriterMessage::Status {
            text: status_state.message.clone(),
        });
        *current_status = Some(text.to_string());
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

struct StatusRedraw<'a> {
    stdout: &'a mut io::Stdout,
    rows: &'a mut u16,
    cols: &'a mut u16,
    status: &'a mut Option<String>,
    enhanced_status: &'a mut Option<StatusLineState>,
    pending_status: &'a mut Option<String>,
    pending_enhanced: &'a mut Option<StatusLineState>,
    overlay_panel: &'a mut Option<OverlayPanel>,
    pending_overlay: &'a mut Option<OverlayPanel>,
    pending_overlay_clear: &'a mut bool,
    pending_clear: &'a mut bool,
    needs_redraw: &'a mut bool,
    last_output_at: Instant,
    last_status_draw_at: &'a mut Instant,
    theme: Theme,
    current_banner_height: &'a mut usize,
}

fn maybe_redraw_status(ctx: StatusRedraw<'_>) {
    const STATUS_IDLE_MS: u64 = 50;
    const STATUS_MAX_WAIT_MS: u64 = 500;
    if !*ctx.needs_redraw {
        return;
    }
    let since_output = ctx.last_output_at.elapsed();
    let since_draw = ctx.last_status_draw_at.elapsed();
    if since_output < Duration::from_millis(STATUS_IDLE_MS)
        && since_draw < Duration::from_millis(STATUS_MAX_WAIT_MS)
    {
        return;
    }
    if *ctx.rows == 0 || *ctx.cols == 0 {
        if let Ok((c, r)) = terminal_size() {
            *ctx.rows = r;
            *ctx.cols = c;
        }
    }
    if *ctx.pending_clear {
        // Clear multi-line banner if we had one
        if *ctx.current_banner_height > 1 {
            let _ = clear_status_banner(ctx.stdout, *ctx.rows, *ctx.current_banner_height);
        } else {
            let _ = clear_status_line(ctx.stdout, *ctx.rows, *ctx.cols);
        }
        *ctx.status = None;
        *ctx.enhanced_status = None;
        *ctx.current_banner_height = 0;
        *ctx.pending_clear = false;
    }
    if *ctx.pending_overlay_clear {
        if let Some(panel) = ctx.overlay_panel.as_ref() {
            let _ = clear_overlay_panel(ctx.stdout, *ctx.rows, panel.height);
        }
        *ctx.overlay_panel = None;
        *ctx.pending_overlay_clear = false;
    }
    if let Some(panel) = ctx.pending_overlay.take() {
        *ctx.overlay_panel = Some(panel);
    }
    // Handle enhanced status (takes priority)
    if let Some(state) = ctx.pending_enhanced.take() {
        *ctx.enhanced_status = Some(state);
        *ctx.status = None;
    }
    // Handle simple status
    if let Some(text) = ctx.pending_status.take() {
        *ctx.status = Some(text);
        *ctx.enhanced_status = None;
    }
    // Render help overlay or status line
    if let Some(panel) = ctx.overlay_panel.as_ref() {
        let _ = write_overlay_panel(ctx.stdout, panel, *ctx.rows);
    } else if let Some(state) = ctx.enhanced_status.as_ref() {
        let banner = format_status_banner(state, ctx.theme, *ctx.cols as usize);
        // Clear old banner if height changed
        if *ctx.current_banner_height > 0 && *ctx.current_banner_height != banner.height {
            let _ = clear_status_banner(ctx.stdout, *ctx.rows, *ctx.current_banner_height);
        }
        *ctx.current_banner_height = banner.height;
        let _ = write_status_banner(ctx.stdout, &banner, *ctx.rows);
    } else if let Some(text) = ctx.status.as_deref() {
        let _ = write_status_line(ctx.stdout, text, *ctx.rows, *ctx.cols, ctx.theme);
    }
    *ctx.needs_redraw = false;
    *ctx.last_status_draw_at = Instant::now();
    let _ = ctx.stdout.flush();
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
    sequence.extend_from_slice(b"\x1b7");
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(formatted.as_bytes());
    sequence.extend_from_slice(b"\x1b8");
    stdout.write_all(&sequence)
}

/// Write a multi-line status banner at the bottom of the terminal.
fn write_status_banner(
    stdout: &mut dyn Write,
    banner: &StatusBanner,
    rows: u16,
) -> io::Result<()> {
    if rows == 0 || banner.height == 0 {
        return Ok(());
    }
    let height = banner.height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7"); // Save cursor

    for (idx, line) in banner.lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
        sequence.extend_from_slice(line.as_bytes()); // Write content
    }

    sequence.extend_from_slice(b"\x1b8"); // Restore cursor
    stdout.write_all(&sequence)
}

/// Clear a multi-line status banner.
fn clear_status_banner(stdout: &mut dyn Write, rows: u16, height: usize) -> io::Result<()> {
    if rows == 0 || height == 0 {
        return Ok(());
    }
    let height = height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);

    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7"); // Save cursor

    for idx in 0..height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes()); // Move to row
        sequence.extend_from_slice(b"\x1b[2K"); // Clear line
    }

    sequence.extend_from_slice(b"\x1b8"); // Restore cursor
    stdout.write_all(&sequence)
}

fn clear_status_line(stdout: &mut dyn Write, rows: u16, cols: u16) -> io::Result<()> {
    if rows == 0 || cols == 0 {
        return Ok(());
    }
    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7");
    sequence.extend_from_slice(format!("\x1b[{rows};1H").as_bytes());
    sequence.extend_from_slice(b"\x1b[2K");
    sequence.extend_from_slice(b"\x1b8");
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
    sequence.extend_from_slice(b"\x1b7");
    for (idx, line) in lines.iter().take(height).enumerate() {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
        sequence.extend_from_slice(line.as_bytes());
    }
    sequence.extend_from_slice(b"\x1b8");
    stdout.write_all(&sequence)
}

fn clear_overlay_panel(stdout: &mut dyn Write, rows: u16, height: usize) -> io::Result<()> {
    if rows == 0 {
        return Ok(());
    }
    let height = height.min(rows as usize);
    let start_row = rows.saturating_sub(height as u16).saturating_add(1);
    let mut sequence = Vec::new();
    sequence.extend_from_slice(b"\x1b7");
    for idx in 0..height {
        let row = start_row + idx as u16;
        sequence.extend_from_slice(format!("\x1b[{row};1H").as_bytes());
        sequence.extend_from_slice(b"\x1b[2K");
    }
    sequence.extend_from_slice(b"\x1b8");
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
            WriterMessage::Status { text } => assert_eq!(text, "status"),
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
