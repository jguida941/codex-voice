//! JSON-based IPC protocol for external UI integration.
//!
//! Defines the message types exchanged between the Rust backend and external
//! frontends (e.g., TypeScript UI). Messages are newline-delimited JSON.

use serde::{Deserialize, Serialize};

// ============================================================================
// IPC Events (Rust → TypeScript)
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
        session_id: String,
        version: String,
        mic_available: bool,
        input_device: Option<String>,
        whisper_model_loaded: bool,
        whisper_model_path: Option<String>,
        python_fallback_allowed: bool,
        providers_available: Vec<String>,
        active_provider: String,
        working_dir: String,
        codex_cmd: String,
        claude_cmd: String,
    },

    /// Provider changed successfully
    #[serde(rename = "provider_changed")]
    ProviderChanged { provider: String },

    /// Error when trying to use a provider-specific command on wrong provider
    #[serde(rename = "provider_error")]
    ProviderError { message: String },

    /// Authentication flow started (TTY login)
    #[serde(rename = "auth_start")]
    AuthStart { provider: String },

    /// Authentication flow ended
    #[serde(rename = "auth_end")]
    AuthEnd {
        provider: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Streaming token from provider
    #[serde(rename = "token")]
    Token { text: String },

    /// Voice capture started
    #[serde(rename = "voice_start")]
    VoiceStart,

    /// Voice capture ended
    #[serde(rename = "voice_end")]
    VoiceEnd {
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Transcript ready from voice capture
    #[serde(rename = "transcript")]
    Transcript { text: String, duration_ms: u64 },

    /// Provider job started
    #[serde(rename = "job_start")]
    JobStart { provider: String },

    /// Provider job ended
    #[serde(rename = "job_end")]
    JobEnd {
        provider: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Status update
    #[serde(rename = "status")]
    Status { message: String },

    /// Error (recoverable or fatal)
    #[serde(rename = "error")]
    Error { message: String, recoverable: bool },
}

// ============================================================================
// IPC Commands (TypeScript → Rust)
// ============================================================================

/// Commands received from the TypeScript frontend
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "cmd")]
pub enum IpcCommand {
    /// Send a prompt to the active provider
    #[serde(rename = "send_prompt")]
    SendPrompt {
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
    SetProvider { provider: String },

    /// Authenticate with provider via /dev/tty login
    #[serde(rename = "auth")]
    Auth {
        #[serde(default)]
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
    Codex,
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
