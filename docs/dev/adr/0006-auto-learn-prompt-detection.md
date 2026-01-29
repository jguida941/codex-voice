# ADR 0006: Auto-Learn Prompt Detection

Status: Accepted
Date: 2026-01-29

## Context

Auto-voice mode needs to detect when Codex is waiting for input (showing a prompt)
so it can automatically start listening. Two approaches:

1. **Regex-based**: User provides `--prompt-regex` to match their prompt
2. **Auto-learn**: System learns the prompt pattern from the first idle line

Problems with regex-only:
- Codex's prompt format can change between versions
- Users don't know what regex to use
- Different shells/configs have different prompts
- Friction for first-time users

## Decision

Use auto-learn as the default, with regex as an override:

1. **On startup**: No prompt pattern is known
2. **On first idle**: Save the current line as the learned prompt
3. **Subsequently**: Match against the learned prompt
4. **Fallback**: If no prompt learned after timeout, trigger on idle anyway
5. **Override**: `--prompt-regex` bypasses auto-learn entirely

The auto-learn algorithm:
- Strip ANSI escape sequences from output
- Track current line being written
- When output goes idle, compare current line to learned prompt
- If no learned prompt, save current line as the pattern

## Consequences

**Positive:**
- Zero-config for most users (just works)
- Adapts to Codex version changes automatically
- No regex knowledge required
- Fallback ensures auto-voice eventually triggers

**Negative:**
- Can mis-learn if first idle happens during output (rare)
- Less predictable than explicit regex
- Debugging prompt detection requires `--prompt-log`
- May need manual reset if prompt changes mid-session

**Trade-offs:**
- Chose UX simplicity over predictability
- Power users can still use `--prompt-regex` for control

## Alternatives Considered

- **Regex-only**: Requires user configuration; high friction.
- **Codex API integration**: No such API exists.
- **Timing-only**: Just wait for idle; less accurate, more false positives.
- **ML-based detection**: Overkill for this problem; adds dependencies.

## Links

- [Architecture docs](../ARCHITECTURE.md#prompt-detection-auto-voice)
- `rust_tui/src/bin/codex_overlay/prompt.rs` - Prompt tracker
- [CLI flags reference](../../CLI_FLAGS.md) - `--prompt-regex` docs
