//! Earshot-powered Voice Activity Detector adapter implementing `VadEngine`.

use crate::audio::{VadDecision, VadEngine};
use crate::config::VoicePipelineConfig;
use earshot::{VoiceActivityDetector, VoiceActivityProfile};

/// Thin wrapper that adapts `earshot` to the crate's `VadEngine` trait.
pub struct EarshotVad {
    detector: VoiceActivityDetector,
    frame_samples: usize,
    scratch: Vec<i16>,
}

impl EarshotVad {
    pub fn from_config(cfg: &VoicePipelineConfig) -> Self {
        let profile = match cfg.vad_threshold_db {
            t if t <= -50.0 => VoiceActivityProfile::VERY_AGGRESSIVE,
            t if t <= -40.0 => VoiceActivityProfile::AGGRESSIVE,
            t if t <= -30.0 => VoiceActivityProfile::LBR,
            _ => VoiceActivityProfile::QUALITY,
        };
        let frame_ms = cfg.vad_frame_ms.clamp(10, 30) as usize;
        let frame_samples = ((cfg.sample_rate as usize) * frame_ms) / 1000;
        Self {
            detector: VoiceActivityDetector::new(profile),
            frame_samples: frame_samples.max(160),
            scratch: Vec::new(),
        }
    }
}

impl VadEngine for EarshotVad {
    fn process_frame(&mut self, samples: &[f32]) -> VadDecision {
        if samples.is_empty() {
            return VadDecision::Uncertain;
        }
        self.scratch.clear();
        self.scratch.reserve(self.frame_samples);
        for chunk in samples.iter().copied() {
            let clamped = chunk.clamp(-1.0, 1.0);
            self.scratch.push((clamped * 32_768.0) as i16);
        }
        if self.scratch.len() < self.frame_samples {
            self.scratch.resize(self.frame_samples, 0);
        } else if self.scratch.len() > self.frame_samples {
            self.scratch.truncate(self.frame_samples);
        }
        match self.detector.predict_16khz(&self.scratch) {
            Ok(true) => VadDecision::Speech,
            Ok(false) => VadDecision::Silence,
            Err(_) => VadDecision::Uncertain,
        }
    }

    fn reset(&mut self) {
        self.detector.reset();
    }

    fn name(&self) -> &'static str {
        "earshot_vad"
    }
}
