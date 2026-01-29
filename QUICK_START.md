# Codex Voice Quick Start (Overlay Mode)

This is the shortest path to run Codex with voice in your terminal.

Supported on macOS and Linux (use WSL2 if you are on Windows).

## 1) One-time setup

- Install Codex CLI:

```bash
npm install -g @anthropic-ai/codex
```

- Install Rust (if you don't have it): https://rustup.rs

## 2) Install Codex Voice (one time)

```bash
git clone https://github.com/jguida941/codex-voice.git
cd codex-voice
./install.sh
```

If `codex-voice` is not found, the installer used the first writable directory in this order:
`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, or `/path/to/codex-voice/bin`. Add that
directory to PATH or set `CODEX_VOICE_INSTALL_DIR` before running `./install.sh`. If a
`codex-voice` command already exists, the installer skips that location; remove the conflicting
binary or set `CODEX_VOICE_INSTALL_DIR` to override.

## 3) Run from any project

```bash
cd ~/my-project
codex-voice
```

First run will download a Whisper model if missing, then start the Rust overlay in your current folder.

## 4) Voice controls

- `Ctrl+R` - start voice capture
- `Ctrl+V` - toggle auto-voice (disabling cancels any running capture)
- `Ctrl+T` - toggle send mode (auto vs insert)
- `Ctrl+]` - increase mic threshold by 5 dB (less sensitive)
- `Ctrl+\` - decrease mic threshold by 5 dB (more sensitive; `Ctrl+/` also works)
- `Ctrl+Q` - exit overlay
- `Ctrl+C` - forwarded to Codex
- `Enter` - in insert mode, stop capture early and transcribe what was captured

Auto-voice keeps listening on silence; press `Ctrl+V` to stop auto-voice mode. The status line stays on "Auto-voice enabled" while waiting.
If Codex is busy, voice transcripts are queued and sent on the next prompt or after a short idle period.
Long dictation is chunked by `--voice-max-capture-ms` (default 30s, max 60s). Use insert mode for continuous dictation while Codex is busy.

## Common flags

```bash
codex-voice --auto-voice
codex-voice --voice-send-mode insert
codex-voice --voice-vad-threshold-db -50
codex-voice --mic-meter
codex-voice --voice-max-capture-ms 60000 --voice-buffer-ms 60000
codex-voice --transcript-idle-ms 250
codex-voice --prompt-regex '^codex> $'
```

## Homebrew (optional)

Install Homebrew (if needed):

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

```bash
brew tap jguida941/homebrew-codex-voice
brew install codex-voice
```

Run from any project (first run downloads the model if missing):

```bash
cd ~/my-project
codex-voice
```

Optional pre-download:

```bash
$(brew --prefix)/opt/codex-voice/libexec/scripts/setup.sh models --base
```

## Troubleshooting

- If voice falls back to Python and fails, install native Whisper model and run again:
  `./scripts/setup.sh models --base`
- Force native Whisper only:
  `./start.sh --no-python-fallback`
- If Homebrew cannot link `codex-voice` because it already exists:
  `brew link --overwrite codex-voice` or remove the conflicting binary.
- If `codex-voice` still shows an older version after a brew update, you likely have another
  install earlier in PATH (often `~/.local/bin/codex-voice` from `./install.sh`). Check and remove it:
  `which -a codex-voice` then rename/delete the older one and run `hash -r`.
- Verify the Homebrew binary directly:
  `$(brew --prefix)/opt/codex-voice/libexec/rust_tui/target/release/codex-voice --version`
- Logs: `${TMPDIR}/codex_voice_tui.log`
- Prompt detection log: `${TMPDIR}/codex_voice_prompt.log`
