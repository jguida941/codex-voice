# ADR 0007: Mono Audio Downmixing

Status: Accepted
Date: 2026-01-29

## Context

Audio input devices can provide mono, stereo, or multi-channel audio. Whisper STT
expects mono 16kHz audio. We need to convert multi-channel input to mono.

Options for downmixing:
1. **Average all channels**: Sum and divide by channel count
2. **Take first channel only**: Simpler but loses spatial information
3. **Weighted mix**: Center channel louder (for surround setups)

## Decision

Use simple averaging across all channels:
- Sum all channel samples for each frame
- Divide by channel count to get mono sample
- Implemented in `audio/dispatch.rs:append_downmixed_samples()`

Target format: 16kHz mono (defined in `audio/mod.rs` as `TARGET_SAMPLE_RATE` and
`TARGET_CHANNELS`).

## Consequences

**Positive:**
- Simple, predictable behavior
- Works with any channel count (stereo, 5.1, etc.)
- No configuration needed
- Maintains consistent volume regardless of input channels

**Negative:**
- Loses stereo/spatial information (acceptable for voice)
- Multi-speaker environments may blend voices together
- No option for channel selection

**Trade-offs:**
- Simplicity over flexibility
- Voice STT doesn't benefit from spatial audio anyway

## Alternatives Considered

- **First channel only**: Risk of picking wrong channel (some mics put voice on channel 2).
- **Configurable channel**: Added complexity for rare use case.
- **Weighted mix**: Overkill for voice input; adds DSP complexity.

## Links

- `rust_tui/src/audio/dispatch.rs:9-38` - Downmixing implementation
- `rust_tui/src/audio/mod.rs:7-11` - Target format constants
