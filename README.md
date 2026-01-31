# Codex Voice

![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white)
![Whisper STT](https://img.shields.io/badge/Whisper-Local%20STT-74aa9c)
![macOS | Linux](https://img.shields.io/badge/macOS%20%7C%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

[![Rust TUI CI](https://github.com/jguida941/codex-voice/actions/workflows/rust_tui.yml/badge.svg)](https://github.com/jguida941/codex-voice/actions/workflows/rust_tui.yml)
[![Mutation Testing](https://github.com/jguida941/codex-voice/actions/workflows/mutation-testing.yml/badge.svg)](https://github.com/jguida941/codex-voice/actions/workflows/mutation-testing.yml)

Voice input for the Codex CLI by default, with optional backend selection for other AI CLIs. Written in Rust for speed. Talk instead of type and boost your productivity. Runs Whisper locally through Rust with ~250ms latency. No cloud, no API keys.

![Codex Voice Startup](img/startup.png)

## What Codex-Voice Does

Wraps Codex (or another AI CLI via `--backend`) in a PTY and adds voice input. You talk → Whisper transcribes → text gets typed into the CLI. Codex-Voice runs over the native CLI, so you keep its features like file editing and code generation.

- Written in Rust for speed
- Local speech-to-text via whisper.cpp
- ~250ms transcription time
- No network calls
- PTY overlay - CLI UI unchanged

## Requirements

- macOS or Linux (Windows needs WSL2)
- Node.js (for Codex CLI) or another supported AI CLI if using `--backend`
- Microphone access
- ~1.5 GB disk for Whisper model

## Install

```bash
# Install Codex CLI first (default backend)
npm install -g @openai/codex

# Clone and build
git clone https://github.com/jguida941/codex-voice.git
cd codex-voice
./install.sh

# Run it
cd ~/your-project
codex-voice
```

First run downloads the Whisper model.

To target another AI CLI instead of Codex, pass `--backend` (example: `codex-voice --backend claude`).

**Other options:** [Homebrew](docs/INSTALL.md#homebrew) | [macOS App](docs/INSTALL.md#macos-app) | [Build from source](docs/INSTALL.md#from-source)

## Controls

| Key | What it does |
|-----|--------------|
| `Ctrl+R` | Record voice |
| `Ctrl+V` | Toggle auto-voice (hands-free mode) |
| `Ctrl+T` | Toggle auto-send vs manual send |
| `Ctrl+Y` | Theme picker |
| `Ctrl+]` | Mic sensitivity up |
| `Ctrl+\` | Mic sensitivity down (also `Ctrl+/`) |
| `?` | Show shortcut help |
| `Enter` | Stop recording early (insert mode) / send prompt |
| `Ctrl+Q` | Quit |
| `Ctrl+C` | Send interrupt to CLI |

More details: [Usage Guide](docs/USAGE.md)

## Features

- **Local STT:** Whisper runs on your local machine
- **PTY passthrough:** Integrates with the selected CLI seamlessly
- **Auto-voice:** Code hands-free, no typing needed
- **Transcript queue:** Speak while the CLI is busy, transcripts send when ready
- **No logging by default:** Enable with `--logs` if you need it

## macOS App

Double-click `Codex Voice.app`, pick a folder, it opens Terminal with codex-voice running.

![Folder Picker](img/folder-picker.png)

## How It Works

```
Mic → Whisper → Text → PTY → CLI
                         ↓
                     Terminal (raw output)
```

The CLI runs in a PTY. Voice transcripts are sent as keystrokes. All CLI output passes through unchanged.

## Docs

**Users**
- [Quick Start](QUICK_START.md)
- [Install](docs/INSTALL.md)
- [Usage](docs/USAGE.md)
- [CLI Flags](docs/CLI_FLAGS.md)
- [Troubleshooting](docs/TROUBLESHOOTING.md)

**Developers**
- [Development](docs/dev/DEVELOPMENT.md)
- [Architecture](docs/dev/ARCHITECTURE.md)
- [ADRs](docs/dev/adr/README.md)
- [Contributing](.github/CONTRIBUTING.md)
- [Changelog](docs/CHANGELOG.md)

## Contributing

PRs welcome. See [CONTRIBUTING.md](.github/CONTRIBUTING.md).

Issues: [github.com/jguida941/codex-voice/issues](https://github.com/jguida941/codex-voice/issues)

## License

MIT - [LICENSE](LICENSE)
