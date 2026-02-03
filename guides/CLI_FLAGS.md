# CLI Flags

All flags for the `voxterm` command. Run `voxterm --help` for the live output.

## Contents

- [Quick Reference](#quick-reference)
- [Voice Behavior](#voice-behavior)
- [Backend Selection](#backend-selection)
- [Microphone & Audio](#microphone--audio)
- [Whisper STT](#whisper-stt)
- [Capture Tuning](#capture-tuning)
- [Themes & Display](#themes--display)
- [Logging](#logging)
- [Environment Variables](#environment-variables)
- [See Also](#see-also)

---

## Quick Reference

Most common flags:

```bash
voxterm --codex                   # Use Codex (default)
voxterm --claude                  # Use Claude Code
voxterm --auto-voice              # Hands-free mode
voxterm --theme dracula           # Change theme
voxterm --voice-vad-threshold-db -50  # Adjust mic sensitivity
voxterm --mic-meter               # Calibrate mic threshold
voxterm --logs                    # Enable debug logging
```

---

## Voice Behavior

| Flag | Purpose | Default |
|------|---------|---------|
| `--auto-voice` | Start in auto-voice mode (hands-free) | off |
| `--auto-voice-idle-ms <MS>` | Idle time before auto-voice triggers when prompt not detected | 1200 |
| `--transcript-idle-ms <MS>` | Idle time before queued transcripts auto-send | 250 |
| `--voice-send-mode <auto\|insert>` | `auto` sends immediately, `insert` waits for Enter | auto |

---

## Backend Selection

| Flag | Purpose | Default |
|------|---------|---------|
| `--codex` | Use Codex CLI (shorthand) | - |
| `--claude` | Use Claude Code (shorthand) | - |
| `--backend <NAME>` | Backend preset: `codex` or `claude` | codex |
| `--prompt-regex <REGEX>` | Override prompt detection pattern | auto-learned |
| `--prompt-log <PATH>` | Log detected prompts to file (debugging) | disabled |
| `--codex-cmd <PATH>` | Path to Codex binary | codex |
| `--codex-arg <ARG>` | Extra args passed to Codex (repeatable) | - |

**Examples:**
```bash
voxterm --codex               # Use Codex (default)
voxterm --claude              # Use Claude Code
```

---

## Microphone & Audio

| Flag | Purpose | Default |
|------|---------|---------|
| `--input-device <NAME>` | Use a specific microphone | system default |
| `--list-input-devices` | Print available audio devices and exit | - |
| `--mic-meter` | Calibration tool: measures ambient noise and speech | - |
| `--mic-meter-ambient-ms <MS>` | Ambient sample duration for calibration | 3000 |
| `--mic-meter-speech-ms <MS>` | Speech sample duration for calibration | 3000 |
| `--doctor` | Print environment diagnostics and exit | - |

---

## Whisper STT

| Flag | Purpose | Default |
|------|---------|---------|
| `--whisper-model <NAME>` | Model size: `tiny`, `base`, `small`, `medium`, `large` | small |
| `--whisper-model-path <PATH>` | Path to GGML model file | auto-detected |
| `--lang <LANG>` | Language code (`en`, `es`, `auto`, etc.) | en |
| `--whisper-beam-size <N>` | Beam search size (0 = greedy) | 0 |
| `--whisper-temperature <T>` | Sampling temperature | 0.0 |
| `--no-python-fallback` | Fail instead of falling back to Python Whisper | off |
| `--voice-stt-timeout-ms <MS>` | Timeout before triggering fallback | 60000 |

---

## Capture Tuning

| Flag | Purpose | Default |
|------|---------|---------|
| `--voice-vad-threshold-db <DB>` | Mic sensitivity (-80 = very sensitive, -10 = less) | -55 |
| `--voice-max-capture-ms <MS>` | Max recording duration (max 60000) | 30000 |
| `--voice-silence-tail-ms <MS>` | Silence duration to stop recording | 1000 |
| `--voice-min-speech-ms-before-stt <MS>` | Minimum speech before STT starts | 300 |
| `--voice-lookback-ms <MS>` | Audio kept before silence stop | 500 |
| `--voice-buffer-ms <MS>` | Total audio buffer (max 120000) | 30000 |
| `--voice-sample-rate <HZ>` | Audio sample rate | 16000 |
| `--voice-vad-frame-ms <MS>` | VAD frame size | 20 |
| `--voice-vad-smoothing-frames <N>` | VAD smoothing window | 3 |
| `--voice-vad-engine <earshot\|simple>` | VAD implementation | earshot |
| `--voice-channel-capacity <N>` | Internal frame channel capacity | 100 |

---

## Themes & Display

| Flag | Purpose | Default |
|------|---------|---------|
| `--theme <NAME>` | Theme name | backend default |
| `--no-color` | Disable all colors | off |
| `--hud-style <MODE>` | HUD display style: `full`, `minimal`, `hidden` | full |
| `--minimal-hud` | Shorthand for `--hud-style minimal` | off |
| `--hud-right-panel <MODE>` | Right-side HUD panel: `off`, `ribbon`, `dots`, `heartbeat` | ribbon |
| `--hud-right-panel-recording-only` | Only animate right panel while recording | on |
| `--term <TERM>` | TERM value for the CLI | inherited |

**Themes:** `chatgpt`, `claude`, `codex`, `coral`, `catppuccin`, `dracula`,
`nord`, `tokyonight`, `gruvbox`, `ansi`, `none`.

**HUD styles:**
- `full`: 4-row banner with borders, mode indicator, dB meter, and shortcuts
- `minimal`: Single-line strip
- `hidden`: No HUD content when idle (keeps a blank row); shows "REC" indicator only while recording

Examples of the Minimal strip: `◉ AUTO · Ready`, `● REC · -55dB`.

**Theme defaults:** If `--theme` is not provided, VoxTerm selects a backend-
appropriate default. Claude → `claude`, Codex → `codex`, others → `coral`.

---

## Logging

| Flag | Purpose | Default |
|------|---------|---------|
| `--logs` | Enable debug logging to file | off |
| `--no-logs` | Force disable logging | off |
| `--log-content` | Include transcript snippets in logs | off |
| `--log-timings` | Verbose timing information | off |

**Log location:** `$TMPDIR/voxterm_tui.log` (macOS) or
`/tmp/voxterm_tui.log` (Linux)

---

## Sounds

| Flag | Purpose | Default |
|------|---------|---------|
| `--sounds` | Enable all notification sounds | off |
| `--sound-on-complete` | Beep when transcript completes | off |
| `--sound-on-error` | Beep on voice capture error | off |

---

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `VOXTERM_CWD` | Run CLI in this directory | current directory |
| `VOXTERM_MODEL_DIR` | Whisper model storage path | `whisper_models/` or `~/.local/share/voxterm/models` |
| `VOXTERM_INSTALL_DIR` | Override install location | unset |
| `VOXTERM_PROMPT_REGEX` | Override prompt detection | unset |
| `VOXTERM_PROMPT_LOG` | Prompt detection log path | unset |
| `VOXTERM_LOGS` | Enable logging (same as `--logs`) | unset |
| `VOXTERM_NO_LOGS` | Disable logging | unset |
| `VOXTERM_LOG_CONTENT` | Allow content in logs | unset |
| `NO_COLOR` | Disable colors (standard) | unset |

---

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Install | [INSTALL.md](INSTALL.md) |
| Usage | [USAGE.md](USAGE.md) |
| Troubleshooting | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
