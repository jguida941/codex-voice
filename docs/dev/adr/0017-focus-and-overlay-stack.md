# ADR 0017: Focus and Overlay Stack Model

Status: Proposed
Date: 2026-01-31

## Context

The overlay and full TUI will add multiple modals (help, settings, palette, history)
that need predictable focus behavior. Without a consistent focus model, inputs can
leak to the wrong control, modal focus can escape, and closing a modal can leave
focus in an undefined state. We also need consistent behavior between overlay
mode (raw ANSI) and the full Ratatui UI.

## Decision

Adopt a shared focus and overlay stack model in the UI core:

- Maintain an `overlay_stack: Vec<OverlayKind>` to track z-order and input routing.
- Maintain a `focus_stack` per overlay for focus trap and restoration.
- On modal open: push overlay + record prior focus target.
- On modal close: pop overlay + restore focus to prior target.
- Tab/Shift+Tab cycles focus within the active overlay only.
- Input routing always targets the top overlay on the stack.

## Consequences

Positive:
- Consistent focus trap and restoration across overlays.
- Overlay and TUI behavior remains identical.
- Predictable input routing and fewer accidental leaks to the CLI.

Negative:
- Slightly more state and bookkeeping in the UI core.
- Requires focused testing (focus trap and restoration paths).

## Alternatives Considered

- Single global focus target (rejected: breaks modal focus restoration).
- Ad hoc per-overlay focus handling (rejected: high risk of drift).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
