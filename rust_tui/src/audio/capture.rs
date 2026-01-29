//! Audio capture state machine with voice activity detection.
//!
//! Manages the recording loop: accumulates audio frames, tracks speech duration,
//! and decides when to stop based on silence detection or time limits.

use super::vad::{FrameLabel, VadConfig, VadEngine, VadSmoother};
use std::collections::VecDeque;

/// Metrics collected during audio capture for observability and debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureMetrics {
    pub capture_ms: u64,
    pub speech_ms: u64,
    pub silence_tail_ms: u64,
    pub frames_processed: usize,
    pub frames_dropped: usize,
    pub early_stop_reason: StopReason,
}

impl Default for CaptureMetrics {
    fn default() -> Self {
        Self {
            capture_ms: 0,
            speech_ms: 0,
            silence_tail_ms: 0,
            frames_processed: 0,
            frames_dropped: 0,
            early_stop_reason: StopReason::MaxDuration,
        }
    }
}

/// Explains why capture stopped so perf smoke tests can classify failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    VadSilence { tail_ms: u64 },
    MaxDuration,
    ManualStop,
    Timeout,
    Error(String),
}

impl StopReason {
    pub fn label(&self) -> &'static str {
        match self {
            StopReason::VadSilence { .. } => "vad_silence",
            StopReason::MaxDuration => "max_duration",
            StopReason::ManualStop => "manual_stop",
            StopReason::Timeout => "timeout",
            StopReason::Error(_) => "error",
        }
    }
}

/// Caller-facing result: mono PCM plus metrics for observability/CI.
#[derive(Debug, Clone)]
pub struct CaptureResult {
    pub audio: Vec<f32>,
    pub metrics: CaptureMetrics,
}

pub(super) struct FrameRecord {
    pub(super) samples: Vec<f32>,
    label: FrameLabel,
}

pub(super) struct FrameAccumulator {
    pub(super) frames: VecDeque<FrameRecord>,
    pub(super) total_samples: usize,
    pub(super) max_samples: usize,
    pub(super) lookback_samples: usize,
}

#[cfg_attr(test, allow(dead_code))]
impl FrameAccumulator {
    pub(super) fn from_config(cfg: &VadConfig) -> Self {
        let max_samples = ((cfg.buffer_ms * u64::from(cfg.sample_rate)) / 1000).max(1) as usize;
        let lookback_samples = ((cfg.lookback_ms * u64::from(cfg.sample_rate)) / 1000) as usize;
        Self {
            frames: VecDeque::new(),
            total_samples: 0,
            max_samples,
            lookback_samples,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_testing(max_samples: usize, lookback_samples: usize) -> Self {
        Self {
            frames: VecDeque::new(),
            total_samples: 0,
            max_samples,
            lookback_samples,
        }
    }

    pub(super) fn push_frame(&mut self, samples: Vec<f32>, label: FrameLabel) {
        self.total_samples = self.total_samples.saturating_add(samples.len());
        self.frames.push_back(FrameRecord { samples, label });
        while self.total_samples > self.max_samples {
            if let Some(record) = self.frames.pop_front() {
                self.total_samples = self.total_samples.saturating_sub(record.samples.len());
            } else {
                break;
            }
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.total_samples == 0
    }

    pub(super) fn into_audio(mut self, stop_reason: &StopReason) -> Vec<f32> {
        if matches!(stop_reason, StopReason::VadSilence { .. }) {
            self.trim_trailing_silence();
        }
        let mut audio = Vec::with_capacity(self.total_samples);
        for record in self.frames {
            audio.extend(record.samples);
        }
        audio
    }

    pub(super) fn trim_trailing_silence(&mut self) {
        let mut trailing_silence_samples = 0usize;
        for record in self.frames.iter().rev() {
            if record.label == FrameLabel::Silence {
                trailing_silence_samples += record.samples.len();
            } else {
                break;
            }
        }
        let excess = trailing_silence_samples.saturating_sub(self.lookback_samples);
        if excess == 0 {
            return;
        }
        let target_total = self.total_samples.saturating_sub(excess);
        loop {
            if self.total_samples <= target_total {
                break;
            }
            let (label, record_len) = match self.frames.back() {
                Some(record) => (record.label, record.samples.len()),
                None => break,
            };
            if label != FrameLabel::Silence {
                break;
            }
            if record_len == 0 {
                self.frames.pop_back();
                continue;
            }
            let remaining = self.total_samples.saturating_sub(target_total);
            let remove = remaining.min(record_len);
            if remove >= record_len {
                self.total_samples = self.total_samples.saturating_sub(record_len);
                self.frames.pop_back();
            } else {
                let keep = record_len - remove;
                if let Some(record) = self.frames.back_mut() {
                    record.samples.truncate(keep);
                }
                self.total_samples = self.total_samples.saturating_sub(remove);
            }
        }
    }
}

/// Tracks recording progress and determines when to stop capture.
///
/// The state machine monitors:
/// - Total recording duration (enforces max limit)
/// - Consecutive silence duration (triggers stop after speech ends)
/// - Speech duration (ensures minimum speech before allowing stop)
pub(super) struct CaptureState<'a> {
    cfg: &'a VadConfig,
    frame_ms: u64,
    speech_ms: u64,
    silence_streak_ms: u64,
    total_ms: u64,
}

#[cfg_attr(test, allow(dead_code))]
impl<'a> CaptureState<'a> {
    pub(super) fn new(cfg: &'a VadConfig, frame_ms: u64) -> Self {
        Self {
            cfg,
            frame_ms,
            speech_ms: 0,
            silence_streak_ms: 0,
            total_ms: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_testing(cfg: &'a VadConfig, frame_ms: u64) -> Self {
        Self::new(cfg, frame_ms)
    }

    /// Processes a frame and returns a stop reason if capture should end.
    ///
    /// Stop conditions:
    /// 1. Max duration reached (hard limit)
    /// 2. Silence detected after speech (user stopped talking)
    ///
    /// Note: Silence only triggers a stop after speech has been detected,
    /// preventing premature stops when the mic starts in a quiet room.
    pub(super) fn on_frame(&mut self, label: FrameLabel) -> Option<StopReason> {
        match label {
            FrameLabel::Speech => {
                self.speech_ms = self.speech_ms.saturating_add(self.frame_ms);
                self.silence_streak_ms = 0;
            }
            FrameLabel::Silence => {
                self.silence_streak_ms = self.silence_streak_ms.saturating_add(self.frame_ms);
            }
            FrameLabel::Uncertain => {
                self.silence_streak_ms = 0;
            }
        }
        self.total_ms = self.total_ms.saturating_add(self.frame_ms);

        if self.total_ms >= self.cfg.max_recording_duration_ms {
            return Some(StopReason::MaxDuration);
        }

        // Only stop on silence after detecting speech (avoids false stops in quiet rooms)
        if self.speech_ms > 0
            && self.total_ms >= self.cfg.min_recording_duration_ms
            && self.silence_streak_ms >= self.cfg.silence_duration_ms
        {
            return Some(StopReason::VadSilence {
                tail_ms: self.silence_streak_ms,
            });
        }
        None
    }

    pub(super) fn on_timeout(&mut self) -> Option<StopReason> {
        self.total_ms = self.total_ms.saturating_add(self.frame_ms);
        if self.total_ms >= self.cfg.max_recording_duration_ms {
            Some(StopReason::Timeout)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub(super) fn manual_stop(&self) -> StopReason {
        StopReason::ManualStop
    }

    pub(super) fn total_ms(&self) -> u64 {
        self.total_ms
    }

    pub(super) fn speech_ms(&self) -> u64 {
        self.speech_ms
    }

    pub(super) fn silence_tail_ms(&self) -> u64 {
        self.silence_streak_ms
    }
}

/// Run the silence-aware capture state machine against synthetic PCM samples.
/// Used by the benchmarking harness so we can measure Phase 2A latency without
/// requiring physical microphones or CPAL devices.
pub fn offline_capture_from_pcm(
    samples: &[f32],
    cfg: &VadConfig,
    vad: &mut dyn VadEngine,
) -> CaptureResult {
    let frame_samples = ((cfg.sample_rate as u64 * cfg.frame_ms) / 1000).max(1) as usize;
    let mut accumulator = FrameAccumulator::from_config(cfg);
    let mut state = CaptureState::new(cfg, cfg.frame_ms);
    let mut smoother = VadSmoother::new(cfg.smoothing_frames);
    let mut metrics = CaptureMetrics::default();
    let mut stop_reason = StopReason::MaxDuration;

    for chunk in samples.chunks(frame_samples) {
        if state.total_ms() >= cfg.max_recording_duration_ms {
            break;
        }
        let mut frame = chunk.to_vec();
        frame.resize(frame_samples, 0.0);
        let decision = vad.process_frame(&frame);
        metrics.frames_processed += 1;
        let label = smoother.smooth(FrameLabel::from(decision));
        accumulator.push_frame(frame, label);
        if let Some(reason) = state.on_frame(label) {
            stop_reason = reason;
            break;
        }
    }

    if accumulator.is_empty() {
        return CaptureResult {
            audio: Vec::new(),
            metrics,
        };
    }

    if matches!(stop_reason, StopReason::MaxDuration)
        && state.silence_tail_ms() >= cfg.silence_duration_ms
    {
        stop_reason = StopReason::VadSilence {
            tail_ms: state.silence_tail_ms(),
        };
    }

    let audio = accumulator.into_audio(&stop_reason);
    metrics.speech_ms = state.speech_ms();
    metrics.silence_tail_ms = state.silence_tail_ms();
    metrics.capture_ms = state.total_ms();
    metrics.early_stop_reason = stop_reason;

    CaptureResult { audio, metrics }
}
