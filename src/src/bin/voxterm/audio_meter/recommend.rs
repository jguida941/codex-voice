//! Threshold recommendation logic so mic calibration maps to usable VAD settings.

use anyhow::{anyhow, Result};
use voxterm::config::{MAX_MIC_METER_SAMPLE_MS, MIN_MIC_METER_SAMPLE_MS};

use super::{RECOMMENDED_CEILING_DB, RECOMMENDED_FLOOR_DB};

pub(super) fn recommend_threshold(ambient_db: f32, speech_db: f32) -> (f32, Option<&'static str>) {
    if speech_db <= ambient_db {
        let suggested = (ambient_db + 1.0).clamp(RECOMMENDED_FLOOR_DB, RECOMMENDED_CEILING_DB);
        return (
            suggested,
            Some("Speech is not louder than ambient noise; results may be unreliable."),
        );
    }

    let margin = speech_db - ambient_db;
    let guard = if margin >= 12.0 {
        6.0
    } else if margin >= 6.0 {
        3.0
    } else {
        1.5
    };

    let mut suggested = ambient_db + guard;
    if suggested > speech_db - 1.0 {
        suggested = (ambient_db + speech_db) / 2.0;
    }

    let warning = if margin < 6.0 {
        Some("Speech is close to ambient noise; consider a quieter room or closer mic.")
    } else {
        None
    };

    (
        suggested.clamp(RECOMMENDED_FLOOR_DB, RECOMMENDED_CEILING_DB),
        warning,
    )
}

pub(super) fn validate_sample_ms(label: &str, value: u64) -> Result<()> {
    if !(MIN_MIC_METER_SAMPLE_MS..=MAX_MIC_METER_SAMPLE_MS).contains(&value) {
        return Err(anyhow!(
            "--mic-meter-{label}-ms must be between {MIN_MIC_METER_SAMPLE_MS} and {MAX_MIC_METER_SAMPLE_MS} ms"
        ));
    }
    Ok(())
}
