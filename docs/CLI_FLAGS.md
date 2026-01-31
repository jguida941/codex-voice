# CLI Flags

Primary user-facing binaries built from this repo:

- `codex-voice` (overlay, normal user path)
- `rust_tui` (standalone TUI, JSON IPC; mostly for dev or integrations)

Additional debug/test binaries live under `rust_tui/src/bin/`.

Everything is grouped by binary so you don't have to cross-reference.

## Index

- [codex-voice](#codex-voice)
- [rust_tui](#rust_tui)
- [See Also](#see-also)

Tip: run `codex-voice --help` or `rust_tui --help` for the live CLI output.

## codex-voice

### Overlay behavior (codex-voice only)

| Flag | Purpose | Default |
|------|---------|---------|
| `--auto-voice` | Start in auto-voice mode | off |
| `--auto-voice-idle-ms <MS>` | Idle time before auto-voice triggers when prompt detection is unknown | 1200 |
| `--transcript-idle-ms <MS>` | Idle time before transcripts auto-send when a prompt has not been detected | 250 |
| `--voice-send-mode <auto\|insert>` | Auto sends newline, insert leaves transcript for editing | auto |
| `--backend <NAME\|CMD>` | Backend CLI preset (`codex`, `claude`, `gemini`, `aider`, `opencode`) or custom command | codex |
| `--prompt-regex <REGEX>` | Override prompt detection regex | auto-learned |
| `--prompt-log <PATH>` | Prompt detection log path | unset (disabled) |
| `--theme <NAME>` | Status line theme (`coral`, `catppuccin`, `dracula`, `nord`, `ansi`, `none`) | coral |
| `--no-color` | Disable status line colors | off |
| `--sounds` | Enable notification sounds (terminal bell) | off |
| `--sound-on-complete` | Beep when a transcript completes | off |
| `--sound-on-error` | Beep on voice capture errors | off |

Prompt detection notes:
- If `--prompt-regex` is not set, the overlay uses a backend-specific prompt pattern when available, otherwise it auto-learns the prompt line after output goes idle.
- Use `--prompt-regex` when your prompt line is unusual or auto-learned incorrectly.

Backend notes:
- For custom commands with spaces, wrap the command or args in quotes (e.g., `--backend "my tool --flag"`).

### Logging (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--logs` | Enable debug file logging | off |
| `--no-logs` | Disable all file logging (overrides `--logs`) | off |
| `--log-content` | Allow prompt/transcript snippets in logs | off |
| `--log-timings` | Enable verbose timing logs (also enables logging) | off |

### Input devices and mic meter (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--input-device <NAME>` | Preferred audio input device | system default |
| `--list-input-devices` | Print available audio devices and exit | - |
| `--mic-meter` | Sample ambient + speech and recommend a VAD threshold | - |
| `--mic-meter-ambient-ms <MS>` | Ambient sample duration for mic meter | 3000 |
| `--mic-meter-speech-ms <MS>` | Speech sample duration for mic meter | 3000 |

### Whisper and STT (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--whisper-model-path <PATH>` | Path to Whisper GGML model (auto-detected in `models/` if present) | - |
| `--whisper-model <NAME>` | Whisper model name | small |
| `--whisper-cmd <PATH>` | Whisper CLI binary (Python fallback) | whisper |
| `--whisper-beam-size <N>` | Beam size (native pipeline only, 0 disables) | 0 |
| `--whisper-temperature <T>` | Temperature (native pipeline only) | 0.0 |
| `--lang <LANG>` | Language for Whisper (`auto` supported) | en |
| `--no-python-fallback` | Fail instead of using Python fallback | off |
| `--voice-stt-timeout-ms <MS>` | STT worker timeout before triggering fallback | 60000 |

### Capture tuning (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--voice-sample-rate <HZ>` | Target sample rate for the voice pipeline | 16000 |
| `--voice-max-capture-ms <MS>` | Max capture duration before hard stop (max 60000) | 30000 |
| `--voice-silence-tail-ms <MS>` | Trailing silence required to stop capture | 1000 |
| `--voice-min-speech-ms-before-stt <MS>` | Minimum speech before STT can begin | 300 |
| `--voice-lookback-ms <MS>` | Audio retained prior to silence stop | 500 |
| `--voice-buffer-ms <MS>` | Total buffered audio budget (max 120000) | 30000 |
| `--voice-channel-capacity <N>` | Frame channel capacity between capture and STT workers | 100 |
| `--voice-vad-threshold-db <DB>` | Mic sensitivity (lower = more sensitive) | -40 |
| `--voice-vad-frame-ms <MS>` | VAD frame size | 20 |
| `--voice-vad-smoothing-frames <N>` | VAD smoothing window (frames) | 3 |
| `--voice-vad-engine <earshot\|simple>` | VAD implementation | earshot (if available) |

### Codex process + pipeline (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--codex-cmd <PATH>` | Path to Codex CLI binary (used when `--backend codex`) | codex |
| `--codex-arg <ARG>` | Extra args to Codex (repeatable, when `--backend codex`) | - |
| `--term <TERM>` | TERM value exported to Codex | `TERM` or `xterm-256color` |
| `--ffmpeg-cmd <PATH>` | FFmpeg binary location | ffmpeg |
| `--ffmpeg-device <NAME>` | FFmpeg audio device override | - |
| `--python-cmd <PATH>` | Python interpreter for helper scripts | python3 |
| `--pipeline-script <PATH>` | Pipeline script location | `scripts/codex_voice.py` |
| `--seconds <N>` | Recording duration for pipeline scripts (seconds) | 5 |

### Environment variables (codex-voice)

| Variable | Description | Default |
|----------|-------------|---------|
| `CODEX_VOICE_MODEL_DIR` | Override model storage directory | auto (`models/` or `~/.local/share/codex-voice/models`) |
| `CODEX_VOICE_CWD` | Run Codex in a chosen project directory | current directory |
| `CODEX_VOICE_INSTALL_DIR` | Override install location for `./install.sh` | unset |
| `CODEX_VOICE_PROMPT_REGEX` | Override prompt detection regex | unset |
| `CODEX_VOICE_PROMPT_LOG` | Prompt detection log path | unset |
| `CODEX_VOICE_LOGS` | Enable debug logging | unset |
| `CODEX_VOICE_NO_LOGS` | Disable debug logging | unset |
| `CODEX_VOICE_LOG_CONTENT` | Allow prompt/transcript snippets in logs | unset |
| `CODEX_VOICE_FORCE_COLUMNS` | Force terminal columns for `start.sh` (banner/testing) | unset |
| `CODEX_VOICE_FORCE_LINES` | Force terminal rows for `start.sh` (banner/testing) | unset |
| `CODEX_VOICE_STARTUP_ONLY` | Print startup banner/controls then exit (`start.sh` only) | unset |
| `NO_COLOR` | Disable ANSI colors (standard) | unset |

## rust_tui

### TUI / IPC modes (rust_tui only)

| Flag | Purpose | Default |
|------|---------|---------|
| `--json-ipc` | Run JSON IPC mode for external UI integration | off |
| `--persistent-codex` | Keep a persistent Codex PTY session | off |
| `--claude-skip-permissions` | Allow Claude CLI to run without permission prompts | off |
| `--claude-cmd <PATH>` | Path to Claude CLI binary | claude |

### Notifications (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--sounds` | Enable notification sounds (terminal bell) | off |
| `--sound-on-complete` | Beep when a transcript completes | off |
| `--sound-on-error` | Beep on voice capture errors | off |

### Logging (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--logs` | Enable debug file logging | off |
| `--no-logs` | Disable all file logging (overrides `--logs`) | off |
| `--log-content` | Allow prompt/transcript snippets in logs | off |
| `--log-timings` | Enable verbose timing logs (also enables logging) | off |

### Input devices and mic meter (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--input-device <NAME>` | Preferred audio input device | system default |
| `--list-input-devices` | Print available audio devices and exit | - |
| `--mic-meter` | Sample ambient + speech and recommend a VAD threshold | - |
| `--mic-meter-ambient-ms <MS>` | Ambient sample duration for mic meter | 3000 |
| `--mic-meter-speech-ms <MS>` | Speech sample duration for mic meter | 3000 |

### Whisper and STT (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--whisper-model-path <PATH>` | Path to Whisper GGML model (auto-detected in `models/` if present) | - |
| `--whisper-model <NAME>` | Whisper model name | small |
| `--whisper-cmd <PATH>` | Whisper CLI binary (Python fallback) | whisper |
| `--whisper-beam-size <N>` | Beam size (native pipeline only, 0 disables) | 0 |
| `--whisper-temperature <T>` | Temperature (native pipeline only) | 0.0 |
| `--lang <LANG>` | Language for Whisper (`auto` supported) | en |
| `--no-python-fallback` | Fail instead of using Python fallback | off |
| `--voice-stt-timeout-ms <MS>` | STT worker timeout before triggering fallback | 60000 |

### Capture tuning (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--voice-sample-rate <HZ>` | Target sample rate for the voice pipeline | 16000 |
| `--voice-max-capture-ms <MS>` | Max capture duration before hard stop (max 60000) | 30000 |
| `--voice-silence-tail-ms <MS>` | Trailing silence required to stop capture | 1000 |
| `--voice-min-speech-ms-before-stt <MS>` | Minimum speech before STT can begin | 300 |
| `--voice-lookback-ms <MS>` | Audio retained prior to silence stop | 500 |
| `--voice-buffer-ms <MS>` | Total buffered audio budget (max 120000) | 30000 |
| `--voice-channel-capacity <N>` | Frame channel capacity between capture and STT workers | 100 |
| `--voice-vad-threshold-db <DB>` | Mic sensitivity (lower = more sensitive) | -40 |
| `--voice-vad-frame-ms <MS>` | VAD frame size | 20 |
| `--voice-vad-smoothing-frames <N>` | VAD smoothing window (frames) | 3 |
| `--voice-vad-engine <earshot\|simple>` | VAD implementation | earshot (if available) |

### Codex process + pipeline (shared flags)

| Flag | Purpose | Default |
|------|---------|---------|
| `--codex-cmd <PATH>` | Path to Codex CLI binary | codex |
| `--codex-arg <ARG>` | Extra args to Codex (repeatable) | - |
| `--term <TERM>` | TERM value exported to Codex | `TERM` or `xterm-256color` |
| `--ffmpeg-cmd <PATH>` | FFmpeg binary location | ffmpeg |
| `--ffmpeg-device <NAME>` | FFmpeg audio device override | - |
| `--python-cmd <PATH>` | Python interpreter for helper scripts | python3 |
| `--pipeline-script <PATH>` | Pipeline script location | `scripts/codex_voice.py` |
| `--seconds <N>` | Recording duration for pipeline scripts (seconds) | 5 |

### Environment variables (rust_tui)

| Variable | Description | Default |
|----------|-------------|---------|
| `CODEX_VOICE_MODEL_DIR` | Override model storage directory | auto (`models/` or `~/.local/share/codex-voice/models`) |
| `CODEX_VOICE_CWD` | Run Codex in a chosen project directory | current directory |
| `CODEX_VOICE_LOGS` | Enable debug logging | unset |
| `CODEX_VOICE_NO_LOGS` | Disable debug logging | unset |
| `CODEX_VOICE_LOG_CONTENT` | Allow prompt/transcript snippets in logs | unset |
| `CODEX_VOICE_PROVIDER` | Default provider for IPC mode | codex |
| `CLAUDE_CMD` | Path to Claude CLI for IPC mode | claude |
| `CODEX_VOICE_TEST_DEVICES` | Test-only override for device listing (comma-separated) | unset |

## See Also

| Topic | Link |
|-------|------|
| Quick Start | [QUICK_START.md](../QUICK_START.md) |
| Install | [INSTALL.md](INSTALL.md) |
| Usage | [USAGE.md](USAGE.md) |
| Troubleshooting | [TROUBLESHOOTING.md](TROUBLESHOOTING.md) |
