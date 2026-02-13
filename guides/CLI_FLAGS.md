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
voxterm --login --codex           # Run Codex login before starting
voxterm --login --claude          # Run Claude login before starting
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
| `--transcript-idle-ms <MS>` | Idle time before queued transcripts are injected into the terminal | 250 |
| `--voice-send-mode <auto\|insert>` | `auto` types text and presses Enter; `insert` types text, you press Enter | auto |
| `--seconds <N>` | Recording duration for the Python fallback pipeline (1-60) | 5 |

`Macros` is currently a runtime Settings toggle (`Ctrl+O`) and does not yet
have a CLI flag.

---

## Backend Selection

| Flag | Purpose | Default |
|------|---------|---------|
| `--codex` | Use Codex CLI (shorthand) | - |
| `--claude` | Use Claude Code (shorthand) | - |
| `--gemini` | Use Gemini CLI (experimental; currently not working) | - |
| `--backend <NAME\|CMD>` | Backend preset: `codex`, `claude`, `gemini` (not working), `aider` (untested), `opencode` (untested), or a custom command string | codex |
| `--login` | Run backend login before starting the overlay | off |
| `--prompt-regex <REGEX>` | Override prompt detection pattern | auto-learned |
| `--prompt-log <PATH>` | Log detected prompts to file (debugging) | disabled |
| `--codex-cmd <PATH>` | Path to Codex binary | codex |
| `--claude-cmd <PATH>` | Path to Claude binary (IPC + overlay) | claude |
| `--codex-arg <ARG>` | Extra args passed to Codex (repeatable) | - |
| `--persistent-codex` | Keep a persistent Codex PTY session (advanced) | off |

**Examples:**
```bash
voxterm --codex               # Use Codex (default)
voxterm --claude              # Use Claude Code
voxterm --login --codex       # Login to Codex CLI
voxterm --login --claude      # Login to Claude CLI
```

**Notes:**
- `--backend` accepts a custom command string.
- Gemini is currently nonfunctional; Aider/OpenCode presets exist but are untested. Only Codex and Claude are fully supported.

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
| `--ffmpeg-cmd <PATH>` | FFmpeg binary path (python fallback) | ffmpeg |
| `--ffmpeg-device <NAME>` | FFmpeg audio device override (python fallback) | - |

---

## Whisper STT

| Flag | Purpose | Default |
|------|---------|---------|
| `--whisper-model <NAME>` | Model size: `tiny`, `base`, `small`, `medium`, `large` | small |
| `--whisper-model-path <PATH>` | Path to GGML model file | auto-detected |
| `--lang <LANG>` | Language code (`en`, `es`, `auto`, etc.) | en |
| `--whisper-cmd <PATH>` | Whisper CLI path (python fallback) | whisper |
| `--whisper-beam-size <N>` | Beam search size (0 = greedy) | 0 |
| `--whisper-temperature <T>` | Sampling temperature | 0.0 |
| `--no-python-fallback` | Fail instead of falling back to Python Whisper | off |
| `--voice-stt-timeout-ms <MS>` | Timeout before triggering fallback | 60000 |
| `--python-cmd <PATH>` | Python interpreter for fallback scripts | python3 |
| `--pipeline-script <PATH>` | Python fallback pipeline script (bundled in the install by default) | built-in |

---

## Capture Tuning

| Flag | Purpose | Default |
|------|---------|---------|
| `--voice-vad-threshold-db <DB>` | Mic sensitivity (-120 = very sensitive, 0 = less; hotkeys clamp -80..-10) | -55 |
| `--voice-max-capture-ms <MS>` | Max recording duration (max 60000) | 30000 |
| `--voice-silence-tail-ms <MS>` | Silence duration to stop recording | 1000 |
| `--voice-min-speech-ms-before-stt <MS>` | Minimum speech before STT starts | 300 |
| `--voice-lookback-ms <MS>` | Audio kept before silence stop | 500 |
| `--voice-buffer-ms <MS>` | Total audio buffer (max 120000) | 30000 |
| `--voice-sample-rate <HZ>` | Audio sample rate | 16000 |
| `--voice-vad-frame-ms <MS>` | VAD frame size | 20 |
| `--voice-vad-smoothing-frames <N>` | VAD smoothing window | 3 |
| `--voice-vad-engine <earshot\|simple>` | VAD implementation | earshot (when built with `vad_earshot`), otherwise `simple` |
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
| `--latency-display <off\|short\|label>` | Shortcuts-row latency badge style (`off`, `Nms`, or `Latency: Nms`) | short |
| `--term <TERM>` | TERM value for the CLI | inherited |

**Themes:** `chatgpt`, `claude`, `codex`, `coral`, `catppuccin`, `dracula`,
`nord`, `tokyonight`, `gruvbox`, `ansi`, `none`.

**HUD styles:**
- `full`: 4-row banner with borders, mode indicator, dB meter, and shortcuts
- `minimal`: Single-line strip with optional compact right-panel visualization chip
- `hidden`: Branded launcher row when idle (`VoxTerm` + `Ctrl+U` hint + clickable open button); shows dim "REC" indicator while recording

Examples of the Minimal strip: `◉ AUTO · Ready`, `● REC · -55dB`.
Compact HUD modules adapt by state and available width (recording favors meter +
latency + queue, busy favors queue + latency, idle favors latency).
Full HUD formatting keeps a one-column right-edge safety margin to avoid
terminal autowrap drift in IDE terminals; there is no CLI flag for this
behavior.
For this behavior, prefer `v1.0.62` or newer.

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

**Trace log (JSON):** `$TMPDIR/voxterm_trace.jsonl` (macOS) or
`/tmp/voxterm_trace.jsonl` (Linux). Override with `VOXTERM_TRACE_LOG`.

---

## IPC / Integration

| Flag | Purpose | Default |
|------|---------|---------|
| `--json-ipc` | Run in JSON IPC mode (external UI integration) | off |
| `--claude-skip-permissions` | Skip Claude permission prompts (IPC only) | off |

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
| `VOXTERM_MODEL_DIR` | Whisper model storage path (used by install/start scripts) | `whisper_models/` or `~/.local/share/voxterm/models` |
| `VOXTERM_INSTALL_DIR` | Override install location | unset |
| `VOXTERM_NO_STARTUP_BANNER` | Skip the startup splash screen | unset |
| `VOXTERM_STARTUP_SPLASH_MS` | Splash dwell time in milliseconds (0 = immediate, max 30000) | 1500 |
| `VOXTERM_PROMPT_REGEX` | Override prompt detection | unset |
| `VOXTERM_PROMPT_LOG` | Prompt detection log path | unset |
| `VOXTERM_LOGS` | Enable logging (same as `--logs`) | unset |
| `VOXTERM_NO_LOGS` | Disable logging | unset |
| `VOXTERM_LOG_CONTENT` | Allow content in logs | unset |
| `VOXTERM_TRACE_LOG` | Structured trace log path | unset |
| `VOXTERM_DEBUG_INPUT` | Log raw input bytes/events (for terminal compatibility debugging) | unset |
| `CLAUDE_CMD` | Override Claude CLI path | unset |
| `VOXTERM_PROVIDER` | IPC default provider (`codex` or `claude`) | unset |
| `NO_COLOR` | Disable colors (standard) | unset |

---

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Install | [INSTALL.md](INSTALL.md) |
| Usage | [USAGE.md](USAGE.md) |
| Troubleshooting | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
