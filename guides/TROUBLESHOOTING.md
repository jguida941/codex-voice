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
| Many `codex`/`claude` processes remain after quitting | See [Codex Issues → Many codex/claude processes remain after quitting](#many-codexclaude-processes-remain-after-quitting) |
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
3. Run `voiceterm --mic-meter` to calibrate for your environment

### Voice capture failed (see log)

You may also see "Voice capture error (see log)" - use the same fixes below.

The mic couldn't start recording.

**Fixes:**
1. Check mic permissions for your terminal app
2. Run `voiceterm --list-input-devices` to see available mics
3. Try a specific device: `voiceterm --input-device "Your Mic Name"`
4. Enable logs to see details: `voiceterm --logs`

### Voice capture error (see log)

Something went wrong during recording or transcription.

**Fixes:**
1. Enable logs: `voiceterm --logs`
2. Check the log at `${TMPDIR}/voiceterm_tui.log`
3. Restart `voiceterm`

### Processing... (stuck)

Transcription is taking too long.

**Fixes:**
1. Wait up to 60 seconds (large audio takes time)
2. If still stuck, press `Ctrl+C` then restart `voiceterm`
3. Try a smaller Whisper model

### Transcript queued (N)

The CLI is still streaming output, so VoiceTerm queued your transcript.
It will inject into the terminal when the next prompt appears (or after output
is idle for the transcript timeout). In auto mode, Enter is pressed for you.

**Fixes:**
1. Wait for the CLI to finish and return to a prompt
2. If you need to send immediately, stop the current response (usually `Ctrl+C`) and try again

### Voice macro not expanding

VoiceTerm loads macros from `.voiceterm/macros.yaml` in your current working
directory. If the file is missing, malformed, or the trigger does not match,
the transcript is sent as-is.

**Fixes:**
1. Confirm path: `<project>/.voiceterm/macros.yaml`
2. Validate YAML shape:
   - `macros:`
   - trigger as key, string expansion or `{ template: ..., mode: auto|insert }`
3. Match trigger text exactly (case-insensitive, whitespace-insensitive)
4. Restart VoiceTerm after editing the macro file

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

If VoiceTerm does not have enough metrics to estimate latency reliably, the badge
is hidden instead of showing a misleading number.

**Audit steps:**
1. Run with logs enabled: `voiceterm --logs`
2. Reproduce one recording and inspect `${TMPDIR}/voiceterm_tui.log`
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
voiceterm --list-input-devices
```

Use a specific device:
```bash
voiceterm --input-device "MacBook Pro Microphone"
```

### Microphone changed or unplugged

Restart `voiceterm` after plugging in a new mic. Devices are detected at startup.

---

## Mic Sensitivity

### Too sensitive (picks up background noise)

Press `Ctrl+]` to raise the threshold (less sensitive).
Repeat until background noise stops triggering recordings.

Or set it at startup:
```bash
voiceterm --voice-vad-threshold-db -30
```

### Not sensitive enough (misses your voice)

Press `Ctrl+\` (or `Ctrl+/`) to lower the threshold (more sensitive).

Or set it at startup:
```bash
voiceterm --voice-vad-threshold-db -50
```

### Find the right threshold

Run the mic meter to measure your environment:
```bash
voiceterm --mic-meter
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
   voiceterm --login --codex
   voiceterm --login --claude
   ```

3. If the session is stuck, restart `voiceterm`.

---

### Many codex/claude processes remain after quitting

Recent builds terminate the backend PTY process group (not only the direct child)
and reap piped child processes on cancel/exit. If you still observe many
leftover processes, verify the binary version and check for true orphans.

1. Confirm version:
   ```bash
   voiceterm --version
   ```
2. List orphaned backend processes:
   ```bash
   ps -axo ppid,pid,command | egrep '(^ *1 .*\\b(codex|claude)\\b)'
   ```
3. If orphaned entries remain after exiting VoiceTerm, file an issue with:
   - `voiceterm --version`
   - terminal/IDE name and version
   - exact launch command
   - relevant `${TMPDIR}/voiceterm_tui.log` lines

---

### Auto-voice not triggering

Auto-voice waits for the CLI to show a prompt before listening.
If detection fails (especially on Claude with a custom prompt):

#### Override prompt detection

```bash
voiceterm --prompt-regex '^codex> $'
```

Adjust the regex to match your actual prompt.

#### Enable prompt logging

```bash
voiceterm --prompt-log /tmp/voiceterm_prompt.log
```

Check the log to see what lines are being detected.

---

## Install Issues

### Homebrew link conflict

If `brew install voiceterm` fails because the command already exists:

```bash
brew link --overwrite voiceterm
```

### Wrong version after update

Start with a normal update:
```bash
brew update
brew upgrade voiceterm
```

If Homebrew still shows the old version, force a tap refresh:
```bash
brew untap jguida941/voiceterm 2>/dev/null || true
brew untap jguida941/homebrew-voiceterm 2>/dev/null || true
brew tap jguida941/voiceterm
brew update
brew info voiceterm
```

If it still will not update, clear the cache and reinstall:
```bash
rm -f "$(brew --cache)"/voiceterm--*
brew reinstall voiceterm
```

If `voiceterm --version` still looks old, check for duplicate installs earlier in PATH:
```bash
which -a voiceterm
```

Remove or rename the old one (often `~/.local/bin/voiceterm` from `./scripts/install.sh`):
```bash
mv ~/.local/bin/voiceterm ~/.local/bin/voiceterm.bak
hash -r
```

If you used a local install previously, also check for:
```bash
ls -l ~/voiceterm/bin/voiceterm 2>/dev/null
```

Relink Homebrew and clear shell caches:
```bash
brew unlink voiceterm && brew link --overwrite voiceterm
hash -r
```

Verify the Homebrew binary directly (bypasses the wrapper):
```bash
$(brew --prefix)/opt/voiceterm/libexec/src/target/release/voiceterm --version
```

---

## IDE Terminal Controls Not Working (JetBrains/Cursor)

If HUD button clicks or arrow-based HUD focus works in one terminal app but not
another (for example RustRover/PyCharm/WebStorm):

1. Toggle the HUD with `Ctrl+U` and open Settings with `Ctrl+O` to confirm core
   shortcuts still work.
2. Capture input diagnostics:
   ```bash
   voiceterm --logs
   VOICETERM_DEBUG_INPUT=1 voiceterm --logs
   ```
3. Reproduce one failed click/arrow action and inspect `${TMPDIR}/voiceterm_tui.log`
   for `input bytes (...)` and `input events: ...` lines.

VoiceTerm now parses multiple mouse/arrow sequence variants used by different IDE
terminal emulators (SGR, URXVT, X10, and parameterized CSI arrows), but the
debug log above is still the fastest way to confirm what your terminal emits.

### HUD Duplicates in JetBrains Terminals

If you see stacked/repeated Full HUD frames in PyCharm/CLion/RustRover:

This is addressed in `v1.0.64` and newer using the stable `v1.0.53`
writer/render HUD path plus writer-side stale-row cleanup on resize.

Symptoms can include:
- duplicated/staggered HUD borders
- repeated trailing `[back]` strips
- partial HUD rows left behind after redraw

1. Verify version is current:
   ```bash
   voiceterm --version
   ```
2. Re-run once with logging:
   ```bash
   voiceterm --logs
   ```
3. If it still reproduces, share `${TMPDIR}/voiceterm_tui.log` so terminal escape
   handling can be confirmed for your profile.

### Overlay Flickers in JetBrains Terminals

If the HUD/overlay rapidly flashes in JetBrains while Cursor is stable, confirm
you are on a build that ignores no-op resize events (same rows/cols). Some
JetBrains profiles emit extra SIGWINCH notifications without geometry changes,
which can look like redraw flicker. Recent builds also use JetBrains-specific
DEC cursor save/restore with temporary autowrap-disable during HUD writes, plus
write-then-clear-right redraws (`EL`) to avoid clear-then-paint flashes. The
full HUD main row also keeps a one-column right gutter so animated right-panel
content does not touch the terminal edge in JediTerm. Steady-state recording
updates now redraw only changed HUD rows, so static top/bottom borders and
unchanged shortcut rows are not repainted every meter tick.

1. Check version/build:
   ```bash
   voiceterm --version
   ```
2. Reproduce once with logging:
   ```bash
   voiceterm --logs
   ```
3. If it still flickers, share `${TMPDIR}/voiceterm_tui.log` and include terminal
   app/version so resize event patterns can be compared.

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

The startup splash is shown by default in non-JetBrains terminals. In JetBrains
IDE terminals, `v1.0.64+` auto-skips splash by design. If you expect splash in
another terminal and it does not appear, confirm the environment variable below
is not set:

```bash
env | rg VOICETERM_NO_STARTUP_BANNER
```

To explicitly hide it (useful in scripts), set:

```bash
VOICETERM_NO_STARTUP_BANNER=1 voiceterm
```

### Startup Banner Lingers in IDE Terminal

If the startup splash stays on screen in PyCharm/JetBrains terminals while it
clears normally in Cursor/VS Code, use these checks:

1. Verify you are on `v1.0.64` or newer (JetBrains terminals auto-skip splash by default):
   ```bash
   voiceterm --version
   ```
2. Run with a zero splash delay:
   ```bash
   VOICETERM_STARTUP_SPLASH_MS=0 voiceterm
   ```
3. If you prefer no splash in all terminals:
   ```bash
   VOICETERM_NO_STARTUP_BANNER=1 voiceterm
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
   COLORTERM=truecolor voiceterm --theme catppuccin
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
voiceterm --logs
```

### Include transcript snippets in logs

```bash
voiceterm --logs --log-content
```

### Log file location

Debug log: system temp dir (for example `${TMPDIR}/voiceterm_tui.log` on macOS or
`/tmp/voiceterm_tui.log` on Linux)

Trace log (JSON, written when `--logs` is enabled): system temp dir (for example
`${TMPDIR}/voiceterm_trace.jsonl` on macOS or `/tmp/voiceterm_trace.jsonl` on Linux).
Override with `VOICETERM_TRACE_LOG`.

Crash log (panic only, written when `--logs` is enabled; metadata unless
`--log-content`): system temp dir (for example `${TMPDIR}/voiceterm_crash.log`
on macOS or `/tmp/voiceterm_crash.log` on Linux)

### Disable all logging

```bash
voiceterm --no-logs
```

---

## Getting Help

- **Collect diagnostics:** Run `voiceterm --doctor` and include the output in your issue.
- **Report bugs:** [GitHub Issues](https://github.com/jguida941/voiceterm/issues)
- **Check known issues:** [Master Plan](../dev/active/MASTER_PLAN.md)

---

## FAQ

### What languages does Whisper support?

Whisper supports many languages. VoiceTerm has been tested with English (`en`).

Other languages should work but are untested. Use `--lang auto` for automatic
detection, or specify a language code: `voiceterm --lang es`

Full list: [Whisper supported languages](https://github.com/openai/whisper#available-models-and-languages)

### Which AI CLI backends work?

Only Codex and Claude Code are tested. Other presets exist but are experimental.

| Backend | Install | Run | Status |
|---------|---------|-----|--------|
| Codex | `npm install -g @openai/codex` | `voiceterm` | Tested |
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `voiceterm --claude` | Tested |
| Gemini CLI | See vendor docs | `voiceterm --gemini` | Experimental (currently not working) |
| Aider | See vendor docs | `voiceterm --backend aider` | Experimental (untested) |
| OpenCode | See vendor docs | `voiceterm --backend opencode` | Experimental (untested) |

### Which Whisper model should I use?

See the full [Whisper guide](WHISPER.md) for model comparison and language support.

Quick answer: Start with `base` (142 MB, fast). Use `small` or `medium` for better accuracy.

```bash
voiceterm --whisper-model base
```

### Can I use VoiceTerm without Codex?

Yes. Use Claude Code:
```bash
voiceterm --claude
```

### Does VoiceTerm send my voice to the cloud?

No. All speech-to-text happens locally via Whisper. Your audio never leaves your machine.

### How do I update VoiceTerm?

**Homebrew:**
```bash
brew update && brew upgrade voiceterm
```

**From source:**
```bash
cd voiceterm && git pull && ./scripts/install.sh
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
