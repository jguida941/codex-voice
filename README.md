# VoxTerm

![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![Whisper STT](https://img.shields.io/badge/Whisper-Local%20STT-74aa9c?style=for-the-badge)
![macOS](https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)
[![License](https://img.shields.io/github/license/jguida941/voxterm?style=for-the-badge)](LICENSE)

[![Rust TUI CI](https://img.shields.io/github/actions/workflow/status/jguida941/voxterm/rust_tui.yml?branch=master&style=for-the-badge)](https://github.com/jguida941/voxterm/actions/workflows/rust_tui.yml)
[![Mutation Testing](https://img.shields.io/github/actions/workflow/status/jguida941/voxterm/mutation-testing.yml?branch=master&style=for-the-badge)](https://github.com/jguida941/voxterm/actions/workflows/mutation-testing.yml)

Voice input for AI CLIs. Talk instead of type. Runs Whisper locally with ~250ms latency. No cloud, no API keys.

![VoxTerm](https://raw.githubusercontent.com/jguida941/voxterm/master/img/hero.png)

## Quick Start

```bash
# Install Codex CLI (default backend)
npm install -g @openai/codex

# Install VoxTerm via Homebrew
brew tap jguida941/voxterm
brew install voxterm

# Run it
cd ~/your-project
voxterm
```

First run downloads the Whisper model (~1.5 GB).

## How It Works

```mermaid
graph TD
    A["🎤 Microphone"] --> B["Whisper STT"]
    B --> C["Transcript"]
    C --> D["PTY"]
    D --> E["AI CLI"]
    E --> F["Terminal Output"]
```

VoxTerm wraps your AI CLI in a PTY and adds voice input. You talk → Whisper transcribes locally → text gets typed into the CLI. All CLI output passes through unchanged.

## Features

| Feature | Description |
|---------|-------------|
| **Local STT** | Whisper runs on your machine - no cloud calls |
| **~250ms latency** | Fast transcription through whisper.cpp |
| **PTY passthrough** | CLI UI stays unchanged |
| **Auto-voice** | Hands-free mode - no typing needed |
| **Transcript queue** | Speak while CLI is busy, sends when ready |
| **Multiple backends** | Codex, Claude, Gemini, Aider, or custom |
| **Themes** | 6 built-in themes including Catppuccin, Dracula, Nord |

### Theme Picker (Ctrl+Y)

![Theme Picker](https://raw.githubusercontent.com/jguida941/voxterm/master/img/theme-picker.png)

### Settings Menu (Ctrl+O)

![Settings](https://raw.githubusercontent.com/jguida941/voxterm/master/img/settings.png)

### Voice Recording

![Recording](https://raw.githubusercontent.com/jguida941/voxterm/master/img/recording.png)

## Controls

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice recording |
| `Ctrl+V` | Toggle auto-voice (hands-free) |
| `Ctrl+T` | Toggle send mode (auto/insert) |
| `Ctrl+Y` | Theme picker |
| `Ctrl+O` | Settings menu |
| `Ctrl+]` | Mic sensitivity up |
| `Ctrl+\` | Mic sensitivity down |
| `?` | Show help |
| `Enter` | Stop recording / send prompt |
| `Ctrl+Q` | Quit |
| `Ctrl+C` | Send interrupt to CLI |

## Install Options

<details>
<summary><strong>Homebrew (recommended)</strong></summary>

```bash
brew tap jguida941/voxterm
brew install voxterm
```
</details>

<details>
<summary><strong>From source</strong></summary>

```bash
git clone https://github.com/jguida941/voxterm.git
cd voxterm
./install.sh
```
</details>

<details>
<summary><strong>macOS App</strong></summary>

Double-click `VoxTerm.app`, pick a folder, it opens Terminal with VoxTerm running.

![Folder Picker](https://raw.githubusercontent.com/jguida941/voxterm/master/img/folder-picker.png)
</details>

**Requirements:** macOS or Linux (Windows needs WSL2) • Microphone access • ~1.5 GB disk for Whisper model

## Using Other AI CLIs

```bash
# Claude Code
voxterm --backend claude

# Gemini CLI
voxterm --backend gemini

# Aider
voxterm --backend aider

# Custom command
voxterm --backend "my-cli --flag"
```

## Documentation

| Users | Developers |
|-------|------------|
| [Quick Start](QUICK_START.md) | [Development](docs/dev/DEVELOPMENT.md) |
| [Install Guide](docs/INSTALL.md) | [Architecture](docs/dev/ARCHITECTURE.md) |
| [Usage Guide](docs/USAGE.md) | [ADRs](docs/dev/adr/README.md) |
| [CLI Flags](docs/CLI_FLAGS.md) | [Contributing](.github/CONTRIBUTING.md) |
| [Troubleshooting](docs/TROUBLESHOOTING.md) | [Changelog](docs/CHANGELOG.md) |

## Contributing

PRs welcome. See [CONTRIBUTING.md](.github/CONTRIBUTING.md).

Issues: [github.com/jguida941/voxterm/issues](https://github.com/jguida941/voxterm/issues)

## License

MIT - [LICENSE](LICENSE)
