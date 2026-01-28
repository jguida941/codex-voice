//! JSON IPC mode for external UI integration.
//!
//! This module provides a non-blocking JSON-lines protocol over stdin/stdout
//! so that external frontends (like TypeScript CLIs) can drive the voice + provider pipeline.
//!
//! Architecture:
//! - Stdin reader thread: reads JSON commands, sends to main loop via channel
//! - Main event loop: processes commands and job events concurrently
//! - Provider abstraction: supports both Codex and Claude CLIs
//!
//! Protocol:
//! - Each line is a JSON object
//! - Events (Rust → TS): {"event": "...", ...}
//! - Commands (TS → Rust): {"cmd": "...", ...}

use crate::codex::{
    BackendError, BackendEvent, BackendEventKind, BackendJob, CliBackend, CodexBackend,
    CodexRequest,
};

// ============================================================================
// PTY TOGGLE - Set to false to disable PTY completely
// ============================================================================
const USE_PTY: bool = true;
use crate::config::AppConfig;
use crate::voice::{self, VoiceJob, VoiceJobMessage};
use crate::{audio, log_debug, stt};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::io::{self, BufRead, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(any(test, feature = "mutants"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(test, feature = "mutants"))]
use std::sync::OnceLock;

// ============================================================================
// IPC Events (Rust → TypeScript)
// ============================================================================

/// Events sent from Rust to the TypeScript frontend
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
    fn as_str(&self) -> &'static str {
        match self {
            Provider::Codex => "codex",
            Provider::Claude => "claude",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "codex" => Some(Provider::Codex),
            "claude" => Some(Provider::Claude),
            _ => None,
        }
    }
}

// ============================================================================
// IPC State
// ============================================================================

struct IpcState {
    config: AppConfig,
    active_provider: Provider,
    codex_backend: Arc<CliBackend>,
    claude_cmd: String,
    recorder: Option<Arc<Mutex<audio::Recorder>>>,
    transcriber: Option<Arc<Mutex<stt::Transcriber>>>,
    current_job: Option<ActiveJob>,
    current_voice_job: Option<VoiceJob>,
    current_auth_job: Option<AuthJob>,
    session_id: String,
    cancelled: bool,
}

enum ActiveJob {
    Codex(BackendJob),
    Claude(ClaudeJob),
}

struct ClaudeJob {
    child: std::process::Child,
    stdout_rx: Receiver<String>,
    #[allow(dead_code)]
    started_at: Instant,
}

type AuthResult = std::result::Result<(), String>;

struct AuthJob {
    provider: Provider,
    receiver: Receiver<AuthResult>,
    #[allow(dead_code)]
    started_at: Instant,
}

impl IpcState {
    fn new(mut config: AppConfig) -> Self {
        // Force PTY off if toggle is disabled
        if !USE_PTY {
            config.persistent_codex = false;
            log_debug("PTY disabled via USE_PTY toggle");
        }

        // Generate unique session ID
        let session_id = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        // Get Claude command from env or default
        let claude_cmd = env::var("CLAUDE_CMD").unwrap_or_else(|_| "claude".to_string());

        // Initialize Codex backend
        let codex_backend = Arc::new(CliBackend::new(config.clone()));

        // Get default provider from env or config
        let default_provider = env::var("CODEX_VOICE_PROVIDER")
            .ok()
            .and_then(|s| Provider::from_str(&s))
            .unwrap_or(Provider::Codex);

        // Initialize audio recorder
        let recorder = match audio::Recorder::new(config.input_device.as_deref()) {
            Ok(r) => {
                log_debug("Audio recorder initialized");
                Some(Arc::new(Mutex::new(r)))
            }
            Err(e) => {
                log_debug(&format!("Audio recorder not available: {e}"));
                None
            }
        };

        // Initialize STT
        let transcriber = if let Some(model_path) = &config.whisper_model_path {
            match stt::Transcriber::new(model_path) {
                Ok(t) => {
                    log_debug("Whisper transcriber initialized");
                    Some(Arc::new(Mutex::new(t)))
                }
                Err(e) => {
                    log_debug(&format!("Whisper not available: {e}"));
                    None
                }
            }
        } else {
            log_debug("No whisper model path configured");
            None
        };

        Self {
            config,
            active_provider: default_provider,
            codex_backend,
            claude_cmd,
            recorder,
            transcriber,
            current_job: None,
            current_voice_job: None,
            current_auth_job: None,
            session_id,
            cancelled: false,
        }
    }

    fn emit_capabilities(&self) {
        let providers = vec!["codex".to_string(), "claude".to_string()];

        // Get actual device name from recorder if available
        let input_device = self.recorder.as_ref().map(|r| {
            r.lock()
                .map(|recorder| recorder.device_name())
                .unwrap_or_else(|_| "Unknown Device".to_string())
        });

        send_event(&IpcEvent::Capabilities {
            session_id: self.session_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            mic_available: self.recorder.is_some(),
            input_device,
            whisper_model_loaded: self.transcriber.is_some(),
            whisper_model_path: self.config.whisper_model_path.clone(),
            python_fallback_allowed: !self.config.no_python_fallback,
            providers_available: providers,
            active_provider: self.active_provider.as_str().to_string(),
            working_dir: env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
            codex_cmd: self.config.codex_cmd.clone(),
            claude_cmd: self.claude_cmd.clone(),
        });
    }
}

// ============================================================================
// Event Sending
// ============================================================================

fn send_event(event: &IpcEvent) {
    #[cfg(any(test, feature = "mutants"))]
    if capture_test_event(event) {
        return;
    }
    if let Ok(json) = serde_json::to_string(event) {
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{json}");
        let _ = stdout.flush();
    }
}

#[cfg(any(test, feature = "mutants"))]
static EVENT_SINK: OnceLock<Mutex<Vec<IpcEvent>>> = OnceLock::new();
#[cfg(any(test, feature = "mutants"))]
static IPC_LOOP_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(any(test, feature = "mutants"))]
fn capture_test_event(event: &IpcEvent) -> bool {
    if let Some(sink) = EVENT_SINK.get() {
        if let Ok(mut events) = sink.lock() {
            events.push(event.clone());
            return true;
        }
    }
    false
}

#[cfg(any(test, feature = "mutants"))]
fn init_event_sink() {
    let _ = EVENT_SINK.get_or_init(|| Mutex::new(Vec::new()));
}

#[cfg(any(test, feature = "mutants"))]
fn ipc_loop_count_reset() {
    IPC_LOOP_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(any(test, feature = "mutants"))]
fn ipc_loop_count() -> u64 {
    IPC_LOOP_COUNT.load(Ordering::SeqCst)
}

#[cfg(any(test, feature = "mutants"))]
fn event_snapshot() -> usize {
    init_event_sink();
    EVENT_SINK
        .get()
        .and_then(|sink| sink.lock().ok().map(|events| events.len()))
        .unwrap_or(0)
}

#[cfg(any(test, feature = "mutants"))]
fn events_since(start: usize) -> Vec<IpcEvent> {
    EVENT_SINK
        .get()
        .and_then(|sink| {
            sink.lock()
                .ok()
                .map(|events| events.iter().skip(start).cloned().collect())
        })
        .unwrap_or_default()
}

// ============================================================================
// Stdin Reader Thread
// ============================================================================

fn spawn_stdin_reader(tx: Sender<IpcCommand>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let stdin = io::stdin();
        let stdin_lock = stdin.lock();

        for line in stdin_lock.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<IpcCommand>(trimmed) {
                Ok(cmd) => {
                    if tx.send(cmd).is_err() {
                        break; // Main thread has exited
                    }
                }
                Err(e) => {
                    send_event(&IpcEvent::Error {
                        message: format!("Invalid command: {e}"),
                        recoverable: true,
                    });
                }
            }
        }

        log_debug("Stdin reader thread exiting");
    })
}

// ============================================================================
// Claude Backend
// ============================================================================

fn start_claude_job(claude_cmd: &str, prompt: &str) -> Result<ClaudeJob, String> {
    use std::process::{Command, Stdio};

    log_debug(&format!(
        "Starting Claude job with prompt: {}...",
        &prompt[..prompt.len().min(30)]
    ));

    // Use --print with --dangerously-skip-permissions for non-interactive operation
    // This allows file operations without permission prompts
    // TODO: Add PTY support to show thinking/tool calls in real-time
    let mut child = Command::new(claude_cmd)
        .arg("--print")
        .arg("--dangerously-skip-permissions")
        .arg(prompt)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start claude: {e}"))?;

    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

    let (tx, rx) = mpsc::channel();
    let tx_err = tx.clone();

    // Read stdout
    thread::spawn(move || {
        let reader = io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    // Read stderr
    thread::spawn(move || {
        let reader = io::BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            // Only show non-empty stderr lines
            if !line.trim().is_empty() && tx_err.send(format!("[info] {line}")).is_err() {
                break;
            }
        }
    });

    log_debug("Claude job started");
    Ok(ClaudeJob {
        child,
        stdout_rx: rx,
        started_at: Instant::now(),
    })
}

// ============================================================================
// Auth Backend
// ============================================================================

#[cfg(any(test, feature = "mutants"))]
type AuthFlowHook = Box<dyn Fn(Provider, &str, &str) -> AuthResult + Send + Sync + 'static>;

#[cfg(any(test, feature = "mutants"))]
static AUTH_FLOW_HOOK: OnceLock<Mutex<Option<AuthFlowHook>>> = OnceLock::new();

#[cfg(any(test, feature = "mutants"))]
fn set_auth_flow_hook(hook: Option<AuthFlowHook>) {
    let storage = AUTH_FLOW_HOOK.get_or_init(|| Mutex::new(None));
    *storage.lock().unwrap_or_else(|e| e.into_inner()) = hook;
}

fn run_auth_flow(provider: Provider, codex_cmd: &str, claude_cmd: &str) -> AuthResult {
    #[cfg(any(test, feature = "mutants"))]
    if let Some(storage) = AUTH_FLOW_HOOK.get() {
        if let Ok(guard) = storage.lock() {
            if let Some(hook) = guard.as_ref() {
                return hook(provider, codex_cmd, claude_cmd);
            }
        }
    }
    let command = match provider {
        Provider::Codex => codex_cmd,
        Provider::Claude => claude_cmd,
    };
    run_login_command(command).map_err(|err| format!("{} auth failed: {}", provider.as_str(), err))
}

fn run_login_command(command: &str) -> AuthResult {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::process::{Command, Stdio};

        let tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|err| format!("failed to open /dev/tty: {err}"))?;
        let tty_in = tty
            .try_clone()
            .map_err(|err| format!("failed to clone tty for stdin: {err}"))?;
        let tty_out = tty
            .try_clone()
            .map_err(|err| format!("failed to clone tty for stdout: {err}"))?;
        let tty_err = tty;

        let status = Command::new(command)
            .arg("login")
            .stdin(Stdio::from(tty_in))
            .stdout(Stdio::from(tty_out))
            .stderr(Stdio::from(tty_err))
            .status()
            .map_err(|err| format!("failed to spawn {command} login: {err}"))?;

        if status.success() {
            Ok(())
        } else {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            Err(format!("login exited with code {code}"))
        }
    }

    #[cfg(not(unix))]
    {
        let _ = command;
        Err("TTY auth is only supported on Unix platforms".to_string())
    }
}

// ============================================================================
// Slash Command Parsing
// ============================================================================

#[derive(Debug)]
enum ParsedInput {
    /// Wrapper command (handled by us)
    WrapperCommand(WrapperCmd),
    /// Provider command (forwarded to provider)
    ProviderCommand { command: String, args: String },
    /// Plain prompt
    Prompt(String),
}

#[derive(Debug)]
enum WrapperCmd {
    Provider(String),     // /provider codex|claude
    Codex(String),        // /codex <prompt> - one-off
    Claude(String),       // /claude <prompt> - one-off
    Voice,                // /voice
    Auth(Option<String>), // /auth [provider]
    Status,               // /status
    Capabilities,         // /capabilities
    Help,                 // /help
    Exit,                 // /exit
}

fn parse_input(input: &str) -> ParsedInput {
    let trimmed = input.trim();

    if !trimmed.starts_with('/') {
        return ParsedInput::Prompt(trimmed.to_string());
    }

    let parts: Vec<&str> = trimmed[1..].splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let args = parts.get(1).map(|s| s.trim()).unwrap_or("");

    match cmd.as_str() {
        "provider" => ParsedInput::WrapperCommand(WrapperCmd::Provider(args.to_string())),
        "codex" => ParsedInput::WrapperCommand(WrapperCmd::Codex(args.to_string())),
        "claude" => ParsedInput::WrapperCommand(WrapperCmd::Claude(args.to_string())),
        "voice" | "v" => ParsedInput::WrapperCommand(WrapperCmd::Voice),
        "auth" => ParsedInput::WrapperCommand(WrapperCmd::Auth(if args.is_empty() {
            None
        } else {
            Some(args.to_string())
        })),
        "status" => ParsedInput::WrapperCommand(WrapperCmd::Status),
        "capabilities" => ParsedInput::WrapperCommand(WrapperCmd::Capabilities),
        "help" | "h" => ParsedInput::WrapperCommand(WrapperCmd::Help),
        "exit" | "quit" | "q" => ParsedInput::WrapperCommand(WrapperCmd::Exit),
        // All other / commands are forwarded to provider
        _ => ParsedInput::ProviderCommand {
            command: cmd,
            args: args.to_string(),
        },
    }
}

// ============================================================================
// Main Event Loop
// ============================================================================

pub fn run_ipc_mode(config: AppConfig) -> Result<()> {
    log_debug("Starting JSON IPC mode (non-blocking)");

    let mut state = IpcState::new(config);

    // Emit capabilities on startup
    state.emit_capabilities();

    // Start stdin reader thread
    let (cmd_tx, cmd_rx) = mpsc::channel();
    #[cfg(any(test, feature = "mutants"))]
    {
        drop(cmd_tx);
        return run_ipc_loop(&mut state, &cmd_rx, Some(10));
    }
    #[cfg(not(any(test, feature = "mutants")))]
    let _stdin_handle = spawn_stdin_reader(cmd_tx);

    #[cfg(not(any(test, feature = "mutants")))]
    return run_ipc_loop(&mut state, &cmd_rx, None);
}

#[cfg(any(test, feature = "mutants"))]
fn ipc_guard_tripped(elapsed: Duration) -> bool {
    elapsed > Duration::from_secs(2)
}

fn run_ipc_loop(
    state: &mut IpcState,
    cmd_rx: &Receiver<IpcCommand>,
    max_loops: Option<u64>,
) -> Result<()> {
    #[cfg(any(test, feature = "mutants"))]
    let guard_start = Instant::now();
    let mut loop_count: u64 = 0;
    loop {
        #[cfg(any(test, feature = "mutants"))]
        if ipc_guard_tripped(guard_start.elapsed()) {
            panic!("IPC loop guard exceeded");
        }
        loop_count += 1;
        #[cfg(any(test, feature = "mutants"))]
        IPC_LOOP_COUNT.store(loop_count, Ordering::SeqCst);
        if loop_count.is_multiple_of(1000) {
            log_debug(&format!(
                "IPC loop iteration {}, job active: {}",
                loop_count,
                state.current_job.is_some()
            ));
        }

        if let Some(limit) = max_loops {
            if loop_count >= limit {
                log_debug("IPC loop reached test limit, exiting");
                break;
            }
        }

        // Check for new commands (non-blocking)
        match cmd_rx.try_recv() {
            Ok(cmd) => {
                log_debug(&format!("IPC command received: {cmd:?}"));
                state.cancelled = false;

                match cmd {
                    IpcCommand::SendPrompt { prompt, provider } => {
                        handle_send_prompt(state, &prompt, provider);
                    }
                    IpcCommand::StartVoice => {
                        handle_start_voice(state);
                    }
                    IpcCommand::Cancel => {
                        handle_cancel(state);
                    }
                    IpcCommand::SetProvider { provider } => {
                        handle_set_provider(state, &provider);
                    }
                    IpcCommand::Auth { provider } => {
                        handle_auth_command(state, provider);
                    }
                    IpcCommand::GetCapabilities => {
                        state.emit_capabilities();
                    }
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                log_debug("Command channel disconnected, exiting");
                break;
            }
        }

        // Process active job events
        if let Some(job) = &mut state.current_job {
            match job {
                ActiveJob::Codex(codex_job) => {
                    if process_codex_events(codex_job, state.cancelled) {
                        state.current_job = None;
                    }
                }
                ActiveJob::Claude(claude_job) => {
                    if process_claude_events(claude_job, state.cancelled) {
                        state.current_job = None;
                    }
                }
            }
        }

        // Process voice job events
        if let Some(voice_job) = &state.current_voice_job {
            if process_voice_events(voice_job, state.cancelled) {
                state.current_voice_job = None;
            }
        }

        if process_auth_events(state) {
            state.current_auth_job = None;
        }

        // Small sleep to prevent busy-waiting
        thread::sleep(Duration::from_millis(5));
    }

    log_debug("IPC mode exiting");
    Ok(())
}

// ============================================================================
// Command Handlers
// ============================================================================

fn handle_send_prompt(state: &mut IpcState, prompt: &str, provider_override: Option<String>) {
    if state.current_auth_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Authentication in progress. Finish /auth before sending prompts.".to_string(),
            recoverable: true,
        });
        return;
    }

    // Cancel any existing job
    if let Some(job) = state.current_job.take() {
        match job {
            ActiveJob::Codex(j) => state.codex_backend.cancel(j.id),
            ActiveJob::Claude(mut j) => {
                let _ = j.child.kill();
            }
        }
    }

    // Determine which provider to use
    let provider = provider_override
        .as_ref()
        .and_then(|s| Provider::from_str(s))
        .unwrap_or(state.active_provider);

    // Parse input for slash commands
    let parsed = parse_input(prompt);

    match parsed {
        ParsedInput::WrapperCommand(cmd) => {
            handle_wrapper_command(state, cmd);
        }
        ParsedInput::ProviderCommand { command, args } => {
            // Forward to provider only if Codex is active
            if provider == Provider::Codex {
                let full_prompt = if args.is_empty() {
                    format!("/{command}")
                } else {
                    format!("/{command} {args}")
                };
                start_provider_job(state, provider, &full_prompt);
            } else {
                send_event(&IpcEvent::ProviderError {
                    message: format!(
                        "/{command} is a Codex command. Switch with /provider codex or use /codex /{command} {args}"
                    ),
                });
            }
        }
        ParsedInput::Prompt(p) => {
            start_provider_job(state, provider, &p);
        }
    }
}

fn handle_wrapper_command(state: &mut IpcState, cmd: WrapperCmd) {
    match cmd {
        WrapperCmd::Provider(p) => {
            handle_set_provider(state, &p);
        }
        WrapperCmd::Codex(prompt) => {
            if !prompt.is_empty() {
                start_provider_job(state, Provider::Codex, &prompt);
            } else {
                send_event(&IpcEvent::Error {
                    message: "Usage: /codex <prompt>".to_string(),
                    recoverable: true,
                });
            }
        }
        WrapperCmd::Claude(prompt) => {
            if !prompt.is_empty() {
                start_provider_job(state, Provider::Claude, &prompt);
            } else {
                send_event(&IpcEvent::Error {
                    message: "Usage: /claude <prompt>".to_string(),
                    recoverable: true,
                });
            }
        }
        WrapperCmd::Voice => {
            handle_start_voice(state);
        }
        WrapperCmd::Auth(provider) => {
            handle_auth_command(state, provider);
        }
        WrapperCmd::Status => {
            state.emit_capabilities();
        }
        WrapperCmd::Capabilities => {
            state.emit_capabilities();
        }
        WrapperCmd::Help => {
            send_event(&IpcEvent::Status {
                message: "Commands: /provider, /codex, /claude, /auth, /voice, /status, /help, /exit. All other / commands forwarded to Codex.".to_string(),
            });
        }
        WrapperCmd::Exit => {
            std::process::exit(0);
        }
    }
}

fn start_provider_job(state: &mut IpcState, provider: Provider, prompt: &str) {
    send_event(&IpcEvent::JobStart {
        provider: provider.as_str().to_string(),
    });

    match provider {
        Provider::Codex => {
            let request = CodexRequest::chat(prompt.to_string());
            match state.codex_backend.start(request) {
                Ok(job) => {
                    state.current_job = Some(ActiveJob::Codex(job));
                }
                Err(e) => {
                    let msg = match e {
                        BackendError::InvalidRequest(s) => s.to_string(),
                        BackendError::BackendDisabled(s) => s,
                    };
                    send_event(&IpcEvent::JobEnd {
                        provider: "codex".to_string(),
                        success: false,
                        error: Some(msg),
                    });
                }
            }
        }
        Provider::Claude => match start_claude_job(&state.claude_cmd, prompt) {
            Ok(job) => {
                state.current_job = Some(ActiveJob::Claude(job));
            }
            Err(e) => {
                send_event(&IpcEvent::JobEnd {
                    provider: "claude".to_string(),
                    success: false,
                    error: Some(e),
                });
            }
        },
    }
}

fn handle_start_voice(state: &mut IpcState) {
    if state.current_auth_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Authentication in progress. Finish /auth before starting voice.".to_string(),
            recoverable: true,
        });
        return;
    }

    if state.current_voice_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Voice capture already in progress".to_string(),
            recoverable: true,
        });
        return;
    }

    if state.recorder.is_none() && state.config.no_python_fallback {
        send_event(&IpcEvent::Error {
            message: "No microphone available and Python fallback disabled".to_string(),
            recoverable: true,
        });
        return;
    }

    send_event(&IpcEvent::VoiceStart);

    let job = voice::start_voice_job(
        state.recorder.clone(),
        state.transcriber.clone(),
        state.config.clone(),
    );
    state.current_voice_job = Some(job);
}

fn handle_cancel(state: &mut IpcState) {
    state.cancelled = true;

    if state.current_auth_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Authentication in progress. Cancel from the provider prompt.".to_string(),
            recoverable: true,
        });
        return;
    }

    if let Some(job) = state.current_job.take() {
        match job {
            ActiveJob::Codex(j) => {
                state.codex_backend.cancel(j.id);
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
            }
            ActiveJob::Claude(mut j) => {
                let _ = j.child.kill();
                send_event(&IpcEvent::JobEnd {
                    provider: "claude".to_string(),
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
            }
        }
    }

    if state.current_voice_job.is_some() {
        send_event(&IpcEvent::VoiceEnd {
            error: Some("Cancelled".to_string()),
        });
        state.current_voice_job = None;
    }
}

fn handle_set_provider(state: &mut IpcState, provider_str: &str) {
    match Provider::from_str(provider_str) {
        Some(provider) => {
            state.active_provider = provider;
            send_event(&IpcEvent::ProviderChanged {
                provider: provider.as_str().to_string(),
            });
        }
        None => {
            send_event(&IpcEvent::Error {
                message: format!("Unknown provider: {provider_str}. Use 'codex' or 'claude'."),
                recoverable: true,
            });
        }
    }
}

fn handle_auth_command(state: &mut IpcState, provider_override: Option<String>) {
    if state.current_auth_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Authentication already in progress".to_string(),
            recoverable: true,
        });
        return;
    }

    if state.current_job.is_some() || state.current_voice_job.is_some() {
        send_event(&IpcEvent::Error {
            message: "Finish active work before running /auth".to_string(),
            recoverable: true,
        });
        return;
    }

    let provider = match provider_override {
        Some(ref name) => match Provider::from_str(name) {
            Some(parsed) => parsed,
            None => {
                send_event(&IpcEvent::Error {
                    message: format!("Unknown provider: {name}. Use 'codex' or 'claude'."),
                    recoverable: true,
                });
                return;
            }
        },
        None => state.active_provider,
    };

    send_event(&IpcEvent::AuthStart {
        provider: provider.as_str().to_string(),
    });

    let codex_cmd = state.config.codex_cmd.clone();
    let claude_cmd = state.claude_cmd.clone();
    let (auth_result_tx, auth_result_rx) = mpsc::channel();

    thread::spawn(move || {
        let result = run_auth_flow(provider, &codex_cmd, &claude_cmd);
        let _ = auth_result_tx.send(result);
    });

    state.current_auth_job = Some(AuthJob {
        provider,
        receiver: auth_result_rx,
        started_at: Instant::now(),
    });
}

// ============================================================================
// Event Processing (Non-blocking, returns true when job complete)
// ============================================================================

fn process_codex_events(job: &mut BackendJob, cancelled: bool) -> bool {
    if cancelled {
        return true;
    }

    let handle_event = |event: BackendEvent| -> bool {
        match event.kind {
            BackendEventKind::Token { text } => {
                send_event(&IpcEvent::Token { text });
                false
            }
            BackendEventKind::Status { message } => {
                send_event(&IpcEvent::Status { message });
                false
            }
            BackendEventKind::Started { .. } => {
                send_event(&IpcEvent::Status {
                    message: "Processing...".to_string(),
                });
                false
            }
            BackendEventKind::Finished { lines, .. } => {
                for line in lines {
                    send_event(&IpcEvent::Token {
                        text: format!("{line}\n"),
                    });
                }
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: true,
                    error: None,
                });
                true
            }
            BackendEventKind::FatalError { message, .. } => {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some(message),
                });
                true
            }
            BackendEventKind::RecoverableError { message, .. } => {
                send_event(&IpcEvent::Status {
                    message: format!("Retrying: {message}"),
                });
                false
            }
            BackendEventKind::Canceled { .. } => {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
                true
            }
        }
    };

    // Check for new events via signal channel
    match job.try_recv_signal() {
        Ok(()) => {
            // Signal received, drain events
            for event in job.drain_events() {
                if handle_event(event) {
                    return true;
                }
            }
            false
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            // Worker finished, drain any remaining events
            let mut completed = false;
            for event in job.drain_events() {
                if handle_event(event) {
                    completed = true;
                    break;
                }
            }
            if !completed {
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: true,
                    error: None,
                });
            }
            true
        }
    }
}

fn process_claude_events(job: &mut ClaudeJob, cancelled: bool) -> bool {
    if cancelled {
        log_debug("Claude job: cancelled");
        let _ = job.child.kill();
        return true;
    }

    // Check for stdout output
    match job.stdout_rx.try_recv() {
        Ok(line) => {
            log_debug(&format!(
                "Claude job: got line: {}",
                &line[..line.len().min(50)]
            ));
            send_event(&IpcEvent::Token {
                text: format!("{line}\n"),
            });
            false
        }
        Err(TryRecvError::Empty) => {
            // Check if process has exited
            match job.child.try_wait() {
                Ok(Some(status)) => {
                    log_debug(&format!(
                        "Claude job: process exited with status {status:?}"
                    ));
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: status.success(),
                        error: if status.success() {
                            None
                        } else {
                            Some(format!("Exit code: {:?}", status.code()))
                        },
                    });
                    true
                }
                Ok(None) => false, // Still running
                Err(e) => {
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: false,
                        error: Some(format!("Process error: {e}")),
                    });
                    true
                }
            }
        }
        Err(TryRecvError::Disconnected) => {
            log_debug("Claude job: stdout disconnected");
            // stdout closed, check if process has exited (non-blocking)
            match job.child.try_wait() {
                Ok(Some(status)) => {
                    log_debug(&format!(
                        "Claude job: process already exited with {status:?}"
                    ));
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: status.success(),
                        error: None,
                    });
                    true
                }
                Ok(None) => {
                    // Process still running, kill it
                    log_debug("Claude job: process still running, killing it");
                    let _ = job.child.kill();
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: true,
                        error: None,
                    });
                    true
                }
                Err(e) => {
                    send_event(&IpcEvent::JobEnd {
                        provider: "claude".to_string(),
                        success: false,
                        error: Some(format!("Wait error: {e}")),
                    });
                    true
                }
            }
        }
    }
}

fn process_voice_events(job: &VoiceJob, cancelled: bool) -> bool {
    if cancelled {
        return true;
    }

    match job.receiver.try_recv() {
        Ok(msg) => {
            match msg {
                VoiceJobMessage::Transcript { text, source } => {
                    send_event(&IpcEvent::VoiceEnd { error: None });
                    send_event(&IpcEvent::Transcript {
                        text,
                        duration_ms: 0, // TODO: track actual duration
                    });
                    log_debug(&format!("Voice transcript via {}", source.label()));
                }
                VoiceJobMessage::Empty { source } => {
                    send_event(&IpcEvent::VoiceEnd {
                        error: Some("No speech detected".to_string()),
                    });
                    log_debug(&format!("Voice empty via {}", source.label()));
                }
                VoiceJobMessage::Error(message) => {
                    send_event(&IpcEvent::VoiceEnd {
                        error: Some(message),
                    });
                }
            }
            true
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            send_event(&IpcEvent::VoiceEnd {
                error: Some("Voice worker disconnected".to_string()),
            });
            true
        }
    }
}

fn process_auth_events(state: &mut IpcState) -> bool {
    let job = match state.current_auth_job.as_mut() {
        Some(job) => job,
        None => return false,
    };

    match job.receiver.try_recv() {
        Ok(result) => {
            let provider = job.provider;
            let (success, error) = match result {
                Ok(()) => (true, None),
                Err(err) => (false, Some(err)),
            };

            if success && provider == Provider::Codex {
                state.codex_backend.reset_session();
            }

            send_event(&IpcEvent::AuthEnd {
                provider: provider.as_str().to_string(),
                success,
                error,
            });
            state.emit_capabilities();
            true
        }
        Err(TryRecvError::Empty) => false,
        Err(TryRecvError::Disconnected) => {
            let provider = job.provider;
            send_event(&IpcEvent::AuthEnd {
                provider: provider.as_str().to_string(),
                success: false,
                error: Some("Auth worker disconnected".to_string()),
            });
            state.emit_capabilities();
            true
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use crate::codex::{
        build_test_backend_job, reset_session_count, reset_session_count_reset, BackendEvent,
        BackendEventKind, BackendStats, RequestMode, TestSignal,
    };
    use crate::{PipelineJsonResult, PipelineMetrics};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    fn new_test_state(mut config: AppConfig) -> IpcState {
        config.persistent_codex = false;
        IpcState {
            config: config.clone(),
            active_provider: Provider::Codex,
            codex_backend: Arc::new(CliBackend::new(config)),
            claude_cmd: "claude".to_string(),
            recorder: None,
            transcriber: None,
            current_job: None,
            current_voice_job: None,
            current_auth_job: None,
            session_id: "test-session".to_string(),
            cancelled: false,
        }
    }

    type PythonHook = Box<
        dyn Fn(&AppConfig, Option<Arc<AtomicBool>>) -> anyhow::Result<crate::PipelineJsonResult>
            + Send
            + 'static,
    >;

    struct AuthHookGuard;

    impl Drop for AuthHookGuard {
        fn drop(&mut self) {
            set_auth_flow_hook(None);
        }
    }

    fn set_auth_hook(hook: AuthFlowHook) -> AuthHookGuard {
        set_auth_flow_hook(Some(hook));
        AuthHookGuard
    }

    struct PythonHookGuard;

    impl Drop for PythonHookGuard {
        fn drop(&mut self) {
            voice::set_python_transcription_hook(None);
        }
    }

    fn set_python_hook(hook: PythonHook) -> PythonHookGuard {
        voice::set_python_transcription_hook(Some(hook));
        PythonHookGuard
    }

    #[cfg(unix)]
    fn write_stub_script(contents: &str) -> std::path::PathBuf {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("ipc_stub_{nanos}.sh"));
        fs::write(&path, contents).expect("write stub");
        let mut perms = fs::metadata(&path).expect("stat stub").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).expect("chmod stub");
        path
    }

    // -------------------------------------------------------------------------
    // Provider Enum Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_provider_from_str() {
        assert_eq!(Provider::from_str("codex"), Some(Provider::Codex));
        assert_eq!(Provider::from_str("CODEX"), Some(Provider::Codex));
        assert_eq!(Provider::from_str("Codex"), Some(Provider::Codex));

        assert_eq!(Provider::from_str("claude"), Some(Provider::Claude));
        assert_eq!(Provider::from_str("CLAUDE"), Some(Provider::Claude));
        assert_eq!(Provider::from_str("Claude"), Some(Provider::Claude));

        assert_eq!(Provider::from_str("unknown"), None);
        assert_eq!(Provider::from_str(""), None);
        assert_eq!(Provider::from_str("openai"), None);
    }

    #[test]
    fn test_provider_as_str() {
        assert_eq!(Provider::Codex.as_str(), "codex");
        assert_eq!(Provider::Claude.as_str(), "claude");
    }

    // -------------------------------------------------------------------------
    // Input Parsing Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_plain_prompt() {
        match parse_input("hello world") {
            ParsedInput::Prompt(p) => assert_eq!(p, "hello world"),
            _ => panic!("Expected Prompt"),
        }

        match parse_input("  hello world  ") {
            ParsedInput::Prompt(p) => assert_eq!(p, "hello world"),
            _ => panic!("Expected Prompt with trimmed content"),
        }
    }

    #[test]
    fn test_parse_wrapper_commands() {
        // /provider
        match parse_input("/provider codex") {
            ParsedInput::WrapperCommand(WrapperCmd::Provider(p)) => assert_eq!(p, "codex"),
            _ => panic!("Expected Provider command"),
        }

        // /codex
        match parse_input("/codex hello world") {
            ParsedInput::WrapperCommand(WrapperCmd::Codex(p)) => assert_eq!(p, "hello world"),
            _ => panic!("Expected Codex command"),
        }

        // /claude
        match parse_input("/claude hello world") {
            ParsedInput::WrapperCommand(WrapperCmd::Claude(p)) => assert_eq!(p, "hello world"),
            _ => panic!("Expected Claude command"),
        }

        // /voice
        match parse_input("/voice") {
            ParsedInput::WrapperCommand(WrapperCmd::Voice) => {}
            _ => panic!("Expected Voice command"),
        }

        // /auth (default provider)
        match parse_input("/auth") {
            ParsedInput::WrapperCommand(WrapperCmd::Auth(None)) => {}
            _ => panic!("Expected Auth command with default provider"),
        }

        // /auth codex
        match parse_input("/auth codex") {
            ParsedInput::WrapperCommand(WrapperCmd::Auth(Some(provider))) => {
                assert_eq!(provider, "codex");
            }
            _ => panic!("Expected Auth command with provider"),
        }

        // /v (alias)
        match parse_input("/v") {
            ParsedInput::WrapperCommand(WrapperCmd::Voice) => {}
            _ => panic!("Expected Voice command from alias"),
        }

        // /status
        match parse_input("/status") {
            ParsedInput::WrapperCommand(WrapperCmd::Status) => {}
            _ => panic!("Expected Status command"),
        }

        // /help
        match parse_input("/help") {
            ParsedInput::WrapperCommand(WrapperCmd::Help) => {}
            _ => panic!("Expected Help command"),
        }

        // /h (alias)
        match parse_input("/h") {
            ParsedInput::WrapperCommand(WrapperCmd::Help) => {}
            _ => panic!("Expected Help command from alias"),
        }

        // /exit
        match parse_input("/exit") {
            ParsedInput::WrapperCommand(WrapperCmd::Exit) => {}
            _ => panic!("Expected Exit command"),
        }

        // /quit (alias)
        match parse_input("/quit") {
            ParsedInput::WrapperCommand(WrapperCmd::Exit) => {}
            _ => panic!("Expected Exit command from quit alias"),
        }

        // /q (alias)
        match parse_input("/q") {
            ParsedInput::WrapperCommand(WrapperCmd::Exit) => {}
            _ => panic!("Expected Exit command from q alias"),
        }
    }

    #[test]
    fn test_parse_provider_commands() {
        // Provider-specific commands should be forwarded to Codex
        match parse_input("/model gpt-4") {
            ParsedInput::ProviderCommand { command, args } => {
                assert_eq!(command, "model");
                assert_eq!(args, "gpt-4");
            }
            _ => panic!("Expected ProviderCommand"),
        }

        match parse_input("/context") {
            ParsedInput::ProviderCommand { command, args } => {
                assert_eq!(command, "context");
                assert_eq!(args, "");
            }
            _ => panic!("Expected ProviderCommand with no args"),
        }

        match parse_input("/run bash -c 'echo hello'") {
            ParsedInput::ProviderCommand { command, args } => {
                assert_eq!(command, "run");
                assert_eq!(args, "bash -c 'echo hello'");
            }
            _ => panic!("Expected ProviderCommand with complex args"),
        }
    }

    #[test]
    fn test_parse_case_insensitive() {
        // Commands should be case-insensitive
        match parse_input("/PROVIDER codex") {
            ParsedInput::WrapperCommand(WrapperCmd::Provider(_)) => {}
            _ => panic!("Expected Provider command (uppercase)"),
        }

        match parse_input("/Provider codex") {
            ParsedInput::WrapperCommand(WrapperCmd::Provider(_)) => {}
            _ => panic!("Expected Provider command (mixed case)"),
        }

        match parse_input("/CODEX hello") {
            ParsedInput::WrapperCommand(WrapperCmd::Codex(_)) => {}
            _ => panic!("Expected Codex command (uppercase)"),
        }
    }

    #[test]
    fn test_parse_capabilities_command() {
        match parse_input("/capabilities") {
            ParsedInput::WrapperCommand(WrapperCmd::Capabilities) => {}
            _ => panic!("Expected Capabilities command"),
        }
    }

    #[test]
    fn emit_capabilities_reports_state() {
        let snapshot = event_snapshot();
        let mut config = AppConfig::parse_from(["test-app", "--no-python-fallback"]);
        config.whisper_model_path = None;
        let state = new_test_state(config);

        state.emit_capabilities();

        let events = events_since(snapshot);
        let caps = events.iter().find_map(|event| match event {
            IpcEvent::Capabilities {
                mic_available,
                whisper_model_loaded,
                python_fallback_allowed,
                active_provider,
                ..
            } => Some((
                *mic_available,
                *whisper_model_loaded,
                *python_fallback_allowed,
                active_provider.clone(),
            )),
            _ => None,
        });
        assert!(caps.is_some());
        let (mic_available, whisper_loaded, python_fallback_allowed, active_provider) = caps.unwrap();
        assert!(!mic_available);
        assert!(!whisper_loaded);
        assert!(!python_fallback_allowed);
        assert_eq!(active_provider, "codex");
    }

    #[test]
    fn handle_set_provider_emits_events() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));

        handle_set_provider(&mut state, "claude");
        assert_eq!(state.active_provider, Provider::Claude);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::ProviderChanged { provider } if provider == "claude")
        }));

        let snapshot = event_snapshot();
        handle_set_provider(&mut state, "unknown");
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Unknown provider"))
        }));
    }

    #[test]
    fn handle_send_prompt_blocks_during_auth() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });

        handle_send_prompt(&mut state, "hello", None);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Authentication in progress"))
        }));
    }

    #[test]
    fn handle_send_prompt_rejects_provider_commands_on_claude() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        state.active_provider = Provider::Claude;

        handle_send_prompt(&mut state, "/model gpt-4", None);

        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::ProviderError { message } if message.contains("Codex command"))
        }));
    }

    #[test]
    fn handle_wrapper_help_emits_status() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        handle_wrapper_command(&mut state, WrapperCmd::Help);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Status { message } if message.contains("Commands:"))
        }));
    }

    #[test]
    fn handle_wrapper_status_emits_capabilities() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        handle_wrapper_command(&mut state, WrapperCmd::Status);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Capabilities { .. })));
    }

    #[test]
    fn handle_wrapper_capabilities_emits_capabilities() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        handle_wrapper_command(&mut state, WrapperCmd::Capabilities);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Capabilities { .. })));
    }

    #[test]
    fn handle_wrapper_requires_prompt_for_codex_and_claude() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        handle_wrapper_command(&mut state, WrapperCmd::Codex(String::new()));
        handle_wrapper_command(&mut state, WrapperCmd::Claude(String::new()));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Usage: /codex"))
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Usage: /claude"))
        }));
    }

    #[test]
    fn run_ipc_mode_emits_capabilities_on_start() {
        let snapshot = event_snapshot();
        let config = AppConfig::parse_from(["test-app"]);
        run_ipc_mode(config).unwrap();
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Capabilities { .. })));
    }

    #[test]
    fn run_ipc_loop_processes_commands() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        tx.send(IpcCommand::GetCapabilities).unwrap();
        tx.send(IpcCommand::SetProvider {
            provider: "claude".to_string(),
        })
        .unwrap();
        drop(tx);
        run_ipc_loop(&mut state, &rx, Some(10)).unwrap();
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Capabilities { .. })));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::ProviderChanged { provider } if provider == "claude")
        }));
    }

    #[test]
    fn run_ipc_loop_respects_max_loops_with_live_channel() {
        ipc_loop_count_reset();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        let start = Instant::now();
        run_ipc_loop(&mut state, &rx, Some(3)).unwrap();
        assert_eq!(ipc_loop_count(), 3);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn ipc_guard_trips_only_after_threshold() {
        assert!(!ipc_guard_tripped(Duration::from_secs(1)));
        assert!(!ipc_guard_tripped(Duration::from_secs(2)));
        assert!(ipc_guard_tripped(Duration::from_secs(2) + Duration::from_millis(1)));
    }

    #[test]
    fn run_ipc_loop_breaks_when_limit_zero() {
        ipc_loop_count_reset();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        run_ipc_loop(&mut state, &rx, Some(0)).unwrap();
        assert_eq!(ipc_loop_count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn start_provider_job_codex_emits_completion() {
        let snapshot = event_snapshot();
        let mut config = AppConfig::parse_from(["test-app"]);
        config.codex_cmd = "/path/does/not/exist".to_string();
        let mut state = new_test_state(config);

        start_provider_job(&mut state, Provider::Codex, "hello");

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if let Some(ActiveJob::Codex(job)) = &mut state.current_job {
                if process_codex_events(job, false) {
                    state.current_job = None;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(state.current_job.is_none(), "codex job did not complete");
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::JobStart { provider } if provider == "codex")
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::JobEnd { provider, success, .. } if provider == "codex" && !*success)
        }));
    }

    #[cfg(unix)]
    #[test]
    fn process_claude_events_emits_tokens_and_end() {
        let snapshot = event_snapshot();
        let (tx, rx) = mpsc::channel();
        tx.send("hello from claude".to_string()).unwrap();
        let child = std::process::Command::new("true")
            .spawn()
            .expect("spawned child");
        let mut job = ClaudeJob {
            child,
            stdout_rx: rx,
            started_at: Instant::now(),
        };

        assert!(!process_claude_events(&mut job, false));
        let start = Instant::now();
        let mut finished = false;
        while start.elapsed() < Duration::from_secs(1) {
            if process_claude_events(&mut job, false) {
                finished = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(finished);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Token { .. })));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::JobEnd { provider, .. } if provider == "claude")
        }));
    }

    #[test]
    fn process_voice_events_handles_transcript() {
        let snapshot = event_snapshot();
        let (tx, rx) = mpsc::channel();
        let job = VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        };
        tx.send(VoiceJobMessage::Transcript {
            text: "hello".to_string(),
            source: voice::VoiceCaptureSource::Native,
        })
        .unwrap();

        assert!(process_voice_events(&job, false));

        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.is_none())
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Transcript { text, .. } if text == "hello")
        }));
    }

    #[test]
    fn process_voice_events_handles_empty() {
        let snapshot = event_snapshot();
        let (tx, rx) = mpsc::channel();
        let job = VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        };
        tx.send(VoiceJobMessage::Empty {
            source: voice::VoiceCaptureSource::Native,
        })
        .unwrap();

        assert!(process_voice_events(&job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.as_deref() == Some("No speech detected"))
        }));
    }

    #[test]
    fn process_voice_events_handles_error() {
        let snapshot = event_snapshot();
        let (tx, rx) = mpsc::channel();
        let job = VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        };
        tx.send(VoiceJobMessage::Error("boom".to_string()))
            .unwrap();

        assert!(process_voice_events(&job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.as_deref() == Some("boom"))
        }));
    }

    #[test]
    fn process_voice_events_handles_disconnect() {
        let snapshot = event_snapshot();
        let (tx, rx) = mpsc::channel();
        drop(tx);
        let job = VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        };

        assert!(process_voice_events(&job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.as_deref() == Some("Voice worker disconnected"))
        }));
    }

    #[test]
    fn process_auth_events_emits_success_and_capabilities() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });
        tx.send(Ok(())).unwrap();

        assert!(process_auth_events(&mut state));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::AuthEnd { success: true, .. })));
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Capabilities { .. })));
    }

    #[test]
    fn ipc_loop_count_reset_clears_count() {
        IPC_LOOP_COUNT.store(5, Ordering::SeqCst);
        assert_eq!(ipc_loop_count(), 5);
        ipc_loop_count_reset();
        assert_eq!(ipc_loop_count(), 0);
    }

    #[test]
    fn set_auth_flow_hook_overrides_auth_flow() {
        struct HookReset;
        impl Drop for HookReset {
            fn drop(&mut self) {
                set_auth_flow_hook(None);
            }
        }

        let calls = Arc::new(AtomicUsize::new(0));
        let calls_clone = Arc::clone(&calls);
        set_auth_flow_hook(Some(Box::new(move |provider, codex_cmd, claude_cmd| {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            assert_eq!(provider, Provider::Codex);
            assert_eq!(codex_cmd, "codex-bin");
            assert_eq!(claude_cmd, "claude-bin");
            Ok(())
        })));
        let _reset = HookReset;

        let result = run_auth_flow(Provider::Codex, "codex-bin", "claude-bin");
        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn process_auth_events_resets_session_for_successful_codex() {
        reset_session_count_reset();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });
        tx.send(Ok(())).unwrap();

        assert!(process_auth_events(&mut state));
        assert_eq!(reset_session_count(), 1);
    }

    #[test]
    fn process_auth_events_does_not_reset_on_failed_codex() {
        reset_session_count_reset();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });
        tx.send(Err("nope".to_string())).unwrap();

        assert!(process_auth_events(&mut state));
        assert_eq!(reset_session_count(), 0);
    }

    #[test]
    fn process_auth_events_does_not_reset_on_successful_claude() {
        reset_session_count_reset();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Claude,
            receiver: rx,
            started_at: Instant::now(),
        });
        tx.send(Ok(())).unwrap();

        assert!(process_auth_events(&mut state));
        assert_eq!(reset_session_count(), 0);
    }

    #[test]
    fn process_auth_events_emits_error() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Claude,
            receiver: rx,
            started_at: Instant::now(),
        });
        tx.send(Err("nope".to_string())).unwrap();

        assert!(process_auth_events(&mut state));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::AuthEnd { success: false, error, .. } if error.as_deref() == Some("nope"))
        }));
    }

    #[test]
    fn process_auth_events_handles_disconnect() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (tx, rx) = mpsc::channel();
        drop(tx);
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });

        assert!(process_auth_events(&mut state));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::AuthEnd { success: false, error, .. } if error.as_deref() == Some("Auth worker disconnected"))
        }));
    }

    #[test]
    fn handle_cancel_clears_voice_job() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        state.current_voice_job = Some(VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        handle_cancel(&mut state);

        assert!(state.current_voice_job.is_none());
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.as_deref() == Some("Cancelled"))
        }));
    }

    #[test]
    fn handle_start_voice_errors_when_auth_in_progress() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        state.current_auth_job = Some(AuthJob {
            provider: Provider::Codex,
            receiver: rx,
            started_at: Instant::now(),
        });

        handle_start_voice(&mut state);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Authentication in progress"))
        }));
    }

    #[test]
    fn handle_start_voice_errors_when_already_running() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        state.current_voice_job = Some(VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        handle_start_voice(&mut state);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Voice capture already in progress"))
        }));
    }

    #[test]
    fn handle_start_voice_errors_when_no_mic_and_no_python() {
        let snapshot = event_snapshot();
        let config = AppConfig::parse_from(["test-app", "--no-python-fallback"]);
        let mut state = new_test_state(config);
        state.recorder = None;

        handle_start_voice(&mut state);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("No microphone available"))
        }));
    }

    #[test]
    fn handle_start_voice_starts_python_fallback_job() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let _hook = set_python_hook(Box::new(|_cfg, _stop| {
            Ok(PipelineJsonResult {
                transcript: "hello voice".to_string(),
                prompt: String::new(),
                codex_output: None,
                metrics: PipelineMetrics::default(),
            })
        }));

        handle_start_voice(&mut state);

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(1) {
            if let Some(job) = &state.current_voice_job {
                if process_voice_events(job, false) {
                    state.current_voice_job = None;
                    break;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }

        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::VoiceStart)));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Transcript { text, .. } if text == "hello voice")
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::VoiceEnd { error } if error.is_none())
        }));
    }

    #[test]
    fn handle_auth_command_rejects_unknown_provider() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        handle_auth_command(&mut state, Some("unknown".to_string()));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Unknown provider"))
        }));
    }

    #[test]
    fn handle_auth_command_rejects_when_active() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let (_tx, rx) = mpsc::channel();
        state.current_voice_job = Some(VoiceJob {
            receiver: rx,
            handle: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        });

        handle_auth_command(&mut state, None);
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Error { message, .. } if message.contains("Finish active work"))
        }));
    }

    #[test]
    fn handle_auth_command_starts_job_and_completes() {
        let snapshot = event_snapshot();
        let mut state = new_test_state(AppConfig::parse_from(["test-app"]));
        let _guard = set_auth_hook(Box::new(|_provider, _codex, _claude| Ok(())));

        handle_auth_command(&mut state, None);
        assert!(state.current_auth_job.is_some());

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(1) {
            if process_auth_events(&mut state) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::AuthStart { .. })));
        assert!(events.iter().any(|event| matches!(event, IpcEvent::AuthEnd { success: true, .. })));
    }

    #[cfg(unix)]
    #[test]
    fn start_claude_job_emits_stdout_and_stderr() {
        use std::fs;

        let snapshot = event_snapshot();
        let script = write_stub_script("#!/bin/sh\necho out-line\necho '' 1>&2\necho err-line 1>&2\n");
        let mut job = start_claude_job(script.to_str().unwrap(), "prompt").unwrap();

        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            let _ = process_claude_events(&mut job, false);
            thread::sleep(Duration::from_millis(10));
        }

        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Token { text } if text.contains("out-line"))
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Token { text } if text.contains("[info] err-line"))
        }));

        let _ = fs::remove_file(script);
    }

    #[cfg(unix)]
    #[test]
    fn process_claude_events_handles_cancel() {
        let snapshot = event_snapshot();
        let child = std::process::Command::new("sleep")
            .arg("1")
            .spawn()
            .expect("spawned child");
        let (_tx, rx) = mpsc::channel();
        let mut job = ClaudeJob {
            child,
            stdout_rx: rx,
            started_at: Instant::now(),
        };

        assert!(process_claude_events(&mut job, true));
        let _ = events_since(snapshot);
    }

    #[test]
    fn process_codex_events_emits_tokens_and_status() {
        let snapshot = event_snapshot();
        let job_id = 42;
        let events = vec![
            BackendEvent {
                job_id,
                kind: BackendEventKind::Started {
                    mode: RequestMode::Chat,
                },
            },
            BackendEvent {
                job_id,
                kind: BackendEventKind::Status {
                    message: "hello".to_string(),
                },
            },
            BackendEvent {
                job_id,
                kind: BackendEventKind::Token {
                    text: "token".to_string(),
                },
            },
        ];
        let mut job = build_test_backend_job(events, TestSignal::Ready);

        assert!(!process_codex_events(&mut job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Status { message } if message == "Processing...")));
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Status { message } if message == "hello")));
        assert!(events.iter().any(|event| matches!(event, IpcEvent::Token { text } if text == "token")));
    }

    #[test]
    fn process_codex_events_finishes_job() {
        let snapshot = event_snapshot();
        let now = Instant::now();
        let stats = BackendStats {
            backend_type: "cli",
            started_at: now,
            first_token_at: None,
            finished_at: now,
            tokens_received: 0,
            bytes_transferred: 0,
            pty_attempts: 0,
            cli_fallback_used: false,
            disable_pty: false,
        };
        let events = vec![BackendEvent {
            job_id: 1,
            kind: BackendEventKind::Finished {
                lines: vec!["done".to_string()],
                status: "ok".to_string(),
                stats,
            },
        }];
        let mut job = build_test_backend_job(events, TestSignal::Ready);

        assert!(process_codex_events(&mut job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::Token { text } if text.contains("done"))
        }));
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::JobEnd { provider, success, .. } if provider == "codex" && *success)
        }));
    }

    #[test]
    fn process_codex_events_disconnected_sends_end() {
        let snapshot = event_snapshot();
        let mut job = build_test_backend_job(Vec::new(), TestSignal::Disconnected);

        assert!(process_codex_events(&mut job, false));
        let events = events_since(snapshot);
        assert!(events.iter().any(|event| {
            matches!(event, IpcEvent::JobEnd { provider, success, .. } if provider == "codex" && *success)
        }));
    }

    // -------------------------------------------------------------------------
    // IPC Command Deserialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_deserialize_send_prompt() {
        let json = r#"{"cmd": "send_prompt", "prompt": "hello world"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        match cmd {
            IpcCommand::SendPrompt { prompt, provider } => {
                assert_eq!(prompt, "hello world");
                assert!(provider.is_none());
            }
            _ => panic!("Expected SendPrompt"),
        }
    }

    #[test]
    fn test_deserialize_send_prompt_with_provider() {
        let json = r#"{"cmd": "send_prompt", "prompt": "hello", "provider": "claude"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        match cmd {
            IpcCommand::SendPrompt { prompt, provider } => {
                assert_eq!(prompt, "hello");
                assert_eq!(provider, Some("claude".to_string()));
            }
            _ => panic!("Expected SendPrompt with provider"),
        }
    }

    #[test]
    fn test_deserialize_start_voice() {
        let json = r#"{"cmd": "start_voice"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::StartVoice));
    }

    #[test]
    fn test_deserialize_cancel() {
        let json = r#"{"cmd": "cancel"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::Cancel));
    }

    #[test]
    fn test_deserialize_set_provider() {
        let json = r#"{"cmd": "set_provider", "provider": "claude"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        match cmd {
            IpcCommand::SetProvider { provider } => assert_eq!(provider, "claude"),
            _ => panic!("Expected SetProvider"),
        }
    }

    #[test]
    fn test_deserialize_auth() {
        let json = r#"{"cmd": "auth", "provider": "codex"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        match cmd {
            IpcCommand::Auth { provider } => {
                assert_eq!(provider, Some("codex".to_string()));
            }
            _ => panic!("Expected Auth"),
        }
    }

    #[test]
    fn test_deserialize_get_capabilities() {
        let json = r#"{"cmd": "get_capabilities"}"#;
        let cmd: IpcCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, IpcCommand::GetCapabilities));
    }

    // -------------------------------------------------------------------------
    // IPC Event Serialization Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_serialize_capabilities_event() {
        let event = IpcEvent::Capabilities {
            session_id: "test123".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            mic_available: true,
            input_device: Some("Default".to_string()),
            whisper_model_loaded: true,
            whisper_model_path: Some("/path/to/model".to_string()),
            python_fallback_allowed: true,
            providers_available: vec!["codex".to_string(), "claude".to_string()],
            active_provider: "codex".to_string(),
            working_dir: "/home/user".to_string(),
            codex_cmd: "codex".to_string(),
            claude_cmd: "claude".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"capabilities""#));
        assert!(json.contains(r#""session_id":"test123""#));
        assert!(json.contains(r#""mic_available":true"#));
    }

    #[test]
    fn test_serialize_token_event() {
        let event = IpcEvent::Token {
            text: "Hello world".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"token""#));
        assert!(json.contains(r#""text":"Hello world""#));
    }

    #[test]
    fn test_serialize_job_events() {
        let start = IpcEvent::JobStart {
            provider: "codex".to_string(),
        };
        let json = serde_json::to_string(&start).unwrap();
        assert!(json.contains(r#""event":"job_start""#));
        assert!(json.contains(r#""provider":"codex""#));

        let end = IpcEvent::JobEnd {
            provider: "claude".to_string(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&end).unwrap();
        assert!(json.contains(r#""event":"job_end""#));
        assert!(json.contains(r#""success":true"#));
        assert!(!json.contains("error")); // skip_serializing_if = None

        let end_error = IpcEvent::JobEnd {
            provider: "claude".to_string(),
            success: false,
            error: Some("Connection failed".to_string()),
        };
        let json = serde_json::to_string(&end_error).unwrap();
        assert!(json.contains(r#""error":"Connection failed""#));
    }

    #[test]
    fn test_serialize_provider_changed() {
        let event = IpcEvent::ProviderChanged {
            provider: "claude".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"provider_changed""#));
        assert!(json.contains(r#""provider":"claude""#));
    }

    #[test]
    fn test_serialize_auth_events() {
        let start = IpcEvent::AuthStart {
            provider: "codex".to_string(),
        };
        let json = serde_json::to_string(&start).unwrap();
        assert!(json.contains(r#""event":"auth_start""#));
        assert!(json.contains(r#""provider":"codex""#));

        let end = IpcEvent::AuthEnd {
            provider: "codex".to_string(),
            success: true,
            error: None,
        };
        let json = serde_json::to_string(&end).unwrap();
        assert!(json.contains(r#""event":"auth_end""#));
        assert!(json.contains(r#""success":true"#));
        assert!(!json.contains("error"));

        let end_error = IpcEvent::AuthEnd {
            provider: "claude".to_string(),
            success: false,
            error: Some("login failed".to_string()),
        };
        let json = serde_json::to_string(&end_error).unwrap();
        assert!(json.contains(r#""provider":"claude""#));
        assert!(json.contains(r#""error":"login failed""#));
    }

    #[test]
    fn test_serialize_voice_events() {
        let start = IpcEvent::VoiceStart;
        let json = serde_json::to_string(&start).unwrap();
        assert!(json.contains(r#""event":"voice_start""#));

        let end_ok = IpcEvent::VoiceEnd { error: None };
        let json = serde_json::to_string(&end_ok).unwrap();
        assert!(json.contains(r#""event":"voice_end""#));
        assert!(!json.contains("error"));

        let end_err = IpcEvent::VoiceEnd {
            error: Some("Mic unavailable".to_string()),
        };
        let json = serde_json::to_string(&end_err).unwrap();
        assert!(json.contains(r#""error":"Mic unavailable""#));

        let transcript = IpcEvent::Transcript {
            text: "Hello".to_string(),
            duration_ms: 500,
        };
        let json = serde_json::to_string(&transcript).unwrap();
        assert!(json.contains(r#""event":"transcript""#));
        assert!(json.contains(r#""text":"Hello""#));
        assert!(json.contains(r#""duration_ms":500"#));
    }

    #[test]
    fn test_serialize_error_event() {
        let event = IpcEvent::Error {
            message: "Something went wrong".to_string(),
            recoverable: true,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""event":"error""#));
        assert!(json.contains(r#""message":"Something went wrong""#));
        assert!(json.contains(r#""recoverable":true"#));
    }
}
