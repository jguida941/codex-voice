# ADR 0020: Action Registry and Keybindings

Status: Proposed
Date: 2026-01-31

## Context

Help text, command palette entries, and keybinding hints can drift if they are
maintained separately. The UI roadmap requires a single source of truth for
actions and bindings to avoid inconsistencies between overlay and full TUI.

## Decision

Introduce a shared `Action` enum and `ActionRegistry`:

- `ActionRegistry` stores label, default key, scope, and enable conditions.
- Keybinding resolver maps input -> action using the registry.
- Help overlay, command palette, and settings read from the registry.
- User overrides loaded from a `keybindings.toml` file.

## Consequences

Positive:
- Consistent, discoverable shortcuts across all UI surfaces.
- Easier to add new actions without missing help text updates.
- Overlay and TUI stay behaviorally aligned.

Negative:
- Requires refactoring key handling to go through the registry.
- Slightly more overhead to add new actions.

## Alternatives Considered

- Separate key maps for each UI (rejected: drift and maintenance cost).
- Hard-coded help/palette lists (rejected: error-prone).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
