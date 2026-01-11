# Codex Voice

Voice-enabled CLI wrapper for Codex and Claude. Speak your prompts, get AI responses.

## Features

- Voice input via microphone (Whisper STT)
- Supports both **Codex** and **Claude** providers
- TypeScript CLI with Rust backend for low-latency audio processing
- Auto-voice mode for continuous conversation

## Quick Start

### Prerequisites

- Node.js 18+
- Rust toolchain (stable)
- Codex CLI (`npm install -g @anthropic-ai/codex`) and/or Claude CLI
- Whisper model (GGML format) in `models/` directory

### Install & Run

```bash
# Clone and enter project
cd codex-voice

# Build everything and run
./start.sh
```

Or manually:
```bash
# Build Rust backend
cd rust_tui && cargo build --release && cd ..

# Install and run TypeScript CLI
cd ts_cli && npm install && npm start
```

## Using With Your Own Projects

Codex Voice works with **any codebase** - just run it from your project directory.

### macOS

**Option A:** Double-click **Codex Voice.app** - it will prompt you to select your project folder.

**Option B:** Command line:
```bash
cd ~/my-project
/path/to/codex-voice/start.sh
```

### Windows

Double-click **start.bat** - it will prompt you to select your project folder.

### Linux / Command Line

```bash
cd ~/my-project
/path/to/codex-voice/start.sh
```

### Install Globally (All Platforms)

```bash
cd /path/to/codex-voice/ts_cli
npm link

# Now run from any project
cd ~/my-project
codex-voice
```

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/voice` | Start voice capture |
| `/auto` | Toggle auto-voice mode |
| `/status` | Show backend status |
| `/provider` | Show/set active provider (codex/claude) |
| `/auth [provider]` | Login via provider CLI |
| `/codex <msg>` | Send to Codex (one-off or switch) |
| `/claude <msg>` | Send to Claude (one-off or switch) |
| `/clear` | Clear the screen |
| `/exit` | Exit the application |

### Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+C` | Cancel/Exit |

### Example Session

```
[codex] > /provider claude
Switched to claude

[claude] > explain this codebase
Claude: This is a voice-enabled CLI wrapper...

[claude] > /voice
Listening... (speak now)
Transcribed: "add a new feature for..."
Claude: I'll help you add that feature...
```

## Architecture

```
┌─────────────────┐     JSON-IPC      ┌─────────────────┐
│  TypeScript CLI │ ◄──────────────► │   Rust Backend   │
│   (ts_cli/)     │                   │   (rust_tui/)    │
└─────────────────┘                   └─────────────────┘
        │                                     │
        │                              ┌──────┴──────┐
        │                              │             │
        ▼                              ▼             ▼
   User Input                     Audio Capture   Provider CLI
   (keyboard)                     (CPAL + VAD)    (codex/claude)
                                       │
                                       ▼
                                  Whisper STT
```

### Components

| Component | Path | Purpose |
|-----------|------|---------|
| TypeScript CLI | `ts_cli/` | User interface, input handling, display |
| Rust Backend | `rust_tui/` | Audio capture, STT, provider communication |
| IPC Protocol | JSON-lines over stdin/stdout | TypeScript ↔ Rust communication |

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CODEX_VOICE_PROVIDER` | Default provider (`codex` or `claude`) | `codex` |
| `CLAUDE_CMD` | Path to Claude CLI | `claude` |

### CLI Options (Rust Backend)

```bash
cargo run --release -- --help

Options:
  --codex-cmd <CMD>           Path to Codex CLI [default: codex]
  --whisper-model-path <PATH> Path to Whisper GGML model
  --persistent-codex          Enable PTY session for Codex
  --json-ipc                  Run in IPC mode (used by TypeScript CLI)
  --input-device <NAME>       Preferred audio input device
  --list-input-devices        Print available audio devices and exit
```

## Development

### Project Structure

```
codex-voice/
├── ts_cli/              # TypeScript frontend
│   ├── src/
│   │   ├── index.ts     # Main entry, input handling
│   │   ├── bridge/      # Rust IPC communication
│   │   └── ui/          # Spinner, colors, banner
│   └── package.json
├── rust_tui/            # Rust backend
│   ├── src/
│   │   ├── main.rs      # Entry point
│   │   ├── ipc.rs       # JSON IPC protocol
│   │   ├── codex.rs     # Codex backend
│   │   ├── voice.rs     # Voice capture orchestration
│   │   ├── audio.rs     # CPAL recording, VAD
│   │   └── stt.rs       # Whisper transcription
│   └── Cargo.toml
├── models/              # Whisper GGML models
└── docs/                # Architecture documentation
```

### Building

```bash
# Rust backend
cd rust_tui && cargo build --release

# TypeScript CLI
cd ts_cli && npm run build
```

### Testing

```bash
# Rust tests
cd rust_tui && cargo test

# Run with debug output
cd ts_cli && DEBUG=1 npm start
```

## Troubleshooting

### Voice not working

1. Check microphone permissions
2. Verify Whisper model exists: `ls models/*.bin`
3. Run with debug: `DEBUG=1 npm start`
4. Check `/status` for backend capabilities

### Provider not responding

1. Ensure CLI is installed: `which codex` or `which claude`
2. Check authentication: `codex login` or `claude login`
3. Try switching providers: `/provider claude`

### Backend connection failed

1. Rebuild Rust backend: `cd rust_tui && cargo build --release`
2. Check for compilation errors
3. Verify binary exists: `ls rust_tui/target/release/rust_tui`

## License

MIT
