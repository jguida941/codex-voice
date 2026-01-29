# ADR 0002: PTY Passthrough Architecture

Status: Accepted
Date: 2026-01-29

## Context

Codex Voice needs to add voice input to the Codex CLI while preserving its full TUI
experience. Two main approaches exist:

1. **TUI replacement**: Build a custom terminal UI that renders Codex output and adds
   voice controls (like the original `rust_tui/src/app.rs` TUI mode).
2. **PTY passthrough**: Run Codex in a pseudo-terminal and pass all ANSI output through
   unchanged, adding only a minimal overlay.

The TUI replacement approach requires parsing and re-rendering Codex's output, which:
- Breaks when Codex changes its UI
- Loses features like colors, cursor positioning, and interactive elements
- Requires maintaining a parallel rendering engine

## Decision

Use PTY passthrough architecture:
- Spawn Codex CLI in a PTY (`openpty` + `fork`)
- Pass all ANSI output through unchanged to the real terminal
- Intercept only specific control keys (Ctrl+R, Ctrl+V, Ctrl+Q, etc.)
- Draw a single-line status overlay using ANSI save/restore sequences
- Inject voice transcripts as if typed by the user

## Consequences

**Positive:**
- Codex's full TUI is preserved exactly as designed
- No maintenance burden when Codex updates its UI
- Simpler codebase (no rendering logic)
- Works with any terminal that Codex supports

**Negative:**
- Limited control over UI (can only overlay, not integrate)
- Must handle terminal queries (DSR/DA) to avoid confusing Codex
- Status line can be overwritten by Codex output (requires careful timing)
- Prompt detection is heuristic-based (no structured API)

**Trade-offs:**
- Chose simplicity and compatibility over deep UI integration
- Accepted that voice features are "bolted on" rather than native

## Alternatives Considered

- **Full TUI mode** (`rust_tui/src/app.rs`): Built and tested, but fragile and lost
  Codex's native experience. Deprecated in favor of overlay.
- **Codex plugin/extension API**: Does not exist; would require upstream changes.
- **Screen scraping with parsing**: Complex, error-prone, and still loses fidelity.

## Links

- [Architecture docs](../ARCHITECTURE.md)
- `rust_tui/src/pty_session/` - PTY implementation
- `rust_tui/src/bin/codex_overlay/main.rs` - Overlay entry point
