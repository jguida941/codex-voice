# ADR 0005: Logging Opt-In by Default

Status: Accepted
Date: 2026-01-29

## Context

Debug logging is useful for troubleshooting but creates privacy and disk concerns:
- Logs may contain sensitive content (prompts, transcripts, file paths)
- Unbounded logs can fill disk space
- Users may not realize logging is active
- Production audit identified always-on prompt logging as a risk

Previous behavior:
- Debug logs written to temp file by default
- Prompt detection logs always enabled
- No size caps or rotation

## Decision

Make all logging opt-in:

1. **Debug logs**: Disabled by default; enable with `--logs`
2. **Content logging**: Disabled by default; enable with `--log-content` (requires `--logs`)
3. **Prompt logs**: Disabled by default; enable with `--prompt-log` or env var
4. **Log rotation**: All enabled logs have size caps and rotate to prevent unbounded growth

Flags:
- `--logs` - Enable debug logging
- `--log-content` - Include prompt/transcript snippets in logs
- `--prompt-log` - Enable prompt detection logging
- `--no-logs` - Explicitly disable all logging (overrides env vars)

## Consequences

**Positive:**
- Privacy by default (no sensitive data written without consent)
- No disk growth without explicit opt-in
- Clear user control over what gets logged
- Audit-compliant behavior

**Negative:**
- Harder to debug issues (must ask user to enable logs)
- Users may not know to enable logs when reporting issues
- More flags to document and explain

**Trade-offs:**
- Chose privacy over convenience
- Troubleshooting docs must guide users to enable relevant logs

## Alternatives Considered

- **Logging on by default with sanitization**: Complex to implement correctly,
  risk of leaking sensitive data through edge cases.
- **Separate log levels**: Added complexity; opt-in is simpler.
- **No logging at all**: Makes debugging nearly impossible.

## Links

- [Architecture docs](../ARCHITECTURE.md#logging-and-privacy)
- [CLI flags reference](../../CLI_FLAGS.md)
- `rust_tui/src/app/logging.rs` - Log initialization
