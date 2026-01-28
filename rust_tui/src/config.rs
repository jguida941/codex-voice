//! Command-line parsing and validation helpers.

use anyhow::{anyhow, bail, Context, Result};
use clap::{ArgAction, Parser, ValueEnum};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

const MAX_CODEX_ARGS: usize = 64;
const MAX_CODEX_ARG_BYTES: usize = 8 * 1024;
pub const DEFAULT_VOICE_SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_VOICE_MAX_CAPTURE_MS: u64 = 30_000;
pub const DEFAULT_VOICE_SILENCE_TAIL_MS: u64 = 3000;
pub const DEFAULT_VOICE_MIN_SPEECH_MS: u64 = 300;
pub const DEFAULT_VOICE_LOOKBACK_MS: u64 = 500;
pub const DEFAULT_VOICE_BUFFER_MS: u64 = 30_000;
pub const DEFAULT_VOICE_CHANNEL_CAPACITY: usize = 100;
pub const DEFAULT_VOICE_STT_TIMEOUT_MS: u64 = 60_000;
pub const DEFAULT_VOICE_VAD_THRESHOLD_DB: f32 = -40.0;
pub const DEFAULT_VOICE_VAD_FRAME_MS: u64 = 20;
const MAX_CAPTURE_HARD_LIMIT_MS: u64 = 30_000;
const ISO_639_1_CODES: &[&str] = &[
    "af", "am", "ar", "az", "be", "bg", "bn", "bs", "ca", "cs", "cy", "da", "de", "el", "en", "es",
    "et", "eu", "fa", "fi", "fil", "fr", "ga", "gl", "gu", "he", "hi", "hr", "hu", "hy", "id",
    "is", "it", "ja", "jv", "ka", "kk", "km", "kn", "ko", "lo", "lt", "lv", "mk", "ml", "mn", "mr",
    "ms", "my", "ne", "nl", "no", "pa", "pl", "pt", "ro", "ru", "si", "sk", "sl", "sq", "sr", "sv",
    "sw", "ta", "te", "th", "tr", "uk", "ur", "vi", "zh",
];
// FFmpeg devices are passed to the shell, so strip characters that would let users sneak commands in.
const FORBIDDEN_DEVICE_CHARS: &[char] = &[';', '|', '&', '$', '`', '<', '>', '\\', '\'', '"'];
const DEFAULT_PIPELINE_SCRIPT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/codex_voice.py");
// PTY helper removed - using native Rust PtyCodexSession instead

/// CLI options for the Codex Voice TUI. Validated values keep downstream subprocesses safe.
#[derive(Debug, Parser, Clone)]
#[command(about = "Codex Voice TUI", author, version)]
pub struct AppConfig {
    /// Path to the Codex CLI binary
    #[arg(long, default_value = "codex")]
    pub codex_cmd: String,

    /// Extra arguments to pass to the Codex CLI (repeatable)
    #[arg(long = "codex-arg", action = ArgAction::Append, value_name = "ARG")]
    pub codex_args: Vec<String>,

    /// Path to the python interpreter used for helper scripts
    #[arg(long, default_value = "python3")]
    pub python_cmd: String,

    /// Pipeline script location
    #[arg(long, default_value = DEFAULT_PIPELINE_SCRIPT)]
    pub pipeline_script: PathBuf,

    /// TERM value exported to Codex
    #[arg(long = "term", default_value_t = default_term())]
    pub term_value: String,

    // PTY helper removed - using native Rust PtyCodexSession instead
    /// Preferred audio input device name
    #[arg(long)]
    pub input_device: Option<String>,

    /// Print detected audio input devices and exit
    #[arg(long = "list-input-devices", default_value_t = false)]
    pub list_input_devices: bool,

    /// Enable persistent Codex PTY session (captures full TUI, use --persistent-codex to enable)
    #[arg(long = "persistent-codex", default_value_t = false)]
    pub persistent_codex: bool,

    /// Enable verbose timing logs
    #[arg(long)]
    pub log_timings: bool,

    /// Path to whisper executable
    #[arg(long, default_value = "whisper")]
    pub whisper_cmd: String,

    /// Whisper model name
    #[arg(long, default_value = "small")]
    pub whisper_model: String,

    /// Whisper model path (required for whisper.cpp)
    #[arg(long)]
    pub whisper_model_path: Option<String>,

    /// FFmpeg binary location
    #[arg(long, default_value = "ffmpeg")]
    pub ffmpeg_cmd: String,

    /// FFmpeg audio device override
    #[arg(long)]
    pub ffmpeg_device: Option<String>,

    /// Recording duration in seconds
    #[arg(long, default_value_t = 5)]
    pub seconds: u64,

    /// Target sample rate for the voice pipeline (Hz)
    #[arg(long = "voice-sample-rate", default_value_t = DEFAULT_VOICE_SAMPLE_RATE)]
    pub voice_sample_rate: u32,

    /// Maximum capture duration before a hard stop (milliseconds)
    #[arg(long = "voice-max-capture-ms", default_value_t = DEFAULT_VOICE_MAX_CAPTURE_MS)]
    pub voice_max_capture_ms: u64,

    /// Trailing silence required before stopping capture (milliseconds)
    #[arg(long = "voice-silence-tail-ms", default_value_t = DEFAULT_VOICE_SILENCE_TAIL_MS)]
    pub voice_silence_tail_ms: u64,

    /// Minimum speech before STT can begin (milliseconds)
    #[arg(
        long = "voice-min-speech-ms-before-stt",
        default_value_t = DEFAULT_VOICE_MIN_SPEECH_MS
    )]
    pub voice_min_speech_ms_before_stt_start: u64,

    /// Amount of audio retained prior to silence stop (milliseconds)
    #[arg(long = "voice-lookback-ms", default_value_t = DEFAULT_VOICE_LOOKBACK_MS)]
    pub voice_lookback_ms: u64,

    /// Total buffered audio budget (milliseconds)
    #[arg(long = "voice-buffer-ms", default_value_t = DEFAULT_VOICE_BUFFER_MS)]
    pub voice_buffer_ms: u64,

    /// Frame channel capacity between capture and STT workers
    #[arg(
        long = "voice-channel-capacity",
        default_value_t = DEFAULT_VOICE_CHANNEL_CAPACITY
    )]
    pub voice_channel_capacity: usize,

    /// STT worker timeout before triggering fallback (milliseconds)
    #[arg(long = "voice-stt-timeout-ms", default_value_t = DEFAULT_VOICE_STT_TIMEOUT_MS)]
    pub voice_stt_timeout_ms: u64,

    /// Voice activity detection threshold (decibels)
    #[arg(
        long = "voice-vad-threshold-db",
        default_value_t = DEFAULT_VOICE_VAD_THRESHOLD_DB
    )]
    pub voice_vad_threshold_db: f32,

    /// Voice activity detection frame size (milliseconds)
    #[arg(long = "voice-vad-frame-ms", default_value_t = DEFAULT_VOICE_VAD_FRAME_MS)]
    pub voice_vad_frame_ms: u64,

    /// Voice activity detector implementation to use
    #[arg(
        long = "voice-vad-engine",
        value_enum,
        default_value_t = default_vad_engine()
    )]
    pub voice_vad_engine: VadEngineKind,

    /// Language passed to Whisper
    #[arg(long, default_value = "en")]
    pub lang: String,

    /// Fail instead of using the python STT fallback
    #[arg(long = "no-python-fallback")]
    pub no_python_fallback: bool,

    /// Run in JSON IPC mode for external UI integration
    #[arg(long = "json-ipc")]
    pub json_ipc: bool,
}

/// Tunable parameters for the voice capture + STT pipeline.
#[derive(Debug, Clone)]
pub struct VoicePipelineConfig {
    pub sample_rate: u32,
    pub max_capture_ms: u64,
    pub silence_tail_ms: u64,
    pub min_speech_ms_before_stt_start: u64,
    pub lookback_ms: u64,
    pub buffer_ms: u64,
    pub channel_capacity: usize,
    pub stt_timeout_ms: u64,
    pub vad_threshold_db: f32,
    pub vad_frame_ms: u64,
    pub python_fallback_allowed: bool,
    pub vad_engine: VadEngineKind,
}

/// Available runtime-selectable VAD implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VadEngineKind {
    Earshot,
    Simple,
}

impl VadEngineKind {
    pub fn label(self) -> &'static str {
        match self {
            VadEngineKind::Earshot => "earshot",
            VadEngineKind::Simple => "simple",
        }
    }
}

pub const fn default_vad_engine() -> VadEngineKind {
    #[cfg(feature = "vad_earshot")]
    {
        VadEngineKind::Earshot
    }
    #[cfg(not(feature = "vad_earshot"))]
    {
        VadEngineKind::Simple
    }
}

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

        #[cfg(not(feature = "vad_earshot"))]
        if matches!(self.voice_vad_engine, VadEngineKind::Earshot) {
            bail!("--voice-vad-engine earshot requires building with the 'vad_earshot' feature");
        }

        self.codex_cmd = sanitize_binary(&self.codex_cmd, "--codex-cmd", &["codex"])?;
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
        // PTY helper removed - using native Rust PtyCodexSession instead

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

        if self.lang.trim().is_empty()
            || !self
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
                "--lang must start with a valid ISO-639-1 code, got '{}'",
                self.lang
            );
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
            python_fallback_allowed: !self.no_python_fallback,
            vad_engine: self.voice_vad_engine,
        }
    }
}

/// Use the user's TERM if set; default to xterm-256color otherwise.
fn default_term() -> String {
    env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string())
}

/// Resolve the repository root by walking up from the Cargo manifest.
fn canonical_repo_root() -> Result<PathBuf> {
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
fn canonicalize_within_repo(path: &Path, label: &str, repo_root: &Path) -> Result<PathBuf> {
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

/// Try to locate a ggml model in the repo's `models/` directory so the Rust pipeline
/// works out-of-the-box when users haven't provided --whisper-model-path.
fn discover_default_whisper_model(repo_root: &Path, whisper_model: &str) -> Option<PathBuf> {
    let models_dir = repo_root.join("models");
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
fn sanitize_binary(value: &str, flag: &str, allowlist: &[&str]) -> Result<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::fs;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn base_voice_config() -> AppConfig {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.voice_max_capture_ms = 1000;
        cfg.voice_silence_tail_ms = 200;
        cfg.voice_min_speech_ms_before_stt_start = 50;
        cfg.voice_lookback_ms = 0;
        cfg.voice_buffer_ms = 1000;
        cfg.voice_stt_timeout_ms = 1000;
        cfg
    }

    fn base_voice_config_with_capture(max_capture_ms: u64) -> AppConfig {
        let mut cfg = base_voice_config();
        cfg.voice_max_capture_ms = max_capture_ms;
        cfg.voice_buffer_ms = max_capture_ms;
        cfg.voice_stt_timeout_ms = max_capture_ms;
        cfg
    }

    #[test]
    fn rejects_seconds_out_of_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "0"]);
        assert!(cfg.validate().is_err());

        let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "61"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_seconds_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "1"]);
        assert!(cfg.validate().is_ok());

        let mut cfg = AppConfig::parse_from(["test-app", "--seconds", "60"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_invalid_language_code() {
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en$"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_language_with_unknown_primary_code() {
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", "zz-ZZ"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_language_with_invalid_suffix_chars() {
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en-US$"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_language_with_region_suffixes() {
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", "en-US"]);
        assert!(cfg.validate().is_ok());
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", "pt_BR"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn max_codex_arg_bytes_constant_matches_expectation() {
        assert_eq!(MAX_CODEX_ARG_BYTES, 8 * 1024);
    }

    #[test]
    fn vad_engine_labels_are_stable() {
        assert_eq!(VadEngineKind::Earshot.label(), "earshot");
        assert_eq!(VadEngineKind::Simple.label(), "simple");
    }

    #[test]
    fn accepts_valid_defaults() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn voice_vad_engine_flag_round_trips_into_pipeline_config() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-engine", "simple"]);
        cfg.validate().expect("simple VAD should be valid");
        assert!(matches!(
            cfg.voice_pipeline_config().vad_engine,
            VadEngineKind::Simple
        ));
    }

    #[test]
    fn voice_vad_engine_default_matches_feature() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.validate().expect("defaults should be valid");
        assert_eq!(cfg.voice_vad_engine, default_vad_engine());
    }

    #[cfg(feature = "vad_earshot")]
    #[test]
    fn default_vad_engine_prefers_earshot_when_feature_enabled() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.validate().expect("defaults should be valid");
        assert!(matches!(
            cfg.voice_pipeline_config().vad_engine,
            VadEngineKind::Earshot
        ));
    }

    #[cfg(not(feature = "vad_earshot"))]
    #[test]
    fn default_vad_engine_prefers_simple_when_feature_disabled() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.validate().expect("defaults should be valid");
        assert!(matches!(
            cfg.voice_pipeline_config().vad_engine,
            VadEngineKind::Simple
        ));
    }

    #[test]
    fn rejects_ffmpeg_device_with_shell_metacharacters() {
        for dangerous in [
            "default;rm -rf /",
            "mix|pipe",
            "out & more",
            "name$VAR",
            "quote\"",
            "single'",
            "newline\nbreak",
            "carriage\rreturn",
            "back\\slash",
        ] {
            let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", dangerous]);
            assert!(
                cfg.validate().is_err(),
                "device '{dangerous}' should be rejected"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn codex_cmd_path_must_be_executable() {
        use std::os::unix::fs::PermissionsExt;

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let temp_path = env::temp_dir().join(format!("codex_cmd_test_{unique}"));
        fs::write(&temp_path, "#!/bin/sh\necho test\n").unwrap();
        let mut perms = fs::metadata(&temp_path).unwrap().permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&temp_path, perms.clone()).unwrap();

        let mut cfg =
            AppConfig::parse_from(["test-app", "--codex-cmd", temp_path.to_str().unwrap()]);
        assert!(
            cfg.validate().is_err(),
            "non-executable binary path should be rejected"
        );

        perms.set_mode(0o700);
        fs::set_permissions(&temp_path, perms).unwrap();
        let mut cfg =
            AppConfig::parse_from(["test-app", "--codex-cmd", temp_path.to_str().unwrap()]);
        assert!(
            cfg.validate().is_ok(),
            "executable binary path should be accepted"
        );

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn rejects_invalid_voice_sample_rate() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-sample-rate", "4000"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_sample_rate_bounds() {
        let mut cfg = base_voice_config();
        cfg.voice_sample_rate = 8000;
        assert!(cfg.validate().is_ok());
        let mut cfg = base_voice_config();
        cfg.voice_sample_rate = 96000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_sample_rate_above_max() {
        let mut cfg = base_voice_config();
        cfg.voice_sample_rate = 96001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_voice_max_capture_out_of_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-max-capture-ms", "0"]);
        assert!(cfg.validate().is_err());
        let mut cfg = base_voice_config_with_capture(MAX_CAPTURE_HARD_LIMIT_MS + 1);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_max_capture_limit() {
        let mut cfg = base_voice_config_with_capture(30000);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn accepts_voice_max_capture_minimum() {
        let mut cfg = base_voice_config_with_capture(200);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_silence_tail_out_of_bounds() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_silence_tail_ms = 199;
        assert!(cfg.validate().is_err());
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_silence_tail_ms = 1001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_silence_tail_lower_bound() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_silence_tail_ms = 200;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn accepts_voice_silence_tail_equal_to_max_capture() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_silence_tail_ms = 1000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_min_speech_out_of_bounds() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_min_speech_ms_before_stt_start = 49;
        assert!(cfg.validate().is_err());
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_min_speech_ms_before_stt_start = 1001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_min_speech_lower_bound() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_min_speech_ms_before_stt_start = 50;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn accepts_voice_min_speech_equal_to_max_capture() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_min_speech_ms_before_stt_start = 1000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_lookback_exceeds_capture() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_lookback_ms = 1001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_lookback_equal_to_capture() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_lookback_ms = 1000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_buffer_smaller_than_capture_window() {
        let mut cfg = base_voice_config_with_capture(15000);
        cfg.voice_buffer_ms = 10000;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_voice_buffer_above_max() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_buffer_ms = 120001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_buffer_at_bounds() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_buffer_ms = 1000;
        assert!(cfg.validate().is_ok());
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_buffer_ms = 120000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_channel_capacity_out_of_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "4"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_voice_channel_capacity_above_max() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "1025"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_channel_capacity_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "8"]);
        assert!(cfg.validate().is_ok());
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-channel-capacity", "1024"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_stt_timeout_out_of_bounds() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_stt_timeout_ms = 999;
        assert!(cfg.validate().is_err());
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_stt_timeout_ms = 120001;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_stt_timeout_bounds() {
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_stt_timeout_ms = 1000;
        assert!(cfg.validate().is_ok());
        let mut cfg = base_voice_config_with_capture(1000);
        cfg.voice_stt_timeout_ms = 120000;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_vad_threshold_out_of_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db", "1.0"]);
        assert!(cfg.validate().is_err());
        let mut cfg =
            AppConfig::parse_from(["test-app", "--voice-vad-threshold-db=-120.1"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_vad_threshold_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-threshold-db", "0.0"]);
        assert!(cfg.validate().is_ok());
        let mut cfg =
            AppConfig::parse_from(["test-app", "--voice-vad-threshold-db=-120.0"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_voice_vad_frame_out_of_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "4"]);
        assert!(cfg.validate().is_err());
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "121"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_voice_vad_frame_bounds() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "5"]);
        assert!(cfg.validate().is_ok());
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-frame-ms", "120"]);
        assert!(cfg.validate().is_ok());
    }

    #[cfg(not(feature = "vad_earshot"))]
    #[test]
    fn rejects_earshot_vad_engine_without_feature() {
        let mut cfg = AppConfig::parse_from(["test-app", "--voice-vad-engine", "earshot"]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_empty_language() {
        let mut cfg = AppConfig::parse_from(["test-app", "--lang", ""]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_too_many_codex_args() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.codex_args = (0..=MAX_CODEX_ARGS).map(|_| "x".to_string()).collect();
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_codex_args_at_limit() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.codex_args = (0..MAX_CODEX_ARGS).map(|_| "x".to_string()).collect();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_excessive_codex_arg_bytes() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.codex_args = vec!["a".repeat(MAX_CODEX_ARG_BYTES + 1)];
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_codex_arg_bytes_at_limit() {
        let mut cfg = AppConfig::parse_from(["test-app"]);
        cfg.codex_args = vec!["a".repeat(MAX_CODEX_ARG_BYTES)];
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn rejects_ffmpeg_device_over_max_length() {
        let long_name = "a".repeat(257);
        let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", &long_name]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_ffmpeg_device_at_max_length() {
        let name = "a".repeat(256);
        let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", &name]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn accepts_ffmpeg_device_without_shell_chars() {
        let mut cfg = AppConfig::parse_from(["test-app", "--ffmpeg-device", "BuiltInMic"]);
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn voice_pipeline_config_respects_python_fallback_flag() {
        let mut cfg = AppConfig::parse_from(["test-app", "--no-python-fallback"]);
        cfg.validate().unwrap();
        assert!(!cfg.voice_pipeline_config().python_fallback_allowed);
    }

    #[test]
    fn default_term_prefers_env() {
        static TERM_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = TERM_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let original = env::var("TERM").ok();
        env::set_var("TERM", "vt100");
        assert_eq!(default_term(), "vt100");
        env::remove_var("TERM");
        assert_eq!(default_term(), "xterm-256color");
        if let Some(value) = original {
            env::set_var("TERM", value);
        } else {
            env::remove_var("TERM");
        }
    }

    #[test]
    fn canonical_repo_root_matches_manifest_parent() {
        let root = canonical_repo_root().unwrap();
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let expected = manifest
            .parent()
            .unwrap_or(manifest)
            .canonicalize()
            .unwrap();
        assert_eq!(root, expected);
    }

    #[test]
    fn canonicalize_within_repo_rejects_outside_path() {
        let repo_root = canonical_repo_root().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut outside_dir = repo_root
            .parent()
            .unwrap_or(&repo_root)
            .join(format!("codex_voice_outside_{unique}"));
        if outside_dir.starts_with(&repo_root) {
            outside_dir = env::temp_dir().join(format!("codex_voice_outside_{unique}"));
        }
        let outside = outside_dir.join("outside.txt");
        fs::create_dir_all(&outside_dir).unwrap();
        fs::write(&outside, "x").unwrap();
        assert!(canonicalize_within_repo(&outside, "outside", &repo_root).is_err());
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(outside_dir);
    }

    #[test]
    fn canonicalize_within_repo_accepts_inside_path() {
        let repo_root = canonical_repo_root().unwrap();
        let temp_dir = repo_root.join("tmp_test_config");
        fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("inside.txt");
        fs::write(&file_path, "x").unwrap();
        let canonical =
            canonicalize_within_repo(&file_path, "inside", &repo_root).unwrap();
        assert!(canonical.starts_with(&repo_root));
        let _ = fs::remove_file(&file_path);
        let _ = fs::remove_dir(&temp_dir);
    }

    #[test]
    fn validate_rejects_pipeline_script_outside_repo() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let repo_root = canonical_repo_root().unwrap();
        let mut outside_dir = repo_root
            .parent()
            .unwrap_or(&repo_root)
            .join(format!("pipeline_outside_{unique}"));
        if outside_dir.starts_with(&repo_root) {
            outside_dir = env::temp_dir().join(format!("pipeline_outside_{unique}"));
        }
        let script_path = outside_dir.join("pipeline.py");
        fs::create_dir_all(&outside_dir).unwrap();
        let mut cfg = AppConfig::parse_from([
            "test-app",
            "--pipeline-script",
            script_path.to_str().unwrap(),
        ]);
        assert!(cfg.validate().is_err());
        let _ = fs::remove_file(&script_path);
        let _ = fs::remove_dir_all(&outside_dir);
    }

    #[test]
    fn discover_default_whisper_model_finds_candidate() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let repo_root = env::temp_dir().join(format!("whisper_models_{unique}"));
        let models_dir = repo_root.join("models");
        fs::create_dir_all(&models_dir).unwrap();
        let candidate = models_dir.join("ggml-unit.en.bin");
        fs::write(&candidate, "x").unwrap();
        let found = discover_default_whisper_model(&repo_root, "unit");
        assert!(found.is_some());
        let _ = fs::remove_file(&candidate);
        let _ = fs::remove_dir(&models_dir);
        let _ = fs::remove_dir(&repo_root);
    }

    #[test]
    fn discover_default_whisper_model_returns_none_when_missing() {
        let repo_root = env::temp_dir().join("whisper_models_empty");
        let _ = fs::remove_dir_all(&repo_root);
        assert!(discover_default_whisper_model(&repo_root, "unit").is_none());
    }

    #[test]
    fn validate_rejects_missing_whisper_model_path() {
        let missing = env::temp_dir().join("missing_model.bin");
        let mut cfg = AppConfig::parse_from([
            "test-app",
            "--whisper-model-path",
            missing.to_str().unwrap(),
        ]);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_accepts_existing_whisper_model_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let model_path = env::temp_dir().join(format!("model_{unique}.bin"));
        fs::write(&model_path, "x").unwrap();
        let mut cfg = AppConfig::parse_from([
            "test-app",
            "--whisper-model-path",
            model_path.to_str().unwrap(),
        ]);
        assert!(cfg.validate().is_ok());
        let canonical = model_path.canonicalize().unwrap();
        assert_eq!(
            cfg.whisper_model_path.as_deref(),
            canonical.to_str()
        );
        let _ = fs::remove_file(&model_path);
    }

    #[test]
    fn sanitize_binary_accepts_allowlist_case_insensitive() {
        let sanitized = sanitize_binary("CoDeX", "--codex-cmd", &["codex"]).unwrap();
        assert_eq!(sanitized, "codex");
    }

    #[test]
    fn sanitize_binary_rejects_empty() {
        assert!(sanitize_binary("   ", "--codex-cmd", &["codex"]).is_err());
    }

    #[test]
    fn sanitize_binary_rejects_missing_relative_path() {
        let result = sanitize_binary("bin/does-not-exist", "--codex-cmd", &["codex"]);
        assert!(result.is_err());
    }

    #[test]
    fn sanitize_binary_rejects_directory_path() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let dir_path = env::temp_dir().join(format!("codex_dir_{unique}"));
        fs::create_dir_all(&dir_path).unwrap();
        let result =
            sanitize_binary(dir_path.to_str().unwrap(), "--codex-cmd", &["codex"]);
        assert!(result.is_err());
        let _ = fs::remove_dir(&dir_path);
    }

    #[cfg(unix)]
    #[test]
    fn sanitize_binary_accepts_relative_path_with_separator() {
        use std::os::unix::fs::PermissionsExt;

        let cwd = env::current_dir().unwrap();
        let temp_dir = cwd.join("tmp_rel_bin");
        fs::create_dir_all(&temp_dir).unwrap();
        let temp_path = temp_dir.join("codex-rel");
        fs::write(&temp_path, "#!/bin/sh\n").unwrap();
        let mut perms = fs::metadata(&temp_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_path, perms).unwrap();
        let rel_path = Path::new("tmp_rel_bin").join("codex-rel");
        let sanitized =
            sanitize_binary(rel_path.to_str().unwrap(), "--codex-cmd", &["codex"]).unwrap();
        assert!(sanitized.contains("tmp_rel_bin"));
        let _ = fs::remove_file(&temp_path);
        let _ = fs::remove_dir(&temp_dir);
    }

    #[cfg(unix)]
    #[test]
    fn sanitize_binary_accepts_executable_path() {
        use std::os::unix::fs::PermissionsExt;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let temp_path = env::temp_dir().join(format!("codex_bin_{unique}"));
        fs::write(&temp_path, "#!/bin/sh\n").unwrap();
        let mut perms = fs::metadata(&temp_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&temp_path, perms).unwrap();
        let sanitized =
            sanitize_binary(temp_path.to_str().unwrap(), "--codex-cmd", &["codex"]).unwrap();
        assert!(sanitized.contains("codex_bin_"));
        let _ = fs::remove_file(temp_path);
    }
}
