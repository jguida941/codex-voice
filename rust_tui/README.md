# Codex Voice Backend

Rust backend for Codex Voice - handles audio capture, speech-to-text, and provider communication.

## Role

This is the **backend** component that:
- Captures audio via CPAL
- Transcribes speech using Whisper
- Communicates with Codex/Claude CLIs
- Exposes a JSON-IPC interface for the TypeScript frontend

## Building

```bash
cd rust_tui
cargo build --release
```

The binary is at `target/release/rust_tui`.

## Usage

### As IPC Backend (Primary Mode)

Used by the TypeScript CLI:

```bash
./target/release/rust_tui --json-ipc
```

Communicates via JSON-lines on stdin/stdout.

### Standalone TUI Mode

Can also run as a standalone terminal UI:

```bash
cargo run --release -- --seconds 5 --whisper-model-path ../models/ggml-base.en.bin
```

## CLI Options

### Core Options
| Flag | Purpose |
|------|---------|
| `--json-ipc` | Run in IPC mode (for TypeScript frontend) |
| `--codex-cmd <CMD>` | Path to Codex CLI [default: codex] |
| `--persistent-codex` | Enable PTY session for Codex |
| `--log-timings` | Enable verbose timing logs |

### Audio Options
| Flag | Purpose |
|------|---------|
| `--input-device <NAME>` | Preferred audio input device |
| `--list-input-devices` | Print available devices and exit |
| `--voice-vad-engine <ENGINE>` | VAD implementation: `earshot` or `simple` |
| `--voice-vad-threshold-db <DB>` | VAD threshold in decibels [default: -40] |
| `--seconds <N>` | Recording duration in seconds [default: 5] |

### Whisper Options
| Flag | Purpose |
|------|---------|
| `--whisper-model-path <PATH>` | Path to Whisper GGML model (required) |
| `--whisper-model <NAME>` | Whisper model name [default: small] |
| `--lang <LANG>` | Language for Whisper [default: en] |
| `--no-python-fallback` | Fail instead of using Python STT fallback |

Run `cargo run -- --help` for the full list of 30+ options.

## IPC Protocol

### Commands (TypeScript → Rust)

```json
{"cmd": "send_prompt", "prompt": "hello", "provider": "claude"}
{"cmd": "start_voice"}
{"cmd": "set_provider", "provider": "codex"}
{"cmd": "cancel"}
{"cmd": "auth", "provider": "codex"}
{"cmd": "get_capabilities"}
```

### Events (Rust → TypeScript)

```json
{"event": "capabilities", "mic_available": true, "whisper_model_loaded": true, ...}
{"event": "provider_changed", "provider": "claude"}
{"event": "provider_error", "message": "..."}
{"event": "auth_start", "provider": "codex"}
{"event": "auth_end", "provider": "codex", "success": true}
{"event": "voice_start"}
{"event": "transcript", "text": "user said this", "duration_ms": 1200}
{"event": "token", "text": "Hello there!"}
{"event": "job_start", "provider": "claude"}
{"event": "job_end", "provider": "claude", "success": true}
{"event": "status", "message": "..."}
{"event": "error", "message": "...", "recoverable": true}
```

## Architecture

```
src/
├── main.rs          # Entry point, CLI parsing
├── lib.rs           # Module exports
├── ipc.rs           # JSON-IPC protocol handler
├── codex.rs         # Codex/Claude backend (PTY + CLI modes)
├── voice.rs         # Voice capture orchestration
├── audio.rs         # CPAL recording, VAD
├── stt.rs           # Whisper transcription
├── pty_session.rs   # Unix PTY wrapper
├── config.rs        # Configuration and validation
├── utf8_safe.rs     # UTF-8 safety utilities
├── vad_earshot.rs   # Voice activity detection
├── ui.rs            # TUI rendering (standalone mode)
├── app.rs           # TUI state machine (standalone mode)
└── bin/             # Additional binaries
```

## Providers

### Claude

Uses `claude --print <prompt>` for simple, non-interactive responses.

### Codex

Two modes:
- **CLI mode** (default): `codex exec - -C <dir>` per prompt
- **PTY mode** (`--persistent-codex`): Persistent pseudo-terminal session

## Development

```bash
# Format
cargo fmt

# Lint
cargo clippy --all-features

# Test
cargo test

# Generate docs
cargo doc --open
```

## Troubleshooting

- **No audio captured**: Check microphone permissions, try `--list-input-devices`
- **Whisper model not found**: Ensure model exists at specified path
- **Provider not responding**: Verify CLI is installed and authenticated
- **PTY issues**: Try without `--persistent-codex`

Logs are written to `$TMPDIR/codex_voice_tui.log`.
