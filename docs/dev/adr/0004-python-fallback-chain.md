# ADR 0004: Python Fallback Chain

Status: Accepted
Date: 2026-01-29

## Context

The native Rust voice pipeline (CPAL audio + whisper-rs) can fail for several reasons:
- Whisper model not downloaded or path misconfigured
- Audio device unavailable or permissions denied
- Platform-specific audio issues (especially on macOS with permissions)
- whisper-rs build issues on some systems

Users need a working voice experience even when the native pipeline fails.

## Decision

Implement a fallback chain:

1. **Try native pipeline first**: CPAL capture + whisper-rs transcription
2. **On failure, try Python fallback**: `scripts/codex_voice.py` using ffmpeg + whisper CLI
3. **Allow disabling fallback**: `--no-python-fallback` forces native-only mode

The Python fallback:
- Uses `ffmpeg` for audio capture (more compatible across platforms)
- Uses `whisper` CLI for transcription (pip-installable, well-tested)
- Requires Python 3, ffmpeg, and whisper on PATH
- Shows "Python pipeline" in status line when active

## Consequences

**Positive:**
- Higher success rate for first-time users
- Graceful degradation when native pipeline has issues
- Python pipeline is well-documented and debuggable
- Users can explicitly choose native-only for performance

**Negative:**
- Two code paths to maintain
- Python pipeline is slower (process spawning, disk I/O)
- Additional dependencies (Python, ffmpeg, whisper CLI)
- Harder to diagnose which pipeline is being used

**Trade-offs:**
- Reliability over performance for default behavior
- Power users can disable fallback for lower latency

## Alternatives Considered

- **Native-only**: Simpler but frustrating for users with setup issues.
- **Python-only**: More portable but slower and requires Python ecosystem.
- **Docker/container**: Too heavy for a CLI tool.
- **Multiple native backends**: CPAL alternatives (portaudio, etc.) add build complexity.

## Links

- [Architecture docs](../ARCHITECTURE.md#voice-error-and-fallback-flow)
- `rust_tui/src/voice.rs` - Fallback logic
- `scripts/codex_voice.py` - Python pipeline
