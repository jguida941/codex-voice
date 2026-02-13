# Troubleshooting

## Quick Fixes

| Problem | Fix (jump to details) |
|---------|------------------------|
| No speech detected | See [Status Messages → No speech detected](#no-speech-detected) |
| Voice not recording | See [Audio Setup → Check microphone permissions](#check-microphone-permissions) |
| Codex not responding | See [Codex Issues → Codex not responding](#codex-not-responding) |
| Auto-voice not triggering | See [Codex Issues → Auto-voice not triggering](#auto-voice-not-triggering) |
| Minimal HUD has no telemetry chip | See [Status Messages → Minimal HUD right-panel chip is missing](#minimal-hud-right-panel-chip-is-missing) |
| Startup splash lingers in IDE terminal | See [Startup Banner Lingers in IDE Terminal](#startup-banner-lingers-in-ide-terminal) |
| Theme colors look muted in IDE terminal | See [Theme Colors Look Muted in IDE Terminal](#theme-colors-look-muted-in-ide-terminal) |
| Full HUD appears multiple times in JetBrains terminal | See [HUD Duplicates in JetBrains Terminals](#hud-duplicates-in-jetbrains-terminals) |
| `PTY write failed: Input/output error` appears on exit | See [PTY Exit Write Error in Logs](#pty-exit-write-error-in-logs) |
| Voice macro not expanding | See [Status Messages → Voice macro not expanding](#voice-macro-not-expanding) |
| Voice macro expanded unexpectedly | See [Status Messages → Voice macro expanded unexpectedly](#voice-macro-expanded-unexpectedly) |
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

You may also see "Voice capture error (see log)" - use the same fixes below.

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
It will inject into the terminal when the next prompt appears (or after output
is idle for the transcript timeout). In auto mode, Enter is pressed for you.

**Fixes:**
1. Wait for the CLI to finish and return to a prompt
2. If you need to send immediately, stop the current response (usually `Ctrl+C`) and try again

### Voice macro not expanding

VoxTerm loads macros from `.voxterm/macros.yaml` in your current working
directory. If the file is missing, malformed, or the trigger does not match,
the transcript is sent as-is.

**Fixes:**
1. Confirm path: `<project>/.voxterm/macros.yaml`
2. Validate YAML shape:
   - `macros:`
   - trigger as key, string expansion or `{ template: ..., mode: auto|insert }`
3. Match trigger text exactly (case-insensitive, whitespace-insensitive)
4. Restart VoxTerm after editing the macro file

### Voice macro expanded unexpectedly

Macro expansion runs when **Settings -> Macros** is ON.
If you are dictating natural language, turn **Macros** OFF in Settings (`Ctrl+O`).

**Fixes:**
1. Open Settings (`Ctrl+O`)
2. Set **Macros** to **OFF**
3. Keep your preferred send mode (`auto` or `insert`) unchanged

### Minimal HUD right-panel chip is missing

Minimal HUD can now render compact right-panel telemetry chips (`Ribbon`,
`Dots`, `Heartbeat`). If the chip does not appear, one of the panel settings is
usually disabling it.

**Fixes:**
1. Open Settings (`Ctrl+O`) and set **Right panel** to `Ribbon`, `Dots`, or `Heartbeat` (not `Off`)
2. If **Anim only** is ON, start recording once to confirm the chip appears during recording
3. If you expect it while idle too, set **Anim only** to OFF

Compact HUD also adapts module emphasis by context and width:
- recording: meter + latency + queue (space permitting)
- backend busy: queue + latency
- idle: latency-focused compact view

### Latency badge looks inaccurate

The HUD latency badge represents post-capture processing time (mainly STT), not
the full time you spent talking. Recording duration is shown separately while
you are speaking.

If VoxTerm does not have enough metrics to estimate latency reliably, the badge
is hidden instead of showing a misleading number.

**Audit steps:**
1. Run with logs enabled: `voxterm --logs`
2. Reproduce one recording and inspect `${TMPDIR}/voxterm_tui.log`
3. Look for `latency_audit|display_ms=...|elapsed_ms=...|capture_ms=...|stt_ms=...`
4. For deeper profiling, run `./dev/scripts/tests/measure_latency.sh --voice-only --synthetic`

### Transcript queue full (oldest dropped)

You spoke 5+ times while Codex was busy. The oldest transcript was discarded.

**Fix:** Wait for Codex to finish before speaking again. The queue holds up to 5 transcripts.

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

**Hotkey range:** -80 dB (very sensitive) to -10 dB (less sensitive). Default: -55 dB.
The CLI flag accepts a wider range (-120 dB to 0 dB).

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
   Or run:
   ```bash
   voxterm --login --codex
   voxterm --login --claude
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

## IDE Terminal Controls Not Working (JetBrains/Cursor)

If HUD button clicks or arrow-based HUD focus works in one terminal app but not
another (for example RustRover/PyCharm/WebStorm):

1. Toggle the HUD with `Ctrl+U` and open Settings with `Ctrl+O` to confirm core
   shortcuts still work.
2. Capture input diagnostics:
   ```bash
   voxterm --logs
   VOXTERM_DEBUG_INPUT=1 voxterm --logs
   ```
3. Reproduce one failed click/arrow action and inspect `${TMPDIR}/voxterm_tui.log`
   for `input bytes (...)` and `input events: ...` lines.

VoxTerm now parses multiple mouse/arrow sequence variants used by different IDE
terminal emulators (SGR, URXVT, X10, and parameterized CSI arrows), but the
debug log above is still the fastest way to confirm what your terminal emits.

### HUD Duplicates in JetBrains Terminals

If you see stacked/repeated Full HUD frames in PyCharm/CLion/RustRover:

This is addressed in `v1.0.62` and newer using the stable `v1.0.53`
writer/render HUD path plus a one-column layout safety margin.

Symptoms can include:
- duplicated/staggered HUD borders
- repeated trailing `[back]` strips
- partial HUD rows left behind after redraw

1. Verify version is current:
   ```bash
   voxterm --version
   ```
2. Re-run once with logging:
   ```bash
   voxterm --logs
   ```
3. If it still reproduces, share `${TMPDIR}/voxterm_tui.log` so terminal escape
   handling can be confirmed for your profile.

### PTY Exit Write Error in Logs

If you see this on shutdown:

```text
failed to send PTY exit command: PTY write failed: Input/output error (os error 5)
```

That message came from a race where the backend PTY had already started closing.
It is treated as benign in `v1.0.60` and newer and should no longer be reported
as an error during normal exit.

---

## Startup Banner Missing

The startup splash is shown by default. If it does not appear, confirm the
environment variable below is not set:

```bash
env | rg VOXTERM_NO_STARTUP_BANNER
```

To explicitly hide it (useful in scripts), set:

```bash
VOXTERM_NO_STARTUP_BANNER=1 voxterm
```

### Startup Banner Lingers in IDE Terminal

If the startup splash stays on screen in PyCharm/JetBrains terminals while it
clears normally in Cursor/VS Code, use these checks:

1. Run with a zero splash delay:
   ```bash
   VOXTERM_STARTUP_SPLASH_MS=0 voxterm
   ```
2. If you prefer no splash in IDE terminals:
   ```bash
   VOXTERM_NO_STARTUP_BANNER=1 voxterm
   ```

### Theme Colors Look Muted in IDE Terminal

Some IDE terminals do not export `COLORTERM=truecolor`, which can make color
themes look like ANSI fallbacks.

**Checks:**
1. Inspect terminal env:
   ```bash
   env | rg 'COLORTERM|TERM|TERM_PROGRAM|TERMINAL_EMULATOR|NO_COLOR'
   ```
2. Ensure `NO_COLOR` is not set.
3. Force truecolor for a quick A/B check:
   ```bash
   COLORTERM=truecolor voxterm --theme catppuccin
   ```

If forced truecolor fixes appearance, you can keep using that override for that
terminal profile. Current builds intentionally use conservative fallback:
truecolor themes resolve to `ansi` unless truecolor capability is explicitly
detected, to avoid broken rendering in IDE terminals with partial color support.

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

Trace log (JSON, written when `--logs` is enabled): system temp dir (for example
`${TMPDIR}/voxterm_trace.jsonl` on macOS or `/tmp/voxterm_trace.jsonl` on Linux).
Override with `VOXTERM_TRACE_LOG`.

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

Whisper supports many languages. VoxTerm has been tested with English (`en`).

Other languages should work but are untested. Use `--lang auto` for automatic
detection, or specify a language code: `voxterm --lang es`

Full list: [Whisper supported languages](https://github.com/openai/whisper#available-models-and-languages)

### Which AI CLI backends work?

Only Codex and Claude Code are tested. Other presets exist but are experimental.

| Backend | Install | Run | Status |
|---------|---------|-----|--------|
| Codex | `npm install -g @openai/codex` | `voxterm` | Tested |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `voxterm --claude` | Tested |
| Gemini CLI | See vendor docs | `voxterm --gemini` | Experimental (currently not working) |
| Aider | See vendor docs | `voxterm --backend aider` | Experimental (untested) |
| OpenCode | See vendor docs | `voxterm --backend opencode` | Experimental (untested) |

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
