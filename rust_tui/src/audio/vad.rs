//! Voice Activity Detection (VAD) for speech/silence classification.
//!
//! Processes audio frames and determines whether the user is speaking.
//! Used to automatically stop recording after a period of silence.

use super::TARGET_RATE;
use crate::config::VoicePipelineConfig;
use std::cmp::Ordering as CmpOrdering;
use std::collections::VecDeque;

/// Configuration for silence-aware audio capture.
#[derive(Debug, Clone)]
pub struct VadConfig {
    pub sample_rate: u32,
    pub frame_ms: u64,
    pub silence_threshold_db: f32,
    pub silence_duration_ms: u64,
    pub max_recording_duration_ms: u64,
    pub min_recording_duration_ms: u64,
    pub lookback_ms: u64,
    pub buffer_ms: u64,
    pub channel_capacity: usize,
    pub smoothing_frames: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            sample_rate: TARGET_RATE,
            frame_ms: 20,
            silence_threshold_db: -40.0,
            silence_duration_ms: 500,
            max_recording_duration_ms: 10_000,
            min_recording_duration_ms: 200,
            lookback_ms: 500,
            buffer_ms: 10_000,
            channel_capacity: 64,
            smoothing_frames: 3,
        }
    }
}

impl From<&VoicePipelineConfig> for VadConfig {
    fn from(cfg: &VoicePipelineConfig) -> Self {
        Self {
            sample_rate: cfg.sample_rate,
            frame_ms: cfg.vad_frame_ms,
            silence_threshold_db: cfg.vad_threshold_db,
            silence_duration_ms: cfg.silence_tail_ms,
            max_recording_duration_ms: cfg.max_capture_ms,
            min_recording_duration_ms: cfg.min_speech_ms_before_stt_start,
            lookback_ms: cfg.lookback_ms,
            buffer_ms: cfg.buffer_ms,
            channel_capacity: cfg.channel_capacity,
            smoothing_frames: cfg.vad_smoothing_frames,
        }
    }
}

/// Voice Activity Detection engine that processes audio frames.
///
/// # Frame Size Contract
/// Implementations may require specific frame sizes. For example, Earshot
/// expects frames of 10ms, 20ms, or 30ms duration at 16kHz sample rate.
///
/// Frame size in samples = (sample_rate * frame_duration_ms) / 1000
/// Example: 20ms @ 16kHz = 320 samples
///
/// Callers must ensure frames passed to `process_frame` match the engine's
/// expected frame size, or the VAD may produce incorrect results.
pub trait VadEngine {
    fn process_frame(&mut self, samples: &[f32]) -> VadDecision;
    fn reset(&mut self);
    fn name(&self) -> &'static str {
        "unknown_vad"
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VadDecision {
    Speech,
    Silence,
    Uncertain,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum FrameLabel {
    Speech,
    Silence,
    Uncertain,
}

impl From<VadDecision> for FrameLabel {
    fn from(decision: VadDecision) -> Self {
        match decision {
            VadDecision::Speech => FrameLabel::Speech,
            VadDecision::Silence => FrameLabel::Silence,
            VadDecision::Uncertain => FrameLabel::Uncertain,
        }
    }
}

/// Smooths VAD decisions using a sliding window majority vote.
///
/// Reduces false positives from brief noise spikes by requiring multiple
/// consecutive frames to agree before changing the speech/silence state.
pub(super) struct VadSmoother {
    window: VecDeque<FrameLabel>,
    window_size: usize,
}

impl VadSmoother {
    pub(super) fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::new(),
            window_size: window_size.max(1),
        }
    }

    /// Returns the majority label from the last `window_size` frames.
    pub(super) fn smooth(&mut self, label: FrameLabel) -> FrameLabel {
        if self.window_size <= 1 {
            return label;
        }
        self.window.push_back(label);
        if self.window.len() > self.window_size {
            self.window.pop_front();
        }

        let mut speech = 0usize;
        let mut silence = 0usize;
        for item in &self.window {
            match item {
                FrameLabel::Speech => speech += 1,
                FrameLabel::Silence => silence += 1,
                FrameLabel::Uncertain => {}
            }
        }
        match speech.cmp(&silence) {
            CmpOrdering::Greater => FrameLabel::Speech,
            CmpOrdering::Less => FrameLabel::Silence,
            CmpOrdering::Equal => label,
        }
    }
}

/// Lightweight fallback VAD that operates on RMS energy. Used when Earshot is
/// disabled or unavailable.
#[derive(Debug, Clone)]
pub struct SimpleThresholdVad {
    threshold_db: f32,
}

impl SimpleThresholdVad {
    pub fn new(threshold_db: f32) -> Self {
        Self { threshold_db }
    }
}

impl VadEngine for SimpleThresholdVad {
    fn process_frame(&mut self, samples: &[f32]) -> VadDecision {
        if samples.is_empty() {
            return VadDecision::Uncertain;
        }
        let energy: f32 = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        let rms = energy.sqrt().max(1e-6);
        let db = 20.0 * rms.log10();
        if db >= self.threshold_db {
            VadDecision::Speech
        } else {
            VadDecision::Silence
        }
    }

    fn reset(&mut self) {}

    fn name(&self) -> &'static str {
        "simple_threshold_vad"
    }
}
