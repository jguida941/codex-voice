# Codex Voice Quick Start (Overlay Mode)

This is the shortest path to run Codex with voice in your terminal.
Supported on macOS and Linux (use WSL2 if you are on Windows).

## 1) Prerequisites

- Install Codex CLI:

```bash
npm install -g @openai/codex
```

- Install Rust (if you do not have it): https://rustup.rs

## 2) Install Codex Voice

```bash
git clone https://github.com/jguida941/codex-voice.git
cd codex-voice
./install.sh
```

If `codex-voice` is not found, see PATH and install notes in
[docs/INSTALL.md](docs/INSTALL.md).

## 3) Run from any project

```bash
cd ~/my-project
codex-voice
```

First run downloads a Whisper model if missing.

## 4) Essential controls

- `Ctrl+R` - start voice capture
- `Ctrl+V` - toggle auto-voice (disabling cancels any running capture)
- `Ctrl+T` - toggle send mode (auto vs insert)
- `Ctrl+]` - increase mic threshold by 5 dB (less sensitive)
- `Ctrl+\` - decrease mic threshold by 5 dB (more sensitive; `Ctrl+/` also works)
- `Ctrl+Q` - exit overlay
- `Ctrl+C` - forwarded to Codex
- `Enter` - in insert mode, stop capture early and transcribe what was captured

Full behavior notes and screenshots are in [docs/USAGE.md](docs/USAGE.md).

## 5) Common flags

```bash
codex-voice --auto-voice
codex-voice --voice-send-mode insert
codex-voice --voice-vad-threshold-db -50
codex-voice --mic-meter
codex-voice --logs
codex-voice --voice-max-capture-ms 60000 --voice-buffer-ms 60000
codex-voice --transcript-idle-ms 250
codex-voice --prompt-regex '^codex> $'
```

See [docs/CLI_FLAGS.md](docs/CLI_FLAGS.md) for the full CLI flag and env var list.

## 6) Need help?

- Install options: [docs/INSTALL.md](docs/INSTALL.md)
- Troubleshooting: [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
