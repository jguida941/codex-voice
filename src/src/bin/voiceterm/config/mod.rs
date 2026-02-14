//! Overlay configuration assembly so CLI flags and backend defaults resolve consistently.

mod backend;
mod cli;
mod theme;
mod util;

#[allow(unused_imports)]
pub(crate) use backend::ResolvedBackend;
pub(crate) use cli::{
    HudBorderStyle, HudRightPanel, HudStyle, LatencyDisplayMode, OverlayConfig, VoiceSendMode,
};
#[allow(unused_imports)]
pub(crate) use theme::default_theme_for_backend;
