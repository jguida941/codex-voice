# Codex Voice Architecture Redesign

## Executive Summary

The current implementation is fundamentally backwards. We're fighting against Codex's design instead of working with it. Every voice capture spawns new processes, loses context, and requires increasingly complex workarounds. This document outlines the correct architecture.

## Current Problems (Your Analysis is 100% Correct)

### 1. Process Lifecycle Disaster
**Problem**: Every request spawns fresh codex, ffmpeg, and whisper processes
- Codex never maintains session state
- Lost context after each interaction
- Lost approvals and tool access
- Forces PTY juggling and exec fallbacks

**Impact**: The Enter key bug is a SYMPTOM of this - we're corrupting state because we keep reinitializing everything.

### 2. Environment Assumptions
**Problem**: Hardcoded paths and binaries
- Assumes whisper in `.venv/bin/`
- Assumes codex on PATH
- No discovery or verification
- User has to manually fix paths

**Impact**: "It doesn't work" issues are mostly path problems we shouldn't have.

### 3. Output Handling
**Problem**: Buffering destroys interactivity
- Hide Codex's real-time output
- Make approvals impossible
- Force --skip-git-repo-check workarounds
- Silent waits with no feedback

**Impact**: Users can't see what Codex is doing, can't approve operations.

### 4. Packaging Nightmare
**Problem**: No real installation process
- Scripts scattered in repo
- Manual path configuration
- Can't drop into other projects
- No pip/homebrew/cargo package

**Impact**: Users copy commands by hand, nothing "just works".

## The Correct Architecture

### Core Principle
**Codex is the application, we are a thin input layer**. We should:
1. Start Codex once
2. Keep it alive
3. Stream its output
4. Inject voice-transcribed prompts
5. Never alter its behavior

### Architecture Components

```
┌─────────────────────────────────────────────────────────┐
│                     User Interface                       │
│  (TUI with streaming output, input field, status bar)    │
└─────────────────┬───────────────────┬───────────────────┘
                  │                   │
         ┌────────▼────────┐  ┌──────▼──────┐
         │ Voice Pipeline  │  │   Keyboard  │
         │    Service      │  │    Input    │
         └────────┬────────┘  └──────┬──────┘
                  │                   │
         ┌────────▼───────────────────▼──────┐
         │     Session Manager               │
         │  - Maintains single Codex PTY     │
         │  - Streams output to UI           │
         │  - Injects prompts from any source│
         │  - Preserves all Codex state      │
         └────────────────┬──────────────────┘
                          │
         ┌────────────────▼──────────────────┐
         │        Codex Process              │
         │  (Long-lived, stable session)     │
         └───────────────────────────────────┘
```

### 1. Stable Codex Session

```rust
struct CodexSession {
    pty: PtyProcess,
    state: SessionState,
    output_stream: mpsc::Sender<String>,
}

impl CodexSession {
    fn start() -> Result<Self> {
        // Start Codex ONCE with proper PTY
        let pty = PtyProcess::spawn("codex", &["--interactive"])?;

        // Stream output continuously
        let output_stream = spawn_output_reader(pty.stdout);

        Ok(Self { pty, state: Active, output_stream })
    }

    fn send_prompt(&mut self, text: &str) -> Result<()> {
        // Just write to PTY stdin
        self.pty.stdin.write_all(text.as_bytes())?;
        Ok(())
    }
}
```

### 2. Voice Pipeline as Service

```rust
struct VoiceService {
    whisper: WhisperServer,  // Long-lived, model loaded once
}

impl VoiceService {
    async fn transcribe(&self, audio: AudioBuffer) -> Result<String> {
        // Whisper server already running, just send audio
        self.whisper.transcribe(audio).await
    }
}
```

### 3. Streaming Output

```rust
fn stream_codex_output(pty: &mut PtyProcess, ui: &mut UI) {
    let reader = BufReader::new(&pty.stdout);
    for line in reader.lines() {
        let line = line?;

        // Stream to UI immediately
        ui.append_output(&line);

        // Also log for debugging
        log::debug!("Codex: {}", line);

        // Let UI handle any needed responses
        if line.contains("Approve?") {
            ui.show_approval_prompt();
        }
    }
}
```

### 4. Proper Configuration

```toml
# ~/.config/codex_voice/config.toml

[codex]
command = "codex"
args = ["--interactive"]
working_dir = "."

[whisper]
command = "/opt/homebrew/bin/whisper"
model = "base"
# OR use server mode
server_url = "http://localhost:8080"

[audio]
device = ":0"  # macOS default
duration = 5
format = "wav"

[ui]
theme = "dark"
show_status_bar = true
```

### 5. Installation That Works

```bash
# Method 1: Homebrew
brew tap codex-voice/tap
brew install codex-voice

# Method 2: Cargo
cargo install codex-voice

# Method 3: pip
pip install codex-voice

# First run auto-configures
codex-voice init
# > Found ffmpeg at /usr/local/bin/ffmpeg ✓
# > Found whisper at /opt/homebrew/bin/whisper ✓
# > Found codex at /usr/local/bin/codex ✓
# > Configuration saved to ~/.config/codex_voice/config.toml

# Then from ANY directory
codex-voice
# Just works. No path juggling.
```

## Migration Path

### Phase 1: Fix Current Critical Issues (1-2 days)
- Keep Codex session alive between voice captures
- Stream output instead of buffering
- This alone fixes the Enter key bug

### Phase 2: Proper Service Architecture (3-5 days)
- Extract VoiceService with persistent whisper
- Create SessionManager for Codex PTY
- Implement streaming pipeline

### Phase 3: Configuration Layer (2-3 days)
- Config file discovery and validation
- Dependency verification on startup
- Clean error messages for missing tools

### Phase 4: Packaging (2-3 days)
- Create Cargo.toml for binary distribution
- Setup GitHub releases with CI
- Document installation process

## Why This Fixes Everything

1. **Enter key bug**: Goes away because we're not corrupting state between captures
2. **5-6 second delays**: Gone, Codex stays warm
3. **Lost approvals**: Fixed, session persists
4. **Path issues**: Config layer handles discovery
5. **Can't see output**: Streaming shows everything
6. **Hard to install**: Package manager handles it

## Immediate Next Steps

1. **Agree on this design** - This is the correct path
2. **Start with Phase 1** - Quick win, fixes current bugs
3. **Document requirements** - Lock down expectations
4. **Begin refactor** - SessionManager first

## Conclusion

Your diagnosis is perfect. We've been building workarounds instead of a proper integration. The current bugs (Enter key, F2/Alt+R not working, delays) are all symptoms of fighting against Codex's design instead of embracing it.

Let's stop hacking and build this correctly.