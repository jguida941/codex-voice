//! Audio capture and voice activity detection (VAD) pipeline.
//!
//! Provides microphone recording with automatic silence detection. Audio is
//! captured via CPAL, resampled to 16kHz mono (Whisper's expected format),
//! and returned when the user stops speaking.

/// Target sample rate for Whisper STT.
pub const TARGET_RATE: u32 = 16_000;

/// Target channel count for Whisper STT.
pub const TARGET_CHANNELS: u32 = 1;

mod capture;
mod dispatch;
mod recorder;
mod resample;
#[cfg(test)]
mod tests;
mod vad;

pub use capture::{offline_capture_from_pcm, CaptureMetrics, CaptureResult, StopReason};
pub use recorder::Recorder;
pub use vad::{SimpleThresholdVad, VadConfig, VadDecision, VadEngine};
