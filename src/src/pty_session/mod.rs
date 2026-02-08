//! Minimal PTY wrapper used to host backend CLIs in a real terminal so persistent sessions
//! can keep state (tools, environment) between prompts.

mod counters;
mod io;
mod osc;
mod pty;

#[cfg(test)]
mod tests;

pub use pty::{PtyCliSession, PtyOverlaySession};

#[cfg(any(test, feature = "mutants"))]
#[allow(unused_imports)]
pub(crate) use counters::{
    apply_linestart_recalc_count, apply_osc_hits, apply_osc_start, pty_session_read_count,
    pty_session_send_count, reset_apply_linestart_recalc_count, reset_apply_osc_counters,
    reset_pty_session_counters, reset_respond_osc_counters, reset_wait_for_exit_counters,
    respond_osc_hits, respond_osc_start, set_read_output_elapsed_override,
    set_read_output_grace_override, set_terminal_size_override, set_wait_for_exit_elapsed_override,
    set_write_all_limit, wait_for_exit_error_count, wait_for_exit_poll_count,
    wait_for_exit_reap_count,
};

#[cfg(any(test, feature = "mutants"))]
#[allow(unused_imports)]
pub(crate) use pty::test_pty_session;
