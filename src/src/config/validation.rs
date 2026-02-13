//! Configuration validation and normalization so invalid runtime settings fail fast.

use super::defaults::{
    FORBIDDEN_DEVICE_CHARS, ISO_639_1_CODES, MAX_CAPTURE_HARD_LIMIT_MS, MAX_CODEX_ARGS,
    MAX_CODEX_ARG_BYTES,
};
use super::{AppConfig, VoicePipelineConfig, MAX_MIC_METER_SAMPLE_MS, MIN_MIC_METER_SAMPLE_MS};
use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

impl AppConfig {
    /// Parse CLI arguments and validate them right away.
    pub fn parse_args() -> Result<Self> {
        let mut config = Self::parse();
        config.validate()?;
        Ok(config)
    }

    /// Check CLI values and normalize paths.
    pub fn validate(&mut self) -> Result<()> {
        const MIN_RECORD_SECONDS: u64 = 1;
        const MAX_RECORD_SECONDS: u64 = 60;
        let repo_root = canonical_repo_root()?;

        if !(MIN_RECORD_SECONDS..=MAX_RECORD_SECONDS).contains(&self.seconds) {
            bail!(
                "--seconds must be between {MIN_RECORD_SECONDS} and {MAX_RECORD_SECONDS}, got {}",
                self.seconds
            );
        }

        if !(8_000..=96_000).contains(&self.voice_sample_rate) {
            bail!(
                "--voice-sample-rate must be between 8000 and 96000 Hz, got {}",
                self.voice_sample_rate
            );
        }
        if self.voice_max_capture_ms == 0 || self.voice_max_capture_ms > MAX_CAPTURE_HARD_LIMIT_MS {
            bail!(
                "--voice-max-capture-ms must be between 1 and {MAX_CAPTURE_HARD_LIMIT_MS} ms, got {}",
                self.voice_max_capture_ms
            );
        }
        if self.voice_silence_tail_ms < 200
            || self.voice_silence_tail_ms > self.voice_max_capture_ms
        {
            bail!(
                "--voice-silence-tail-ms must be >=200 and <= --voice-max-capture-ms ({})",
                self.voice_max_capture_ms
            );
        }
        if self.voice_min_speech_ms_before_stt_start < 50
            || self.voice_min_speech_ms_before_stt_start > self.voice_max_capture_ms
        {
            bail!(
                "--voice-min-speech-ms-before-stt must be between 50 and {}",
                self.voice_max_capture_ms
            );
        }
        if self.voice_lookback_ms > self.voice_max_capture_ms {
            bail!(
                "--voice-lookback-ms ({}) cannot exceed --voice-max-capture-ms ({})",
                self.voice_lookback_ms,
                self.voice_max_capture_ms
            );
        }
        if self.voice_buffer_ms < self.voice_max_capture_ms || self.voice_buffer_ms > 120_000 {
            bail!(
                "--voice-buffer-ms must be between {} and 120000 (ms)",
                self.voice_max_capture_ms
            );
        }
        if !(8..=1024).contains(&self.voice_channel_capacity) {
            bail!(
                "--voice-channel-capacity must be between 8 and 1024, got {}",
                self.voice_channel_capacity
            );
        }
        if self.voice_stt_timeout_ms < self.voice_max_capture_ms
            || self.voice_stt_timeout_ms > 120_000
        {
            bail!(
                "--voice-stt-timeout-ms must be between {} and 120000",
                self.voice_max_capture_ms
            );
        }
        if !(-120.0..=0.0).contains(&self.voice_vad_threshold_db) {
            bail!(
                "--voice-vad-threshold-db must be between -120.0 and 0.0 dB, got {}",
                self.voice_vad_threshold_db
            );
        }
        if !(5..=120).contains(&self.voice_vad_frame_ms) {
            bail!(
                "--voice-vad-frame-ms must be between 5 and 120, got {}",
                self.voice_vad_frame_ms
            );
        }
        if !(1..=10).contains(&self.voice_vad_smoothing_frames) {
            bail!(
                "--voice-vad-smoothing-frames must be between 1 and 10, got {}",
                self.voice_vad_smoothing_frames
            );
        }
        if !(MIN_MIC_METER_SAMPLE_MS..=MAX_MIC_METER_SAMPLE_MS).contains(&self.mic_meter_ambient_ms)
        {
            bail!(
                "--mic-meter-ambient-ms must be between {MIN_MIC_METER_SAMPLE_MS} and {MAX_MIC_METER_SAMPLE_MS} ms"
            );
        }
        if !(MIN_MIC_METER_SAMPLE_MS..=MAX_MIC_METER_SAMPLE_MS).contains(&self.mic_meter_speech_ms)
        {
            bail!(
                "--mic-meter-speech-ms must be between {MIN_MIC_METER_SAMPLE_MS} and {MAX_MIC_METER_SAMPLE_MS} ms"
            );
        }
        if self.whisper_beam_size > 10 {
            bail!(
                "--whisper-beam-size must be between 0 and 10, got {}",
                self.whisper_beam_size
            );
        }
        if !(0.0..=5.0).contains(&self.whisper_temperature) {
            bail!(
                "--whisper-temperature must be between 0.0 and 5.0, got {}",
                self.whisper_temperature
            );
        }

        #[cfg(not(feature = "vad_earshot"))]
        if matches!(self.voice_vad_engine, super::VadEngineKind::Earshot) {
            bail!("--voice-vad-engine earshot requires building with the 'vad_earshot' feature");
        }

        self.codex_cmd = sanitize_binary(&self.codex_cmd, "--codex-cmd", &["codex"])?;
        self.claude_cmd = sanitize_binary(&self.claude_cmd, "--claude-cmd", &["claude"])?;
        self.python_cmd =
            sanitize_binary(&self.python_cmd, "--python-cmd", &["python3", "python"])?;
        self.ffmpeg_cmd = sanitize_binary(&self.ffmpeg_cmd, "--ffmpeg-cmd", &["ffmpeg"])?;
        self.whisper_cmd = sanitize_binary(
            &self.whisper_cmd,
            "--whisper-cmd",
            &["whisper", "whisper.cpp"],
        )?;

        #[cfg(test)]
        ensure_test_pipeline_script(&self.pipeline_script);
        // Keep helper scripts inside this repo.
        self.pipeline_script =
            canonicalize_within_repo(&self.pipeline_script, "pipeline script", &repo_root)?;
        // PTY helper removed - using native Rust PtyCliSession instead

        if self.whisper_model_path.is_none() {
            if let Some(auto_model) =
                discover_default_whisper_model(&repo_root, &self.whisper_model)
            {
                self.whisper_model_path = Some(auto_model.to_string_lossy().to_string());
            }
        }

        // If a model path was supplied (explicitly or via auto-detect), make sure it exists.
        if let Some(model) = &self.whisper_model_path {
            let model_path = Path::new(model);
            if !model_path.exists() {
                bail!(
                    "whisper model path '{}' does not exist",
                    model_path.display()
                );
            }
        }

        if let Some(model) = &mut self.whisper_model_path {
            // Store a canonical absolute path for subprocesses.
            let canonical = Path::new(model)
                .canonicalize()
                .with_context(|| format!("failed to canonicalize whisper model path '{model}'"))?;
            *model = canonical
                .to_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow!("whisper model path must be valid UTF-8"))?;
        }

        if self.lang.trim().is_empty() {
            bail!("--lang must not be empty");
        }
        if !self.lang.eq_ignore_ascii_case("auto") {
            if !self
                .lang
                .chars()
                .all(|ch| ch.is_ascii_alphabetic() || ch == '-' || ch == '_')
            {
                bail!("--lang must contain only alphabetic characters or '-'/'_' separators");
            }
            // Allow locale-style values but only check the leading ISO-639-1 code.
            let lang_primary = self
                .lang
                .split(['-', '_'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            if !ISO_639_1_CODES.contains(&lang_primary.as_str()) {
                bail!(
                    "--lang must start with a valid ISO-639-1 code or be 'auto', got '{}'",
                    self.lang
                );
            }
        }

        // Avoid huge argument lists when forwarding to Codex.
        if self.codex_args.len() > MAX_CODEX_ARGS {
            bail!(
                "--codex-arg repeated too many times (max {MAX_CODEX_ARGS}, got {})",
                self.codex_args.len()
            );
        }
        // Also limit the total byte length to keep argv small.
        let total_arg_bytes: usize = self.codex_args.iter().map(|arg| arg.len()).sum();
        if total_arg_bytes > MAX_CODEX_ARG_BYTES {
            bail!("combined --codex-arg length exceeds {MAX_CODEX_ARG_BYTES} bytes");
        }

        // The FFmpeg device string is passed straight to the shell, so keep it simple.
        if let Some(device) = &self.ffmpeg_device {
            if device.len() > 256
                || device.chars().any(|ch| matches!(ch, '\n' | '\r'))
                || device
                    .chars()
                    .any(|ch| FORBIDDEN_DEVICE_CHARS.contains(&ch))
            {
                bail!(
                    "--ffmpeg-device must be <=256 characters with no control or shell metacharacters"
                );
            }
        }

        Ok(())
    }

    /// Snapshot the current CLI-controlled voice/VAD settings for downstream consumers.
    pub fn voice_pipeline_config(&self) -> VoicePipelineConfig {
        VoicePipelineConfig {
            sample_rate: self.voice_sample_rate,
            max_capture_ms: self.voice_max_capture_ms,
            silence_tail_ms: self.voice_silence_tail_ms,
            min_speech_ms_before_stt_start: self.voice_min_speech_ms_before_stt_start,
            lookback_ms: self.voice_lookback_ms,
            buffer_ms: self.voice_buffer_ms,
            channel_capacity: self.voice_channel_capacity,
            stt_timeout_ms: self.voice_stt_timeout_ms,
            vad_threshold_db: self.voice_vad_threshold_db,
            vad_frame_ms: self.voice_vad_frame_ms,
            vad_smoothing_frames: self.voice_vad_smoothing_frames,
            python_fallback_allowed: !self.no_python_fallback,
            vad_engine: self.voice_vad_engine,
        }
    }
}

/// Resolve the repository root by walking up from the Cargo manifest.
pub(super) fn canonical_repo_root() -> Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf();
    repo_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize repo root '{}'", repo_root.display()))
}

#[cfg(test)]
fn ensure_test_pipeline_script(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|err| {
            panic!(
                "failed to create '{parent}': {err}",
                parent = parent.display()
            )
        });
    }
    fs::write(path, "#!/usr/bin/env python3\n")
        .unwrap_or_else(|err| panic!("failed to write '{path}': {err}", path = path.display()));
}

/// Canonicalize a path and ensure it still lives under the repo root.
pub(super) fn canonicalize_within_repo(
    path: &Path,
    label: &str,
    repo_root: &Path,
) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {label} '{}'", path.display()))?;
    if !canonical.starts_with(repo_root) {
        bail!(
            "{label} '{}' must reside within {}",
            canonical.display(),
            repo_root.display()
        );
    }
    Ok(canonical)
}

/// Try to locate a ggml model in the repo's `whisper_models/` directory so the Rust pipeline
/// works out-of-the-box when users haven't provided --whisper-model-path.
pub(super) fn discover_default_whisper_model(
    repo_root: &Path,
    whisper_model: &str,
) -> Option<PathBuf> {
    let models_dir = repo_root.join("whisper_models");
    if !models_dir.exists() {
        return None;
    }

    let mut candidates = Vec::new();
    candidates.push(models_dir.join(format!("ggml-{whisper_model}.en.bin")));
    candidates.push(models_dir.join(format!("ggml-{whisper_model}.bin")));
    candidates.push(models_dir.join("ggml-base.en.bin"));
    candidates.push(models_dir.join("ggml-base.bin"));

    for candidate in candidates {
        if candidate.exists() {
            if let Ok(canonical) = candidate.canonicalize() {
                return Some(canonical);
            }
        }
    }

    None
}

/// Allow either a known binary name or an absolute path.
pub(super) fn sanitize_binary(value: &str, flag: &str, allowlist: &[&str]) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{flag} cannot be empty");
    }
    if let Some(allowed) = allowlist
        .iter()
        .find(|candidate| candidate.eq_ignore_ascii_case(trimmed))
    {
        return Ok((*allowed).to_string());
    }

    let path = Path::new(trimmed);
    if path.is_absolute() || trimmed.contains(std::path::MAIN_SEPARATOR) {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {flag} '{trimmed}'"))?;
        let metadata = fs::metadata(&canonical)
            .with_context(|| format!("failed to inspect {flag} '{}'", canonical.display()))?;
        if !metadata.is_file() {
            bail!("{flag} '{}' is not a file", canonical.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();
            if mode & 0o111 == 0 {
                bail!(
                    "{flag} '{}' exists but is not executable (mode {:o})",
                    canonical.display(),
                    mode
                );
            }
        }
        return canonical
            .to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("{flag} must be valid UTF-8"));
    }

    bail!("{flag} must be one of {allowlist:?} or an existing binary path");
}
