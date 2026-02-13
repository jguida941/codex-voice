//! IPC command router that keeps transport commands decoupled from backend logic.

use crate::codex::{CodexBackendError, CodexJobRunner, CodexRequest};
use crate::voice;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use super::protocol::{IpcEvent, Provider};
use super::session::{run_auth_flow, send_event, start_claude_job, ActiveJob, AuthJob, IpcState};

// ============================================================================
// Slash Command Parsing
// ============================================================================

#[derive(Debug)]
pub(super) enum ParsedInput {
    /// Wrapper command (handled by us)
    WrapperCommand(WrapperCmd),
    /// Provider command (forwarded to provider)
    ProviderCommand { command: String, args: String },
    /// Plain prompt
    Prompt(String),
}

#[derive(Debug)]
pub(super) enum WrapperCmd {
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

pub(super) fn parse_input(input: &str) -> ParsedInput {
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
// Command Handlers
// ============================================================================

pub(super) fn handle_send_prompt(
    state: &mut IpcState,
    prompt: &str,
    provider_override: Option<String>,
) {
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
            ActiveJob::Codex(j) => state.codex_cli_backend.cancel(j.id),
            ActiveJob::Claude(mut j) => {
                j.cancel();
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

pub(super) fn handle_wrapper_command(state: &mut IpcState, cmd: WrapperCmd) {
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

pub(super) fn start_provider_job(state: &mut IpcState, provider: Provider, prompt: &str) {
    send_event(&IpcEvent::JobStart {
        provider: provider.as_str().to_string(),
    });

    match provider {
        Provider::Codex => {
            let request = CodexRequest::chat(prompt.to_string());
            match state.codex_cli_backend.start(request) {
                Ok(job) => {
                    state.current_job = Some(ActiveJob::Codex(job));
                }
                Err(e) => {
                    let msg = match e {
                        CodexBackendError::InvalidRequest(s) => s.to_string(),
                        CodexBackendError::BackendDisabled(s) => s,
                    };
                    send_event(&IpcEvent::JobEnd {
                        provider: "codex".to_string(),
                        success: false,
                        error: Some(msg),
                    });
                }
            }
        }
        Provider::Claude => match start_claude_job(
            &state.claude_cmd,
            prompt,
            state.config.claude_skip_permissions,
            &state.config.term_value,
        ) {
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

pub(super) fn handle_start_voice(state: &mut IpcState) {
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
        None,
    );
    state.current_voice_job = Some(job);
}

pub(super) fn handle_cancel(state: &mut IpcState) {
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
                state.codex_cli_backend.cancel(j.id);
                send_event(&IpcEvent::JobEnd {
                    provider: "codex".to_string(),
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
            }
            ActiveJob::Claude(mut j) => {
                j.cancel();
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

pub(super) fn handle_set_provider(state: &mut IpcState, provider_str: &str) {
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

pub(super) fn handle_auth_command(state: &mut IpcState, provider_override: Option<String>) {
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
