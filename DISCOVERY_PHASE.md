# Discovery Phase: Codex Voice Wrapper Requirements

## Phase 1: Discovery (No Coding Yet)

### 1. Current Codex Behavior Analysis

#### What We Need to Understand
- [ ] How Codex handles stdin/stdout in interactive mode
- [ ] When and how Codex prompts for approvals
- [ ] How Codex maintains session state across prompts
- [ ] What environment variables Codex expects
- [ ] How Codex handles tool execution and file operations

#### Testing Protocol
```bash
# Test 1: Basic interaction
echo "Hello" | codex

# Test 2: Interactive session
codex --interactive

# Test 3: Tool usage and approvals
# Send: "create a file test.txt"
# Observe: Does it ask for approval? How?

# Test 4: Session persistence
# Send multiple related prompts
# Observe: Does it remember context?
```

### 2. Concrete User Flows

#### Flow A: Single Voice Prompt
```
User: Presses Ctrl+R
System: Records 5 seconds of audio
System: Transcribes to "Create a Python hello world"
System: Sends to Codex
Codex: "I'll create a hello world script..."
Codex: [Tool: Write file hello.py]
Codex: "Approve? (y/n)"
User: Types 'y'
Codex: Creates file
System: Shows success
```

**Success Criteria**:
- Voice captured and transcribed < 2 seconds
- Codex output streams in real-time
- Approval prompts are interactive
- File created successfully

#### Flow B: Multi-Turn Session
```
Turn 1:
User: "What files are in this directory?"
Codex: Lists files

Turn 2:
User: "Open the README"
Codex: Shows README content
[Context maintained from Turn 1]

Turn 3:
User: "Add a section about installation"
Codex: Edits README
[Still has context from previous turns]
```

**Success Criteria**:
- Codex maintains context across turns
- No session reinitialization between prompts
- < 500ms latency between turns

#### Flow C: Approval Handling
```
User: "Delete all test files"
Codex: "This will delete 5 files: [list]"
Codex: "Approve? (y/n)"
System: Shows approval in UI
User: Reviews list, types 'n'
Codex: "Operation cancelled"
```

**Success Criteria**:
- Approval prompts surface to UI
- User can review before approving
- Denials are handled gracefully

### 3. Configuration Layer Specification

#### Config File Format (TOML)
```toml
# ~/.config/codex_voice/config.toml
[general]
version = "1.0"
first_run_complete = true

[codex]
# Required
command = "codex"  # or full path

# Optional with defaults
args = ["--interactive"]
env = { "CODEX_LOG_LEVEL" = "info" }
working_dir = "."  # or "inherit" for cwd
timeout_ms = 30000

[audio]
# Required
device = ":0"  # Platform-specific

# Optional with defaults
duration_seconds = 5
format = "wav"
sample_rate = 16000

[whisper]
# Mode 1: Local binary
mode = "binary"
command = "/opt/homebrew/bin/whisper"
model = "base"
language = "en"

# Mode 2: Server (alternative)
# mode = "server"
# url = "http://localhost:8080"
# api_key = "optional"

[ui]
theme = "dark"
show_timestamps = true
show_status_bar = true
```

#### Environment Detection Logic
```
1. Check for config file at:
   - $CODEX_VOICE_CONFIG (if set)
   - ~/.config/codex_voice/config.toml
   - ./codex_voice.toml (local override)

2. If no config exists:
   - Run interactive setup wizard
   - Auto-detect binaries in PATH
   - Verify each component works
   - Write config file

3. Validate on every startup:
   - All binaries exist and are executable
   - Audio device is available
   - Whisper model is downloaded
   - Codex responds to --version
```

#### Fallback Behavior
```
Missing ffmpeg:
  -> Error: "ffmpeg not found. Install with: brew install ffmpeg"

Missing whisper:
  -> Offer to install: "pip install openai-whisper"
  -> Or use server mode

Missing codex:
  -> Error: "codex not found. Install from: [URL]"

Missing config:
  -> Run setup wizard

Invalid config:
  -> Show specific error
  -> Offer to recreate
```

### 4. Success Criteria & Acceptance Tests

#### Installation Success
```bash
# Test 1: Clean installation
brew install codex-voice
codex-voice --version
# Expected: Version output, no errors

# Test 2: First run setup
codex-voice
# Expected: Setup wizard if no config
# Creates ~/.config/codex_voice/config.toml

# Test 3: Run from any directory
cd /tmp
codex-voice
# Expected: Starts successfully
# Uses codex in current directory
```

#### Integration Success
```bash
# Test 1: Voice capture works
# Start codex-voice
# Press Ctrl+R
# Speak "list files"
# Expected: Files listed via Codex

# Test 2: Session persistence
# Send first prompt
# Wait for response
# Send second related prompt
# Expected: Context maintained

# Test 3: Approval flow
# Send "delete test.txt"
# Expected: Approval prompt appears
# Can approve or deny
```

#### Performance Criteria
- Voice capture: < 100ms to start recording
- Transcription: < 2s for 5s of audio
- Codex response: Starts streaming < 500ms
- Between prompts: < 100ms latency
- Memory usage: < 100MB for wrapper

### 5. Technical Decisions Required

#### Binary Distribution
```
Option A: Rust binary via Cargo
  Pros: Single binary, fast, cross-platform
  Cons: Need Rust toolchain to build

Option B: Python package via pip
  Pros: Easy dependencies, wide ecosystem
  Cons: Requires Python runtime

Option C: Homebrew formula
  Pros: Handles all dependencies
  Cons: macOS only initially

Decision: _________________
```

#### Whisper Integration
```
Option A: Shell out to whisper CLI
  Pros: Simple, works with existing install
  Cons: Slow startup per transcription

Option B: Python binding (if Python)
  Pros: Fast, in-process
  Cons: Ties us to Python

Option C: whisper.cpp server
  Pros: Fast, language agnostic
  Cons: Additional setup

Decision: _________________
```

#### Codex Communication
```
Option A: PTY (pseudo-terminal)
  Pros: Full terminal emulation
  Cons: Platform-specific code

Option B: Pipes (stdin/stdout)
  Pros: Simple, portable
  Cons: May miss terminal features

Option C: API (if available)
  Pros: Cleanest integration
  Cons: May not exist yet

Decision: _________________
```

### 6. Validation Checklist

Before moving to Phase 2 (Implementation):

- [ ] All user flows documented and approved
- [ ] Config format finalized
- [ ] Binary detection logic specified
- [ ] Error messages defined
- [ ] Installation method chosen
- [ ] Success criteria measurable
- [ ] Performance targets set
- [ ] Technical decisions made

### 7. Risks & Unknowns

#### High Priority
- **Unknown**: How does Codex handle approval timeouts?
- **Risk**: PTY implementation differs per OS
- **Unknown**: Can we detect when Codex needs approval?

#### Medium Priority
- **Risk**: Whisper model download is large (140MB+)
- **Unknown**: Best way to handle Codex errors
- **Risk**: Audio device access requires permissions

#### Mitigation Plan
1. Build POC for Codex session management
2. Test on macOS, Linux, Windows
3. Create fallback for each integration point

## Phase 2: Implementation Plan

### Prerequisites
- [ ] Phase 1 Discovery complete
- [ ] Technical decisions documented
- [ ] Success criteria approved

### Implementation Stages

#### Stage 1: Core Session Manager (2 days)
```rust
// Deliverable: Stable Codex session
struct CodexSession {
    pty: PtyProcess,
    output_stream: Channel<String>,
}
```

#### Stage 2: Voice Pipeline (2 days)
```rust
// Deliverable: Audio -> Text pipeline
trait VoiceService {
    async fn capture() -> AudioBuffer;
    async fn transcribe(audio: AudioBuffer) -> String;
}
```

#### Stage 3: Configuration Layer (1 day)
```rust
// Deliverable: Config file handling
struct Config {
    fn load() -> Result<Config>;
    fn validate() -> Result<()>;
    fn setup_wizard() -> Result<Config>;
}
```

#### Stage 4: UI Integration (2 days)
```rust
// Deliverable: TUI with streaming
struct UI {
    fn stream_output(line: String);
    fn handle_approval() -> bool;
}
```

#### Stage 5: Packaging (2 days)
```bash
# Deliverable: Installable package
brew install codex-voice
# or
cargo install codex-voice
# or
pip install codex-voice
```

### Definition of Done

Each stage is complete when:
1. Tests pass (unit + integration)
2. Documentation updated
3. Error handling complete
4. Performance criteria met
5. Works on macOS (minimum)

## Next Actions

1. **You approve this discovery plan**
2. **We execute Phase 1 tests against real Codex**
3. **We make technical decisions based on findings**
4. **We get sign-off on requirements**
5. **Only then do we start coding**

This ensures we build the RIGHT thing, not just fix symptoms.