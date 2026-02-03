# Troubleshooting

## Quick Fixes

| Problem | Fix (jump to details) |
|---------|------------------------|
| No speech detected | See [Status Messages → No speech detected](#no-speech-detected) |
| Voice not recording | See [Audio Setup → Check microphone permissions](#check-microphone-permissions) |
| Codex not responding | See [Codex Issues → Codex not responding](#codex-not-responding) |
| Auto-voice not triggering | See [Codex Issues → Auto-voice not triggering](#auto-voice-not-triggering) |
| Wrong version after update | [Install Issues → Wrong version after update](#wrong-version-after-update) |

Other sections: [Status Messages](#status-messages) · [Audio Setup](#audio-setup) ·
[Mic Sensitivity](#mic-sensitivity) · [Codex Issues](#codex-issues) ·
[Install Issues](#install-issues) · [Enabling Logs](#enabling-logs) ·
[FAQ](#faq) · [Getting Help](#getting-help)

---

## Status Messages

### No speech detected

The mic recorded but no voice was heard above the threshold.

**Fixes:**
1. Speak louder or closer to the mic
2. Lower the threshold: press `Ctrl+\` (or `Ctrl+/`) to make it more sensitive
3. Run `voxterm --mic-meter` to calibrate for your environment

### Voice capture failed (see log)

The mic couldn't start recording.

**Fixes:**
1. Check mic permissions for your terminal app
2. Run `voxterm --list-input-devices` to see available mics
3. Try a specific device: `voxterm --input-device "Your Mic Name"`
4. Enable logs to see details: `voxterm --logs`

### Voice capture error (see log)

Something went wrong during recording or transcription.

**Fixes:**
1. Enable logs: `voxterm --logs`
2. Check the log at `${TMPDIR}/voxterm_tui.log`
3. Restart `voxterm`

### Processing... (stuck)

Transcription is taking too long.

**Fixes:**
1. Wait up to 60 seconds (large audio takes time)
2. If still stuck, press `Ctrl+C` then restart `voxterm`
3. Try a smaller Whisper model

### Transcript queued (N)

The CLI is still streaming output, so VoxTerm queued your transcript.
It will send when the next prompt appears (or after output is idle for the
transcript timeout).

**Fixes:**
1. Wait for the CLI to finish and return to a prompt
2. If you need to send immediately, stop the current response (usually `Ctrl+C`) and try again

### Transcript queue full (oldest dropped)

You spoke 5+ times while Codex was busy. Oldest transcript was discarded.

**Fix:** Wait for Codex to finish before speaking again.
Queue flushing is unreliable and tracked in the
[master plan](../dev/active/MASTER_PLAN.md).

### Voice capture already running

You pressed `Ctrl+R` while already recording.

**Fix:** Wait for the current recording to finish, or enable auto-voice
(`Ctrl+V`) so you don't need to press `Ctrl+R`.

### Python pipeline

Native Whisper isn't available, using slower Python fallback.

**Fixes:**
1. Verify model exists: `ls whisper_models/ggml-*.bin`
2. Download model: `./scripts/setup.sh models --base`
3. Or install Python dependencies: `python3`, `ffmpeg`, `whisper` CLI

---

## Audio Setup

### Check microphone permissions

**macOS:** System Settings → Privacy & Security → Microphone → Enable for your
terminal app (Terminal, iTerm2, etc.)

**Linux:** Ensure your user has access to PulseAudio/PipeWire. Check with `pactl list sources`.

### Verify Whisper model exists

```bash
ls whisper_models/ggml-*.bin
```

If missing:
```bash
./scripts/setup.sh models --base
```

### List and select audio devices

```bash
voxterm --list-input-devices
```

Use a specific device:
```bash
voxterm --input-device "MacBook Pro Microphone"
```

### Microphone changed or unplugged

Restart `voxterm` after plugging in a new mic. Devices are detected at startup.

---

## Mic Sensitivity

### Too sensitive (picks up background noise)

Press `Ctrl+]` to raise the threshold (less sensitive).
Repeat until background noise stops triggering recordings.

Or set it at startup:
```bash
voxterm --voice-vad-threshold-db -30
```

### Not sensitive enough (misses your voice)

Press `Ctrl+\` (or `Ctrl+/`) to lower the threshold (more sensitive).

Or set it at startup:
```bash
voxterm --voice-vad-threshold-db -50
```

### Find the right threshold

Run the mic meter to measure your environment:
```bash
voxterm --mic-meter
```

It samples ambient noise and your speech, then suggests a threshold.

**Range:** -80 dB (very sensitive) to -10 dB (less sensitive). Default: -55 dB.

---

## Codex Issues

If you're using Claude Code, substitute `claude` wherever you see `codex` below.

### Codex not responding

1. Verify Codex CLI is installed:
   ```bash
   which codex
   ```

2. Check authentication:
   ```bash
   codex login
   ```

3. If the session is stuck, restart `voxterm`.

---

### Auto-voice not triggering

Auto-voice waits for the CLI to show a prompt before listening.
If detection fails (especially on Claude with a custom prompt):

#### Override prompt detection

```bash
voxterm --prompt-regex '^codex> $'
```

Adjust the regex to match your actual prompt.

#### Enable prompt logging

```bash
voxterm --prompt-log /tmp/voxterm_prompt.log
```

Check the log to see what lines are being detected.

---

## Install Issues

### Homebrew link conflict

If `brew install voxterm` fails because the command already exists:

```bash
brew link --overwrite voxterm
```

### Wrong version after update

Start with a normal update:
```bash
brew update
brew upgrade voxterm
```

If Homebrew still shows the old version, force a tap refresh:
```bash
brew untap jguida941/voxterm 2>/dev/null || true
brew untap jguida941/homebrew-voxterm 2>/dev/null || true
brew tap jguida941/voxterm
brew update
brew info voxterm
```

If it still will not update, clear the cache and reinstall:
```bash
rm -f "$(brew --cache)"/voxterm--*
brew reinstall voxterm
```

If `voxterm --version` still looks old, check for duplicate installs earlier in PATH:
```bash
which -a voxterm
```

Remove or rename the old one (often `~/.local/bin/voxterm` from `./scripts/install.sh`):
```bash
mv ~/.local/bin/voxterm ~/.local/bin/voxterm.bak
hash -r
```

If you used a local install previously, also check for:
```bash
ls -l ~/voxterm/bin/voxterm 2>/dev/null
```

Relink Homebrew and clear shell caches:
```bash
brew unlink voxterm && brew link --overwrite voxterm
hash -r
```

Verify the Homebrew binary directly (bypasses the wrapper):
```bash
$(brew --prefix)/opt/voxterm/libexec/src/target/release/voxterm --version
```

---

## Enabling Logs

Logs are disabled by default for privacy.

### Enable debug logging

```bash
voxterm --logs
```

### Include transcript snippets in logs

```bash
voxterm --logs --log-content
```

### Log file location

Debug log: system temp dir (for example `${TMPDIR}/voxterm_tui.log` on macOS or
`/tmp/voxterm_tui.log` on Linux)

Crash log (panic only, written when `--logs` is enabled; metadata unless
`--log-content`): system temp dir (for example `${TMPDIR}/voxterm_crash.log`
on macOS or `/tmp/voxterm_crash.log` on Linux)

### Disable all logging

```bash
voxterm --no-logs
```

---

## Getting Help

- **Collect diagnostics:** Run `voxterm --doctor` and include the output in your issue.
- **Report bugs:** [GitHub Issues](https://github.com/jguida941/voxterm/issues)
- **Check known issues:** [Master Plan](../dev/active/MASTER_PLAN.md)

---

## FAQ

### What languages does Whisper support?

Whisper supports 99 languages. VoxTerm has been tested with English (`en`).

Other languages should work but are untested. Use `--lang auto` for automatic
detection, or specify a language code: `voxterm --lang es`

Full list: [Whisper supported languages](https://github.com/openai/whisper#available-models-and-languages)

### Which AI CLI backends work?

| Backend | Install | Run | Status |
|---------|---------|-----|--------|
| Codex | `npm install -g @openai/codex` | `voxterm` | Tested |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `voxterm --claude` | Tested |

### Which Whisper model should I use?

See the full [Whisper guide](WHISPER.md) for model comparison and language support.

Quick answer: Start with `base` (142 MB, fast). Use `small` or `medium` for better accuracy.

```bash
voxterm --whisper-model base
```

### Can I use VoxTerm without Codex?

Yes. Use Claude Code:
```bash
voxterm --claude
```

### Does VoxTerm send my voice to the cloud?

No. All speech-to-text happens locally via Whisper. Your audio never leaves your machine.

### How do I update VoxTerm?

**Homebrew:**
```bash
brew update && brew upgrade voxterm
```

**From source:**
```bash
cd voxterm && git pull && ./scripts/install.sh
```

---

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Install | [INSTALL.md](INSTALL.md) |
| Usage | [USAGE.md](USAGE.md) |
| CLI Flags | [CLI_FLAGS.md](CLI_FLAGS.md) |
| Whisper & Languages | [WHISPER.md](WHISPER.md) |
