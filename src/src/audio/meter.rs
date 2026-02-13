//! Shared decibel meter state that keeps UI level reads thread-safe and cheap.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const DEFAULT_METER_DB: f32 = -60.0;

/// Thread-safe meter that tracks the latest audio level in dB.
#[derive(Clone, Debug)]
pub struct LiveMeter {
    level_bits: Arc<AtomicU32>,
}

impl LiveMeter {
    /// Initialize to a floor value so UI status starts from silence, not zero.
    pub fn new() -> Self {
        Self {
            level_bits: Arc::new(AtomicU32::new(DEFAULT_METER_DB.to_bits())),
        }
    }

    /// Store a new dB value.
    pub fn set_db(&self, db: f32) {
        self.level_bits.store(db.to_bits(), Ordering::Relaxed);
    }

    /// Read the most recent dB value.
    pub fn level_db(&self) -> f32 {
        f32::from_bits(self.level_bits.load(Ordering::Relaxed))
    }
}

impl Default for LiveMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute RMS level in dB for the provided samples.
pub(crate) fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return DEFAULT_METER_DB;
    }
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = energy.sqrt().max(1e-6);
    20.0 * rms.log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_meter_defaults_to_floor() {
        let meter = LiveMeter::new();
        assert_eq!(meter.level_db(), DEFAULT_METER_DB);
    }

    #[test]
    fn live_meter_updates_level() {
        let meter = LiveMeter::new();
        meter.set_db(-20.0);
        assert_eq!(meter.level_db(), -20.0);
    }

    #[test]
    fn rms_db_handles_empty() {
        assert_eq!(rms_db(&[]), DEFAULT_METER_DB);
    }

    #[test]
    fn rms_db_reports_unity_as_zero_db() {
        let db = rms_db(&[1.0, -1.0]);
        assert!(db.abs() < 1e-4, "expected 0 dB, got {db}");
    }

    #[test]
    fn rms_db_reports_half_scale_level() {
        let db = rms_db(&[0.5, -0.5]);
        let expected = -6.0206_f32;
        assert!(
            (db - expected).abs() < 0.05,
            "expected ~{expected} dB, got {db}"
        );
    }
}
