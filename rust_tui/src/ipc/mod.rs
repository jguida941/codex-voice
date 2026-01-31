//! JSON IPC mode for external UI integration.
//!
//! This module provides a non-blocking JSON-lines protocol over stdin/stdout
//! so that external frontends can drive the voice + provider pipeline.
//!
//! Architecture:
//! - Stdin reader thread: reads JSON commands, sends to main loop via channel
//! - Main event loop: processes commands and job events concurrently
//! - Provider abstraction: supports both Codex and Claude CLIs
//!
//! Protocol:
//! - Each line is a JSON object
//! - Events (Rust → client): {"event": "...", ...}
//! - Commands (client → Rust): {"cmd": "...", ...}

mod protocol;
mod router;
mod session;

#[cfg(test)]
mod tests;

pub use protocol::{IpcCommand, IpcEvent, Provider};
pub use session::run_ipc_mode;
