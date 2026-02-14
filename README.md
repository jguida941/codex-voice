<p align="center">
  <img src="img/logo.svg" alt="VoiceTerm">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
  <img src="https://img.shields.io/badge/Whisper-Voice_Input-8B5CF6?style=for-the-badge&logo=audacity&logoColor=white" alt="Whisper">
  <img src="https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white" alt="macOS">
  <img src="https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black" alt="Linux">
  <a href="dev/CHANGELOG.md"><img src="https://img.shields.io/github/v/tag/jguida941/voiceterm?style=for-the-badge&label=Version" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License"></a>
</p>

<p align="center">
  <a href="https://github.com/jguida941/voiceterm/actions/workflows/rust_ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/jguida941/voiceterm/rust_ci.yml?branch=master&style=for-the-badge&label=CI&logo=github" alt="CI"></a>
  <a href="https://github.com/jguida941/voiceterm/actions/workflows/mutation-testing.yml"><img src="https://img.shields.io/github/actions/workflow/status/jguida941/voiceterm/mutation-testing.yml?branch=master&style=for-the-badge&label=Mutation&logo=github" alt="Mutation Testing"></a>
</p>

Voice input for AI CLIs. Talk instead of type.
Runs Whisper locally with ~250ms latency. No cloud, no API keys.

## Quick Nav

- [Quick Start](#quick-start)
- [How It Works](#how-it-works)
- [Requirements](#requirements)
- [Supported AI CLIs](#supported-ai-clis)
- [UI Tour](#ui-tour)
- [Install Options](#install-options)
- [Documentation](#documentation)
- [Support](#support)

## Quick Start

```bash
# Install Codex CLI (default backend)
npm install -g @openai/codex

# Install VoiceTerm via Homebrew
brew tap jguida941/voiceterm
brew install voiceterm

# Run it
cd ~/your-project
voiceterm

# Alternative install via PyPI
pipx install voiceterm
```

If you haven't authenticated yet:
```bash
voiceterm --login --codex
voiceterm --login --claude
```

First run downloads a Whisper model (install/start scripts default to base ~142 MB; CLI default is small ~466 MB). To choose a different size:
- `./scripts/install.sh --small`
- `./scripts/setup.sh models --medium`
- Or pass `--whisper-model-path` directly
See [Whisper docs](guides/WHISPER.md) for details.

Startup splash behavior:
- default dwell is short (`VOICETERM_STARTUP_SPLASH_MS=1500`)
- set `VOICETERM_STARTUP_SPLASH_MS=0` for immediate clear
- set `VOICETERM_NO_STARTUP_BANNER=1` to skip it entirely

IDE terminal HUD compatibility (`v1.0.64`):
- Full HUD uses the proven `v1.0.53` writer/render baseline and clears stale
  HUD rows on resize so ghost frames do not remain after terminal geometry
  changes in Cursor/JetBrains terminals.
- Startup splash is auto-skipped in JetBrains IDE terminals (PyCharm/IntelliJ/
  CLion/WebStorm) to avoid alternate-screen handoff artifacts.

## How It Works

```mermaid
graph TD
    A["Microphone"] --> B["Whisper STT"]
    B --> C["Transcript"]
    C --> D["PTY"]
    D --> E["AI CLI"]
    E --> F["Terminal Output"]
```

VoiceTerm wraps your AI CLI in a PTY and adds voice input.
You talk → Whisper transcribes locally → text gets typed into the CLI.
All CLI output passes through unchanged.

## Requirements

- macOS or Linux (Windows needs WSL2)
- Microphone access
- ~0.5 GB disk for the default small model (base is ~142 MB, medium is ~1.5 GB)

## Features

| Feature | Description |
|---------|-------------|
| **Local STT** | Whisper runs on your machine - no cloud calls |
| **~250ms latency** | Fast transcription through whisper.cpp |
| **PTY passthrough** | CLI UI stays unchanged |
| **Auto-voice** | Hands-free mode - no typing needed |
| **Transcript queue** | Speak while CLI is busy, types when ready |
| **Project voice macros** | Expand trigger phrases from `.voiceterm/macros.yaml` before typing |
| **Macros toggle** | Runtime ON/OFF control for macro expansion from Settings |
| **Adaptive HUD telemetry** | Compact meter/latency trend chips that adapt to recording, busy, and idle states |
| **Backends** | Codex + Claude supported; experimental presets for Gemini (not working), Aider, OpenCode |
| **Themes** | 11 built-in themes including ChatGPT, Catppuccin, Dracula, Nord, Tokyo Night, Gruvbox |

## Supported AI CLIs

VoiceTerm is optimized for Codex and Claude Code. Experimental presets exist for
Gemini, Aider, and OpenCode, plus custom command strings via `--backend`. Gemini
is currently nonfunctional, and Aider/OpenCode are untested.

### Codex (default)

```bash
npm install -g @openai/codex
voiceterm
voiceterm --codex   # explicit (optional)
voiceterm --login --codex
```

![Codex Backend](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/codex-backend.png)

### Claude Code

```bash
curl -fsSL https://claude.ai/install.sh | bash
voiceterm --claude
voiceterm --login --claude
```

![Claude Backend](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/claude-backend.png)

### Experimental presets

These presets exist but are not part of the primary support matrix:
- Gemini CLI (`voiceterm --gemini`)
- Aider (`voiceterm --backend aider`)
- OpenCode (`voiceterm --backend opencode`)
Gemini is currently not working; Aider/OpenCode are untested.

## UI Tour

### Theme Picker (Ctrl+Y)

![Theme Picker](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/theme-picker.png)
Use ↑/↓ to move and Enter to select, or type the theme number.

### Settings Menu (Ctrl+O)

![Settings](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/settings.png)

Mouse control is on by default (toggle it off in Settings).
You can click HUD buttons or use ←/→ to focus and Enter to activate.
Settings also include a `Macros` ON/OFF toggle, alongside send mode and auto-voice.
Latency badge rendering is configurable in Settings via `Latency display`
(`Off`, `Nms`, `Latency: Nms`).
Right-panel telemetry modes (`Ribbon`, `Dots`, `Heartbeat`) render in Full HUD and
now also render as compact chips in Minimal HUD.
Full HUD border style is configurable (`Theme`, `Single`, `Rounded`, `Double`,
`Heavy`, `None`), and right-panel telemetry can be disabled with `Off`.
See the [Usage Guide](guides/USAGE.md) for details.

### Voice Recording

![Recording](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/recording.png)

## Controls

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice recording |
| `Ctrl+V` | Toggle auto-voice (hands-free) |
| `Ctrl+T` | Toggle send mode (auto/insert) |
| `Ctrl+U` | Cycle HUD style |
| `Ctrl+Y` | Theme picker |
| `Ctrl+O` | Settings menu |
| `Ctrl+]` | Mic less sensitive |
| `Ctrl+\` | Mic more sensitive |
| `?` | Show help |
| `Enter` | Stop recording / send text |
| `Ctrl+Q` | Quit |
| `Ctrl+C` | Send interrupt to CLI |

Macro behavior is controlled by Settings -> `Macros`:
- `ON`: apply `.voiceterm/macros.yaml` expansions before injection.
- `OFF`: inject raw transcripts unchanged.

## Install Options

<details>
<summary><strong>Homebrew (recommended)</strong></summary>

```bash
brew tap jguida941/voiceterm
brew install voiceterm
```
</details>

<details>
<summary><strong>PyPI (pipx/pip)</strong></summary>

```bash
pipx install voiceterm
# or: python3 -m pip install --user voiceterm
```

The PyPI package installs a launcher and bootstraps the native binary on first run
(`git` + `cargo` required).
</details>

<details>
<summary><strong>From source</strong></summary>

```bash
git clone https://github.com/jguida941/voiceterm.git
cd voiceterm
./scripts/install.sh
```
</details>

<details>
<summary><strong>macOS App</strong></summary>

Double-click `app/macos/VoiceTerm.app`, pick a folder, it opens Terminal with VoiceTerm running.

![Folder Picker](https://raw.githubusercontent.com/jguida941/voiceterm/master/img/folder-picker.png)
</details>

## Documentation

| Users | Developers |
|-------|------------|
| [Quick Start](QUICK_START.md) | [Development](dev/DEVELOPMENT.md) |
| [Install Guide](guides/INSTALL.md) | [Architecture](dev/ARCHITECTURE.md) |
| [Usage Guide](guides/USAGE.md) | [ADRs](dev/adr/README.md) |
| [CLI Flags](guides/CLI_FLAGS.md) | [Contributing](.github/CONTRIBUTING.md) |
| [Whisper & Languages](guides/WHISPER.md) | [Changelog](dev/CHANGELOG.md) |
| [Troubleshooting](guides/TROUBLESHOOTING.md) | |

## Support

- Troubleshooting: [guides/TROUBLESHOOTING.md](guides/TROUBLESHOOTING.md)
- Bug reports and feature requests: [GitHub Issues](https://github.com/jguida941/voiceterm/issues)
- Security concerns: [.github/SECURITY.md](.github/SECURITY.md)

## Contributing

PRs welcome. See [CONTRIBUTING.md](.github/CONTRIBUTING.md).
Before opening a PR, run `python3 dev/scripts/devctl.py check --profile prepush`.
For governance/docs consistency, also run `python3 dev/scripts/devctl.py hygiene`.

## License

MIT - [LICENSE](LICENSE)
