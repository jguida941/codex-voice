# VoxTerm Quick Start

Get voice input for your AI CLI in under 2 minutes. Works on macOS and Linux (Windows needs WSL2).

## 1) Install Codex CLI (default backend)

```bash
npm install -g @openai/codex
```

Or use another AI CLI: `voxterm --claude` or `voxterm --gemini`.

## 2) Install VoxTerm

**Homebrew (recommended):**
```bash
brew tap jguida941/voxterm
brew install voxterm
```

**From source:**
```bash
git clone https://github.com/jguida941/voxterm.git
cd voxterm
./install.sh
```

**macOS App:** Double-click **VoxTerm.app** and choose your project folder.

If `voxterm` is not found after install, see [docs/INSTALL.md](docs/INSTALL.md) for PATH notes.

## 3) Run from any project

```bash
cd ~/my-project
voxterm
```

First run downloads a Whisper model if missing.

To target another AI CLI instead of Codex, pass `--backend`:

```bash
voxterm --backend claude
```

## 4) Essential controls

- `Ctrl+R` - start voice capture
- `Ctrl+V` - toggle auto-voice (disabling cancels any running capture)
- `Ctrl+T` - toggle send mode (auto vs insert)
- `Ctrl+Y` - open theme picker
- `Ctrl+O` - open settings menu (use ↑↓←→ + Enter)
- `Ctrl+]` - increase mic threshold by 5 dB (less sensitive)
- `Ctrl+\` - decrease mic threshold by 5 dB (more sensitive; `Ctrl+/` also works)
- `?` - show shortcut help
- `Ctrl+Q` - exit overlay
- `Ctrl+C` - forwarded to the CLI
- `Enter` - in insert mode, stop capture early and transcribe what was captured

Full behavior notes and screenshots are in [docs/USAGE.md](docs/USAGE.md).

## 5) Common flags

```bash
voxterm --auto-voice
voxterm --voice-send-mode insert
voxterm --voice-vad-threshold-db -50
voxterm --mic-meter
voxterm --logs
voxterm --voice-max-capture-ms 60000 --voice-buffer-ms 60000
voxterm --transcript-idle-ms 250
voxterm --prompt-regex '^codex> $'
```

See [docs/CLI_FLAGS.md](docs/CLI_FLAGS.md) for the full CLI flag and env var list.

## 6) Need help?

- Install options: [docs/INSTALL.md](docs/INSTALL.md)
- Troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
