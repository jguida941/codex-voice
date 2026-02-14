# ADR 0011: Voice Send Modes (Auto vs Insert)

Status: Accepted
Date: 2026-01-29

## Context

After transcribing speech, there are two reasonable behaviors:
1. **Auto-send**: Immediately send transcript + newline to Codex
2. **Insert-only**: Insert transcript text, let user edit and press Enter

Different users prefer different workflows:
- Power users want fire-and-forget voice commands
- Careful users want to review/edit before sending
- Some commands benefit from editing (complex code, corrections)

## Decision

Support both modes, runtime-toggleable:
- **Auto mode** (`--voice-send-mode auto`): Append newline, send immediately
- **Insert mode** (`--voice-send-mode insert`): Insert text only, no newline
- Toggle with `Ctrl+T` at runtime
- Status line shows current mode

Mode affects auto-voice re-arming:
- Auto mode: Re-arm immediately after transcript injected
- Insert mode: Wait for user to press Enter before re-arming

## Consequences

**Positive:**
- Supports both power-user and careful-user workflows
- Runtime toggle allows adapting to task at hand
- Clear status indication of current mode

**Negative:**
- More state to track (current mode, re-arm conditions)
- Two code paths for transcript handling
- Users must learn about modes

**Trade-offs:**
- Flexibility over simplicity
- Default is Auto (most common use case)

## Alternatives Considered

- **Auto-only**: Frustrating for users who want to edit.
- **Insert-only**: Slow for power users; extra keystrokes.
- **Separate hotkeys**: More keys to remember; mode is cleaner.

## Links

- `src/src/bin/voiceterm/config/cli.rs` - `VoiceSendMode` enum
- `src/src/bin/voiceterm/main.rs:220-236` - Toggle handling
- `src/src/bin/voiceterm/transcript/delivery.rs` - Mode-specific send paths
