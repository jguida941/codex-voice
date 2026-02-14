use crossterm::terminal::size as terminal_size;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use voxterm::log_debug;

use super::mouse::{disable_mouse, enable_mouse};
use super::render::{
    clear_overlay_panel, clear_status_banner, clear_status_line, write_overlay_panel,
    write_status_banner, write_status_line,
};
use super::WriterMessage;
use crate::status_line::{format_status_banner, StatusLineState};
use crate::theme::Theme;

const OUTPUT_FLUSH_INTERVAL_MS: u64 = 16;

#[derive(Debug, Clone)]
pub(super) struct OverlayPanel {
    pub(super) content: String,
    pub(super) height: usize,
}

#[derive(Debug, Default)]
struct DisplayState {
    status: Option<String>,
    enhanced_status: Option<StatusLineState>,
    overlay_panel: Option<OverlayPanel>,
    banner_height: usize,
}

impl DisplayState {
    fn has_any(&self) -> bool {
        self.status.is_some() || self.enhanced_status.is_some() || self.overlay_panel.is_some()
    }
}

#[derive(Debug, Default)]
struct PendingState {
    status: Option<String>,
    enhanced_status: Option<StatusLineState>,
    overlay_panel: Option<OverlayPanel>,
    clear_status: bool,
    clear_overlay: bool,
}

impl PendingState {
    fn has_any(&self) -> bool {
        self.status.is_some()
            || self.enhanced_status.is_some()
            || self.overlay_panel.is_some()
            || self.clear_status
            || self.clear_overlay
    }
}

pub(super) struct WriterState {
    stdout: io::Stdout,
    display: DisplayState,
    pending: PendingState,
    needs_redraw: bool,
    rows: u16,
    cols: u16,
    last_output_at: Instant,
    last_output_flush_at: Instant,
    last_status_draw_at: Instant,
    theme: Theme,
    mouse_enabled: bool,
}

impl WriterState {
    pub(super) fn new() -> Self {
        Self {
            stdout: io::stdout(),
            display: DisplayState::default(),
            pending: PendingState::default(),
            needs_redraw: false,
            rows: 0,
            cols: 0,
            last_output_at: Instant::now(),
            last_output_flush_at: Instant::now(),
            last_status_draw_at: Instant::now(),
            theme: Theme::default(),
            mouse_enabled: false,
        }
    }

    pub(super) fn handle_message(&mut self, message: WriterMessage) -> bool {
        match message {
            WriterMessage::PtyOutput(bytes) => {
                if let Err(err) = self.stdout.write_all(&bytes) {
                    log_debug(&format!("stdout write_all failed: {err}"));
                    return false;
                }
                let now = Instant::now();
                self.last_output_at = now;
                if self.display.has_any() {
                    self.needs_redraw = true;
                }
                if now.duration_since(self.last_output_flush_at)
                    >= Duration::from_millis(OUTPUT_FLUSH_INTERVAL_MS)
                    || bytes.contains(&b'\n')
                {
                    if let Err(err) = self.stdout.flush() {
                        log_debug(&format!("stdout flush failed: {err}"));
                    } else {
                        self.last_output_flush_at = now;
                    }
                }
                // Keep overlays/HUD responsive while PTY output is continuous.
                // Without this, recv_timeout-based redraws can be starved.
                self.maybe_redraw_status();
            }
            WriterMessage::Status { text } => {
                self.pending.status = Some(text);
                self.pending.enhanced_status = None;
                self.pending.clear_status = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::EnhancedStatus(state) => {
                self.pending.enhanced_status = Some(state);
                self.pending.status = None;
                self.pending.clear_status = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ShowOverlay { content, height } => {
                self.pending.overlay_panel = Some(OverlayPanel { content, height });
                self.pending.clear_overlay = false;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ClearOverlay => {
                self.pending.overlay_panel = None;
                self.pending.clear_overlay = true;
                self.needs_redraw = true;
                self.maybe_redraw_status();
            }
            WriterMessage::ClearStatus => {
                self.pending.status = None;
                self.pending.enhanced_status = None;
                self.pending.clear_status = true;
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
                if self.rows > 0 && (self.rows != rows || self.cols != cols) {
                    // Clear HUD/overlay at the old terminal geometry before moving to the new one.
                    // This prevents stale frames when startup rows are briefly reported incorrectly.
                    if let Some(panel) = self.display.overlay_panel.as_ref() {
                        let _ = clear_overlay_panel(&mut self.stdout, self.rows, panel.height);
                    }
                    if self.display.banner_height > 1 {
                        let _ = clear_status_banner(
                            &mut self.stdout,
                            self.rows,
                            self.display.banner_height,
                        );
                    } else if self.display.status.is_some()
                        || self.display.enhanced_status.is_some()
                    {
                        let _ = clear_status_line(&mut self.stdout, self.rows, self.cols.max(1));
                    }
                    let _ = self.stdout.flush();
                }
                self.rows = rows;
                self.cols = cols;
                if self.display.has_any() || self.pending.has_any() {
                    self.needs_redraw = true;
                }
                self.maybe_redraw_status();
            }
            WriterMessage::SetTheme(new_theme) => {
                self.theme = new_theme;
                if self.display.has_any() {
                    self.needs_redraw = true;
                }
            }
            WriterMessage::EnableMouse => {
                enable_mouse(&mut self.stdout, &mut self.mouse_enabled);
            }
            WriterMessage::DisableMouse => {
                disable_mouse(&mut self.stdout, &mut self.mouse_enabled);
            }
            WriterMessage::Shutdown => {
                // Disable mouse before exiting to restore terminal state
                disable_mouse(&mut self.stdout, &mut self.mouse_enabled);
                return false;
            }
        }
        true
    }

    pub(super) fn maybe_redraw_status(&mut self) {
        const STATUS_IDLE_MS: u64 = 50;
        const STATUS_MAX_WAIT_MS: u64 = 150;
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
        if self.pending.clear_status {
            let current_banner_height = self.display.banner_height;
            if current_banner_height > 1 {
                let _ = clear_status_banner(&mut self.stdout, self.rows, current_banner_height);
            } else {
                let _ = clear_status_line(&mut self.stdout, self.rows, self.cols);
            }
            self.display.status = None;
            self.display.enhanced_status = None;
            self.display.banner_height = 0;
            self.pending.clear_status = false;
        }
        if self.pending.clear_overlay {
            if let Some(panel) = self.display.overlay_panel.as_ref() {
                let _ = clear_overlay_panel(&mut self.stdout, self.rows, panel.height);
            }
            self.display.overlay_panel = None;
            self.pending.clear_overlay = false;
        }
        if let Some(panel) = self.pending.overlay_panel.as_ref() {
            if let Some(current) = self.display.overlay_panel.as_ref() {
                if current.height != panel.height {
                    let _ = clear_overlay_panel(&mut self.stdout, self.rows, current.height);
                }
            }
        }
        if let Some(panel) = self.pending.overlay_panel.take() {
            self.display.overlay_panel = Some(panel);
        }
        if let Some(state) = self.pending.enhanced_status.take() {
            self.display.enhanced_status = Some(state);
            self.display.status = None;
        }
        if let Some(text) = self.pending.status.take() {
            self.display.status = Some(text);
            self.display.enhanced_status = None;
        }

        let flush_error = {
            let rows = self.rows;
            let cols = self.cols;
            let theme = self.theme;
            let (stdout, overlay_panel, enhanced_status, status, current_banner_height) = (
                &mut self.stdout,
                &self.display.overlay_panel,
                &self.display.enhanced_status,
                &self.display.status,
                &mut self.display.banner_height,
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
