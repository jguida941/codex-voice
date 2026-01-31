# ADR 0022: Render Guarantees and Layout Contract

Status: Proposed
Date: 2026-01-31

## Context

Terminal overlays are fragile: drawing outside reserved rows, emitting newlines,
or leaving the cursor in the wrong place can corrupt the user's shell state.
The UI plan requires a strict render contract to prevent jank and terminal
corruption across overlay and TUI modes.

## Decision

Adopt explicit render guarantees for overlay output:

- Render is pure: `state -> lines`, no side effects.
- No newlines are emitted while drawing the HUD region.
- Output is width-bounded by display width for every line.
- Cursor is saved/restored every frame; writes occur only within reserved rows.
- Resize triggers a full redraw and clears the old banner region.
- Periodic updates are coalesced to a fixed tick rate for continuous signals.

## Consequences

Positive:
- Stable, professional overlay behavior across terminals and multiplexers.
- Fewer hard-to-debug rendering artifacts.
- Clear testable invariants.

Negative:
- Requires strict discipline and additional render tests.
- Adds bookkeeping for reserved rows and cursor management.

## Alternatives Considered

- Allowing render shortcuts (rejected: risk of terminal corruption).
- Letting overlays write freely like standard CLI output (rejected: breaks shell).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
