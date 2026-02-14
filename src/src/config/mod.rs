//! App configuration model so CLI flags and defaults produce one validated runtime config.

mod defaults;
#[cfg(test)]
mod tests;
mod validation;

use clap::{ArgAction, Parser, ValueEnum};
use std::path::PathBuf;

use defaults::{default_term, DEFAULT_PIPELINE_SCRIPT};
pub use defaults::{
    default_vad_engine, DEFAULT_MIC_METER_AMBIENT_MS, DEFAULT_MIC_METER_SPEECH_MS,
    DEFAULT_VOICE_BUFFER_MS, DEFAULT_VOICE_CHANNEL_CAPACITY, DEFAULT_VOICE_LOOKBACK_MS,
    DEFAULT_VOICE_MAX_CAPTURE_MS, DEFAULT_VOICE_MIN_SPEECH_MS, DEFAULT_VOICE_SAMPLE_RATE,
    DEFAULT_VOICE_SILENCE_TAIL_MS, DEFAULT_VOICE_STT_TIMEOUT_MS, DEFAULT_VOICE_VAD_FRAME_MS,
    DEFAULT_VOICE_VAD_SMOOTHING_FRAMES, DEFAULT_VOICE_VAD_THRESHOLD_DB, MAX_MIC_METER_SAMPLE_MS,
    MIN_MIC_METER_SAMPLE_MS,
};

/// CLI options for the VoiceTerm TUI. Validated values keep downstream subprocesses safe.
#[derive(Debug, Parser, Clone)]
#[command(about = "VoiceTerm TUI", author, version)]
pub struct AppConfig {
    /// Path to the Codex CLI binary
    #[arg(long, default_value = "codex")]
    pub codex_cmd: String,

    /// Path to the Claude CLI binary (IPC mode)
    #[arg(long = "claude-cmd", env = "CLAUDE_CMD", default_value = "claude")]
    pub claude_cmd: String,

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

    // PTY helper removed - using native Rust PtyCliSession instead
    /// Preferred audio input device name
    #[arg(long)]
    pub input_device: Option<String>,

    /// Print detected audio input devices and exit
    #[arg(long = "list-input-devices", default_value_t = false)]
    pub list_input_devices: bool,

    /// Print environment diagnostics and exit
    #[arg(long = "doctor", default_value_t = false)]
    pub doctor: bool,

    /// Run mic meter and suggest a VAD threshold, then exit
    #[arg(long = "mic-meter", default_value_t = false)]
    pub mic_meter: bool,

    /// Ambient noise sample duration for mic meter (milliseconds)
    #[arg(long = "mic-meter-ambient-ms", default_value_t = DEFAULT_MIC_METER_AMBIENT_MS)]
    pub mic_meter_ambient_ms: u64,

    /// Speech sample duration for mic meter (milliseconds)
    #[arg(long = "mic-meter-speech-ms", default_value_t = DEFAULT_MIC_METER_SPEECH_MS)]
    pub mic_meter_speech_ms: u64,

    /// Enable notification sounds (terminal bell)
    #[arg(long = "sounds", default_value_t = false)]
    pub sounds: bool,

    /// Play a sound when a transcript completes
    #[arg(long = "sound-on-complete", default_value_t = false)]
    pub sound_on_complete: bool,

    /// Play a sound when a voice capture error occurs
    #[arg(long = "sound-on-error", default_value_t = false)]
    pub sound_on_error: bool,

    /// Enable persistent Codex PTY session (captures full TUI, use --persistent-codex to enable)
    #[arg(long = "persistent-codex", default_value_t = false)]
    pub persistent_codex: bool,

    /// Enable file logging (debug)
    #[arg(long = "logs", env = "VOICETERM_LOGS", default_value_t = false)]
    pub logs: bool,

    /// Disable all file logging (overrides --logs and log env vars)
    #[arg(long = "no-logs", env = "VOICETERM_NO_LOGS", default_value_t = false)]
    pub no_logs: bool,

    /// Allow logging prompt/content snippets (debug log only)
    #[arg(
        long = "log-content",
        env = "VOICETERM_LOG_CONTENT",
        default_value_t = false
    )]
    pub log_content: bool,

    /// Enable verbose timing logs
    #[arg(long)]
    pub log_timings: bool,

    /// Allow Claude CLI to run without permission prompts (IPC mode)
    #[arg(long = "claude-skip-permissions", default_value_t = false)]
    pub claude_skip_permissions: bool,

    /// Path to whisper executable
    #[arg(long, default_value = "whisper")]
    pub whisper_cmd: String,

    /// Whisper model name
    #[arg(long, default_value = "small")]
    pub whisper_model: String,

    /// Whisper model path (required for whisper.cpp)
    #[arg(long)]
    pub whisper_model_path: Option<String>,

    /// Whisper beam size (native pipeline only; >1 enables beam search)
    #[arg(long = "whisper-beam-size", default_value_t = 0)]
    pub whisper_beam_size: u32,

    /// Whisper temperature (native pipeline only)
    #[arg(long = "whisper-temperature", default_value_t = 0.0)]
    pub whisper_temperature: f32,

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

    /// VAD smoothing window (frames)
    #[arg(
        long = "voice-vad-smoothing-frames",
        default_value_t = DEFAULT_VOICE_VAD_SMOOTHING_FRAMES
    )]
    pub voice_vad_smoothing_frames: usize,

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
    /// Input sample rate used by capture and VAD.
    pub sample_rate: u32,
    /// Maximum capture duration before forced stop (milliseconds).
    pub max_capture_ms: u64,
    /// Silence required to auto-stop after speech (milliseconds).
    pub silence_tail_ms: u64,
    /// Minimum detected speech before STT processing begins (milliseconds).
    pub min_speech_ms_before_stt_start: u64,
    /// Audio kept before silence stop to avoid clipping endings (milliseconds).
    pub lookback_ms: u64,
    /// Total rolling capture buffer budget (milliseconds).
    pub buffer_ms: u64,
    /// Bounded frame-channel capacity between capture and processing workers.
    pub channel_capacity: usize,
    /// Timeout for STT processing before fallback/error (milliseconds).
    pub stt_timeout_ms: u64,
    /// VAD dB threshold for speech detection.
    pub vad_threshold_db: f32,
    /// Frame size fed to VAD (milliseconds).
    pub vad_frame_ms: u64,
    /// Number of frames used for VAD smoothing/majority vote.
    pub vad_smoothing_frames: usize,
    /// Whether Python fallback is allowed when native path fails.
    pub python_fallback_allowed: bool,
    /// Selected VAD engine implementation.
    pub vad_engine: VadEngineKind,
}

/// Available runtime-selectable VAD implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VadEngineKind {
    /// Earshot-based VAD implementation.
    Earshot,
    /// Simple RMS-threshold VAD implementation.
    Simple,
}

impl VadEngineKind {
    /// Stable lowercase identifier used in logs, metrics, and status output.
    pub fn label(self) -> &'static str {
        match self {
            VadEngineKind::Earshot => "earshot",
            VadEngineKind::Simple => "simple",
        }
    }
}
