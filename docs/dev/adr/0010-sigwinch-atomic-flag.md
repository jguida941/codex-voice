# ADR 0010: SIGWINCH Signal Handling with Atomic Flag

Status: Accepted
Date: 2026-01-29

## Context

When the terminal is resized, the kernel sends SIGWINCH to the process. We need to:
1. Detect the resize
2. Update the PTY window size
3. Forward SIGWINCH to Codex

Signal handlers in Unix have strict constraints:
- Must be async-signal-safe (can't call most functions)
- Can't allocate memory or take locks
- Should be minimal and return quickly

## Decision

Use an atomic boolean flag pattern:
- Global `SIGWINCH_RECEIVED: AtomicBool` flag
- Signal handler only sets the flag to `true` (async-signal-safe)
- Main event loop polls the flag every 50ms
- When flag is set: update PTY size, forward signal, clear flag

## Consequences

**Positive:**
- Fully async-signal-safe (no UB risk)
- Simple and well-understood pattern
- Main loop handles resize at safe point
- Works on all Unix platforms

**Negative:**
- Up to 50ms latency between resize and response
- Requires polling in main loop
- Global mutable state (atomic, but still global)

**Trade-offs:**
- Safety over minimal latency
- 50ms is imperceptible for window resize

## Alternatives Considered

- **signalfd/kqueue**: Platform-specific; more complex.
- **Self-pipe trick**: More code; atomic flag is simpler.
- **Direct handling in signal**: Unsafe; can't call Rust functions.

## Links

- `rust_tui/src/bin/codex_overlay/main.rs:46-47` - Atomic flag definition
- `rust_tui/src/bin/codex_overlay/main.rs:55-61` - Signal handler
- `rust_tui/src/bin/codex_overlay/main.rs:329-334` - Polling logic
