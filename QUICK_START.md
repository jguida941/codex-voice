# VoxTerm Quick Start

Get voice input for your AI CLI in under 2 minutes.
Works on macOS and Linux (Windows needs WSL2).

## 1) Install Codex CLI (default backend)

```bash
npm install -g @openai/codex
```

Or use another AI CLI: `voxterm --claude`.

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
./scripts/install.sh
```

**macOS App:** Double-click **app/macos/VoxTerm.app** and choose your project folder.

If `voxterm` is not found after install, see [guides/INSTALL.md](guides/INSTALL.md) for PATH notes.

## 3) Run from any project

```bash
cd ~/my-project
voxterm
```

First run downloads a Whisper model if missing.
To pick a size, use `./scripts/install.sh --small` or
`./scripts/setup.sh models --medium`.

Codex is the default backend; `voxterm --codex` is optional if you want to be explicit.

To target Claude instead of Codex:

```bash
voxterm --claude
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

Full behavior notes and screenshots are in [guides/USAGE.md](guides/USAGE.md).

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

See [guides/CLI_FLAGS.md](guides/CLI_FLAGS.md) for the full CLI flag and env var list.

## 6) Need help?

- Install options: [guides/INSTALL.md](guides/INSTALL.md)
- Troubleshooting: [guides/TROUBLESHOOTING.md](guides/TROUBLESHOOTING.md)
