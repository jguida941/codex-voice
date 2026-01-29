# ADR 0014: JSON IPC Protocol for External UIs

Status: Accepted
Date: 2026-01-29

## Context

The terminal-based overlay serves most users, but some want:
- Web-based UI for remote access
- GUI frontend for accessibility
- Integration with other tools (editors, IDEs)

These require a way to control the voice pipeline from external processes.

## Decision

Implement a JSON-based IPC protocol:
- **Transport**: Newline-delimited JSON over stdin/stdout
- **Direction**: Events (Rust → external), Commands (external → Rust)
- **Discrimination**: `{"event": "..."}` vs `{"cmd": "..."}`
- **Mode**: Enabled via `--json-ipc` flag

Protocol supports:
- Voice control commands (start, stop, cancel)
- State events (listening, transcribing, ready)
- Transcript delivery
- Error reporting
- Provider-agnostic (works with Codex and Claude)

## Consequences

**Positive:**
- Enables non-terminal frontends
- Clean separation of voice logic from UI
- JSON is universal and debuggable
- Newline-delimited is simple to parse

**Negative:**
- Significant code complexity (separate mode)
- State synchronization between processes
- Requires external UI implementation
- Two architectures to maintain (overlay + IPC)

**Trade-offs:**
- Flexibility over simplicity
- IPC mode is opt-in; doesn't affect normal users

## Alternatives Considered

- **WebSocket server**: More complex; requires networking setup.
- **Unix socket**: Platform-specific; harder to debug.
- **gRPC/protobuf**: Heavy dependencies; overkill for this use case.
- **Shared memory**: Complex; JSON over pipes is sufficient.

## Links

- `rust_tui/src/ipc/protocol.rs` - Protocol types
- `rust_tui/src/ipc/session.rs` - Session state machine
- `rust_tui/src/ipc/router.rs` - Command routing
