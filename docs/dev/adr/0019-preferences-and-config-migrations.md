# ADR 0019: Preferences and Config Migrations

Status: Proposed
Date: 2026-01-31

## Context

The UI roadmap introduces persistent preferences (theme, keybindings, history
retention). Without versioning and migrations, config changes can break upgrades
or silently misconfigure behavior. CLI flags must continue to override defaults.

## Decision

Adopt a versioned preferences file with explicit migrations:

- Store preferences at `~/.config/codex-voice/preferences.toml`.
- Include a `schema_version` field.
- On load, migrate older versions to the current schema.
- CLI flags override persisted preferences at runtime.
- Writes are atomic (write temp, fsync, rename) and validated.

## Consequences

Positive:
- Safe upgrades and backwards compatibility.
- Clear precedence rules between config and CLI.
- Easier to add new settings without breaking users.

Negative:
- Additional migration code and tests.
- Slight complexity in config load path.

## Alternatives Considered

- Unversioned config (rejected: brittle upgrades).
- Breaking changes with manual user edits (rejected: poor UX).

## Links

- `docs/active/UI_ENHANCEMENT_PLAN.md`
- `docs/active/MASTER_PLAN.md` (MP-038)
