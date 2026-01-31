# ADR 0021: History Storage and Retention

Status: Proposed
Date: 2026-01-31

## Context

Transcript history can contain sensitive content. The UI plan calls for history
browsing and retention controls, which require a clear storage and privacy
policy to avoid accidental data leakage.

## Decision

Define a privacy-first history system:

- Default retention is "none" unless explicitly enabled.
- When enabled, store history in a local data directory with restrictive
  permissions.
- Provide modes: full, truncated, or metadata-only.
- Sanitize control characters before storing or rendering.
- Support explicit user purge and retention duration limits.

## Consequences

Positive:
- Safer defaults for sensitive transcript data.
- Clear user control over persistence and retention.
- Reduced risk of terminal escape injection from stored data.

Negative:
- Extra configuration surface area.
- Some features (search/history) are limited by strict defaults.

## Alternatives Considered

- Always-on history (rejected: privacy risk).
- No history feature at all (rejected: limits usability).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
