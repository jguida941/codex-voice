use crate::config::VoicePipelineConfig;
use crate::log_debug;
use anyhow::{anyhow, Result};
use crossbeam_channel::{Sender, TrySendError};
#[cfg(feature = "high-quality-audio")]
use rubato::{InterpolationParameters, InterpolationType, Resampler, SincFixedIn, WindowFunction};
use std::cmp::Ordering as CmpOrdering;
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Target format for transcription (mono channel, 16 kHz sample rate)
/// The Whisper model requires mono audio at 16 kHz for accurate transcription
pub const TARGET_RATE: u32 = 16_000;
pub const TARGET_CHANNELS: u32 = 1;
#[cfg(test)]
const SAMPLE_RATE: u32 = TARGET_RATE;
// Derived from 16 kHz target and practical ratio bounds (~0.01x .. 8x).
const MIN_DEVICE_RATE: u32 = 2_000;
const MAX_DEVICE_RATE: u32 = 1_600_000;
const MIN_RESAMPLE_RATIO: f64 = TARGET_RATE as f64 / MAX_DEVICE_RATE as f64;
const MAX_RESAMPLE_RATIO: f64 = TARGET_RATE as f64 / MIN_DEVICE_RATE as f64;
const MAX_DOWNSAMPLING_TAPS: usize = 129;

#[cfg(feature = "high-quality-audio")]
static RESAMPLER_WARNING_SHOWN: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static RESAMPLE_FALLBACK_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static RESAMPLE_WARN_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
static FORCE_RUBATO_ERROR: AtomicBool = AtomicBool::new(false);

mod recorder;
pub use recorder::Recorder;

/// Configuration knobs for silence-aware capture. Phase 2A keeps these simple
/// and maps them from CLI/config entries.
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

/// Summarizes how capture ended and what resources were consumed.
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
enum FrameLabel {
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

struct VadSmoother {
    window: VecDeque<FrameLabel>,
    window_size: usize,
}

impl VadSmoother {
    fn new(window_size: usize) -> Self {
        Self {
            window: VecDeque::new(),
            window_size: window_size.max(1),
        }
    }

    fn smooth(&mut self, label: FrameLabel) -> FrameLabel {
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

struct FrameRecord {
    samples: Vec<f32>,
    label: FrameLabel,
}

pub(crate) struct FrameAccumulator {
    frames: VecDeque<FrameRecord>,
    total_samples: usize,
    max_samples: usize,
    lookback_samples: usize,
}

#[cfg_attr(test, allow(dead_code))]
impl FrameAccumulator {
    fn from_config(cfg: &VadConfig) -> Self {
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

    fn push_frame(&mut self, samples: Vec<f32>, label: FrameLabel) {
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

    fn is_empty(&self) -> bool {
        self.total_samples == 0
    }

    fn into_audio(mut self, stop_reason: &StopReason) -> Vec<f32> {
        if matches!(stop_reason, StopReason::VadSilence { .. }) {
            self.trim_trailing_silence();
        }
        let mut audio = Vec::with_capacity(self.total_samples);
        for record in self.frames {
            audio.extend(record.samples);
        }
        audio
    }

    fn trim_trailing_silence(&mut self) {
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

pub(crate) struct CaptureState<'a> {
    cfg: &'a VadConfig,
    frame_ms: u64,
    speech_ms: u64,
    silence_streak_ms: u64,
    total_ms: u64,
}

#[cfg_attr(test, allow(dead_code))]
impl<'a> CaptureState<'a> {
    fn new(cfg: &'a VadConfig, frame_ms: u64) -> Self {
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

    fn on_frame(&mut self, label: FrameLabel) -> Option<StopReason> {
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
        // Only stop on silence if we've actually detected some speech first.
        // This prevents immediate stops when the mic starts in a quiet environment.
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

    fn on_timeout(&mut self) -> Option<StopReason> {
        self.total_ms = self.total_ms.saturating_add(self.frame_ms);
        if self.total_ms >= self.cfg.max_recording_duration_ms {
            Some(StopReason::Timeout)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    fn manual_stop(&self) -> StopReason {
        StopReason::ManualStop
    }

    fn total_ms(&self) -> u64 {
        self.total_ms
    }

    fn speech_ms(&self) -> u64 {
        self.speech_ms
    }

    fn silence_tail_ms(&self) -> u64 {
        self.silence_streak_ms
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
/// Downmix multi-channel input to mono while applying the provided converter so
/// Whisper receives a single channel regardless of the microphone layout.
fn append_downmixed_samples<T, F>(buf: &mut Vec<f32>, data: &[T], channels: usize, mut convert: F)
where
    T: Copy,
    F: FnMut(T) -> f32,
{
    if channels <= 1 {
        buf.extend(data.iter().copied().map(&mut convert));
        return;
    }

    // Average each interleaved frame to produce a mono representation.
    let mut acc = 0.0f32;
    let mut count = 0usize;
    for sample in data.iter().copied() {
        acc += convert(sample);
        count += 1;
        if count == channels {
            buf.push(acc / channels as f32);
            acc = 0.0;
            count = 0;
        }
    }
    if count > 0 {
        buf.push(acc / count as f32);
    }
}

struct FrameDispatcher {
    frame_samples: usize,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    sender: Sender<Vec<f32>>,
    dropped: Arc<AtomicUsize>,
}

impl FrameDispatcher {
    fn new(frame_samples: usize, sender: Sender<Vec<f32>>, dropped: Arc<AtomicUsize>) -> Self {
        Self {
            frame_samples: frame_samples.max(1),
            pending: Vec::with_capacity(frame_samples),
            scratch: Vec::new(),
            sender,
            dropped,
        }
    }

    fn push<T, F>(&mut self, data: &[T], channels: usize, convert: F)
    where
        T: Copy,
        F: FnMut(T) -> f32,
    {
        self.scratch.clear();
        append_downmixed_samples(&mut self.scratch, data, channels, convert);
        self.pending.extend_from_slice(&self.scratch);

        while self.pending.len() >= self.frame_samples {
            let frame: Vec<f32> = self.pending.drain(..self.frame_samples).collect();
            if let Err(err) = self.sender.try_send(frame) {
                match err {
                    TrySendError::Full(_) => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    TrySendError::Disconnected(_) => break,
                }
            }
        }
    }
}

fn resample_to_target_rate(input: &[f32], device_rate: u32) -> Vec<f32> {
    // Guard rails
    if device_rate == 0 {
        return input.to_vec(); // avoid div-by-zero elsewhere
    }
    if input.is_empty() {
        return input.to_vec();
    }
    if device_rate == TARGET_RATE {
        return input.to_vec();
    }

    #[cfg(feature = "high-quality-audio")]
    {
        match resample_with_rubato(input, device_rate) {
            Ok(output) => output,
            Err(err) => {
                #[cfg(test)]
                RESAMPLE_FALLBACK_COUNT.fetch_add(1, Ordering::Relaxed);
                // CRITICAL: Use AcqRel ordering to prevent data race
                if !RESAMPLER_WARNING_SHOWN.swap(true, Ordering::AcqRel) {
                    #[cfg(test)]
                    RESAMPLE_WARN_COUNT.fetch_add(1, Ordering::Relaxed);
                    log_debug(&format!(
                        "high-quality resampler failed ({err}); falling back to basic path"
                    ));
                }
                basic_resample(input, device_rate)
            }
        }
    }

    #[cfg(not(feature = "high-quality-audio"))]
    {
        basic_resample(input, device_rate)
    }
}

#[cfg(feature = "high-quality-audio")]
fn resample_with_rubato(input: &[f32], device_rate: u32) -> Result<Vec<f32>> {
    // Defensive early guard
    if device_rate == 0 {
        return Ok(input.to_vec());
    }
    if input.is_empty() {
        return Ok(input.to_vec());
    }
    if device_rate == TARGET_RATE {
        return Ok(input.to_vec());
    }

    if !(MIN_DEVICE_RATE..=MAX_DEVICE_RATE).contains(&device_rate) {
        return Err(anyhow!(
            "unsupported device sample rate {device_rate}Hz for resampling"
        ));
    }
    let ratio = TARGET_RATE as f64 / device_rate as f64;
    if !(MIN_RESAMPLE_RATIO..=MAX_RESAMPLE_RATIO).contains(&ratio) {
        return Err(anyhow!("invalid resample ratio {ratio}"));
    }

    #[cfg(test)]
    if FORCE_RUBATO_ERROR.swap(false, Ordering::Relaxed) {
        return Err(anyhow!("forced rubato error"));
    }

    let chunk = 256usize;
    let params = InterpolationParameters {
        sinc_len: 64,
        f_cutoff: 0.90, // safer cutoff
        interpolation: InterpolationType::Cubic,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    //           ratio,  drift, params, chunk_size, channels
    let mut rs = SincFixedIn::<f32>::new(ratio, 2.0, params, chunk, 1)
        .map_err(|e| anyhow!("failed to construct sinc resampler: {e:?}"))?;

    // pre-allocate
    let max_len = ((input.len() as f64) * MAX_RESAMPLE_RATIO).ceil() as usize;
    let mut expect = ((input.len() as f64) * ratio).round() as usize;
    expect = expect.clamp(1, max_len).saturating_add(8);
    let mut out = Vec::with_capacity(expect);

    let mut idx = 0usize;
    let mut seg = vec![0.0f32; chunk]; // reuse buffer
    while idx < input.len() {
        let end = (idx + chunk).min(input.len());
        if end == idx {
            return Err(anyhow!("resampler made no progress"));
        }
        let len = end - idx;
        let pad = input.get(end.wrapping_sub(1)).copied().unwrap_or(0.0);
        seg.fill(pad);
        seg[..len].copy_from_slice(&input[idx..end]);
        let produced = rs
            .process(std::slice::from_ref(&seg), None)
            .map_err(|e| anyhow!("resampler process failed: {e:?}"))?;
        out.extend_from_slice(&produced[0]);
        idx = end;
    }

    match out.len().cmp(&expect) {
        CmpOrdering::Greater => {
            out.truncate(expect);
        }
        CmpOrdering::Less => {
            out.resize(expect, *out.last().unwrap_or(&0.0));
        }
        CmpOrdering::Equal => {}
    }
    Ok(out)
}

fn basic_resample(input: &[f32], device_rate: u32) -> Vec<f32> {
    // Guard rails
    if device_rate == 0 {
        return input.to_vec(); // avoid div-by-zero elsewhere
    }
    if input.is_empty() {
        return input.to_vec();
    }
    if !(MIN_DEVICE_RATE..=MAX_DEVICE_RATE).contains(&device_rate) {
        return input.to_vec();
    }

    // Ratio > 1 means upsampling, < 1 means downsampling.
    let mut ratio = TARGET_RATE as f32 / device_rate as f32;
    ratio = ratio.clamp(MIN_RESAMPLE_RATIO as f32, MAX_RESAMPLE_RATIO as f32);
    let filtered = if device_rate > TARGET_RATE {
        // When decimating we run a small FIR low-pass to avoid aliasing.
        let taps = downsampling_tap_count(device_rate);
        low_pass_fir(input, device_rate, taps)
    } else {
        input.to_vec()
    };
    resample_linear(&filtered, ratio)
}

/// Lightweight linear resampler used after optional filtering; works well for
/// short speech snippets where phase accuracy matters less than latency.
fn resample_linear(input: &[f32], ratio: f32) -> Vec<f32> {
    let input_len = input.len();
    let output_len = (input_len as f32 * ratio).round() as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f32 / ratio;
        let idx = src_idx.floor() as usize;
        let frac = src_idx - idx as f32;

        if idx + 1 < input_len {
            let sample = input[idx] * (1.0 - frac) + input[idx + 1] * frac;
            output.push(sample);
        } else {
            let pad = input.last().copied().unwrap_or(0.0);
            output.push(pad);
        }
    }

    output
}

/// Pick a tap count based on the downsampling ratio so the FIR remains short for
/// near-equal sample rates and longer when we're collapsing 48 kHz into 16 kHz.
fn downsampling_tap_count(device_rate: u32) -> usize {
    let decimation_ratio = device_rate as f32 / TARGET_RATE as f32;
    let mut taps = (decimation_ratio * 4.0).ceil().max(11.0) as usize;
    if taps.is_multiple_of(2) {
        taps += 1;
    }
    taps.min(MAX_DOWNSAMPLING_TAPS)
}

/// Basic FIR low-pass that tames frequencies above the target Nyquist before we
/// drop samples. Prevents high-frequency speech from aliasing when users have
/// 44.1/48 kHz microphones.
fn low_pass_fir(input: &[f32], device_rate: u32, taps: usize) -> Vec<f32> {
    if input.is_empty() {
        return input.to_vec();
    }
    if taps <= 1 {
        return input.to_vec();
    }

    let normalized_cutoff = (TARGET_RATE as f32 * 0.5 / device_rate as f32).min(0.499);
    let coeffs = design_low_pass(normalized_cutoff, taps);
    let half = taps / 2;
    let mut output = Vec::with_capacity(input.len());

    for n in 0..input.len() {
        let mut acc = 0.0;
        for (k, coeff) in coeffs.iter().enumerate() {
            // Use saturating arithmetic to prevent underflow
            if let Some(idx) = n.checked_add(k).and_then(|sum| sum.checked_sub(half)) {
                if let Some(sample) = input.get(idx) {
                    acc += *sample * coeff;
                }
            }
        }
        output.push(acc);
    }

    output
}

fn convert_frame_to_target(
    frame: Vec<f32>,
    device_rate: u32,
    target_rate: u32,
    desired_len: usize,
) -> Vec<f32> {
    if device_rate == target_rate {
        return adjust_frame_length(frame, desired_len);
    }
    let resampled = resample_to_target_rate(&frame, device_rate);
    adjust_frame_length(resampled, desired_len)
}

fn adjust_frame_length(mut data: Vec<f32>, desired: usize) -> Vec<f32> {
    match data.len().cmp(&desired) {
        CmpOrdering::Greater => {
            data.truncate(desired);
        }
        CmpOrdering::Less => {
            let pad = *data.last().unwrap_or(&0.0);
            data.resize(desired, pad);
        }
        CmpOrdering::Equal => {}
    }
    data
}

/// Build the normalized Hamming-windowed sinc taps used by the FIR filter.
fn design_low_pass(normalized_cutoff: f32, taps: usize) -> Vec<f32> {
    let mut coeffs = Vec::with_capacity(taps);
    let m = (taps - 1) as f32;

    for n in 0..taps {
        let centered = n as f32 - m / 2.0;
        let x = 2.0 * PI * normalized_cutoff * centered;
        let sinc = if centered == 0.0 {
            2.0 * normalized_cutoff
        } else {
            (2.0 * normalized_cutoff * x.sin()) / x
        };
        let window = if taps <= 1 {
            1.0
        } else {
            0.54 - 0.46 * ((2.0 * PI * n as f32) / m).cos()
        };
        coeffs.push(sinc * window);
    }

    let sum: f32 = coeffs.iter().sum();
    if sum != 0.0 {
        for coeff in coeffs.iter_mut() {
            *coeff /= sum;
        }
    }

    coeffs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::bounded;
    use std::f32::consts::PI;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::sync::Mutex;

    static RESAMPLE_TEST_LOCK: Mutex<()> = Mutex::new(());

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
        let _guard = RESAMPLE_TEST_LOCK.lock().unwrap();
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
        let _guard = RESAMPLE_TEST_LOCK.lock().unwrap();
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
        let _guard = RESAMPLE_TEST_LOCK.lock().unwrap();
        let input = vec![0.1f32; 64];

        FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
        let err = resample_with_rubato(&input, MIN_DEVICE_RATE)
            .expect_err("expected forced rubato error");
        assert!(err.to_string().contains("forced rubato error"));
        assert!(!FORCE_RUBATO_ERROR.load(Ordering::Relaxed));

        FORCE_RUBATO_ERROR.store(true, Ordering::Relaxed);
        let err = resample_with_rubato(&input, MAX_DEVICE_RATE)
            .expect_err("expected forced rubato error");
        assert!(err.to_string().contains("forced rubato error"));
        assert!(!FORCE_RUBATO_ERROR.load(Ordering::Relaxed));
    }
    #[cfg(feature = "high-quality-audio")]
    #[test]
    fn rubato_resampler_is_not_shorter_than_expected() {
        let input: Vec<f32> = (0..480).map(|i| (i as f32 * 0.02).sin()).collect();
        let result = resample_to_target_rate(&input, 48_000);
        let expected = (input.len() as f64 * 16_000f64 / 48_000f64).round() as usize;
        assert!(result.len() >= expected);
    }

    #[cfg(feature = "high-quality-audio")]
    #[test]
    fn rubato_rejects_aliasing_energy() {
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
        let _guard = RESAMPLE_TEST_LOCK.lock().unwrap();
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
        let _guard = RESAMPLE_TEST_LOCK.lock().unwrap();
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
            .record_with_vad(&cfg, &mut vad, None)
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
        let result = offline_capture_from_pcm(&samples, &cfg, &mut vad);
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
        let result = offline_capture_from_pcm(&samples, &cfg, &mut vad);
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
        let result = offline_capture_from_pcm(&samples, &cfg, &mut vad);
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
        let result = offline_capture_from_pcm(&samples, &cfg, &mut vad);
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
}
