# ADR 0009: Serialized Output Writer Thread

Status: Accepted
Date: 2026-01-29

## Context

The overlay needs to:
1. Pass PTY output to the terminal (Codex's UI)
2. Draw a status line at the bottom of the screen
3. Avoid interleaving these outputs (causes visual corruption)

Without serialization, PTY output and status updates can race, producing garbled
terminal output.

## Decision

Serialize all terminal output through a dedicated writer thread:
- Single writer thread owns stdout
- Main loop sends messages via bounded channel (512 capacity)
- Message types: `PtyOutput`, `Status`, `Shutdown`
- Status line uses ANSI save/restore (`ESC 7` / `ESC 8`) for positioning
- Status redraws are debounced (25ms) to reduce flicker

## Consequences

**Positive:**
- No output interleaving or corruption
- Clean separation of concerns (main loop doesn't touch stdout)
- Bounded channel provides backpressure
- ANSI save/restore is widely compatible

**Negative:**
- Added latency (channel hop + thread scheduling)
- More complex architecture (another thread to manage)
- Channel capacity (512) is a tuning parameter

**Trade-offs:**
- Correctness over minimal latency
- 512-message buffer balances memory vs throughput

## Alternatives Considered

- **Mutex on stdout**: Simpler but risks blocking main loop on slow writes.
- **Lock-free queue**: More complex; bounded channel is sufficient.
- **Direct writes with careful ordering**: Error-prone; race conditions likely.

## Links

- `src/src/bin/voiceterm/main.rs:50` - Channel capacity constant
- `src/src/bin/voiceterm/writer.rs` - Writer thread implementation
