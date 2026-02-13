//! Audio-level sampling so mic calibration reflects real ambient/speech levels.

use anyhow::Result;
use std::time::Duration;
use voxterm::audio::Recorder;

use super::{AudioLevel, RECOMMENDED_FLOOR_DB};

#[inline]
fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return RECOMMENDED_FLOOR_DB;
    }
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = energy.sqrt().max(1e-6);
    20.0 * rms.log10()
}

#[inline]
fn peak_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return RECOMMENDED_FLOOR_DB;
    }
    let peak = samples
        .iter()
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max)
        .max(1e-6);
    20.0 * peak.log10()
}

pub(super) fn measure(recorder: &Recorder, duration: Duration) -> Result<AudioLevel> {
    let samples = recorder.record_for(duration)?;
    Ok(AudioLevel {
        rms_db: rms_db(&samples),
        peak_db: peak_db(&samples),
    })
}
