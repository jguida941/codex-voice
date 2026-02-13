//! Mutex lock recovery so one poisoned lock does not kill long-running sessions.

use std::sync::{Mutex, MutexGuard};

pub(crate) fn lock_or_recover<'a, T>(lock: &'a Mutex<T>, context: &str) -> MutexGuard<'a, T> {
    match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            crate::log_debug(&format!("Mutex poisoned in {context}; recovering"));
            poisoned.into_inner()
        }
    }
}
