# Troubleshooting

## Quick Fixes

| Problem | Fix (jump to details) |
|---------|------------------------|
| No speech detected | See [Status Messages → No speech detected](#no-speech-detected) |
| Voice not recording | See [Audio Setup → Check microphone permissions](#check-microphone-permissions) |
| Codex not responding | See [Codex Issues → Codex not responding](#codex-not-responding) |
| Auto-voice not triggering | See [Codex Issues → Auto-voice not triggering](#auto-voice-not-triggering) |
| Wrong version after update | See [Install Issues → Wrong version after update](#wrong-version-after-update) |

Other sections: [Status Messages](#status-messages) · [Audio Setup](#audio-setup) ·
[Mic Sensitivity](#mic-sensitivity) · [Codex Issues](#codex-issues) ·
[Install Issues](#install-issues) · [Enabling Logs](#enabling-logs) ·
[Getting Help](#getting-help)

---

## Status Messages

### No speech detected

The mic recorded but no voice was heard above the threshold.

**Fixes:**
1. Speak louder or closer to the mic
2. Lower the threshold: press `Ctrl+\` (or `Ctrl+/`) to make it more sensitive
3. Run `codex-voice --mic-meter` to calibrate for your environment

### Voice capture failed (see log)

The mic couldn't start recording.

**Fixes:**
1. Check mic permissions for your terminal app
2. Run `codex-voice --list-input-devices` to see available mics
3. Try a specific device: `codex-voice --input-device "Your Mic Name"`
4. Enable logs to see details: `codex-voice --logs`

### Voice capture error (see log)

Something went wrong during recording or transcription.

**Fixes:**
1. Enable logs: `codex-voice --logs`
2. Check the log at `${TMPDIR}/codex_voice_tui.log`
3. Restart `codex-voice`

### Processing... (stuck)

Transcription is taking too long.

**Fixes:**
1. Wait up to 60 seconds (large audio takes time)
2. If still stuck, press `Ctrl+C` then restart `codex-voice`
3. Try a smaller Whisper model

### Transcript queue full (oldest dropped)

You spoke 5+ times while Codex was busy. Oldest transcript was discarded.

**Fix:** Wait for Codex to finish before speaking again. Queue flushing is unreliable and tracked in the [master plan](active/MASTER_PLAN.md).

### Voice capture already running

You pressed `Ctrl+R` while already recording.

**Fix:** Wait for the current recording to finish, or enable auto-voice (`Ctrl+V`) so you don't need to press `Ctrl+R`.

### Python pipeline

Native Whisper isn't available, using slower Python fallback. 

**Fixes:**
1. Verify model exists: `ls models/ggml-*.bin`
2. Download model: `./scripts/setup.sh models --base`
3. Or install Python dependencies: `python3`, `ffmpeg`, `whisper` CLI

---

## Audio Setup

### Check microphone permissions

**macOS:** System Settings → Privacy & Security → Microphone → Enable for your terminal app (Terminal, iTerm2, etc.)

**Linux:** Ensure your user has access to PulseAudio/PipeWire. Check with `pactl list sources`.

### Verify Whisper model exists

```bash
ls models/ggml-*.bin
```

If missing:
```bash
./scripts/setup.sh models --base
```

### List and select audio devices

```bash
codex-voice --list-input-devices
```

Use a specific device:
```bash
codex-voice --input-device "MacBook Pro Microphone"
```

### Microphone changed or unplugged

Restart `codex-voice` after plugging in a new mic. Devices are detected at startup.

---

## Mic Sensitivity

### Too sensitive (picks up background noise)

Press `Ctrl+]` to raise the threshold (less sensitive). Repeat until background noise stops triggering recordings.

Or set it at startup:
```bash
codex-voice --voice-vad-threshold-db -30
```

### Not sensitive enough (misses your voice)

Press `Ctrl+\` (or `Ctrl+/`) to lower the threshold (more sensitive).

Or set it at startup:
```bash
codex-voice --voice-vad-threshold-db -50
```

### Find the right threshold

Run the mic meter to measure your environment:
```bash
codex-voice --mic-meter
```

It samples ambient noise and your speech, then suggests a threshold.

**Range:** -80 dB (very sensitive) to -10 dB (less sensitive). Default: -40 dB.

---

## Codex Issues

If you're using `--backend` with a different AI CLI, substitute that CLI's command wherever you see `codex` below.

### Codex not responding

1. Verify Codex CLI is installed:
   ```bash
   which codex
   ```

2. Check authentication:
   ```bash
   codex login
   ```

3. If the session is stuck, restart `codex-voice`.

---

### Auto-voice not triggering

Auto-voice waits for the CLI to show a prompt before listening. If detection fails (especially on non-Codex backends):

#### Override prompt detection

```bash
codex-voice --prompt-regex '^codex> $'
```

Adjust the regex to match your actual prompt.

#### Enable prompt logging

```bash
codex-voice --prompt-log /tmp/codex_voice_prompt.log
```

Check the log to see what lines are being detected.

---

## Install Issues

### Homebrew link conflict

If `brew install codex-voice` fails because the command already exists:

```bash
brew link --overwrite codex-voice
```

### Wrong version after update

Check for duplicate installs:
```bash
which -a codex-voice
```

Remove or rename the old one (often `~/.local/bin/codex-voice` from `./install.sh`):
```bash
mv ~/.local/bin/codex-voice ~/.local/bin/codex-voice.bak
hash -r
```

See [INSTALL.md](INSTALL.md) for full PATH cleanup steps.

---

## Enabling Logs

Logs are disabled by default for privacy.

### Enable debug logging

```bash
codex-voice --logs
```

### Include transcript snippets in logs

```bash
codex-voice --logs --log-content
```

### Log file location

Debug log: system temp dir (for example `${TMPDIR}/codex_voice_tui.log` on macOS or `/tmp/codex_voice_tui.log` on Linux)

### Disable all logging

```bash
codex-voice --no-logs
```

---

## Getting Help

- **Report bugs:** [GitHub Issues](https://github.com/jguida941/codex-voice/issues)
- **Check known issues:** [Master Plan](active/MASTER_PLAN.md)

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Install | [INSTALL.md](INSTALL.md) |
| Usage | [USAGE.md](USAGE.md) |
| CLI Flags | [CLI_FLAGS.md](CLI_FLAGS.md) |
