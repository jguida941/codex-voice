//! Theme-switch operations so shortcuts and buttons share one mutation path.

use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use voxterm::pty_session::PtyOverlaySession;

use crate::config::OverlayConfig;
use crate::overlays::OverlayMode;
use crate::status_line::StatusLineState;
use crate::terminal::update_pty_winsize;
use crate::theme::Theme;
use crate::theme_picker::THEME_OPTIONS;
use crate::writer::{set_status, WriterMessage};

pub(crate) fn cycle_theme(current: Theme, direction: i32) -> Theme {
    let len = THEME_OPTIONS.len() as i32;
    if len == 0 {
        return current;
    }
    let idx = THEME_OPTIONS
        .iter()
        .position(|(theme, _, _)| *theme == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    THEME_OPTIONS[next].0
}

pub(crate) fn theme_index_from_theme(theme: Theme) -> usize {
    THEME_OPTIONS
        .iter()
        .position(|(candidate, _, _)| *candidate == theme)
        .unwrap_or(0)
}

pub(crate) fn apply_theme_selection(
    config: &mut OverlayConfig,
    requested: Theme,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
) -> Theme {
    config.theme_name = Some(requested.to_string());
    let (resolved, note) = resolve_theme_choice(config, requested);
    let _ = writer_tx.send(WriterMessage::SetTheme(resolved));
    let mut status = if resolved == Theme::None && requested != Theme::None {
        "Theme set: none".to_string()
    } else {
        format!("Theme set: {}", requested)
    };
    if let Some(note) = note {
        status = format!("{status} ({note})");
    }
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        &status,
        Some(Duration::from_secs(2)),
    );
    resolved
}

pub(crate) fn theme_picker_parse_index(digits: &str, total: usize) -> Option<usize> {
    if digits.is_empty() {
        return None;
    }
    let value: usize = digits.parse().ok()?;
    if value == 0 || value > total {
        return None;
    }
    Some(value - 1)
}

pub(crate) fn theme_picker_has_longer_match(prefix: &str, total: usize) -> bool {
    if prefix.is_empty() {
        return false;
    }
    (1..=total).any(|idx| {
        let label = idx.to_string();
        label.len() > prefix.len() && label.starts_with(prefix)
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_theme_picker_index(
    idx: usize,
    theme: &mut Theme,
    config: &mut OverlayConfig,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    session: &mut PtyOverlaySession,
    terminal_rows: &mut u16,
    terminal_cols: &mut u16,
    overlay_mode: &mut OverlayMode,
) -> bool {
    let Some((_, name, _)) = THEME_OPTIONS.get(idx) else {
        return false;
    };
    let Some(requested) = Theme::from_name(name) else {
        return false;
    };
    *theme = apply_theme_selection(
        config,
        requested,
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
    );
    *overlay_mode = OverlayMode::None;
    let _ = writer_tx.send(WriterMessage::ClearOverlay);
    update_pty_winsize(
        session,
        terminal_rows,
        terminal_cols,
        *overlay_mode,
        status_state.hud_style,
    );
    true
}

fn resolve_theme_choice(config: &OverlayConfig, requested: Theme) -> (Theme, Option<&'static str>) {
    if config.no_color || std::env::var("NO_COLOR").is_ok() {
        return (Theme::None, Some("colors disabled"));
    }
    let mode = config.color_mode();
    if !mode.supports_color() {
        return (Theme::None, Some("no color support"));
    }
    if !mode.supports_truecolor() {
        let resolved = requested.fallback_for_ansi();
        if resolved != requested {
            return (resolved, Some("ansi fallback"));
        }
        return (resolved, None);
    }
    (requested, None)
}
