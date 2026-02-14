# VoiceTerm Quick Start

Get voice input for your AI CLI in under 2 minutes.
Works on macOS and Linux (Windows needs WSL2).

## 1) Install Codex CLI (default backend)

```bash
npm install -g @openai/codex
```

Or use another AI CLI: `voiceterm --claude`.

## 2) Install VoiceTerm

**Homebrew (recommended):**
```bash
brew tap jguida941/voiceterm
brew install voiceterm
```

**PyPI (pipx):**
```bash
pipx install voiceterm
```

**From source:**
```bash
git clone https://github.com/jguida941/voiceterm.git
cd voiceterm
./scripts/install.sh
```

**macOS App:** Double-click **app/macos/VoiceTerm.app** and choose your project folder.

If `voiceterm` is not found after install, see [guides/INSTALL.md](guides/INSTALL.md) for PATH notes.

## 3) Run from any project

```bash
cd ~/my-project
voiceterm
```

If you haven't authenticated with your backend CLI yet:

```bash
voiceterm --login --codex
voiceterm --login --claude
```

First run downloads a Whisper model if missing.
To pick a size, use `./scripts/install.sh --small` or
`./scripts/setup.sh models --medium`.

Codex is the default backend; `voiceterm --codex` is optional if you want to be explicit.

To target Claude instead of Codex:

```bash
voiceterm --claude
```

## 4) Essential controls

- `Ctrl+R` - start voice capture
- `Ctrl+V` - toggle auto-voice (disabling cancels any running capture)
- `Ctrl+T` - toggle send mode (auto vs insert)
- `Ctrl+Y` - open theme picker
- `Ctrl+O` - open settings menu (use ↑↓←→ + Enter)
- `Ctrl+U` - cycle HUD style (full/minimal/hidden)
- `Ctrl+]` - increase mic threshold by 5 dB (less sensitive)
- `Ctrl+\` - decrease mic threshold by 5 dB (more sensitive; `Ctrl+/` also works)
- `?` - show shortcut help
- `Ctrl+Q` - exit overlay
- `Ctrl+C` - forwarded to the CLI
- `Enter` - in insert mode, stop capture early and transcribe what was captured

Full behavior notes and screenshots are in [guides/USAGE.md](guides/USAGE.md).

Send mode note: "auto" types your words and presses Enter. "Insert" types your words
but lets you press `Enter` yourself. VoiceTerm only writes to the terminal (PTY) and
does not call Codex/Claude directly.

Macros note: in Settings (`Ctrl+O`), `Macros` controls whether transcript text is
expanded via `.voiceterm/macros.yaml` before injection.
Visual note: right-panel telemetry modes (`Ribbon`, `Dots`, `Heartbeat`) show in
Minimal HUD as compact chips when enabled in Settings.
Compact HUD telemetry also adapts by context (recording/busy/idle).
IDE note (`v1.0.64`): Full HUD uses the stable `v1.0.53` writer/render path and
clears stale HUD rows on resize to prevent duplicate/ghost frames in
Cursor/JetBrains terminals.
Startup splash note: default dwell is short (`VOICETERM_STARTUP_SPLASH_MS=1500`).
Use `VOICETERM_STARTUP_SPLASH_MS=0` for immediate clear, or
`VOICETERM_NO_STARTUP_BANNER=1` to skip it. JetBrains IDE terminals auto-skip
the splash by default.

## 5) Common flags

```bash
voiceterm --auto-voice
voiceterm --voice-send-mode insert
voiceterm --voice-vad-threshold-db -50
voiceterm --mic-meter
voiceterm --logs
voiceterm --latency-display label
voiceterm --voice-max-capture-ms 60000 --voice-buffer-ms 60000
voiceterm --transcript-idle-ms 250
voiceterm --prompt-regex '^codex> $'
```

See [guides/CLI_FLAGS.md](guides/CLI_FLAGS.md) for the full CLI flag and env var list.

## 6) Need help?

- Install options: [guides/INSTALL.md](guides/INSTALL.md)
- Troubleshooting: [guides/TROUBLESHOOTING.md](guides/TROUBLESHOOTING.md)
