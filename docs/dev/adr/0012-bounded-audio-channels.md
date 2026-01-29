# ADR 0012: Bounded Audio Channel Capacities

Status: Accepted
Date: 2026-01-29

## Context

Audio frames flow from the recorder thread to the VAD processor via channels.
The recorder runs in a real-time audio callback that must not block. If the
channel is unbounded or too small:
- Unbounded: Memory grows if VAD is slow
- Too small: Frames dropped frequently, degraded transcription

Need to balance memory bounds with audio fidelity.

## Decision

Use bounded channels with configurable capacity:
- Default capacity: 100 frames (`DEFAULT_VOICE_CHANNEL_CAPACITY`)
- Configurable via `--voice-channel-capacity`
- On overflow: Drop frame (non-blocking), increment counter
- Report dropped frames in capture metrics

Frame dispatcher behavior:
- `try_send()` to avoid blocking audio callback
- Count drops via `frames_dropped` metric
- Surface drops in status line when significant

## Consequences

**Positive:**
- Bounded memory usage
- Audio callback never blocks (real-time safe)
- Drops are observable via metrics
- Tunable for different systems

**Negative:**
- Dropped frames degrade transcription quality
- Default (100) may need tuning for slow systems
- Users may not notice dropped frames

**Trade-offs:**
- Real-time safety over perfect audio capture
- Observable degradation (metrics) over silent failure

## Alternatives Considered

- **Unbounded channel**: Memory risk in slow-consumer scenarios.
- **Blocking send**: Would cause audio glitches; unacceptable.
- **Ring buffer**: More complex; channel with drops achieves same effect.

## Links

- `rust_tui/src/config/defaults.rs:10` - Default capacity
- `rust_tui/src/audio/dispatch.rs:40-84` - Frame dispatcher
- `rust_tui/src/audio/capture.rs:10-18` - Metrics struct
