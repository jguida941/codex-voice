//! PTY loop counters used to detect backpressure and guard regressions in tests.

use std::time::{Duration, Instant};

#[cfg(any(test, feature = "mutants"))]
use std::cell::Cell;
#[cfg(any(test, feature = "mutants"))]
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
#[cfg(any(test, feature = "mutants"))]
use std::sync::{Mutex, OnceLock};

#[cfg(any(test, feature = "mutants"))]
thread_local! {
    static PTY_SEND_COUNT: Cell<usize> = const { Cell::new(0) };
    static PTY_READ_COUNT: Cell<usize> = const { Cell::new(0) };
}
#[cfg(any(test, feature = "mutants"))]
static READ_OUTPUT_GRACE_OVERRIDE_MS: AtomicU64 = AtomicU64::new(u64::MAX);
#[cfg(any(test, feature = "mutants"))]
static READ_OUTPUT_ELAPSED_OVERRIDE_MS: AtomicU64 = AtomicU64::new(u64::MAX);
#[cfg(any(test, feature = "mutants"))]
static WAIT_FOR_EXIT_ELAPSED_OVERRIDE_MS: AtomicU64 = AtomicU64::new(u64::MAX);
#[cfg(any(test, feature = "mutants"))]
static WAIT_FOR_EXIT_POLL_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(any(test, feature = "mutants"))]
static WAIT_FOR_EXIT_REAP_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(any(test, feature = "mutants"))]
static WAIT_FOR_EXIT_ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(any(test, feature = "mutants"))]
thread_local! {
    static RESPOND_OSC_START: Cell<usize> = const { Cell::new(usize::MAX) };
    static RESPOND_OSC_HITS: Cell<usize> = const { Cell::new(0) };
    static APPLY_OSC_START: Cell<usize> = const { Cell::new(usize::MAX) };
    static APPLY_OSC_HITS: Cell<usize> = const { Cell::new(0) };
    static APPLY_LINESTART_RECALC_COUNT: Cell<usize> = const { Cell::new(0) };
}
#[cfg(any(test, feature = "mutants"))]
thread_local! {
    static WRITE_ALL_LIMIT: Cell<usize> = const { Cell::new(usize::MAX) };
}
#[cfg(any(test, feature = "mutants"))]
static TERMINAL_SIZE_OVERRIDE: OnceLock<Mutex<Option<(bool, u16, u16)>>> = OnceLock::new();

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn reset_pty_session_counters() {
    PTY_SEND_COUNT.with(|count| count.set(0));
    PTY_READ_COUNT.with(|count| count.set(0));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn pty_session_send_count() -> usize {
    PTY_SEND_COUNT.with(|count| count.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn pty_session_read_count() -> usize {
    PTY_READ_COUNT.with(|count| count.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn set_read_output_grace_override(ms: Option<u64>) {
    READ_OUTPUT_GRACE_OVERRIDE_MS.store(ms.unwrap_or(u64::MAX), Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn set_read_output_elapsed_override(ms: Option<u64>) {
    READ_OUTPUT_ELAPSED_OVERRIDE_MS.store(ms.unwrap_or(u64::MAX), Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn set_write_all_limit(limit: Option<usize>) {
    WRITE_ALL_LIMIT.with(|value| value.set(limit.unwrap_or(usize::MAX)));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn set_terminal_size_override(value: Option<(bool, u16, u16)>) {
    let lock = TERMINAL_SIZE_OVERRIDE.get_or_init(|| Mutex::new(None));
    *lock.lock().unwrap() = value;
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn set_wait_for_exit_elapsed_override(ms: Option<u64>) {
    WAIT_FOR_EXIT_ELAPSED_OVERRIDE_MS.store(ms.unwrap_or(u64::MAX), Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn reset_wait_for_exit_counters() {
    WAIT_FOR_EXIT_POLL_COUNT.store(0, Ordering::SeqCst);
    WAIT_FOR_EXIT_REAP_COUNT.store(0, Ordering::SeqCst);
    WAIT_FOR_EXIT_ERROR_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn wait_for_exit_poll_count() -> usize {
    WAIT_FOR_EXIT_POLL_COUNT.load(Ordering::SeqCst)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn wait_for_exit_reap_count() -> usize {
    WAIT_FOR_EXIT_REAP_COUNT.load(Ordering::SeqCst)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn wait_for_exit_error_count() -> usize {
    WAIT_FOR_EXIT_ERROR_COUNT.load(Ordering::SeqCst)
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn reset_respond_osc_counters() {
    RESPOND_OSC_START.with(|val| val.set(usize::MAX));
    RESPOND_OSC_HITS.with(|val| val.set(0));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn respond_osc_start() -> usize {
    RESPOND_OSC_START.with(|val| val.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn respond_osc_hits() -> usize {
    RESPOND_OSC_HITS.with(|val| val.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn reset_apply_osc_counters() {
    APPLY_OSC_START.with(|val| val.set(usize::MAX));
    APPLY_OSC_HITS.with(|val| val.set(0));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn apply_osc_start() -> usize {
    APPLY_OSC_START.with(|val| val.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn apply_osc_hits() -> usize {
    APPLY_OSC_HITS.with(|val| val.get())
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn reset_apply_linestart_recalc_count() {
    APPLY_LINESTART_RECALC_COUNT.with(|val| val.set(0));
}

#[cfg(any(test, feature = "mutants"))]
#[allow(dead_code)]
pub(crate) fn apply_linestart_recalc_count() -> usize {
    APPLY_LINESTART_RECALC_COUNT.with(|val| val.get())
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_respond_osc_start(start: usize) {
    RESPOND_OSC_START.with(|val| val.set(start));
    RESPOND_OSC_HITS.with(|val| val.set(val.get().saturating_add(1)));
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_apply_osc_start(start: usize) {
    APPLY_OSC_START.with(|val| val.set(start));
    APPLY_OSC_HITS.with(|val| val.set(val.get().saturating_add(1)));
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_apply_linestart_recalc() {
    APPLY_LINESTART_RECALC_COUNT.with(|val| val.set(val.get().saturating_add(1)));
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_pty_send() {
    PTY_SEND_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_pty_read() {
    PTY_READ_COUNT.with(|count| count.set(count.get().saturating_add(1)));
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_wait_for_exit_poll() {
    WAIT_FOR_EXIT_POLL_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_wait_for_exit_reap() {
    WAIT_FOR_EXIT_REAP_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn record_wait_for_exit_error() {
    WAIT_FOR_EXIT_ERROR_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn guard_elapsed_exceeded(elapsed: Duration, iterations: usize, limit: usize) -> bool {
    elapsed > Duration::from_secs(2) || iterations > limit
}

#[cfg(any(test, feature = "mutants"))]
pub(super) fn guard_loop(start: Instant, iterations: usize, limit: usize, label: &str) {
    if guard_elapsed_exceeded(start.elapsed(), iterations, limit) {
        panic!("{label} loop guard exceeded");
    }
}

pub(super) fn read_output_elapsed(start: Instant) -> Duration {
    #[cfg(any(test, feature = "mutants"))]
    {
        let override_ms = READ_OUTPUT_ELAPSED_OVERRIDE_MS.load(Ordering::SeqCst);
        if override_ms != u64::MAX {
            return Duration::from_millis(override_ms);
        }
    }
    start.elapsed()
}

pub(super) fn read_output_grace_elapsed(last: Instant) -> Duration {
    #[cfg(any(test, feature = "mutants"))]
    {
        let override_ms = READ_OUTPUT_GRACE_OVERRIDE_MS.load(Ordering::SeqCst);
        if override_ms != u64::MAX {
            return Duration::from_millis(override_ms);
        }
    }
    last.elapsed()
}

pub(super) fn wait_for_exit_elapsed(start: Instant) -> Duration {
    #[cfg(any(test, feature = "mutants"))]
    {
        let override_ms = WAIT_FOR_EXIT_ELAPSED_OVERRIDE_MS.load(Ordering::SeqCst);
        if override_ms != u64::MAX {
            return Duration::from_millis(override_ms);
        }
    }
    start.elapsed()
}

pub(super) fn write_all_limit(len: usize) -> usize {
    #[cfg(any(test, feature = "mutants"))]
    {
        WRITE_ALL_LIMIT.with(|limit| len.min(limit.get()))
    }
    #[cfg(not(any(test, feature = "mutants")))]
    {
        len
    }
}

pub(super) fn terminal_size_override() -> Option<(bool, u16, u16)> {
    #[cfg(any(test, feature = "mutants"))]
    {
        *TERMINAL_SIZE_OVERRIDE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap()
    }
    #[cfg(not(any(test, feature = "mutants")))]
    {
        None
    }
}
