# Codex Voice

**Rust voice overlay for the official Codex CLI. Speak to Codex using local Whisper speech-to-text.**

Voice transcription runs entirely on your machine via `whisper.cpp`. The overlay wraps Codex in a PTY,
preserving its native TUI while injecting voice transcripts.

![Codex Voice Startup](img/startup.png)

## Quick Start

```bash
# Install Codex CLI
npm install -g @openai/codex

# Clone and install Codex Voice
git clone https://github.com/jguida941/codex-voice.git
cd codex-voice
./install.sh

# Run from any project
cd ~/my-project
codex-voice
```

First run downloads a Whisper model if missing. For other install options (Homebrew, macOS app,
manual), see [docs/INSTALL.md](docs/INSTALL.md).
Want the shortest path? See [QUICK_START.md](QUICK_START.md).

## Logs (opt-in)

Logs are disabled by default. To enable debug logs:
`codex-voice --logs` (add `--log-content` for prompt/response snippets).

## Controls (essentials)

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+T` | Toggle send mode (auto vs insert) |
| `Ctrl+]` | Increase mic threshold +5 dB |
| `Ctrl+\` | Decrease mic threshold -5 dB (or `Ctrl+/`) |
| `Ctrl+Q` | Exit overlay |
| `Ctrl+C` | Forward to Codex |

For full behavior notes (insert mode, transcript queueing, long dictation), see
[docs/USAGE.md](docs/USAGE.md).

## Features

- Voice input via microphone (Whisper STT)
- Fast local transcription (~250ms after speech ends), no cloud API calls
- Rust overlay preserves the full Codex TUI (PTY passthrough)
- Auto-voice mode with queued transcripts when Codex is busy
- Optional Python fallback when native capture is unavailable

## UI modes

- `codex-voice` (default): lightweight overlay with a single status line.
- `rust_tui`: full-screen Rust TUI (primarily for IPC/dev workflows).

## Project layout (contributor view)

- `rust_tui/src/bin/codex_overlay.rs` - overlay entrypoint
- `rust_tui/src/audio/recorder.rs` - CPAL capture + resample
- `rust_tui/src/mic_meter.rs` - mic meter + threshold recommendation
- `rust_tui/src/stt.rs` - Whisper transcription
- `scripts/codex_voice.py` - optional Python fallback

## Docs

- [QUICK_START.md](QUICK_START.md)
- [docs/INSTALL.md](docs/INSTALL.md)
- [docs/USAGE.md](docs/USAGE.md)
- [docs/CLI_FLAGS.md](docs/CLI_FLAGS.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- [docs/CHANGELOG.md](docs/CHANGELOG.md)

## How It Works

Codex Voice runs the Codex CLI in a real PTY and forwards raw ANSI output directly to your terminal.
Voice transcripts are injected as keystrokes, so Codex stays fully native. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for diagrams and data flow.

## License

MIT
