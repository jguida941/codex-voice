# codex_voice

Voice-driven wrapper that records microphone audio, transcribes it with Whisper (OpenAI CLI or whisper.cpp), and forwards the transcript to the Codex CLI. The repo currently includes:

- `codex_voice.py` — fully functional Python MVP with configurable flags and PTY-aware Codex invocation.
- `rust_tui/` — scaffolding for the upcoming Rust terminal UI (ratatui + crossterm).
- `stubs/` — fake ffmpeg/whisper/codex helpers used for local smoke tests without real binaries.

## Commands Executed

The following commands have been run during development and verification:

- `python codex_voice.py --seconds 8 --ffmpeg-cmd ffmpeg --ffmpeg-device ":0" --whisper-cmd whisper --whisper-model base --codex-cmd codex`
- `cd rust_tui && cargo run -- --seconds 8 --ffmpeg-device ":0" --whisper-cmd whisper --whisper-model base --codex-cmd codex`
- `printf '\n' | python codex_voice.py --seconds 1 --ffmpeg-cmd ./stubs/fake_ffmpeg --whisper-cmd ./stubs/whisper --codex-cmd ./stubs/fake_codex`
- `./scripts/run_tui.sh`

## Run Codex Voice Locally

On macOS using the real binaries:

```bash
python codex_voice.py \
  --seconds 8 \
  --ffmpeg-cmd ffmpeg \
  --ffmpeg-device ":0" \
  --whisper-cmd whisper \
  --whisper-model base \
  --codex-cmd codex
```

Adjust `--ffmpeg-device`, `--whisper-*`, and `--codex-*` options based on your environment. Include any extra CLI flags for Codex with `--codex-args "..."`.

### Using the Rust TUI inside an IDE

- **JetBrains IDEs**: Create a run configuration for `cargo run` and enable *Run with terminal / Emulate terminal in output console* so the process receives a proper TTY. You can also use the built-in *Terminal* tool window instead of the Run panel.
- **VS Code**: Launch the app from the integrated terminal (`Ctrl+``). The default "Run" button does not allocate a terminal, so ratatui will not receive keystrokes.
- **Other editors**: Ensure the launch configuration allocates a pseudo terminal (look for options such as “Allocate TTY” or “Run in Terminal”). Without it, the TUI cannot switch into raw mode, and shortcuts like `Ctrl+R` will not work.
- The TUI falls back to `scripts/run_in_pty.py` (via `python3`) when Codex insists on a TTY. If your Python lives elsewhere, pass `--python-cmd` or `--pty-helper` to `cargo run` to override the defaults.
- To avoid copy/pasting the long command, use `./scripts/run_tui.sh`. Override defaults with env vars such as `SECONDS_OVERRIDE=10`, `FFMPEG_DEVICE_OVERRIDE=":0"`, or `CODEX_CMD_OVERRIDE=codex-beta`.
