# ADR 0013: Security Hard Limits

Status: Accepted
Date: 2026-01-29

## Context

User-provided configuration values flow into subprocess invocations and resource
allocation. Without limits:
- Infinite recording could exhaust memory/disk
- Malicious args could inject shell commands
- Large allocations could DoS the system

Need defense-in-depth against misconfiguration and malicious input.

## Decision

Enforce hard limits on security-sensitive parameters:

| Parameter | Limit | Rationale |
|-----------|-------|-----------|
| Max capture duration | 60 seconds | Prevent infinite recording |
| Max Codex args | 64 | Limit subprocess arg parsing |
| Max arg bytes | 8 KB per arg | Prevent huge memory args |
| FFmpeg device chars | Block `;|&$`<>\\'"` | Prevent shell injection |
| Binary paths | Must exist, validated | Prevent arbitrary execution |

Validation happens at config parse time; invalid configs fail fast with clear errors.

## Consequences

**Positive:**
- Defense against DoS and injection attacks
- Fail-fast on invalid configuration
- Clear error messages for users
- Limits are documented and predictable

**Negative:**
- Power users may hit limits (60s max capture)
- Shell metacharacter blocking may be overly restrictive
- Limits require documentation

**Trade-offs:**
- Security over flexibility
- Conservative limits can be raised if needed

## Alternatives Considered

- **No limits**: Security risk; unbounded resources.
- **Warnings only**: Users ignore warnings; enforcement is safer.
- **Runtime limits**: Harder to reason about; config-time is clearer.

## Links

- `rust_tui/src/config/defaults.rs:20-32` - Limit constants
- `rust_tui/src/config/validation.rs` - Validation logic
- [CLI flags reference](../../CLI_FLAGS.md) - Documented limits
