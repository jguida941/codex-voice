use crate::audio::Recorder;
use crate::config::AppConfig;
use anyhow::{anyhow, Result};
use std::io::{self, Write};
use std::time::Duration;

const MIN_SAMPLE_MS: u64 = 500;
const MAX_SAMPLE_MS: u64 = 30_000;
const RECOMMENDED_FLOOR_DB: f32 = -80.0;
const RECOMMENDED_CEILING_DB: f32 = -10.0;

#[derive(Debug, Clone, Copy)]
struct MeterReading {
    rms_db: f32,
    peak_db: f32,
}

fn rms_db(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return RECOMMENDED_FLOOR_DB;
    }
    let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = energy.sqrt().max(1e-6);
    20.0 * rms.log10()
}

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

fn measure(recorder: &Recorder, duration: Duration) -> Result<MeterReading> {
    let samples = recorder.record_for(duration)?;
    Ok(MeterReading {
        rms_db: rms_db(&samples),
        peak_db: peak_db(&samples),
    })
}

fn recommend_threshold(ambient_db: f32, speech_db: f32) -> (f32, Option<&'static str>) {
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

    (suggested.clamp(RECOMMENDED_FLOOR_DB, RECOMMENDED_CEILING_DB), warning)
}

fn validate_sample_ms(label: &str, value: u64) -> Result<()> {
    if value < MIN_SAMPLE_MS || value > MAX_SAMPLE_MS {
        return Err(anyhow!(
            "--mic-meter-{label}-ms must be between {MIN_SAMPLE_MS} and {MAX_SAMPLE_MS} ms"
        ));
    }
    Ok(())
}

pub fn run_mic_meter(config: &AppConfig) -> Result<()> {
    validate_sample_ms("ambient", config.mic_meter_ambient_ms)?;
    validate_sample_ms("speech", config.mic_meter_speech_ms)?;

    let recorder = Recorder::new(config.input_device.as_deref())?;
    println!("Mic meter using input device: {}", recorder.device_name());

    let ambient_ms = config.mic_meter_ambient_ms;
    let speech_ms = config.mic_meter_speech_ms;

    print!(
        "Sampling ambient noise for {:.1}s... stay quiet.\n",
        ambient_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let ambient = measure(&recorder, Duration::from_millis(ambient_ms))?;

    print!(
        "Sampling speech for {:.1}s... speak normally.\n",
        speech_ms as f32 / 1000.0
    );
    io::stdout().flush().ok();
    let speech = measure(&recorder, Duration::from_millis(speech_ms))?;

    println!("\nResults (dBFS)");
    println!("Ambient: RMS {ambient_rms:.1} dB, Peak {ambient_peak:.1} dB",
        ambient_rms = ambient.rms_db,
        ambient_peak = ambient.peak_db
    );
    println!("Speech:  RMS {speech_rms:.1} dB, Peak {speech_peak:.1} dB",
        speech_rms = speech.rms_db,
        speech_peak = speech.peak_db
    );

    let (suggested, warning) = recommend_threshold(ambient.rms_db, speech.rms_db);
    println!("\nSuggested --voice-vad-threshold-db: {suggested:.1}");
    println!("Example: codex-voice --voice-vad-threshold-db {suggested:.1}");

    if let Some(message) = warning {
        println!("Note: {message}");
    }

    Ok(())
}
