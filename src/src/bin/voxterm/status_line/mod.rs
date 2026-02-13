//! Status-line module wiring that keeps multi-row HUD formatting composable.
//!
//! Provides a structured status line with mode indicator, pipeline tag,
//! sensitivity level, status message, and keyboard shortcuts.
//!
//! Now supports a multi-row banner layout with themed borders.
//! Buttons are clickable - click positions are tracked for mouse interaction.

mod animation;
mod buttons;
mod format;
mod layout;
mod state;
mod text;

pub use buttons::get_button_positions;
pub use format::format_status_banner;
pub use layout::status_banner_height;
#[allow(unused_imports)]
pub use state::ButtonPosition;
pub use state::{
    Pipeline, RecordingState, StatusBanner, StatusLineState, VoiceIntentMode, VoiceMode,
    METER_HISTORY_MAX,
};
