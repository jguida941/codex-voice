# Codex Voice CLI

TypeScript frontend for Codex Voice - provides the user interface for voice-enabled AI interaction.

## Quick Start

```bash
# Install dependencies
npm install

# Build
npm run build

# Run
npm start
```

## Using With Your Own Projects

Codex Voice works with any codebase - just run it from your project directory.

### Option 1: Command Line (All Platforms)

```bash
# Navigate to your project
cd ~/my-awesome-project

# Run codex-voice (use full path to start.sh)
/path/to/codex-voice/start.sh
```

Or install globally:
```bash
cd /path/to/codex-voice/ts_cli
npm link

# Now run from anywhere
cd ~/my-awesome-project
codex-voice
```

### Option 2: macOS App

Double-click **Codex Voice.app** - it will prompt you to select your project folder.

### Option 3: Windows

Double-click **start.bat** - it will prompt you to select your project folder.

### How It Works

Codex Voice uses your current working directory as the project context. When you ask questions or give commands, Codex/Claude will read and modify files in that directory.

## Requirements

- Node.js 18+
- Rust backend built (`../rust_tui/target/release/rust_tui`)

## Commands

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

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+R` | Start voice capture |
| `Ctrl+V` | Toggle auto-voice mode |
| `Ctrl+C` | Cancel/Exit |

## Architecture

```
src/
├── index.ts          # Main entry, input handling, event loop
├── bridge/
│   └── rust-ipc.ts   # JSON-IPC communication with Rust backend
└── ui/
    ├── banner.ts     # Welcome banner
    ├── colors.ts     # Theme and formatting
    └── spinner.ts    # Loading spinners
```

## IPC Protocol

Communication with Rust backend uses JSON-lines over stdin/stdout:

### Commands (TS → Rust)

```json
{"cmd": "send_prompt", "prompt": "hello"}
{"cmd": "start_voice"}
{"cmd": "set_provider", "provider": "claude"}
{"cmd": "cancel"}
```

### Events (Rust → TS)

```json
{"event": "capabilities", "mic_available": true, ...}
{"event": "token", "text": "Hello"}
{"event": "job_end", "provider": "claude", "success": true}
{"event": "transcript", "text": "user said this", "duration_ms": 1200}
```

## Development

```bash
# Run with debug output
DEBUG=1 npm start

# Watch mode (rebuild on changes)
npm run watch
```
