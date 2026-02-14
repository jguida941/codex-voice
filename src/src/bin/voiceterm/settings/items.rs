//! Settings item metadata so menus render and dispatch actions from one schema.

use crate::config::{HudBorderStyle, HudRightPanel, HudStyle, LatencyDisplayMode, VoiceSendMode};
use crate::status_line::Pipeline;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsItem {
    AutoVoice,
    SendMode,
    Macros,
    Sensitivity,
    Theme,
    HudStyle,
    HudBorders,
    HudPanel,
    HudAnimate,
    Latency,
    Mouse,
    Backend,
    Pipeline,
    Close,
    Quit,
}

pub const SETTINGS_ITEMS: &[SettingsItem] = &[
    SettingsItem::AutoVoice,
    SettingsItem::SendMode,
    SettingsItem::Macros,
    SettingsItem::Sensitivity,
    SettingsItem::Theme,
    SettingsItem::HudStyle,
    SettingsItem::HudBorders,
    SettingsItem::HudPanel,
    SettingsItem::HudAnimate,
    SettingsItem::Latency,
    SettingsItem::Mouse,
    SettingsItem::Backend,
    SettingsItem::Pipeline,
    SettingsItem::Close,
    SettingsItem::Quit,
];

pub const SETTINGS_OVERLAY_FOOTER: &str = "[x] close · arrows · Enter select";

pub fn settings_overlay_width_for_terminal(width: usize) -> usize {
    width.saturating_sub(4).clamp(24, 70)
}

pub fn settings_overlay_inner_width_for_terminal(width: usize) -> usize {
    settings_overlay_width_for_terminal(width).saturating_sub(2)
}

pub fn settings_overlay_height() -> usize {
    // Top border + title + separator + items + separator + footer + bottom border
    SETTINGS_ITEMS.len() + 6
}

pub struct SettingsView<'a> {
    pub selected: usize,
    pub auto_voice_enabled: bool,
    pub send_mode: VoiceSendMode,
    pub macros_enabled: bool,
    pub sensitivity_db: f32,
    pub theme: Theme,
    pub hud_style: HudStyle,
    pub hud_border_style: HudBorderStyle,
    pub hud_right_panel: HudRightPanel,
    pub hud_right_panel_recording_only: bool,
    pub latency_display: LatencyDisplayMode,
    pub mouse_enabled: bool,
    pub backend_label: &'a str,
    pub pipeline: Pipeline,
}
