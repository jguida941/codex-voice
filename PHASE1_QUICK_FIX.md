# Phase 1: Quick Fix - Session Persistence

## The Immediate Problem
The Enter key fails after the second voice capture because we're spawning a new Codex process each time, corrupting the TUI state. This is the most critical bug blocking usage.

## Quick Fix Approach (1-2 days)
Keep the Codex process alive between voice captures. This alone should fix the Enter key bug and reduce latency.

## Implementation Plan

### Step 1: Test Codex Behavior (30 min)
```bash
# Test if codex supports interactive mode
codex --help | grep -i interactive

# Test if we can keep codex alive
mkfifo /tmp/codex_pipe
codex < /tmp/codex_pipe &
echo "Hello" > /tmp/codex_pipe
# Does it respond and stay alive?

# Test PTY approach
python3 << 'EOF'
import pty
import os

master, slave = pty.openpty()
pid = os.fork()

if pid == 0:  # Child
    os.close(master)
    os.dup2(slave, 0)
    os.dup2(slave, 1)
    os.dup2(slave, 2)
    os.execvp("codex", ["codex"])
else:  # Parent
    os.close(slave)
    # Can we write multiple prompts?
    os.write(master, b"Hello\n")
    print(os.read(master, 1024))
    os.write(master, b"What is 2+2?\n")
    print(os.read(master, 1024))
EOF
```

### Step 2: Modify main.rs for Persistent Session

```rust
// Add to struct App
struct App {
    // ... existing fields ...
    codex_session: Option<CodexSession>,
}

struct CodexSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl App {
    fn ensure_codex_session(&mut self) -> Result<()> {
        if self.codex_session.is_none() {
            // Start Codex once
            let mut cmd = Command::new(&self.config.codex_cmd);
            cmd.args(&["exec", "--skip-git-repo-check", "-C", ".."]);
            cmd.stdin(Stdio::piped());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::null());

            let mut child = cmd.spawn()?;
            let stdin = child.stdin.take().unwrap();
            let stdout = BufReader::new(child.stdout.take().unwrap());

            self.codex_session = Some(CodexSession {
                child,
                stdin,
                stdout,
            });
        }
        Ok(())
    }

    fn send_to_codex(&mut self, prompt: &str) -> Result<String> {
        self.ensure_codex_session()?;

        if let Some(ref mut session) = self.codex_session {
            // Send prompt
            writeln!(session.stdin, "{}", prompt)?;
            session.stdin.flush()?;

            // Read response (with timeout)
            let mut response = String::new();
            // ... read logic with timeout ...

            Ok(response)
        } else {
            Err("Failed to establish Codex session")
        }
    }
}

// Modify existing call_codex to use persistent session
fn call_codex(&mut self, prompt: &str) -> Result<String> {
    self.send_to_codex(prompt)
}
```

### Step 3: Test the Fix

1. **Build and run**:
```bash
cd rust_tui
cargo build
cargo run -- --seconds 5 --whisper-cmd ../stubs/fake_whisper
```

2. **Test sequence**:
- Ctrl+R → Speak → Enter (should work)
- Ctrl+R → Speak → Enter (should ALSO work now)
- Repeat 5+ times to verify stability

3. **Verify session persistence**:
```bash
# Check process list - should see only ONE codex process
ps aux | grep codex | grep -v grep
```

### Step 4: Add Streaming Output

```rust
impl App {
    fn read_codex_output(&mut self) -> Result<()> {
        if let Some(ref mut session) = self.codex_session {
            // Non-blocking read
            let mut line = String::new();
            match session.stdout.read_line(&mut line) {
                Ok(0) => {} // No data
                Ok(_) => {
                    // Stream to output area
                    self.output_text.push_str(&line);
                    self.output_scroll = self.output_text.lines().count();
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

// In main loop
loop {
    // ... existing event handling ...

    // Check for Codex output
    app.read_codex_output()?;

    // ... rendering ...
}
```

## Success Criteria

### Must Have (Fixes Current Bug)
- [ ] Enter key works after multiple voice captures
- [ ] No TUI corruption
- [ ] Codex process stays alive

### Nice to Have (Better UX)
- [ ] Output streams in real-time
- [ ] Reduced latency (< 1s vs 5-6s)
- [ ] Session context maintained

## Testing Protocol

```bash
# Test 1: Multiple captures
for i in 1 2 3 4 5; do
    echo "Test $i: Ctrl+R, speak, Enter"
    # Verify Enter works each time
done

# Test 2: Process count
ps aux | grep codex | wc -l
# Should be 1, not multiple

# Test 3: Response time
time echo "Hello" | ./target/debug/rust_tui
# Should be < 1 second

# Test 4: Context persistence
# Send "my name is Bob"
# Then send "what is my name?"
# Should respond "Bob"
```

## Rollback Plan

If the persistent session causes issues:

1. Add feature flag:
```rust
const USE_PERSISTENT_SESSION: bool = true;

if USE_PERSISTENT_SESSION {
    self.send_to_codex(prompt)
} else {
    self.old_call_codex(prompt)  // Original implementation
}
```

2. Can disable with environment variable:
```bash
CODEX_VOICE_LEGACY=1 ./scripts/run_tui.sh
```

## Timeline

- **Hour 1-2**: Test Codex persistence approaches
- **Hour 3-6**: Implement CodexSession in main.rs
- **Hour 7-8**: Test with real voice captures
- **Hour 9-10**: Add streaming output
- **Hour 11-12**: Documentation and cleanup

Total: ~1.5 days with testing

## Decision Point

After this quick fix:

1. **If it works**: Enter key bug fixed, we continue with full redesign at measured pace
2. **If it fails**: We know Codex can't be kept alive, need different approach (like API)

Either way, we learn critical information about Codex's behavior without major refactoring.