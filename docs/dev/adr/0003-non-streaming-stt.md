# ADR 0003: Non-Streaming Speech-to-Text

Status: Accepted
Date: 2026-01-29

## Context

Voice-to-text can be implemented in two ways:

1. **Streaming STT**: Transcribe audio in real-time as the user speaks, showing
   partial results that update continuously.
2. **Non-streaming STT**: Record the full utterance first, then transcribe the
   complete audio in one pass.

Streaming provides faster perceived feedback but requires:
- A streaming-capable STT backend (many Whisper implementations don't support this)
- Complex state management for partial/final results
- UI to show updating text without confusing the user
- Handling of corrections when final differs from partial

## Decision

Use non-streaming STT:
- Capture audio with VAD (voice activity detection) until silence
- Send the complete audio buffer to Whisper for transcription
- Display the final transcript only after processing completes

## Consequences

**Positive:**
- Simpler implementation (one transcription call per utterance)
- More accurate results (Whisper sees full context)
- No UI complexity for partial results
- Works with any Whisper backend (whisper-rs, CLI, Python)

**Negative:**
- Higher perceived latency (user waits for full transcription)
- No feedback during recording (only "Listening..." status)
- Longer utterances = longer wait times
- User can't see/correct text while speaking

**Trade-offs:**
- Latency scales with capture length: ~1-3s for typical commands
- Acceptable for coding voice commands (short phrases)
- Less suitable for long-form dictation (but `--voice-max-duration` can cap this)

## Alternatives Considered

- **Streaming Whisper** (whisper.cpp streaming mode): More complex, requires
  different architecture, partial results often inaccurate for short phrases.
- **Hybrid approach** (show waveform during capture, then transcript): Added
  complexity for marginal UX benefit.
- **Cloud streaming APIs** (Google, AWS): Adds latency, cost, privacy concerns;
  conflicts with local-first design goal.

## Links

- [Architecture docs](../ARCHITECTURE.md#stt-behavior-non-streaming)
- `rust_tui/src/stt.rs` - Whisper transcription
- `rust_tui/src/audio/vad.rs` - Voice activity detection
