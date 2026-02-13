//! Synthetic benchmark binary that tracks voice-capture and VAD latency regressions.

use std::f32::consts::PI;

#[cfg(not(feature = "vad_earshot"))]
use anyhow::bail;
use anyhow::Result;
use clap::Parser;
use voxterm::audio::{self, VadEngine};
use voxterm::config::{
    default_vad_engine, VadEngineKind, VoicePipelineConfig, DEFAULT_VOICE_BUFFER_MS,
    DEFAULT_VOICE_CHANNEL_CAPACITY, DEFAULT_VOICE_LOOKBACK_MS, DEFAULT_VOICE_MAX_CAPTURE_MS,
    DEFAULT_VOICE_MIN_SPEECH_MS, DEFAULT_VOICE_SAMPLE_RATE, DEFAULT_VOICE_SILENCE_TAIL_MS,
    DEFAULT_VOICE_STT_TIMEOUT_MS, DEFAULT_VOICE_VAD_FRAME_MS, DEFAULT_VOICE_VAD_SMOOTHING_FRAMES,
    DEFAULT_VOICE_VAD_THRESHOLD_DB,
};
#[cfg(feature = "vad_earshot")]
use voxterm::vad_earshot;

/// Synthetic benchmark harness for voice capture latency.
#[derive(Debug, Parser)]
#[command(about = "Benchmark the silence-aware capture loop with synthetic clips")]
struct Args {
    /// Human-friendly label recorded in the output metrics
    #[arg(long, default_value = "clip")]
    label: String,

    /// Duration of the synthetic speech segment (milliseconds)
    #[arg(long, default_value_t = 1_000)]
    speech_ms: u64,

    /// Duration of trailing silence appended after speech (milliseconds)
    #[arg(long, default_value_t = 500)]
    silence_ms: u64,

    #[arg(long = "voice-sample-rate", default_value_t = DEFAULT_VOICE_SAMPLE_RATE)]
    voice_sample_rate: u32,

    #[arg(long = "voice-max-capture-ms", default_value_t = DEFAULT_VOICE_MAX_CAPTURE_MS)]
    voice_max_capture_ms: u64,

    #[arg(long = "voice-silence-tail-ms", default_value_t = DEFAULT_VOICE_SILENCE_TAIL_MS)]
    voice_silence_tail_ms: u64,

    #[arg(
        long = "voice-min-speech-ms-before-stt",
        default_value_t = DEFAULT_VOICE_MIN_SPEECH_MS
    )]
    voice_min_speech_ms_before_stt_start: u64,

    #[arg(long = "voice-lookback-ms", default_value_t = DEFAULT_VOICE_LOOKBACK_MS)]
    voice_lookback_ms: u64,

    #[arg(long = "voice-buffer-ms", default_value_t = DEFAULT_VOICE_BUFFER_MS)]
    voice_buffer_ms: u64,

    #[arg(
        long = "voice-channel-capacity",
        default_value_t = DEFAULT_VOICE_CHANNEL_CAPACITY
    )]
    voice_channel_capacity: usize,

    #[arg(long = "voice-stt-timeout-ms", default_value_t = DEFAULT_VOICE_STT_TIMEOUT_MS)]
    voice_stt_timeout_ms: u64,

    #[arg(
        long = "voice-vad-threshold-db",
        default_value_t = DEFAULT_VOICE_VAD_THRESHOLD_DB
    )]
    voice_vad_threshold_db: f32,

    #[arg(long = "voice-vad-frame-ms", default_value_t = DEFAULT_VOICE_VAD_FRAME_MS)]
    voice_vad_frame_ms: u64,

    #[arg(
        long = "voice-vad-smoothing-frames",
        default_value_t = DEFAULT_VOICE_VAD_SMOOTHING_FRAMES
    )]
    voice_vad_smoothing_frames: usize,

    #[arg(
        long = "voice-vad-engine",
        value_enum,
        default_value_t = default_vad_engine()
    )]
    voice_vad_engine: VadEngineKind,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure_vad_engine_supported(&args)?;
    let clip = synthesize_clip(args.speech_ms, args.silence_ms, args.voice_sample_rate);
    let pipeline_cfg = build_pipeline_config(&args);
    let vad_cfg: audio::VadConfig = (&pipeline_cfg).into();
    let mut vad_engine = build_vad_engine(&pipeline_cfg);
    let result = audio::offline_capture_from_pcm(&clip, &vad_cfg, vad_engine.as_mut());

    println!(
        "voice_metrics|label={}|capture_ms={}|speech_ms={}|silence_tail_ms={}|frames_processed={}|frames_dropped={}|early_stop={}",
        args.label,
        result.metrics.capture_ms,
        result.metrics.speech_ms,
        result.metrics.silence_tail_ms,
        result.metrics.frames_processed,
        result.metrics.frames_dropped,
        result.metrics.early_stop_reason.label()
    );

    Ok(())
}

fn build_pipeline_config(args: &Args) -> VoicePipelineConfig {
    VoicePipelineConfig {
        sample_rate: args.voice_sample_rate,
        max_capture_ms: args.voice_max_capture_ms,
        silence_tail_ms: args.voice_silence_tail_ms,
        min_speech_ms_before_stt_start: args.voice_min_speech_ms_before_stt_start,
        lookback_ms: args.voice_lookback_ms,
        buffer_ms: args.voice_buffer_ms,
        channel_capacity: args.voice_channel_capacity,
        stt_timeout_ms: args.voice_stt_timeout_ms,
        vad_threshold_db: args.voice_vad_threshold_db,
        vad_frame_ms: args.voice_vad_frame_ms,
        vad_smoothing_frames: args.voice_vad_smoothing_frames,
        python_fallback_allowed: true,
        vad_engine: args.voice_vad_engine,
    }
}

fn synthesize_clip(speech_ms: u64, silence_ms: u64, sample_rate: u32) -> Vec<f32> {
    let mut samples = Vec::new();
    let speech_samples = (speech_ms * sample_rate as u64 / 1000) as usize;
    let silence_samples = (silence_ms * sample_rate as u64 / 1000) as usize;
    for n in 0..speech_samples {
        let t = n as f32 / sample_rate as f32;
        let sample = (2.0 * PI * 440.0 * t).sin() * 0.4;
        samples.push(sample);
    }
    samples.extend(std::iter::repeat_n(0.0, silence_samples));
    samples
}

fn build_vad_engine(cfg: &VoicePipelineConfig) -> Box<dyn VadEngine> {
    match cfg.vad_engine {
        VadEngineKind::Simple => Box::new(audio::SimpleThresholdVad::new(cfg.vad_threshold_db)),
        VadEngineKind::Earshot => {
            #[cfg(feature = "vad_earshot")]
            {
                Box::new(vad_earshot::EarshotVad::from_config(cfg))
            }
            #[cfg(not(feature = "vad_earshot"))]
            {
                unreachable!("earshot VAD requested without enabling the 'vad_earshot' feature")
            }
        }
    }
}

/// Keep the benchmark binary in lockstep with the main TUI validation.
#[cfg(not(feature = "vad_earshot"))]
fn ensure_vad_engine_supported(args: &Args) -> Result<()> {
    if matches!(args.voice_vad_engine, VadEngineKind::Earshot) {
        bail!("--voice-vad-engine earshot requires building with the 'vad_earshot' feature");
    }

    Ok(())
}

#[cfg(feature = "vad_earshot")]
fn ensure_vad_engine_supported(_args: &Args) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[cfg(not(feature = "vad_earshot"))]
    #[test]
    fn earshot_flag_errors_without_feature() {
        let args =
            Args::try_parse_from(["voice_benchmark", "--voice-vad-engine", "earshot"]).unwrap();
        let err = ensure_vad_engine_supported(&args).unwrap_err();
        assert!(err
            .to_string()
            .contains("requires building with the 'vad_earshot' feature"));
    }

    #[cfg(not(feature = "vad_earshot"))]
    #[test]
    fn simple_flag_allowed_without_earshot_feature() {
        let args =
            Args::try_parse_from(["voice_benchmark", "--voice-vad-engine", "simple"]).unwrap();
        ensure_vad_engine_supported(&args).expect("simple should remain available");
    }

    #[cfg(feature = "vad_earshot")]
    #[test]
    fn earshot_flag_passes_when_feature_enabled() {
        let args =
            Args::try_parse_from(["voice_benchmark", "--voice-vad-engine", "earshot"]).unwrap();
        ensure_vad_engine_supported(&args).expect("earshot feature enabled");
    }
}
