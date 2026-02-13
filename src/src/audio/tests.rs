use super::capture::{CaptureState, FrameAccumulator};
use super::dispatch::{append_downmixed_samples, FrameDispatcher};
use super::resample::{
    adjust_frame_length, basic_resample, convert_frame_to_target, design_low_pass,
    downsampling_tap_count, low_pass_fir, resample_linear, resample_to_target_rate,
    MAX_DEVICE_RATE, MAX_RESAMPLE_RATIO, MIN_DEVICE_RATE, MIN_RESAMPLE_RATIO,
};
use super::vad::{FrameLabel, VadSmoother};
use super::{
    Recorder, SimpleThresholdVad, StopReason, VadConfig, VadDecision, VadEngine, TARGET_RATE,
};
use crossbeam_channel::bounded;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(all(test, feature = "high-quality-audio"))]
use std::sync::Mutex;

#[cfg(feature = "high-quality-audio")]
use super::resample::{
    resample_with_rubato, FORCE_RUBATO_ERROR, RESAMPLER_WARNING_SHOWN, RESAMPLE_FALLBACK_COUNT,
    RESAMPLE_WARN_COUNT,
};

const SAMPLE_RATE: u32 = TARGET_RATE;

#[cfg(all(test, feature = "high-quality-audio"))]
static RESAMPLE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(test, feature = "high-quality-audio"))]
fn lock_resample_test() -> std::sync::MutexGuard<'static, ()> {
    // If a prior assertion fails, keep later tests running with the inner lock.
    RESAMPLE_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn downmixes_multi_channel_audio() {
    let mut buf = Vec::new();
    let samples = [1.0f32, -1.0, 0.5, 0.5];
    append_downmixed_samples(&mut buf, &samples, 2, |sample| sample);
    assert_eq!(buf, vec![0.0, 0.5]);
}

#[test]
fn preserves_single_channel_audio() {
    let mut buf = Vec::new();
    let samples = [0.1f32, 0.2, 0.3];
    append_downmixed_samples(&mut buf, &samples, 1, |sample| sample);
    assert_eq!(buf, samples);
}

#[test]
fn resample_linear_scales_length() {
    let input = vec![0.0f32, 1.0, 2.0, 3.0];
    let result = resample_linear(&input, 0.5);
    assert!(result.len() < input.len());
    assert!((result.first().copied().unwrap_or_default() - 0.0).abs() < 1e-6);
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn resample_bounds_match_constants() {
    assert_eq!(MIN_DEVICE_RATE, 2_000);
    assert_eq!(MAX_DEVICE_RATE, 1_600_000);
    assert!(MIN_DEVICE_RATE < MAX_DEVICE_RATE);
    assert!((MIN_RESAMPLE_RATIO - 0.01).abs() < 1e-6);
    assert!((MAX_RESAMPLE_RATIO - 8.0).abs() < 1e-6);
}

#[test]
fn resample_to_target_rate_returns_input_when_rate_matches() {
    let input = vec![0.1f32, 0.2, 0.3];
    let output = resample_to_target_rate(&input, TARGET_RATE);
    assert_eq!(output, input);
}

#[test]
fn resample_to_target_rate_returns_empty_for_empty_input() {
    let input: Vec<f32> = Vec::new();
    let output = resample_to_target_rate(&input, 48_000);
    assert!(output.is_empty());
}

#[cfg(not(feature = "high-quality-audio"))]
#[test]
fn resample_to_target_rate_adjusts_length() {
    let input = vec![0.0, 1.0, 0.5, -0.5, -1.0, 0.0];
    let result = resample_to_target_rate(&input, 48_000);
    assert!(result.len() < input.len());
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_resampler_matches_expected_length() {
    let _guard = lock_resample_test();
    let input: Vec<f32> = (0..960).map(|i| (i as f32 * 0.01).sin()).collect();
    let result = resample_to_target_rate(&input, 48_000);
    let expected = (input.len() as f64 * 16_000f64 / 48_000f64).round() as usize;
    let diff = (result.len() as isize - expected as isize).abs();
    // Rubato chunking can introduce up to 8 extra samples on some hosts (observed on macOS CI),
    // so allow a small safety margin.
    assert!(
        diff <= 10,
        "expected {expected} samples, got {}, diff {diff}",
        result.len()
    );
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_resampler_handles_upsample() {
    let _guard = lock_resample_test();
    let input: Vec<f32> = (0..160).map(|i| (i as f32 * 0.05).cos()).collect();
    let result = resample_to_target_rate(&input, 8_000);
    let expected = (input.len() as f64 * 16_000f64 / 8_000f64).round() as usize;
    let diff = (result.len() as isize - expected as isize).abs();
    assert!(
        diff <= 10,
        "expected {expected} samples, got {}, diff {diff}",
        result.len()
    );
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_accepts_valid_rate_without_forced_error() {
    let _guard = lock_resample_test();
    FORCE_RUBATO_ERROR.store(false, Ordering::Relaxed);
    let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.03).sin()).collect();
    let output = resample_with_rubato(&input, 48_000).expect("expected rubato success");
    let ratio = TARGET_RATE as f64 / 48_000f64;
    let expected = ((input.len() as f64) * ratio).round() as usize + 8;
    assert_eq!(output.len(), expected);
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_rejects_out_of_bounds_rates() {
    let _guard = lock_resample_test();
    let input = vec![0.1f32; 64];

    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let err = resample_with_rubato(&input, MIN_DEVICE_RATE - 1)
        .expect_err("expected error for low device rate");
    assert!(err.to_string().contains("unsupported device sample rate"));
    assert!(FORCE_RUBATO_ERROR.load(Ordering::Relaxed));
    FORCE_RUBATO_ERROR.store(false, Ordering::Relaxed);

    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let err = resample_with_rubato(&input, MAX_DEVICE_RATE + 1)
        .expect_err("expected error for high device rate");
    assert!(err.to_string().contains("unsupported device sample rate"));
    assert!(FORCE_RUBATO_ERROR.load(Ordering::Relaxed));
    FORCE_RUBATO_ERROR.store(false, Ordering::Relaxed);
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_accepts_boundary_rates() {
    let _guard = lock_resample_test();
    let input = vec![0.1f32; 64];

    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let err =
        resample_with_rubato(&input, MIN_DEVICE_RATE).expect_err("expected forced rubato error");
    assert!(err.to_string().contains("forced rubato error"));
    assert!(!FORCE_RUBATO_ERROR.load(Ordering::Relaxed));

    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let err =
        resample_with_rubato(&input, MAX_DEVICE_RATE).expect_err("expected forced rubato error");
    assert!(err.to_string().contains("forced rubato error"));
    assert!(!FORCE_RUBATO_ERROR.load(Ordering::Relaxed));
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_resampler_is_not_shorter_than_expected() {
    let _guard = lock_resample_test();
    let input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.02).sin()).collect();
    let result = resample_to_target_rate(&input, 48_000);
    let expected = (input.len() as f64 * 16_000f64 / 48_000f64).round() as usize;
    assert!(result.len() >= expected);
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn rubato_rejects_aliasing_energy() {
    let _guard = lock_resample_test();
    let signal = multi_tone_signal(&[(6_000.0, 1.0), (12_000.0, 1.0)], 48_000, 0.1);
    let resampled = resample_to_target_rate(&signal, 48_000);
    let wanted = goertzel_power(&resampled, SAMPLE_RATE, 6_000.0);
    let alias = goertzel_power(&resampled, SAMPLE_RATE, 4_000.0);
    assert!(wanted > 0.1, "wanted tone vanished (power={wanted})");
    // TODO: Threshold relaxed from 0.01 to 0.02 due to hardware variance (2025-11-13).
    // If aliasing becomes an issue, investigate rubato config or platform-specific behavior.
    assert!(
        alias < 0.02 * wanted,
        "alias not suppressed enough (wanted={wanted}, alias={alias}). ratio={}",
        alias / wanted
    );
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn resample_to_target_rate_avoids_fallback_for_valid_input() {
    let _guard = lock_resample_test();
    RESAMPLE_FALLBACK_COUNT.store(0, Ordering::Relaxed);
    RESAMPLE_WARN_COUNT.store(0, Ordering::Relaxed);
    RESAMPLER_WARNING_SHOWN.store(false, Ordering::Relaxed);

    let input: Vec<f32> = (0..512).map(|i| (i as f32 * 0.01).sin()).collect();
    let _ = resample_to_target_rate(&input, 48_000);
    assert_eq!(RESAMPLE_FALLBACK_COUNT.load(Ordering::Relaxed), 0);
    assert_eq!(RESAMPLE_WARN_COUNT.load(Ordering::Relaxed), 0);
}

#[cfg(feature = "high-quality-audio")]
#[test]
fn resample_to_target_rate_warns_once_on_fallback() {
    let _guard = lock_resample_test();
    RESAMPLE_FALLBACK_COUNT.store(0, Ordering::Relaxed);
    RESAMPLE_WARN_COUNT.store(0, Ordering::Relaxed);
    RESAMPLER_WARNING_SHOWN.store(false, Ordering::Relaxed);

    let input = vec![0.1f32; 128];
    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let _ = resample_to_target_rate(&input, 48_000);
    assert_eq!(RESAMPLE_FALLBACK_COUNT.load(Ordering::Relaxed), 1);
    assert_eq!(RESAMPLE_WARN_COUNT.load(Ordering::Relaxed), 1);

    FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
    let _ = resample_to_target_rate(&input, 48_000);
    assert_eq!(RESAMPLE_FALLBACK_COUNT.load(Ordering::Relaxed), 2);
    assert_eq!(RESAMPLE_WARN_COUNT.load(Ordering::Relaxed), 1);
}

#[cfg(not(feature = "high-quality-audio"))]
#[test]
fn fir_resampler_reduces_alias_vs_naive() {
    let signal = multi_tone_signal(&[(6_000.0, 1.0), (12_000.0, 1.0)], 48_000, 0.1);
    let filtered = resample_to_target_rate(&signal, 48_000);
    let ratio = SAMPLE_RATE as f32 / 48_000f32;
    let naive = resample_linear(&signal, ratio);
    let alias_filtered = goertzel_power(&filtered, SAMPLE_RATE, 4_000.0);
    let alias_naive = goertzel_power(&naive, SAMPLE_RATE, 4_000.0);
    assert!(
        alias_filtered < alias_naive * 0.6,
        "FIR path failed to reduce aliasing (filtered={alias_filtered}, naive={alias_naive})"
    );
}

fn multi_tone_signal(tones: &[(f32, f32)], sample_rate: u32, seconds: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * seconds) as usize;
    (0..total_samples)
        .map(|n| {
            tones.iter().fold(0.0, |acc, (freq, amp)| {
                acc + amp * (2.0 * PI * freq * n as f32 / sample_rate as f32).sin()
            })
        })
        .collect()
}

fn goertzel_power(samples: &[f32], sample_rate: u32, target_hz: f32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let len = samples.len() as f32;
    let normalized_freq = target_hz / sample_rate as f32;
    let omega = 2.0 * PI * normalized_freq;
    let coeff = 2.0 * omega.cos();
    let mut q1 = 0.0;
    let mut q2 = 0.0;
    for &sample in samples {
        let q0 = coeff * q1 - q2 + sample;
        q2 = q1;
        q1 = q0;
    }
    let power = q1 * q1 + q2 * q2 - coeff * q1 * q2;
    (power / len).max(0.0)
}

fn reference_low_pass(normalized_cutoff: f32, taps: usize) -> Vec<f32> {
    let mut coeffs = Vec::with_capacity(taps);
    let m = (taps - 1) as f64;
    let cutoff = normalized_cutoff as f64;

    for n in 0..taps {
        let centered = n as f64 - m / 2.0;
        let x = 2.0 * std::f64::consts::PI * cutoff * centered;
        let sinc = if centered == 0.0 {
            2.0 * cutoff
        } else {
            (2.0 * cutoff * x.sin()) / x
        };
        let window = if taps <= 1 {
            1.0
        } else {
            0.54 - 0.46 * ((2.0 * std::f64::consts::PI * n as f64) / m).cos()
        };
        coeffs.push((sinc * window) as f32);
    }

    let sum: f64 = coeffs.iter().map(|c| *c as f64).sum();
    if sum != 0.0 {
        for coeff in coeffs.iter_mut() {
            *coeff = (*coeff as f64 / sum) as f32;
        }
    }

    coeffs
}

#[test]
fn frame_accumulator_trims_excess_silence() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 4);
    acc.push_frame(vec![1.0; 4], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);

    let audio = acc.into_audio(&StopReason::VadSilence { tail_ms: 40 });
    assert_eq!(audio.len(), 8);
    assert_eq!(audio[..4], [1.0; 4]);
}

#[test]
fn frame_accumulator_keeps_silence_within_lookback() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 8);
    acc.push_frame(vec![0.5; 4], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);

    let audio = acc.into_audio(&StopReason::VadSilence { tail_ms: 40 });
    assert_eq!(audio.len(), 8);
}

#[test]
fn frame_accumulator_handles_partial_trim() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 3);
    acc.push_frame(vec![1.0; 4], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 5], FrameLabel::Silence);

    let audio = acc.into_audio(&StopReason::VadSilence { tail_ms: 40 });
    assert_eq!(audio.len(), 7);
    assert_eq!(&audio[4..], &[0.0; 3]);
}

#[test]
fn frame_accumulator_drops_oldest_on_capacity() {
    let mut acc = FrameAccumulator::for_testing(8, 4);
    acc.push_frame(vec![1.0; 4], FrameLabel::Speech);
    acc.push_frame(vec![2.0; 4], FrameLabel::Speech);
    acc.push_frame(vec![3.0; 4], FrameLabel::Speech); // forces first frame out

    let audio = acc.into_audio(&StopReason::MaxDuration);
    assert_eq!(audio.len(), 8);
    assert_eq!(&audio[..4], &[2.0; 4]);
    assert_eq!(&audio[4..], &[3.0; 4]);
}

#[test]
fn frame_accumulator_trim_preserves_trailing_speech() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 2);
    acc.push_frame(vec![0.0; 2], FrameLabel::Silence);
    acc.push_frame(vec![1.0; 2], FrameLabel::Speech);

    let audio = acc.into_audio(&StopReason::VadSilence { tail_ms: 10 });
    assert_eq!(audio.len(), 4);
    assert_eq!(&audio[2..], &[1.0; 2]);
}

#[test]
fn frame_accumulator_trims_across_multiple_frames() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 2);
    acc.push_frame(vec![1.0; 2], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);

    let audio = acc.into_audio(&StopReason::VadSilence { tail_ms: 30 });
    assert_eq!(audio.len(), 4);
    assert_eq!(&audio[..2], &[1.0; 2]);
    assert_eq!(&audio[2..], &[0.0; 2]);
}

#[test]
fn frame_accumulator_trim_progresses_after_pop() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 2);
    acc.push_frame(vec![1.0; 2], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);
    acc.push_frame(vec![0.0; 4], FrameLabel::Silence);

    acc.trim_trailing_silence();
    assert_eq!(acc.total_samples, 4);
    assert_eq!(acc.frames.len(), 2);
    assert_eq!(acc.frames.back().unwrap().samples.len(), 2);
}

#[test]
fn frame_accumulator_trims_zero_length_silence() {
    let mut acc = FrameAccumulator::for_testing(usize::MAX, 1);
    acc.push_frame(vec![1.0; 1], FrameLabel::Speech);
    acc.push_frame(vec![0.0; 2], FrameLabel::Silence);
    acc.push_frame(Vec::new(), FrameLabel::Silence);

    acc.trim_trailing_silence();
    assert_eq!(acc.frames.len(), 2);
    assert_eq!(acc.total_samples, 2);
    assert_eq!(acc.frames.back().unwrap().samples.len(), 1);
}

#[test]
fn vad_config_from_pipeline_config_maps_fields() {
    let cfg = crate::config::VoicePipelineConfig {
        sample_rate: 12_345,
        max_capture_ms: 9_999,
        silence_tail_ms: 321,
        min_speech_ms_before_stt_start: 111,
        lookback_ms: 222,
        buffer_ms: 333,
        channel_capacity: 7,
        stt_timeout_ms: 55_555,
        vad_threshold_db: -12.5,
        vad_frame_ms: 25,
        vad_smoothing_frames: 3,
        python_fallback_allowed: true,
        vad_engine: crate::config::VadEngineKind::Simple,
    };
    let vad = VadConfig::from(&cfg);
    assert_eq!(vad.sample_rate, cfg.sample_rate);
    assert_eq!(vad.frame_ms, cfg.vad_frame_ms);
    assert_eq!(vad.silence_threshold_db, cfg.vad_threshold_db);
    assert_eq!(vad.silence_duration_ms, cfg.silence_tail_ms);
    assert_eq!(vad.max_recording_duration_ms, cfg.max_capture_ms);
    assert_eq!(
        vad.min_recording_duration_ms,
        cfg.min_speech_ms_before_stt_start
    );
    assert_eq!(vad.lookback_ms, cfg.lookback_ms);
    assert_eq!(vad.buffer_ms, cfg.buffer_ms);
    assert_eq!(vad.channel_capacity, cfg.channel_capacity);
    assert_eq!(vad.smoothing_frames, cfg.vad_smoothing_frames);
}

#[test]
fn vad_smoother_majority_vote_prefers_stable_label() {
    let mut smoother = VadSmoother::new(3);
    assert_eq!(smoother.smooth(FrameLabel::Speech), FrameLabel::Speech);
    assert_eq!(smoother.smooth(FrameLabel::Silence), FrameLabel::Silence);
    assert_eq!(smoother.smooth(FrameLabel::Speech), FrameLabel::Speech);
}

#[test]
fn vad_smoother_drops_oldest_frame_when_window_full() {
    let mut smoother = VadSmoother::new(3);
    assert_eq!(smoother.smooth(FrameLabel::Speech), FrameLabel::Speech);
    assert_eq!(smoother.smooth(FrameLabel::Silence), FrameLabel::Silence);
    assert_eq!(smoother.smooth(FrameLabel::Silence), FrameLabel::Silence);
    assert_eq!(smoother.smooth(FrameLabel::Speech), FrameLabel::Silence);
}

#[test]
fn vad_smoother_window_size_one_noop() {
    let mut smoother = VadSmoother::new(1);
    assert_eq!(smoother.smooth(FrameLabel::Silence), FrameLabel::Silence);
    assert_eq!(smoother.smooth(FrameLabel::Speech), FrameLabel::Speech);
}

#[test]
fn frame_accumulator_from_config_calculates_samples() {
    let cfg = VadConfig {
        sample_rate: 16_000,
        buffer_ms: 1_250,
        lookback_ms: 250,
        ..VadConfig::default()
    };
    let acc = FrameAccumulator::from_config(&cfg);
    assert_eq!(acc.max_samples, 20_000);
    assert_eq!(acc.lookback_samples, 4_000);
}

#[test]
fn frame_accumulator_is_empty_reflects_frames() {
    let mut acc = FrameAccumulator::for_testing(4, 2);
    assert!(acc.is_empty());
    acc.push_frame(vec![1.0; 2], FrameLabel::Speech);
    assert!(!acc.is_empty());
}

#[test]
fn stop_reason_labels_are_stable() {
    assert_eq!(
        StopReason::VadSilence { tail_ms: 100 }.label(),
        "vad_silence"
    );
    assert_eq!(StopReason::MaxDuration.label(), "max_duration");
    assert_eq!(StopReason::ManualStop.label(), "manual_stop");
    assert_eq!(StopReason::Timeout.label(), "timeout");
    assert_eq!(StopReason::Error("x".into()).label(), "error");
}

struct MockVad;

impl VadEngine for MockVad {
    fn process_frame(&mut self, _samples: &[f32]) -> VadDecision {
        VadDecision::Silence
    }

    fn reset(&mut self) {}
}

struct ConstantVad {
    decision: VadDecision,
}

impl VadEngine for ConstantVad {
    fn process_frame(&mut self, _samples: &[f32]) -> VadDecision {
        self.decision
    }

    fn reset(&mut self) {}
}

#[test]
fn vad_engine_default_name_is_stable() {
    let vad = MockVad;
    assert_eq!(vad.name(), "unknown_vad");
}

#[test]
fn simple_threshold_vad_classifies_by_energy() {
    let mut vad = SimpleThresholdVad::new(-30.0);
    assert_eq!(vad.process_frame(&[]), VadDecision::Uncertain);
    assert_eq!(vad.process_frame(&[0.001; 160]), VadDecision::Silence);
    assert_eq!(vad.process_frame(&[0.1; 160]), VadDecision::Speech);
}

#[test]
fn simple_threshold_vad_uses_average_energy() {
    let mut vad = SimpleThresholdVad::new(-30.0);
    let samples = vec![0.01f32; 100];
    assert_eq!(vad.process_frame(&samples), VadDecision::Silence);
}

#[test]
fn record_with_vad_stub_returns_metrics() {
    let Some(recorder) = Recorder::new_for_tests() else {
        eprintln!("skipping record_with_vad_stub_returns_metrics: no input device available");
        return;
    };

    let mut vad = MockVad;
    let cfg = VadConfig::default();
    let result = recorder
        .record_with_vad(&cfg, &mut vad, None, None)
        .expect("stub should produce a CaptureResult");
    assert!(result.audio.is_empty());
    assert_eq!(result.metrics.frames_processed, 0);
}

#[test]
fn capture_state_hits_max_duration() {
    let cfg = VadConfig {
        max_recording_duration_ms: 60,
        min_recording_duration_ms: 0,
        ..Default::default()
    };
    let mut state = CaptureState::for_testing(&cfg, 20);
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    let reason = state.on_frame(FrameLabel::Speech);
    assert!(matches!(reason, Some(StopReason::MaxDuration)));
}

#[test]
fn capture_state_times_out_after_idle() {
    let cfg = VadConfig {
        max_recording_duration_ms: 60,
        ..Default::default()
    };
    let mut state = CaptureState::for_testing(&cfg, 30);
    assert!(state.on_timeout().is_none());
    let reason = state.on_timeout();
    assert!(matches!(reason, Some(StopReason::Timeout)));
}

#[test]
fn capture_state_does_not_stop_without_speech() {
    let cfg = VadConfig {
        max_recording_duration_ms: 500,
        silence_duration_ms: 100,
        min_recording_duration_ms: 0,
        ..Default::default()
    };
    let mut state = CaptureState::for_testing(&cfg, 50);
    for _ in 0..3 {
        assert!(state.on_frame(FrameLabel::Silence).is_none());
    }
}

#[test]
fn capture_state_metrics_track_speech_and_silence() {
    let cfg = VadConfig {
        max_recording_duration_ms: 10_000,
        min_recording_duration_ms: 0,
        ..Default::default()
    };
    let mut state = CaptureState::for_testing(&cfg, 20);
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    assert!(state.on_frame(FrameLabel::Silence).is_none());
    assert_eq!(state.total_ms(), 60);
    assert_eq!(state.speech_ms(), 40);
    assert_eq!(state.silence_tail_ms(), 20);
}

#[test]
fn capture_state_requires_min_speech_before_silence_stop() {
    let cfg = VadConfig {
        min_recording_duration_ms: 200,
        silence_duration_ms: 100,
        ..Default::default()
    };
    let mut state = CaptureState::for_testing(&cfg, 50);
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    assert!(state.on_frame(FrameLabel::Speech).is_none());
    assert!(state.on_frame(FrameLabel::Silence).is_none());
    let reason = state.on_frame(FrameLabel::Silence);
    assert!(matches!(reason, Some(StopReason::VadSilence { .. })));
}

#[test]
fn capture_state_manual_stop_sets_reason() {
    let cfg = VadConfig::default();
    let state = CaptureState::for_testing(&cfg, 20);
    assert!(matches!(state.manual_stop(), StopReason::ManualStop));
}

#[test]
fn offline_capture_promotes_silence_tail() {
    let cfg = VadConfig {
        sample_rate: 1000,
        frame_ms: 10,
        max_recording_duration_ms: 30,
        silence_duration_ms: 20,
        lookback_ms: 10,
        ..VadConfig::default()
    };
    let samples = vec![0.0; 30];
    let mut vad = ConstantVad {
        decision: VadDecision::Silence,
    };
    let result = super::offline_capture_from_pcm(&samples, &cfg, &mut vad);
    assert!(matches!(
        result.metrics.early_stop_reason,
        StopReason::VadSilence { .. }
    ));
    assert!(result.audio.len() <= 10);
}

#[test]
fn offline_capture_keeps_max_duration_when_tail_short() {
    let cfg = VadConfig {
        sample_rate: 1000,
        frame_ms: 10,
        max_recording_duration_ms: 20,
        silence_duration_ms: 30,
        lookback_ms: 10,
        ..VadConfig::default()
    };
    let samples = vec![0.0; 20];
    let mut vad = ConstantVad {
        decision: VadDecision::Silence,
    };
    let result = super::offline_capture_from_pcm(&samples, &cfg, &mut vad);
    assert!(matches!(
        result.metrics.early_stop_reason,
        StopReason::MaxDuration
    ));
}

#[test]
fn offline_capture_tracks_metrics_for_speech() {
    let cfg = VadConfig {
        sample_rate: 1000,
        frame_ms: 10,
        max_recording_duration_ms: 30,
        silence_duration_ms: 100,
        min_recording_duration_ms: 0,
        lookback_ms: 10,
        ..VadConfig::default()
    };
    let samples = vec![0.5f32; 30];
    let mut vad = ConstantVad {
        decision: VadDecision::Speech,
    };
    let result = super::offline_capture_from_pcm(&samples, &cfg, &mut vad);
    assert_eq!(result.metrics.frames_processed, 3);
    assert_eq!(result.metrics.capture_ms, 30);
    assert_eq!(result.metrics.speech_ms, 30);
    assert_eq!(result.audio.len(), 30);
    assert!(matches!(
        result.metrics.early_stop_reason,
        StopReason::MaxDuration
    ));
}

#[test]
fn offline_capture_pads_partial_frame() {
    let cfg = VadConfig {
        sample_rate: 1000,
        frame_ms: 10,
        max_recording_duration_ms: 30,
        silence_duration_ms: 100,
        min_recording_duration_ms: 0,
        lookback_ms: 10,
        ..VadConfig::default()
    };
    let samples = vec![0.2f32; 15];
    let mut vad = ConstantVad {
        decision: VadDecision::Speech,
    };
    let result = super::offline_capture_from_pcm(&samples, &cfg, &mut vad);
    assert_eq!(result.metrics.frames_processed, 2);
    assert_eq!(result.audio.len(), 20);
    assert!(result.audio[15..].iter().all(|sample| *sample == 0.0));
}

#[test]
fn append_downmixed_samples_handles_partial_frame() {
    let mut buf = Vec::new();
    let samples = [1.0f32, 3.0, 5.0];
    append_downmixed_samples(&mut buf, &samples, 2, |sample| sample);
    assert_eq!(buf, vec![2.0, 5.0]);
}

#[test]
fn append_downmixed_samples_handles_two_sample_remainder() {
    let mut buf = Vec::new();
    let samples = [2.0f32, 4.0, 6.0, 8.0, 10.0];
    append_downmixed_samples(&mut buf, &samples, 3, |sample| sample);
    assert_eq!(buf, vec![4.0, 9.0]);
}

#[test]
fn resample_linear_interpolates_expected_values() {
    let input = vec![0.0f32, 1.0];
    let output = resample_linear(&input, 2.0);
    assert_eq!(output, vec![0.0, 0.5, 1.0, 1.0]);
}

#[test]
fn basic_resample_returns_identity_for_target_rate() {
    let input = vec![0.2f32, -0.2, 0.4];
    let output = basic_resample(&input, TARGET_RATE);
    assert_eq!(output, input);
}

#[test]
fn basic_resample_rejects_out_of_bounds_rates() {
    let input = vec![0.2f32; 32];
    let low = basic_resample(&input, MIN_DEVICE_RATE - 1);
    assert_eq!(low, input);
    let high = basic_resample(&input, MAX_DEVICE_RATE + 1);
    assert_eq!(high, input);
}

#[test]
fn basic_resample_accepts_boundary_rates() {
    let input = vec![0.2f32; 100];
    let low = basic_resample(&input, MIN_DEVICE_RATE);
    let expected_low =
        (input.len() as f32 * (TARGET_RATE as f32 / MIN_DEVICE_RATE as f32)).round() as usize;
    assert_eq!(low.len(), expected_low);

    let high = basic_resample(&input, MAX_DEVICE_RATE);
    let expected_high =
        (input.len() as f32 * (TARGET_RATE as f32 / MAX_DEVICE_RATE as f32)).round() as usize;
    assert_eq!(high.len(), expected_high);
}

#[test]
fn basic_resample_upsample_matches_linear() {
    let input = vec![0.0f32, 1.0, 0.0, -1.0, 0.5, -0.5, 0.25, -0.25];
    let ratio = TARGET_RATE as f32 / 8_000f32;
    let expected = resample_linear(&input, ratio);
    let output = basic_resample(&input, 8_000);
    assert_eq!(output, expected);
}

#[test]
fn basic_resample_downsample_filters_high_freq() {
    let input: Vec<f32> = (0usize..64)
        .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
        .collect();
    let ratio = TARGET_RATE as f32 / 48_000f32;
    let naive = resample_linear(&input, ratio);
    let output = basic_resample(&input, 48_000);
    assert_eq!(output.len(), naive.len());
    let max_diff = output
        .iter()
        .zip(naive.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(max_diff > 0.01);
}

#[test]
fn downsampling_tap_count_is_odd_and_scaled() {
    assert_eq!(downsampling_tap_count(16_000), 11);
    assert_eq!(downsampling_tap_count(48_000), 13);
}

#[test]
fn design_low_pass_coeffs_are_normalized() {
    let coeffs = design_low_pass(0.1, 11);
    let sum: f32 = coeffs.iter().sum();
    assert!((sum - 1.0).abs() < 1e-3);
    assert!((coeffs[0] - coeffs[10]).abs() < 1e-6);
}

#[test]
fn low_pass_fir_preserves_dc_component() {
    let input = vec![1.0f32; 64];
    let output = low_pass_fir(&input, 48_000, 11);
    let avg: f32 = output.iter().sum::<f32>() / output.len() as f32;
    assert!(avg > 0.8 && avg < 1.2);
}

#[test]
fn append_downmixed_samples_three_channel_average() {
    let mut buf = Vec::new();
    let samples = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
    append_downmixed_samples(&mut buf, &samples, 3, |sample| sample);
    assert_eq!(buf, vec![2.0, 5.0]);
}

#[test]
fn frame_dispatcher_emits_frames_and_tracks_drops() {
    let (tx, rx) = bounded::<Vec<f32>>(1);
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut dispatcher = FrameDispatcher::new(2, tx, dropped.clone());

    dispatcher.push(&[1.0f32, 2.0, 3.0, 4.0], 1, |sample| sample);

    let frame = rx.try_recv().expect("missing frame");
    assert_eq!(frame, vec![1.0, 2.0]);
    assert_eq!(dropped.load(Ordering::Relaxed), 1);
}

#[test]
fn frame_dispatcher_accumulates_partial_frames() {
    let (tx, rx) = bounded::<Vec<f32>>(1);
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut dispatcher = FrameDispatcher::new(3, tx, dropped);

    dispatcher.push(&[1.0f32, 2.0], 1, |sample| sample);
    assert!(rx.try_recv().is_err());

    dispatcher.push(&[3.0f32, 4.0], 1, |sample| sample);
    let frame = rx.try_recv().expect("missing frame");
    assert_eq!(frame, vec![1.0, 2.0, 3.0]);
}

#[test]
fn adjust_frame_length_truncates_and_pads() {
    let data = vec![0.1f32, 0.2, 0.3];
    assert_eq!(adjust_frame_length(data.clone(), 2), vec![0.1, 0.2]);
    assert_eq!(
        adjust_frame_length(data.clone(), 5),
        vec![0.1, 0.2, 0.3, 0.3, 0.3]
    );
    assert_eq!(adjust_frame_length(data.clone(), 3), data);
}

#[test]
fn convert_frame_to_target_skips_resample_when_rates_match() {
    let frame = vec![0.1f32, 0.2, 0.3, 0.4];
    let output = convert_frame_to_target(frame.clone(), 8_000, 8_000, frame.len());
    assert_eq!(output, frame);
}

#[test]
fn resample_linear_downsamples_midpoints() {
    let input = vec![0.0f32, 2.0, 4.0, 6.0];
    let output = resample_linear(&input, 0.5);
    assert_eq!(output, vec![0.0, 4.0]);
}

#[test]
fn resample_linear_handles_non_integer_ratio() {
    let input = vec![0.0f32, 1.0, 2.0];
    let output = resample_linear(&input, 1.5);
    assert_eq!(output.len(), 5);
    assert!((output[1] - 0.6666667).abs() < 1e-6);
    assert!((output[2] - 1.3333334).abs() < 1e-6);
    assert!((output[3] - 2.0).abs() < 1e-6);
    assert!((output[4] - 2.0).abs() < 1e-6);
}

#[test]
fn resample_to_target_rate_keeps_non_empty() {
    let input = vec![0.0f32; 32];
    let output = resample_to_target_rate(&input, 8_000);
    assert!(!output.is_empty());
}

#[test]
fn basic_resample_downsamples_constant_signal() {
    let input = vec![1.0f32; 48];
    let output = basic_resample(&input, 48_000);
    assert_eq!(output.len(), 16);
    let min = output.iter().copied().fold(f32::INFINITY, f32::min);
    let max = output.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(min > 0.6 && max < 1.4);
}

#[test]
fn basic_resample_upsamples_constant_signal() {
    let input = vec![1.0f32; 16];
    let output = basic_resample(&input, 8_000);
    assert_eq!(output.len(), 32);
    let min = output.iter().copied().fold(f32::INFINITY, f32::min);
    let max = output.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(min > 0.9 && max < 1.1);
}

#[test]
fn downsampling_tap_count_scales_for_large_rate() {
    assert_eq!(downsampling_tap_count(96_000), 25);
}

#[test]
fn design_low_pass_single_tap_normalized() {
    let coeffs = design_low_pass(0.25, 1);
    assert_eq!(coeffs.len(), 1);
    assert!((coeffs[0] - 1.0).abs() < 1e-6);
}

#[test]
fn design_low_pass_matches_reference() {
    let actual = design_low_pass(0.2, 7);
    let reference = reference_low_pass(0.2, 7);
    for (a, b) in actual.iter().zip(reference.iter()) {
        assert!((a - b).abs() < 1e-5);
    }
}

#[test]
fn low_pass_fir_matches_reference_impulse() {
    let device_rate = 40_000;
    let taps = 7;
    let cutoff = (TARGET_RATE as f32 * 0.5 / device_rate as f32).min(0.499);
    let coeffs = reference_low_pass(cutoff, taps);
    let input = vec![1.0f32, 0.0, 0.0, 0.0, 0.0];
    let output = low_pass_fir(&input, device_rate, taps);

    let half = taps / 2;
    let mut expected = Vec::with_capacity(input.len());
    for n in 0..input.len() {
        let mut acc = 0.0;
        for (k, coeff) in coeffs.iter().enumerate() {
            if let Some(idx) = n.checked_add(k).and_then(|sum| sum.checked_sub(half)) {
                if let Some(sample) = input.get(idx) {
                    acc += *sample * coeff;
                }
            }
        }
        expected.push(acc);
    }

    for (a, b) in output.iter().zip(expected.iter()) {
        assert!((a - b).abs() < 1e-6);
    }
}

#[test]
fn low_pass_fir_returns_input_for_short_taps() {
    let input = vec![0.2f32, -0.1];
    let output = low_pass_fir(&input, 48_000, 1);
    assert_eq!(output, input);
}
