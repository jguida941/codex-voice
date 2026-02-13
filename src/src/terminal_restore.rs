//! Terminal-state guards that prevent broken shells after exit or panic paths.

use crossterm::{
    cursor::Show,
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    io::{self, Write},
    panic,
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
};

static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);
static ALT_SCREEN_ENABLED: AtomicBool = AtomicBool::new(false);
static MOUSE_CAPTURE_ENABLED: AtomicBool = AtomicBool::new(false);
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

/// RAII guard to restore terminal state on drop (and on panic via a shared hook).
pub struct TerminalRestoreGuard;

impl TerminalRestoreGuard {
    /// Create a guard and install the shared panic hook (once).
    pub fn new() -> Self {
        install_terminal_panic_hook();
        TerminalRestoreGuard
    }

    /// Enable terminal raw mode and track state for guaranteed restoration.
    pub fn enable_raw_mode(&self) -> io::Result<()> {
        enable_raw_mode()?;
        RAW_MODE_ENABLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Enter alternate screen and track state for guaranteed restoration.
    pub fn enter_alt_screen(&self, stdout: &mut impl Write) -> io::Result<()> {
        execute!(stdout, EnterAlternateScreen)?;
        ALT_SCREEN_ENABLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    #[allow(dead_code)]
    /// Enable mouse capture and track state for guaranteed restoration.
    pub fn enable_mouse_capture(&self, stdout: &mut impl Write) -> io::Result<()> {
        execute!(stdout, crossterm::event::EnableMouseCapture)?;
        MOUSE_CAPTURE_ENABLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Restore all tracked terminal state immediately.
    pub fn restore(&self) {
        restore_terminal();
    }
}

impl Default for TerminalRestoreGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalRestoreGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

/// Restore terminal raw mode, mouse capture, alt-screen, and cursor visibility.
pub fn restore_terminal() {
    if RAW_MODE_ENABLED.swap(false, Ordering::SeqCst) {
        let _ = disable_raw_mode();
    }
    let mut stdout = io::stdout();
    if MOUSE_CAPTURE_ENABLED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout, DisableMouseCapture);
    }
    if ALT_SCREEN_ENABLED.swap(false, Ordering::SeqCst) {
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
    let _ = execute!(stdout, Show);
    let _ = stdout.flush();
}

/// Install a panic hook that restores terminal state before delegating.
pub fn install_terminal_panic_hook() {
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            restore_terminal();
            crate::log_panic(info);
            let location = info
                .location()
                .map(|loc| format!("{}:{}", loc.file(), loc.line()))
                .unwrap_or_else(|| "unknown".to_string());
            crate::log_debug(&format!("panic at {location}"));
            crate::log_debug_content(&format!("panic: {info}"));
            previous(info);
        }));
    });
}
