//! Typed JSON IPC protocol so frontend/backends exchange stable event contracts.
//!
//! Defines the message types exchanged between the Rust backend and external
//! frontends (e.g., UI clients). Messages are newline-delimited JSON.

use serde::{Deserialize, Serialize};

// ============================================================================
// IPC Events (Rust → client)
// ============================================================================

/// Events emitted by the Rust backend.
///
/// Serialized as JSON with a `"event"` tag field for type discrimination.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event")]
pub enum IpcEvent {
    /// Sent once on startup with full capability information
    #[serde(rename = "capabilities")]
    Capabilities {
        /// Unique runtime session id for event correlation.
        session_id: String,
        /// VoxTerm semantic version.
        version: String,
        /// Whether microphone capture is currently available.
        mic_available: bool,
        /// Active input-device label when microphone is available.
        input_device: Option<String>,
        /// Whether Whisper model is loaded and ready.
        whisper_model_loaded: bool,
        /// Resolved Whisper model path when configured.
        whisper_model_path: Option<String>,
        /// Whether Python fallback is enabled.
        python_fallback_allowed: bool,
        /// Providers available in this runtime.
        providers_available: Vec<String>,
        /// Currently selected provider.
        active_provider: String,
        /// Current working directory for command execution.
        working_dir: String,
        /// Effective Codex command executable.
        codex_cmd: String,
        /// Effective Claude command executable.
        claude_cmd: String,
    },

    /// Provider changed successfully
    #[serde(rename = "provider_changed")]
    ProviderChanged {
        /// Provider name now active after switch.
        provider: String,
    },

    /// Error when trying to use a provider-specific command on wrong provider
    #[serde(rename = "provider_error")]
    ProviderError {
        /// Human-readable explanation of provider mismatch.
        message: String,
    },

    /// Authentication flow started (TTY login)
    #[serde(rename = "auth_start")]
    AuthStart {
        /// Provider currently being authenticated.
        provider: String,
    },

    /// Authentication flow ended
    #[serde(rename = "auth_end")]
    AuthEnd {
        /// Provider that completed authentication.
        provider: String,
        /// Whether authentication succeeded.
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        /// Optional error message when authentication failed.
        error: Option<String>,
    },

    /// Streaming token from provider
    #[serde(rename = "token")]
    Token {
        /// Incremental output chunk from provider.
        text: String,
    },

    /// Voice capture started
    #[serde(rename = "voice_start")]
    VoiceStart,

    /// Voice capture ended
    #[serde(rename = "voice_end")]
    VoiceEnd {
        #[serde(skip_serializing_if = "Option::is_none")]
        /// Optional error when voice capture failed.
        error: Option<String>,
    },

    /// Transcript ready from voice capture
    #[serde(rename = "transcript")]
    Transcript {
        /// Final transcript text.
        text: String,
        /// End-to-end voice capture duration in milliseconds.
        duration_ms: u64,
    },

    /// Provider job started
    #[serde(rename = "job_start")]
    JobStart {
        /// Provider handling the started job.
        provider: String,
    },

    /// Provider job ended
    #[serde(rename = "job_end")]
    JobEnd {
        /// Provider that handled the completed job.
        provider: String,
        /// Whether provider job ended successfully.
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        /// Optional failure detail when `success` is false.
        error: Option<String>,
    },

    /// Status update
    #[serde(rename = "status")]
    Status {
        /// Human-readable runtime status message.
        message: String,
    },

    /// Error (recoverable or fatal)
    #[serde(rename = "error")]
    Error {
        /// Human-readable error description.
        message: String,
        /// Whether caller may continue without restarting session.
        recoverable: bool,
    },
}

// ============================================================================
// IPC Commands (client → Rust)
// ============================================================================

/// Commands received from an IPC client
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd")]
pub enum IpcCommand {
    /// Send a prompt to the active provider
    #[serde(rename = "send_prompt")]
    SendPrompt {
        /// Prompt text to send to the active provider.
        prompt: String,
        /// Optional one-off provider override
        #[serde(default)]
        provider: Option<String>,
    },

    /// Start voice capture
    #[serde(rename = "start_voice")]
    StartVoice,

    /// Cancel current operation
    #[serde(rename = "cancel")]
    Cancel,

    /// Set the active provider
    #[serde(rename = "set_provider")]
    SetProvider {
        /// Provider name to activate.
        provider: String,
    },

    /// Authenticate with provider via /dev/tty login
    #[serde(rename = "auth")]
    Auth {
        #[serde(default)]
        /// Optional provider override for this auth request.
        provider: Option<String>,
    },

    /// Request capabilities (re-emit capabilities event)
    #[serde(rename = "get_capabilities")]
    GetCapabilities,
}

// ============================================================================
// Provider Abstraction
// ============================================================================

/// Supported providers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// OpenAI Codex CLI provider.
    Codex,
    /// Anthropic Claude CLI provider.
    Claude,
}

impl Provider {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Provider::Codex => "codex",
            Provider::Claude => "claude",
        }
    }

    pub(crate) fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "codex" => Some(Provider::Codex),
            "claude" => Some(Provider::Claude),
            _ => None,
        }
    }
}
