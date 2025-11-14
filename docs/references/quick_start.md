# Quick Start

Minimal commands that every developer should memorize. Run everything from the repo root unless noted otherwise.

## 1. Build & Run (Rust TUI)
```bash
cd rust_tui

# Build once (release mode)
cargo build --release

# Run with a Whisper model
cargo run --release -- \
  --seconds 5 \
  --whisper-model-path ../models/ggml-base.en.bin
```

### Helpful Flags
- `--seconds <n>` – recording duration (default 5s)
- `--input-device "Device Name"` – choose a microphone explicitly
- `--list-input-devices` – enumerate microphones and exit
- `--codex-arg="--danger-full-access"` – pass extra Codex CLI flags safely
- `--log-timings` – emit phase timing info to `${TMPDIR}/codex_voice_tui.log`
- `--no-python-fallback` – fail if the native pipeline isn’t available
- `--voice-vad-engine <earshot|simple>` – pick the Earshot VAD (default when built with `vad_earshot`) or the lightweight threshold fallback
- `--voice-vad-threshold-db <dB>` / `--voice-vad-frame-ms <ms>` – tune VAD sensitivity and frame size to match your environment

### Controls Inside the TUI
- `Ctrl+R` – start a voice capture
- `Ctrl+V` – toggle automatic voice mode (capture after every Codex reply)
- `Enter` – send the current prompt to Codex
- `Esc` – clear the input buffer
- `PageUp/PageDown` or `K/J` – scroll Codex output
- `Ctrl+C` – exit

## 2. Whisper Models
Download once and place in `models/`:
```bash
curl -L "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin" \
     -o models/ggml-base.en.bin
```
Use the tiny model for faster tests:
```bash
curl -L "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin" \
     -o models/ggml-tiny.en.bin
```

## 3. Basic Diagnostics
```bash
# Enumerate microphones
cargo run --release -- --list-input-devices

# Standalone audio capture sanity check
cargo run --bin test_audio

# Phase 2A latency benchmark (synthetic clips)
../scripts/benchmark_voice.sh

# Tail the log
LOG_FILE="${TMPDIR:-/tmp}/codex_voice_tui.log"
tail -f "$LOG_FILE"
```

## 4. Reference
- Architecture & daily notes: latest folder under `docs/architecture/YYYY-MM-DD/`
- Roadmap / “You Are Here”: `PROJECT_OVERVIEW.md`
- Procedures: `docs/references/`
- SDLC guidance: `agents.md`
