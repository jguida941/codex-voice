# Codex Voice

Voice-enabled CLI wrapper for Codex and Claude. Speak your prompts, get AI responses.

## Features

- Voice input via microphone (Whisper STT)
- Supports both **Codex** and **Claude** providers
- Rust overlay mode that preserves the full Codex TUI (PTY passthrough)
- Legacy TypeScript CLI with Rust backend for low-latency audio processing
- Auto-voice mode for continuous conversation

## Quick Start

### Prerequisites

- Node.js 18+ (legacy TypeScript CLI only)
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
# Build and run overlay mode
cd rust_tui && cargo build --release --bin codex_overlay
./target/release/codex_overlay

# Legacy TypeScript CLI
cd rust_tui && cargo build --release && cd ..

# Install and run TypeScript CLI
cd ts_cli && npm install && npm start
```

## Install Options

### macOS App (folder picker)

1. Double-click **Codex Voice.app**.
2. Pick your project folder.
3. A Terminal window opens and runs the overlay inside that folder.

### Homebrew (optional, global command)

1. Install Homebrew (see https://brew.sh).
2. Tap and install:

```bash
brew tap jguida941/homebrew-codex-voice
brew install codex-voice
```

3. Download a Whisper model once:

```bash
$(brew --prefix)/opt/codex-voice/libexec/scripts/setup.sh models --base
```

4. Run from any project:

```bash
cd ~/my-project
codex-voice
```

### Manual (no Homebrew)

Run from any project folder:

```bash
CODEX_VOICE_CWD="$(pwd)" /path/to/codex-voice/start.sh
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
Set `CODEX_VOICE_MODE=legacy` to use the TypeScript CLI instead of the overlay.

### Windows

Double-click **start.bat** - it will prompt you to select your project folder.

### Linux / Command Line

```bash
cd ~/my-project
/path/to/codex-voice/start.sh
```
Set `CODEX_VOICE_MODE=legacy` to use the TypeScript CLI instead of the overlay.

### Install Globally (All Platforms)

```bash
cd /path/to/codex-voice/ts_cli
npm link

# Now run from any project
cd ~/my-project
codex-voice
```

## Usage

### Overlay Mode (default)

Overlay mode runs the Codex CLI in a PTY and forwards raw ANSI output. There are no wrapper
slash commands; you interact directly with Codex's native UI.

#### Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+Q` | Exit overlay |
| `Ctrl+C` | Forward to Codex |

#### Common flags

| Flag | Purpose |
|------|---------|
| `--auto-voice` | Start auto-voice immediately |
| `--auto-voice-idle-ms <MS>` | Idle timeout before auto-voice triggers |
| `--prompt-regex <REGEX>` | Prompt detection override |
| `--voice-send-mode <auto|insert>` | Auto-send transcript or insert for editing |

### Legacy TypeScript CLI

#### Commands

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

#### Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+C` | Cancel/Exit |

### Example Session (Legacy CLI)

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

## How It Works

Overlay mode runs Codex in a real PTY and forwards raw ANSI output directly to your terminal:

```
Keyboard / voice
       |
       v
  codex_overlay (Rust)
   |        |
   |        +--> voice pipeline (cpal + Whisper) -> transcript -> PTY input
   |
   +--> PTY -> Codex CLI -> ANSI output -> your terminal
```

The overlay does not parse slash commands; it keeps Codex’s native UI intact and only handles
its own hotkeys (Ctrl+R/Ctrl+V/Ctrl+Q).

## Commands Reference

### Overlay Mode

| Input | Action |
|-------|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+Q` | Exit overlay |
| `Ctrl+C` | Forward to Codex |
| `--auto-voice` | Start auto-voice immediately |
| `--voice-send-mode <auto|insert>` | Auto-send transcript or insert for editing |
| `--prompt-regex <REGEX>` | Prompt detection override |

### Legacy TypeScript CLI

| Command | Description |
|---------|-------------|
| `/voice` | Start voice capture |
| `/auto` | Toggle auto-voice mode |
| `/provider` | Show/set provider (codex/claude) |
| `/auth [provider]` | Login via provider CLI |
| `/exit` | Exit the application |

## Architecture

Overlay mode (default) is Rust-only: `codex_overlay` spawns Codex in a PTY, forwards raw terminal
output to your terminal, and injects voice transcripts as keystrokes.

Legacy TypeScript CLI architecture:

```
┌─────────────────┐     JSON-IPC      ┌─────────────────┐
│  TypeScript CLI │ ◄──────────────►  │   Rust Backend  │
│   (ts_cli/)     │                   │   (rust_tui/)   │
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
| Rust Overlay | `rust_tui/src/bin/codex_overlay.rs` | PTY passthrough UI with voice overlay |
| TypeScript CLI | `ts_cli/` | User interface, input handling, display |
| Rust Backend | `rust_tui/` | Audio capture, STT, provider communication |
| IPC Protocol | JSON-lines over stdin/stdout | TypeScript ↔ Rust communication |

See `ARCHITECTURE.md` for full diagrams and data flow.

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CODEX_VOICE_PROVIDER` | Default provider (`codex` or `claude`) | `codex` |
| `CLAUDE_CMD` | Path to Claude CLI | `claude` |
| `CODEX_VOICE_MODE` | Launcher mode (`overlay` or `legacy`) | `overlay` |
| `CODEX_OVERLAY_PROMPT_REGEX` | Override prompt detection regex | unset |
| `CODEX_OVERLAY_PROMPT_LOG` | Prompt detection log path | `${TMPDIR}/codex_overlay_prompt.log` |

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

### CLI Options (Overlay)

```bash
cargo run --release --bin codex_overlay -- --help

Options:
  --prompt-regex <REGEX>      Regex to detect Codex prompt
  --prompt-log <PATH>         Prompt detection log path
  --auto-voice                Start in auto-voice mode
  --auto-voice-idle-ms <MS>   Idle timeout before auto-voice triggers
  --voice-send-mode <MODE>    auto (send newline) or insert (edit before Enter)
```

## Development

### Project Structure

```
codex-voice/
├── Codex Voice.app/     # macOS double-click launcher
├── ARCHITECTURE.md      # Architecture diagrams and data flow
├── ts_cli/              # TypeScript frontend
│   └── src/
│       ├── index.ts     # Main entry, input handling
│       ├── bridge/      # Rust IPC communication
│       └── ui/          # Spinner, colors, banner
├── rust_tui/            # Rust backend
│   └── src/
│       ├── main.rs      # Entry point
│       ├── ipc.rs       # JSON IPC protocol
│       ├── codex.rs     # Codex/Claude backend
│       ├── voice.rs     # Voice capture orchestration
│       ├── audio.rs     # CPAL recording, VAD
│       ├── stt.rs       # Whisper transcription
│       └── pty_session.rs # PTY wrapper
├── scripts/             # Setup and test scripts
├── models/              # Whisper GGML models
├── docs/                # Local notes (ignored by git)
├── start.sh             # Linux/macOS launcher
└── start.bat            # Windows launcher
```

### Building

```bash
# Rust backend
cd rust_tui && cargo build --release

# Rust overlay
cd rust_tui && cargo build --release --bin codex_overlay

# TypeScript CLI
cd ts_cli && npm run build
```

### Testing

```bash
# Rust tests
cd rust_tui && cargo test

# Overlay tests
cd rust_tui && cargo test --bin codex_overlay

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
