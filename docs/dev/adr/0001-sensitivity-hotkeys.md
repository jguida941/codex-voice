# ADR 0001: Sensitivity Hotkeys and ESC Handling

Status: Accepted
Date: 2026-01-29

## Context

We need consistent, low-friction hotkeys for mic sensitivity that do not
collide with terminal control sequences. Ctrl+[ is the same byte as ESC, so
binding it breaks arrow keys and other escape-prefixed inputs. The previous
Ctrl++ / Ctrl+- pair required Shift in many terminals and was inconsistent.

An experimental `alt_escape` parser enabled Alt+= / Alt+- but added complexity
and risked interfering with ESC-prefixed sequences.

## Decision

- Use `Ctrl+]` (0x1d) to increase sensitivity.
- Use `Ctrl+\` (0x1c) to decrease sensitivity.
- Do not bind `Ctrl+[` because it is ESC (0x1b).
- Remove `alt_escape` handling.

## Consequences

- Users get a consistent pair of unshifted control bindings.
- ESC sequences remain intact and terminal behavior stays predictable.
- Alt+= / Alt+- support is removed.
- Docs and startup hints must reflect the new bindings.

## Alternatives Considered

- Ctrl+_ for decrease (works but still shift-dependent in many terminals).
- Alt+= / Alt+- with ESC parsing (more complex and terminal-dependent).
- Function keys (macOS default media keys reduce reliability).

## Links

- [UI Enhancement Plan](../../active/UI_ENHANCEMENT_PLAN.md)
