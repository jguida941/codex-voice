# ADR 0015: No Audio Device Hotplug Recovery

Status: Accepted
Date: 2026-01-29

## Context

Users may disconnect and reconnect audio devices during a session:
- Unplugging USB microphones
- Bluetooth headset disconnections
- Switching between built-in and external mics
- macOS audio routing changes

Supporting hotplug recovery requires:
- Monitoring device list changes
- Detecting when current device becomes unavailable
- Gracefully stopping capture
- Re-initializing with new/reconnected device
- Handling partial captures during transition

This adds significant complexity to the audio pipeline.

## Decision

Do not implement device hotplug recovery:
- Audio device is selected at startup and used for the session
- If the device disconnects, capture fails with an error
- User must restart `codex-voice` to select a different device
- Python fallback may still work (ffmpeg handles devices differently)

Document this limitation clearly in:
- `docs/dev/ARCHITECTURE.md` (Audio device behavior section)
- `docs/TROUBLESHOOTING.md` (device issues)
- Error messages when capture fails

## Consequences

**Positive:**
- Simpler audio pipeline (no device monitoring)
- Predictable behavior (same device for entire session)
- Fewer edge cases and race conditions
- Less platform-specific code

**Negative:**
- Users must restart after device changes
- Bluetooth users may be frustrated (frequent disconnects)
- No graceful degradation on device loss
- Feels "broken" to users expecting modern hotplug behavior

**Trade-offs:**
- Simplicity and reliability over convenience
- Acceptable for typical use (wired mic, stable session)
- Power users can use `--ffmpeg-device` with Python fallback for more flexibility

## Alternatives Considered

- **Full hotplug support**: Significant complexity; CPAL device enumeration is
  platform-specific and unreliable on some systems.
- **Periodic device re-check**: Still complex; when to re-check? What if mid-capture?
- **Fallback to default device**: Could pick wrong device; confusing behavior.
- **Prompt user to select new device**: Requires UI interruption; out of scope for
  minimal overlay design.

## Links

- [Architecture docs](../ARCHITECTURE.md#audio-device-behavior)
- [Troubleshooting](../../TROUBLESHOOTING.md)
- `rust_tui/src/audio/capture.rs` - Capture implementation
