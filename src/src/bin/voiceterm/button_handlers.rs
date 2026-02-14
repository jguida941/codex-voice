//! Button-action handlers so HUD interactions mutate runtime state consistently.

use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use voiceterm::pty_session::PtyOverlaySession;
use voiceterm::VoiceCaptureTrigger;

use crate::buttons::{ButtonAction, ButtonRegistry};
use crate::config::OverlayConfig;
use crate::log_debug;
use crate::overlays::{
    show_help_overlay, show_settings_overlay, show_theme_picker_overlay, OverlayMode,
};
use crate::settings::SettingsMenuState;
use crate::settings_handlers::SettingsActionContext;
use crate::status_line::{get_button_positions, status_banner_height, StatusLineState};
use crate::terminal::{resolved_cols, update_pty_winsize};
use crate::theme::Theme;
use crate::theme_ops::theme_index_from_theme;
use crate::voice_control::{reset_capture_visuals, start_voice_capture, VoiceManager};
use crate::writer::{send_enhanced_status, set_status, WriterMessage};

pub(crate) struct ButtonActionContext<'a> {
    pub(crate) overlay_mode: &'a mut OverlayMode,
    pub(crate) settings_menu: &'a mut SettingsMenuState,
    pub(crate) config: &'a mut OverlayConfig,
    pub(crate) status_state: &'a mut StatusLineState,
    pub(crate) auto_voice_enabled: &'a mut bool,
    pub(crate) voice_manager: &'a mut VoiceManager,
    pub(crate) session: &'a mut PtyOverlaySession,
    pub(crate) writer_tx: &'a Sender<WriterMessage>,
    pub(crate) status_clear_deadline: &'a mut Option<Instant>,
    pub(crate) current_status: &'a mut Option<String>,
    pub(crate) recording_started_at: &'a mut Option<Instant>,
    pub(crate) preview_clear_deadline: &'a mut Option<Instant>,
    pub(crate) last_meter_update: &'a mut Instant,
    pub(crate) last_auto_trigger_at: &'a mut Option<Instant>,
    pub(crate) terminal_rows: &'a mut u16,
    pub(crate) terminal_cols: &'a mut u16,
    pub(crate) backend_label: &'a str,
    pub(crate) theme: &'a mut Theme,
    pub(crate) button_registry: &'a ButtonRegistry,
}

impl<'a> ButtonActionContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        overlay_mode: &'a mut OverlayMode,
        settings_menu: &'a mut SettingsMenuState,
        config: &'a mut OverlayConfig,
        status_state: &'a mut StatusLineState,
        auto_voice_enabled: &'a mut bool,
        voice_manager: &'a mut VoiceManager,
        session: &'a mut PtyOverlaySession,
        writer_tx: &'a Sender<WriterMessage>,
        status_clear_deadline: &'a mut Option<Instant>,
        current_status: &'a mut Option<String>,
        recording_started_at: &'a mut Option<Instant>,
        preview_clear_deadline: &'a mut Option<Instant>,
        last_meter_update: &'a mut Instant,
        last_auto_trigger_at: &'a mut Option<Instant>,
        terminal_rows: &'a mut u16,
        terminal_cols: &'a mut u16,
        backend_label: &'a str,
        theme: &'a mut Theme,
        button_registry: &'a ButtonRegistry,
    ) -> Self {
        Self {
            overlay_mode,
            settings_menu,
            config,
            status_state,
            auto_voice_enabled,
            voice_manager,
            session,
            writer_tx,
            status_clear_deadline,
            current_status,
            recording_started_at,
            preview_clear_deadline,
            last_meter_update,
            last_auto_trigger_at,
            terminal_rows,
            terminal_cols,
            backend_label,
            theme,
            button_registry,
        }
    }

    pub(crate) fn handle_action(&mut self, action: ButtonAction) {
        if *self.overlay_mode != OverlayMode::None {
            return;
        }

        match action {
            ButtonAction::VoiceTrigger => {
                if let Err(err) = start_voice_capture(
                    self.voice_manager,
                    VoiceCaptureTrigger::Manual,
                    self.writer_tx,
                    self.status_clear_deadline,
                    self.current_status,
                    self.status_state,
                ) {
                    set_status(
                        self.writer_tx,
                        self.status_clear_deadline,
                        self.current_status,
                        self.status_state,
                        "Voice capture failed (see log)",
                        Some(Duration::from_secs(2)),
                    );
                    log_debug(&format!("voice capture failed: {err:#}"));
                } else {
                    *self.recording_started_at = Some(Instant::now());
                    reset_capture_visuals(
                        self.status_state,
                        self.preview_clear_deadline,
                        self.last_meter_update,
                    );
                }
            }
            ButtonAction::ToggleAutoVoice => {
                let mut settings_ctx = self.settings_context();
                settings_ctx.toggle_auto_voice();
            }
            ButtonAction::ToggleSendMode => {
                let mut settings_ctx = self.settings_context();
                settings_ctx.toggle_send_mode();
            }
            ButtonAction::SettingsToggle => {
                *self.overlay_mode = OverlayMode::Settings;
                update_pty_winsize(
                    self.session,
                    self.terminal_rows,
                    self.terminal_cols,
                    *self.overlay_mode,
                    self.status_state.hud_style,
                );
                let cols = resolved_cols(*self.terminal_cols);
                show_settings_overlay(
                    self.writer_tx,
                    *self.theme,
                    cols,
                    self.settings_menu,
                    self.config,
                    self.status_state,
                    self.backend_label,
                );
            }
            ButtonAction::ToggleHudStyle => {
                let mut settings_ctx = self.settings_context();
                settings_ctx.update_hud_style(1);
            }
            ButtonAction::HudBack => {
                let mut settings_ctx = self.settings_context();
                settings_ctx.update_hud_style(-1);
            }
            ButtonAction::HelpToggle => {
                *self.overlay_mode = OverlayMode::Help;
                update_pty_winsize(
                    self.session,
                    self.terminal_rows,
                    self.terminal_cols,
                    *self.overlay_mode,
                    self.status_state.hud_style,
                );
                let cols = resolved_cols(*self.terminal_cols);
                show_help_overlay(self.writer_tx, *self.theme, cols);
            }
            ButtonAction::ThemePicker => {
                *self.overlay_mode = OverlayMode::ThemePicker;
                update_pty_winsize(
                    self.session,
                    self.terminal_rows,
                    self.terminal_cols,
                    *self.overlay_mode,
                    self.status_state.hud_style,
                );
                let cols = resolved_cols(*self.terminal_cols);
                show_theme_picker_overlay(
                    self.writer_tx,
                    *self.theme,
                    theme_index_from_theme(*self.theme),
                    cols,
                );
            }
        }

        if self.status_state.mouse_enabled {
            update_button_registry(
                self.button_registry,
                self.status_state,
                *self.overlay_mode,
                *self.terminal_cols,
                *self.theme,
            );
        }
    }

    fn settings_context(&mut self) -> SettingsActionContext<'_> {
        SettingsActionContext::new(
            &mut *self.config,
            &mut *self.status_state,
            &mut *self.auto_voice_enabled,
            &mut *self.voice_manager,
            self.writer_tx,
            &mut *self.status_clear_deadline,
            &mut *self.current_status,
            &mut *self.last_auto_trigger_at,
            &mut *self.recording_started_at,
            &mut *self.preview_clear_deadline,
            &mut *self.last_meter_update,
            self.button_registry,
            *self.overlay_mode,
            &mut *self.terminal_rows,
            &mut *self.terminal_cols,
            &mut *self.theme,
            Some(&mut *self.session),
        )
    }
}

pub(crate) fn update_button_registry(
    registry: &ButtonRegistry,
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) {
    registry.clear();
    if overlay_mode != OverlayMode::None {
        registry.set_hud_offset(0);
        return;
    }
    let banner_height = status_banner_height(terminal_cols as usize, status_state.hud_style);
    registry.set_hud_offset(banner_height as u16);
    let positions = get_button_positions(status_state, theme, terminal_cols as usize);
    for pos in positions {
        registry.register(pos.start_x, pos.end_x, pos.row, pos.action);
    }
}

pub(crate) fn advance_hud_button_focus(
    status_state: &mut StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
    direction: i32,
) -> bool {
    let actions = visible_button_actions(status_state, overlay_mode, terminal_cols, theme);
    if actions.is_empty() {
        if status_state.hud_button_focus.is_some() {
            status_state.hud_button_focus = None;
            return true;
        }
        return false;
    }

    let current = status_state.hud_button_focus;
    let mut idx =
        current.and_then(|action| actions.iter().position(|candidate| *candidate == action));

    if idx.is_none() {
        idx = if direction >= 0 {
            Some(0)
        } else {
            Some(actions.len().saturating_sub(1))
        };
    } else {
        let len = actions.len() as i32;
        let next = (idx.unwrap() as i32 + direction).rem_euclid(len) as usize;
        idx = Some(next);
    }

    let next_action = actions[idx.unwrap()];
    if status_state.hud_button_focus == Some(next_action) {
        return false;
    }
    status_state.hud_button_focus = Some(next_action);
    true
}

pub(crate) fn send_enhanced_status_with_buttons(
    writer_tx: &Sender<WriterMessage>,
    button_registry: &ButtonRegistry,
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) {
    send_enhanced_status(writer_tx, status_state);
    if status_state.mouse_enabled {
        update_button_registry(
            button_registry,
            status_state,
            overlay_mode,
            terminal_cols,
            theme,
        );
    }
}

fn visible_button_actions(
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) -> Vec<ButtonAction> {
    if overlay_mode != OverlayMode::None {
        return Vec::new();
    }
    let mut positions = get_button_positions(status_state, theme, terminal_cols as usize);
    positions.sort_by_key(|pos| pos.start_x);
    positions.into_iter().map(|pos| pos.action).collect()
}
