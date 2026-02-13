//! Settings action handlers so runtime config and HUD state change atomically.

use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use voxterm::pty_session::PtyOverlaySession;
use voxterm::VoiceCaptureTrigger;

use crate::button_handlers::update_button_registry;
use crate::buttons::ButtonRegistry;
use crate::config::{HudRightPanel, HudStyle, OverlayConfig, VoiceSendMode};
use crate::log_debug;
use crate::overlays::OverlayMode;
use crate::status_line::{RecordingState, StatusLineState, VoiceIntentMode, VoiceMode};
use crate::terminal::update_pty_winsize;
use crate::theme::Theme;
use crate::theme_ops::{apply_theme_selection, cycle_theme};
use crate::voice_control::{
    clear_capture_metrics, reset_capture_visuals, start_voice_capture, VoiceManager,
};
use crate::writer::{set_status, WriterMessage};

pub(crate) struct SettingsActionContext<'a> {
    pub(crate) config: &'a mut OverlayConfig,
    pub(crate) status_state: &'a mut StatusLineState,
    pub(crate) auto_voice_enabled: &'a mut bool,
    pub(crate) voice_manager: &'a mut VoiceManager,
    pub(crate) writer_tx: &'a Sender<WriterMessage>,
    pub(crate) status_clear_deadline: &'a mut Option<Instant>,
    pub(crate) current_status: &'a mut Option<String>,
    pub(crate) last_auto_trigger_at: &'a mut Option<Instant>,
    pub(crate) recording_started_at: &'a mut Option<Instant>,
    pub(crate) preview_clear_deadline: &'a mut Option<Instant>,
    pub(crate) last_meter_update: &'a mut Instant,
    pub(crate) button_registry: &'a ButtonRegistry,
    pub(crate) overlay_mode: OverlayMode,
    pub(crate) terminal_rows: &'a mut u16,
    pub(crate) terminal_cols: &'a mut u16,
    pub(crate) theme: &'a mut Theme,
    pub(crate) pty_session: Option<&'a mut PtyOverlaySession>,
}

impl<'a> SettingsActionContext<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        config: &'a mut OverlayConfig,
        status_state: &'a mut StatusLineState,
        auto_voice_enabled: &'a mut bool,
        voice_manager: &'a mut VoiceManager,
        writer_tx: &'a Sender<WriterMessage>,
        status_clear_deadline: &'a mut Option<Instant>,
        current_status: &'a mut Option<String>,
        last_auto_trigger_at: &'a mut Option<Instant>,
        recording_started_at: &'a mut Option<Instant>,
        preview_clear_deadline: &'a mut Option<Instant>,
        last_meter_update: &'a mut Instant,
        button_registry: &'a ButtonRegistry,
        overlay_mode: OverlayMode,
        terminal_rows: &'a mut u16,
        terminal_cols: &'a mut u16,
        theme: &'a mut Theme,
        pty_session: Option<&'a mut PtyOverlaySession>,
    ) -> Self {
        Self {
            config,
            status_state,
            auto_voice_enabled,
            voice_manager,
            writer_tx,
            status_clear_deadline,
            current_status,
            last_auto_trigger_at,
            recording_started_at,
            preview_clear_deadline,
            last_meter_update,
            button_registry,
            overlay_mode,
            terminal_rows,
            terminal_cols,
            theme,
            pty_session,
        }
    }

    pub(crate) fn toggle_auto_voice(&mut self) {
        *self.auto_voice_enabled = !*self.auto_voice_enabled;
        self.status_state.auto_voice_enabled = *self.auto_voice_enabled;
        self.status_state.voice_mode = if *self.auto_voice_enabled {
            VoiceMode::Auto
        } else {
            VoiceMode::Manual
        };
        let msg = if *self.auto_voice_enabled {
            "Auto-voice enabled"
        } else {
            let cancelled = self.voice_manager.cancel_capture();
            if cancelled {
                self.status_state.recording_state = RecordingState::Idle;
                *self.recording_started_at = None;
            }
            clear_capture_metrics(self.status_state);
            reset_capture_visuals(
                self.status_state,
                self.preview_clear_deadline,
                self.last_meter_update,
            );
            if cancelled {
                "Auto-voice disabled (capture cancelled)"
            } else {
                "Auto-voice disabled"
            }
        };
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            msg,
            Some(Duration::from_secs(2)),
        );
        if *self.auto_voice_enabled && self.voice_manager.is_idle() {
            if let Err(err) = start_voice_capture(
                self.voice_manager,
                VoiceCaptureTrigger::Auto,
                self.writer_tx,
                self.status_clear_deadline,
                self.current_status,
                self.status_state,
            ) {
                log_debug(&format!("auto voice capture failed: {err:#}"));
            } else {
                let now = Instant::now();
                *self.last_auto_trigger_at = Some(now);
                *self.recording_started_at = Some(now);
                reset_capture_visuals(
                    self.status_state,
                    self.preview_clear_deadline,
                    self.last_meter_update,
                );
            }
        }
    }

    pub(crate) fn toggle_send_mode(&mut self) {
        self.config.voice_send_mode = match self.config.voice_send_mode {
            VoiceSendMode::Auto => VoiceSendMode::Insert,
            VoiceSendMode::Insert => VoiceSendMode::Auto,
        };
        self.status_state.send_mode = self.config.voice_send_mode;
        let msg = match self.config.voice_send_mode {
            VoiceSendMode::Auto => "Send mode: auto (sends Enter)",
            VoiceSendMode::Insert => "Send mode: insert (press Enter to send)",
        };
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            msg,
            Some(Duration::from_secs(3)),
        );
    }

    pub(crate) fn toggle_voice_intent_mode(&mut self) {
        self.status_state.voice_intent_mode = match self.status_state.voice_intent_mode {
            VoiceIntentMode::Command => VoiceIntentMode::Dictation,
            VoiceIntentMode::Dictation => VoiceIntentMode::Command,
        };
        let msg = match self.status_state.voice_intent_mode {
            VoiceIntentMode::Command => "Voice mode: command (macros enabled)",
            VoiceIntentMode::Dictation => "Voice mode: dictation (macros disabled)",
        };
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            msg,
            Some(Duration::from_secs(3)),
        );
    }

    pub(crate) fn adjust_sensitivity(&mut self, delta_db: f32) {
        let threshold_db = self.voice_manager.adjust_sensitivity(delta_db);
        self.status_state.sensitivity_db = threshold_db;
        let direction = if delta_db >= 0.0 {
            "less sensitive"
        } else {
            "more sensitive"
        };
        let msg = format!("Mic sensitivity: {threshold_db:.0} dB ({direction})");
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            &msg,
            Some(Duration::from_secs(3)),
        );
    }

    pub(crate) fn cycle_theme(&mut self, direction: i32) {
        let next = cycle_theme(*self.theme, direction);
        *self.theme = apply_theme_selection(
            self.config,
            next,
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
        );
    }

    pub(crate) fn update_hud_panel(&mut self, direction: i32) {
        self.config.hud_right_panel = cycle_hud_right_panel(self.config.hud_right_panel, direction);
        self.status_state.hud_right_panel = self.config.hud_right_panel;
        let label = format!("HUD right panel: {}", self.config.hud_right_panel);
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            &label,
            Some(Duration::from_secs(2)),
        );
    }

    pub(crate) fn update_hud_style(&mut self, direction: i32) {
        self.status_state.hud_style = cycle_hud_style(self.status_state.hud_style, direction);
        let label = format!("HUD style: {}", self.status_state.hud_style);
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            &label,
            Some(Duration::from_secs(2)),
        );
        if let Some(session) = self.pty_session.as_mut() {
            update_pty_winsize(
                session,
                self.terminal_rows,
                self.terminal_cols,
                self.overlay_mode,
                self.status_state.hud_style,
            );
        }
    }

    pub(crate) fn toggle_hud_panel_recording_only(&mut self) {
        self.config.hud_right_panel_recording_only = !self.config.hud_right_panel_recording_only;
        self.status_state.hud_right_panel_recording_only =
            self.config.hud_right_panel_recording_only;
        let label = if self.config.hud_right_panel_recording_only {
            "HUD right panel: recording-only"
        } else {
            "HUD right panel: always on"
        };
        set_status(
            self.writer_tx,
            self.status_clear_deadline,
            self.current_status,
            self.status_state,
            label,
            Some(Duration::from_secs(2)),
        );
    }

    pub(crate) fn toggle_mouse(&mut self) {
        self.status_state.mouse_enabled = !self.status_state.mouse_enabled;
        if self.status_state.mouse_enabled {
            let _ = self.writer_tx.send(WriterMessage::EnableMouse);
            update_button_registry(
                self.button_registry,
                self.status_state,
                self.overlay_mode,
                *self.terminal_cols,
                *self.theme,
            );
            set_status(
                self.writer_tx,
                self.status_clear_deadline,
                self.current_status,
                self.status_state,
                "Mouse: ON - click HUD buttons",
                Some(Duration::from_secs(2)),
            );
        } else {
            let _ = self.writer_tx.send(WriterMessage::DisableMouse);
            self.button_registry.clear();
            self.status_state.hud_button_focus = None;
            set_status(
                self.writer_tx,
                self.status_clear_deadline,
                self.current_status,
                self.status_state,
                "Mouse: OFF",
                Some(Duration::from_secs(2)),
            );
        }
    }
}

fn cycle_hud_right_panel(current: HudRightPanel, direction: i32) -> HudRightPanel {
    const OPTIONS: &[HudRightPanel] = &[
        HudRightPanel::Ribbon,
        HudRightPanel::Dots,
        HudRightPanel::Heartbeat,
        HudRightPanel::Off,
    ];
    let len = OPTIONS.len() as i32;
    let idx = OPTIONS
        .iter()
        .position(|panel| *panel == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    OPTIONS[next]
}

fn cycle_hud_style(current: HudStyle, direction: i32) -> HudStyle {
    const OPTIONS: &[HudStyle] = &[HudStyle::Full, HudStyle::Minimal, HudStyle::Hidden];
    let len = OPTIONS.len() as i32;
    let idx = OPTIONS
        .iter()
        .position(|style| *style == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    OPTIONS[next]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buttons::ButtonRegistry;
    use crate::overlays::OverlayMode;
    use crate::status_line::StatusLineState;
    use crate::theme::Theme;
    use crate::voice_control::VoiceManager;
    use clap::Parser;
    use crossbeam_channel::bounded;

    fn make_context<'a>(
        config: &'a mut OverlayConfig,
        voice_manager: &'a mut VoiceManager,
        writer_tx: &'a Sender<WriterMessage>,
        status_clear_deadline: &'a mut Option<Instant>,
        current_status: &'a mut Option<String>,
        status_state: &'a mut StatusLineState,
        auto_voice_enabled: &'a mut bool,
        last_auto_trigger_at: &'a mut Option<Instant>,
        recording_started_at: &'a mut Option<Instant>,
        preview_clear_deadline: &'a mut Option<Instant>,
        last_meter_update: &'a mut Instant,
        button_registry: &'a ButtonRegistry,
        terminal_rows: &'a mut u16,
        terminal_cols: &'a mut u16,
        theme: &'a mut Theme,
    ) -> SettingsActionContext<'a> {
        SettingsActionContext::new(
            config,
            status_state,
            auto_voice_enabled,
            voice_manager,
            writer_tx,
            status_clear_deadline,
            current_status,
            last_auto_trigger_at,
            recording_started_at,
            preview_clear_deadline,
            last_meter_update,
            button_registry,
            OverlayMode::None,
            terminal_rows,
            terminal_cols,
            theme,
            None,
        )
    }

    #[test]
    fn toggle_send_mode_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_send_mode();
        }
        assert_eq!(config.voice_send_mode, VoiceSendMode::Insert);
        assert_eq!(status_state.send_mode, VoiceSendMode::Insert);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("insert"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        status_state.recording_duration = Some(1.7);
        status_state.meter_db = Some(-40.0);
        status_state.meter_levels.push(-40.0);
        status_state.transcript_preview = Some("stale preview".to_string());
        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_send_mode();
        }
        assert_eq!(config.voice_send_mode, VoiceSendMode::Auto);
        assert_eq!(status_state.send_mode, VoiceSendMode::Auto);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("auto"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn toggle_voice_intent_mode_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_voice_intent_mode();
        }
        assert_eq!(status_state.voice_intent_mode, VoiceIntentMode::Dictation);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("dictation"));
                assert!(state.message.contains("disabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_voice_intent_mode();
        }
        assert_eq!(status_state.voice_intent_mode, VoiceIntentMode::Command);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("command"));
                assert!(state.message.contains("enabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn adjust_sensitivity_updates_threshold_and_message() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.adjust_sensitivity(5.0);
        }
        let up_threshold = status_state.sensitivity_db;
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("less sensitive"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.adjust_sensitivity(-5.0);
        }
        assert!(status_state.sensitivity_db < up_threshold);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("more sensitive"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn cycle_hud_right_panel_wraps() {
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Ribbon, 1),
            HudRightPanel::Dots
        );
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Ribbon, -1),
            HudRightPanel::Off
        );
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Off, 1),
            HudRightPanel::Ribbon
        );
    }

    #[test]
    fn update_hud_panel_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        let mut ctx = make_context(
            &mut config,
            &mut voice_manager,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
            &mut auto_voice_enabled,
            &mut last_auto_trigger_at,
            &mut recording_started_at,
            &mut preview_clear_deadline,
            &mut last_meter_update,
            &button_registry,
            &mut terminal_rows,
            &mut terminal_cols,
            &mut theme,
        );

        ctx.update_hud_panel(1);
        assert_eq!(config.hud_right_panel, HudRightPanel::Dots);
        assert_eq!(status_state.hud_right_panel, HudRightPanel::Dots);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("HUD right panel"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn cycle_hud_style_wraps() {
        assert_eq!(cycle_hud_style(HudStyle::Full, 1), HudStyle::Minimal);
        assert_eq!(cycle_hud_style(HudStyle::Full, -1), HudStyle::Hidden);
        assert_eq!(cycle_hud_style(HudStyle::Hidden, 1), HudStyle::Full);
    }

    #[test]
    fn update_hud_style_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        let mut ctx = make_context(
            &mut config,
            &mut voice_manager,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
            &mut auto_voice_enabled,
            &mut last_auto_trigger_at,
            &mut recording_started_at,
            &mut preview_clear_deadline,
            &mut last_meter_update,
            &button_registry,
            &mut terminal_rows,
            &mut terminal_cols,
            &mut theme,
        );

        ctx.update_hud_style(1);
        assert_eq!(status_state.hud_style, HudStyle::Minimal);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("HUD style"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn toggle_auto_voice_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        config.app.no_python_fallback = true;
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();
        let button_registry = ButtonRegistry::new();
        let mut terminal_rows = 24;
        let mut terminal_cols = 80;
        let mut theme = Theme::Coral;

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_auto_voice();
        }
        assert!(auto_voice_enabled);
        assert_eq!(status_state.voice_mode, VoiceMode::Auto);
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("Auto-voice enabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        {
            let mut ctx = make_context(
                &mut config,
                &mut voice_manager,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
                &mut auto_voice_enabled,
                &mut last_auto_trigger_at,
                &mut recording_started_at,
                &mut preview_clear_deadline,
                &mut last_meter_update,
                &button_registry,
                &mut terminal_rows,
                &mut terminal_cols,
                &mut theme,
            );
            ctx.toggle_auto_voice();
        }
        assert!(!auto_voice_enabled);
        assert_eq!(status_state.voice_mode, VoiceMode::Manual);
        assert!(status_state.recording_duration.is_none());
        assert!(status_state.meter_db.is_none());
        assert!(status_state.meter_levels.is_empty());
        assert!(status_state.transcript_preview.is_none());
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("status message")
        {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("Auto-voice disabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }
}
