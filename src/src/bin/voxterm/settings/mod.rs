//! Settings overlay wiring so menu rendering and actions evolve together.

mod items;
mod render;
mod state;

#[allow(unused_imports)]
pub use items::{
    settings_overlay_height, settings_overlay_inner_width_for_terminal,
    settings_overlay_width_for_terminal, SettingsItem, SettingsView, SETTINGS_ITEMS,
    SETTINGS_OVERLAY_FOOTER,
};
pub use render::format_settings_overlay;
pub use state::SettingsMenuState;
