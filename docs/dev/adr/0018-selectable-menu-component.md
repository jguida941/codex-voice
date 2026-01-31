# ADR 0018: SelectableMenu Component Contract

Status: Proposed
Date: 2026-01-31

## Context

Multiple overlays (command palette, settings, history, theme picker) require a
consistent selectable list with filtering, keyboard navigation, and optional
mouse selection. Building separate list logic for each overlay risks divergent
behavior and inconsistent UX.

## Decision

Create a shared `SelectableMenu<T>` component with a stable contract:

- Generic items with a display label and an optional metadata payload.
- Keyboard navigation (up/down, page, home/end), optional wrap.
- Typeahead filter with debounce and match highlighting.
- Optional mouse selection for overlays that enable mouse input.
- Pure render: state + width -> lines or spans, no side effects.
- Shared tests for navigation bounds, filtering, and focus handling.

## Consequences

Positive:
- Consistent list behavior across overlays and TUI.
- Lower maintenance cost and fewer UX regressions.
- Easier to add new list-based overlays.

Negative:
- A bit more up-front design and test work.
- Requires careful API design to avoid future lock-in.

## Alternatives Considered

- Separate list logic per overlay (rejected: drift and duplicated bugs).
- Use a third-party widget directly (rejected: overlay needs raw ANSI renderer).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
