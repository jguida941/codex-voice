# Troubleshooting

## Voice not working

1. Check microphone permissions for your terminal (macOS: System Settings > Privacy & Security > Microphone; Windows: Settings > Privacy & Security > Microphone; Linux: PipeWire/PulseAudio permissions).
2. Verify a Whisper model exists (or run `./scripts/setup.sh models --base`).
3. List devices: `codex-voice --list-input-devices`.
4. Run the mic meter: `codex-voice --mic-meter` to get a recommended VAD threshold.
5. If you unplug or change microphones while running, restart `codex-voice` to re-detect devices.

## Python fallback issues

If the status line shows "Python pipeline" and transcription fails:

- Install `python3`, `ffmpeg`, and the `whisper` CLI.
- Or force native Whisper only: `./start.sh --no-python-fallback`.

## Codex not responding

1. Ensure Codex CLI is installed: `which codex`.
2. Check authentication: `codex login`.
3. Restart `codex-voice` if the PTY session is stuck.

## Auto-voice not triggering or prompt detection issues

- Prompt detection is auto-learned by default. If it guesses wrong, override it:
  `codex-voice --prompt-regex '^codex> $'`.
- Enable a prompt log (opt-in) to see detections:
  `codex-voice --prompt-log /tmp/codex_voice_prompt.log`.

## Homebrew link conflict

If `brew install codex-voice` cannot link because `codex-voice` already exists:

```bash
brew link --overwrite codex-voice
```

## Older version still running

If `codex-voice` still shows an older version after a brew update, you likely have another
install earlier in PATH (often `~/.local/bin/codex-voice` from `./install.sh`). See
[docs/INSTALL.md](INSTALL.md) for the full PATH cleanup steps and Homebrew cache refresh.

## Logs

- Logs are disabled by default. Enable them with `codex-voice --logs` (add `--log-content` for prompt/transcript snippets).
- `--no-logs` disables all logging, including prompt logs.
- Debug log: `${TMPDIR}/codex_voice_tui.log` (created only when logs are enabled).
- Prompt log: only when `--prompt-log` or `CODEX_VOICE_PROMPT_LOG` is set (and logs are not disabled).
