//! Terminal/theme detection so defaults match the host terminal environment.

use std::env;

pub(super) fn is_warp_terminal() -> bool {
    env::var("TERM_PROGRAM")
        .map(|value| value.to_lowercase().contains("warp"))
        .unwrap_or(false)
}
