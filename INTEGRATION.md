# Codex Voice Integration Guide

## Overview

Codex Voice is a universal voice-to-text wrapper for Anthropic's Codex CLI that works with **any project, in any directory, in any programming language**. It acts as a transparent layer between your voice and Codex, allowing hands-free coding assistance wherever you are in your terminal.

## How It Works as a Universal Wrapper

### Core Concept
```
Your Voice → Codex Voice → Transcription → Codex CLI → AI Response → Your Project
```

Codex Voice is **project-agnostic** - it simply:
1. Captures your voice input from any directory
2. Transcribes it to text using Whisper
3. Passes the text to Codex CLI (which handles the project context)
4. Displays Codex's response in a clean TUI interface

### Universal Usage

Once installed globally, you can use it from anywhere:

```bash
# In a Python project
cd ~/projects/my-python-app
codex-voice  # "Add error handling to the database connection"

# In a Rust project
cd ~/projects/my-rust-app
codex-voice  # "Refactor this to use async/await"

# In a React project
cd ~/projects/my-react-app
codex-voice  # "Create a new component for user authentication"

# Even in non-code directories
cd ~/Documents
codex-voice  # "Help me write a technical design document"
```

## Installation & Distribution Plan

### 1. **Global Installation Methods**

#### Cargo (Rust developers)
```bash
cargo install codex-voice
```

#### Homebrew (macOS/Linux)
```bash
brew tap codex-voice/tap
brew install codex-voice
```

#### Direct Binary
```bash
curl -sSL https://github.com/[user]/codex-voice/releases/latest/download/install.sh | sh
```

#### NPM (JavaScript developers)
```bash
npm install -g codex-voice-cli
```

### 2. **Configuration Management**

#### Configuration Hierarchy
The tool will check for configuration in this order:
1. Command-line arguments (highest priority)
2. Project-specific: `./.codex-voice.toml`
3. User global: `~/.config/codex-voice/config.toml`
4. System-wide: `/etc/codex-voice/config.toml`
5. Built-in defaults (lowest priority)

#### Example Global Config
```toml
# ~/.config/codex-voice/config.toml

[codex]
command = "codex"
args = ["--model", "o3-mini"]
auto_skip_git_check = true

[audio]
device = "default"  # or specific device name
sample_rate = 16000
channels = 1
duration_seconds = 8

[whisper]
model = "base"
language = "en"
# Use local model for privacy
local_model_path = "~/.cache/whisper/models/base.pt"

[ui]
theme = "dark"
auto_voice_mode = false
show_transcription = true
```

#### Project-Specific Override
```toml
# /path/to/project/.codex-voice.toml

[codex]
# Use a different model for this specific project
args = ["--model", "o3", "--context", "embedded-systems"]

[whisper]
# Technical jargon specific to this project
language = "en"
initial_prompt = "This is an embedded Rust project using ESP32"
```

### 3. **Cross-Platform Audio Handling**

#### Current Implementation
- macOS: `-f avfoundation -i :0`
- Linux: `-f pulse -i default`
- Windows: `-f dshow -i audio=Microphone`

#### Future Enhancement: Native Audio
```rust
// Using cpal for cross-platform audio
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// Automatically find the default input device
let host = cpal::default_host();
let device = host.default_input_device()
    .expect("No input device available");
```

### 4. **Multiple Backend Support**

#### Speech-to-Text Providers
```toml
[stt]
provider = "whisper"  # or "google", "azure", "deepgram", "local"

[stt.whisper]
api_key = "${OPENAI_API_KEY}"  # Environment variable expansion

[stt.local]
model_path = "~/.cache/whisper.cpp/models/ggml-base.bin"
```

#### LLM Providers (Future)
```toml
[llm]
provider = "codex"  # or "claude", "gpt4", "ollama"

[llm.codex]
command = "codex"

[llm.claude]
command = "claude"

[llm.ollama]
command = "ollama"
model = "codestral"
```

### 5. **Professional Features Roadmap**

#### Phase 1: Core Stability ✅ (Current)
- [x] Basic voice capture with FFmpeg
- [x] Whisper transcription
- [x] Codex CLI integration
- [x] TUI with real-time feedback
- [ ] Better error handling and recovery
- [ ] Cross-platform install script

#### Phase 2: Global Distribution (Next)
- [ ] Publish to crates.io
- [ ] GitHub Actions for multi-platform binaries
- [ ] Homebrew formula
- [ ] Config file support (`~/.config/codex-voice/`)
- [ ] Shell completions (bash, zsh, fish, powershell)

#### Phase 3: Enhanced Integration
- [ ] Multiple STT providers (Google, Azure, local)
- [ ] Session history (`~/.cache/codex-voice/history`)
- [ ] Custom voice commands/macros
- [ ] Shell integration (hotkeys to activate)
- [ ] Streaming responses
- [ ] Context awareness (auto-include relevant files)

#### Phase 4: Professional Features
- [ ] Team config sharing (Git-trackable configs)
- [ ] Plugin system for custom processors
- [ ] IDE extensions (VS Code, IntelliJ)
- [ ] Web UI option
- [ ] Voice training for better accuracy
- [ ] Multi-language support

#### Phase 5: Enterprise Ready
- [ ] Fully local/offline mode
- [ ] Corporate proxy support
- [ ] LDAP/SSO integration
- [ ] Usage analytics and reporting
- [ ] Audit logging
- [ ] Role-based access control

### 6. **Security & Privacy**

#### Local-First Architecture
```toml
[privacy]
mode = "local"  # Options: "local", "hybrid", "cloud"

[privacy.local]
stt_provider = "whisper.cpp"
llm_provider = "ollama"
never_send_audio = true
never_send_context = true
```

#### Credential Management
- Integrate with system keychains (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- Never store API keys in plain text
- Support environment variables with `${VAR}` expansion

### 7. **Developer Experience**

#### One-Line Install
```bash
# Detect OS and architecture, download appropriate binary
curl -sSL https://codex-voice.dev/install | sh
```

#### Shell Integration
```bash
# Add to ~/.zshrc or ~/.bashrc
eval "$(codex-voice shell-init)"

# Now use Ctrl+Shift+V to activate voice input from anywhere
```

#### IDE Plugins
- **VS Code**: Command palette integration
- **Vim/Neovim**: `:CodxVoice` command
- **IntelliJ**: Tool window with voice button

### 8. **Performance Optimizations**

#### Background Processing
```rust
// Start recording before user finishes speaking
let recorder = AudioRecorder::new();
recorder.start_background();

// Process previous recording while new one captures
while let Some(audio) = recorder.get_chunk() {
    transcriber.process_async(audio);
}
```

#### Smart Caching
- Cache frequently used commands
- Pre-load Whisper model in memory
- Keep Codex connection warm

### 9. **Testing Strategy**

#### Unit Tests
```bash
cargo test --lib
```

#### Integration Tests
```bash
cargo test --test integration
```

#### Platform Matrix CI
```yaml
# .github/workflows/test.yml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    rust: [stable, beta]
```

### 10. **Documentation**

#### Quick Start (5 minutes)
```bash
# Install
brew install codex-voice

# Configure (optional)
codex-voice config init

# Run from any project
codex-voice

# Or use with hotkey (after shell init)
# Press Ctrl+Shift+V and start speaking
```

#### Example Workflows

**Debugging Session**
```bash
cd my-project
codex-voice
# "Why is this function returning null?"
# "Add debug logging to track the issue"
# "Run the tests and show me what fails"
```

**Code Review**
```bash
codex-voice
# "Review this PR and suggest improvements"
# "What security issues do you see?"
# "How can I make this more performant?"
```

**Documentation**
```bash
codex-voice
# "Generate API docs for this module"
# "Write a README for this project"
# "Create examples for each function"
```

## Architecture Benefits

### Why This Design Works Everywhere

1. **No Project Modification Required**: Unlike project-specific tools, Codex Voice requires zero changes to your existing projects

2. **Language Agnostic**: Works with any programming language that Codex supports

3. **Context Preserved**: Codex CLI handles all the project context - Codex Voice just provides the voice interface

4. **Flexible Configuration**: Global defaults with project-specific overrides means it works out-of-the-box but is customizable

5. **Tool Agnostic**: Can wrap any CLI tool in the future, not just Codex (Claude, GPT-4, etc.)

## Migration Path for Current Users

If you're currently using the Python prototype:

1. **Install the Rust version globally**:
   ```bash
   cargo install codex-voice
   ```

2. **Copy your configuration** (if any):
   ```bash
   mkdir -p ~/.config/codex-voice
   cp ~/old-codex-voice/config.toml ~/.config/codex-voice/
   ```

3. **Remove the old prototype**:
   ```bash
   rm -rf ~/old-codex-voice
   ```

4. **Use from anywhere**:
   ```bash
   cd ~/any/project
   codex-voice
   ```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT - See [LICENSE](LICENSE) for details.