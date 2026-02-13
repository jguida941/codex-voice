//! Help/settings overlay rendering so panel layout stays centralized and consistent.

use crossbeam_channel::Sender;

use crate::config::OverlayConfig;
use crate::help::{format_help_overlay, help_overlay_height};
use crate::settings::{
    format_settings_overlay, settings_overlay_height, SettingsMenuState, SettingsView,
};
use crate::status_line::StatusLineState;
use crate::theme::Theme;
use crate::theme_picker::{format_theme_picker, theme_picker_height};
use crate::writer::WriterMessage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayMode {
    None,
    Help,
    ThemePicker,
    Settings,
}

pub(crate) fn show_settings_overlay(
    writer_tx: &Sender<WriterMessage>,
    theme: Theme,
    cols: u16,
    settings_menu: &SettingsMenuState,
    config: &OverlayConfig,
    status_state: &StatusLineState,
    backend_label: &str,
) {
    let view = SettingsView {
        selected: settings_menu.selected,
        auto_voice_enabled: status_state.auto_voice_enabled,
        send_mode: config.voice_send_mode,
        voice_intent_mode: status_state.voice_intent_mode,
        sensitivity_db: status_state.sensitivity_db,
        theme,
        hud_style: status_state.hud_style,
        hud_right_panel: config.hud_right_panel,
        hud_right_panel_recording_only: config.hud_right_panel_recording_only,
        mouse_enabled: status_state.mouse_enabled,
        backend_label,
        pipeline: status_state.pipeline,
    };
    let content = format_settings_overlay(&view, cols as usize);
    let height = settings_overlay_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

pub(crate) fn show_theme_picker_overlay(
    writer_tx: &Sender<WriterMessage>,
    theme: Theme,
    selected_idx: usize,
    cols: u16,
) {
    let content = format_theme_picker(theme, selected_idx, cols as usize);
    let height = theme_picker_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

pub(crate) fn show_help_overlay(writer_tx: &Sender<WriterMessage>, theme: Theme, cols: u16) {
    let content = format_help_overlay(theme, cols as usize);
    let height = help_overlay_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status_line::Pipeline;
    use clap::Parser;
    use crossbeam_channel::bounded;

    #[test]
    fn show_settings_overlay_sends_overlay() {
        let config = OverlayConfig::parse_from(["test-app"]);
        let settings_menu = SettingsMenuState::new();
        let mut status_state = StatusLineState::new();
        status_state.pipeline = Pipeline::Rust;
        let (writer_tx, writer_rx) = bounded(4);
        show_settings_overlay(
            &writer_tx,
            Theme::Coral,
            80,
            &settings_menu,
            &config,
            &status_state,
            "codex",
        );
        match writer_rx
            .recv_timeout(std::time::Duration::from_millis(200))
            .expect("overlay message")
        {
            WriterMessage::ShowOverlay { height, .. } => {
                assert_eq!(height, settings_overlay_height());
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }
}
