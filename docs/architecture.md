# Architecture Overview

This repository implements a voice-driven interface for the Codex CLI. Two
frontends share the same three-stage pipeline:

```
microphone → ffmpeg (WAV) → Whisper STT → Codex CLI → terminal output
```

The Python script (`codex_voice.py`) provides the minimal proof-of-concept and
serves as the canonical definition of how each stage should behave. The Rust
TUI (`rust_tui/`) builds on top of those contracts to deliver a richer terminal
experience without reimplementing signal-processing logic.

## Core Pipeline

1. **Audio capture** – `ffmpeg` records a mono, 16 kHz WAV file using
   platform-specific defaults (see `record_wav`).
2. **Speech-to-text** – The recorded clip is handed to the OpenAI `whisper` CLI
   or the `whisper.cpp` binary (see `transcribe`). Output files are staged in a
   per-run temporary directory to avoid collisions.
3. **Codex dispatch** – The resulting transcript is sent to the Codex CLI
   (see `call_codex_auto`). The helper automatically retries multiple invocation
   strategies: positional argument, stdin piping, and—on POSIX—PTY emulation for
   tools that insist on a TTY.

Each step shells out to an external tool. This keeps the implementation small
and makes it easy to reproduce the pipeline from other languages.

## `codex_voice.py`

- Provides CLI flags for tweaking capture duration, Whisper model, device names,
  and Codex invocation details.
- Exports three reusable functions (`record_wav`, `transcribe`, `call_codex_auto`)
  so downstream frontends can call into the same logic.
- Stores additional Codex flags (`--codex-args`) in a module-level cache so that
  helper functions remain stateless.
- Prints the raw transcript for optional manual edits before invoking Codex.
- Reports latency metrics (`record_s`, `stt_s`, `codex_s`, `total_s`) to help
  diagnose bottlenecks.
- Offers macOS voice feedback (`say`) when requested and cleans up temporary
  audio artifacts unless `--keep-audio` is set.

## `rust_tui/`

- Uses `ratatui` and `crossterm` to render a split-screen UI with scrollback,
  prompt input, and status bar.
- Shares the same shell contracts as the Python version: it shells out to
  `ffmpeg`, `whisper`/`whisper.cpp`, and the Codex CLI.
- Provides keyboard shortcuts:
  - `Ctrl+R` captures voice input and populates the prompt.
  - `Ctrl+V` toggles persistent voice mode; after each send, the next transcript
    is captured automatically so you can stay hands-free.
  - `Enter` sends the prompt; `Ctrl+C` exits.
- Truncates scrollback to `OUTPUT_MAX_LINES` to prevent unbounded memory use.
- Stores application state (`App`) separately from configuration (`AppConfig`)
  to keep rendering logic deterministic.

## Test Stubs (`stubs/`)

The `stubs/` directory contains lightweight drop-in replacements for `ffmpeg`,
`whisper`, and `codex`. They generate deterministic outputs, which makes it
possible to smoke-test the pipeline without real audio hardware or the actual
Codex CLI.

- `fake_ffmpeg` writes a silent WAV file with the expected format.
- `fake_whisper`/`whisper` create plain-text transcripts alongside the WAV file.
- `fake_codex` echoes the received prompt regardless of whether the input was
  passed as an argument or via stdin.

## Extending the System

- **Alternate frontends** – Any new UI can reuse the Python helpers as a spec.
  As long as it shells out to the same commands, the behavior will match.
- **Different STT engines** – Swap the `whisper_cmd` with another binary that
  produces a `.txt` transcript and adjust the CLI flags accordingly.
- **Automation** – `--codex-args` makes it easy to append Codex-specific flags
  (e.g., temperature or model selection) without modifying code.

## Failure Handling

- Missing binaries raise descriptive `RuntimeError`s in Python and `anyhow`
  errors in Rust thanks to `_require` and `Context`.
- Timeouts propagate with stderr attached, making it clear which tool stalled.
- PTY emulation only runs on POSIX platforms; Windows callers receive a clear
  error message if Codex requires a terminal.

Keep this document aligned with code changes so future contributors can quickly
grasp how audio capture, transcription, and Codex integration fit together.
