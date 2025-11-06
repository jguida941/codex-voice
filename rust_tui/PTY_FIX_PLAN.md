# PTY Integration Fix for Codex Voice TUI

## Problem Summary
The current TUI uses `codex exec` (non-interactive mode) which:
- Loses all Codex interactive features
- Can't stream output in real-time
- Doesn't have access to Codex's tools
- Creates an isolated environment

## Solution: Full PTY Integration

### 1. Add PTY Dependencies to Cargo.toml
```toml
[dependencies]
# Existing deps...
portable-pty = "0.8"  # Cross-platform PTY support
tokio = { version = "1", features = ["full"] }  # Async runtime
bytes = "1"  # For handling PTY output buffers
strip-ansi-escapes = "0.2"  # Optional: for cleaning output
```

### 2. Create PTY Manager Module

Create `src/pty_manager.rs`:
```rust
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct CodexPty {
    master: Box<dyn MasterPty>,
    child: Box<dyn Child>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}

impl CodexPty {
    pub fn spawn(config: &AppConfig) -> Result<Self> {
        let pty_system = native_pty_system();
        let pty_pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd = CommandBuilder::new(&config.codex_cmd);
        cmd.args(&config.codex_args);
        // Don't use exec - run in interactive mode

        let child = pty_pair.slave.spawn_command(cmd)?;
        // Set up reader/writer for bidirectional communication
    }

    pub async fn send_input(&mut self, text: &str) -> Result<()> {
        // Send text to Codex stdin
    }

    pub async fn read_output(&mut self) -> Result<String> {
        // Read from Codex stdout, preserving ANSI codes
    }
}
```

### 3. Modify Main Loop for Async Operation

Update `main.rs`:
```rust
#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_args()?;
    let mut app = App::new(config);

    // Spawn Codex in PTY
    let codex_pty = CodexPty::spawn(&app.config)?;
    let pty = Arc::new(Mutex::new(codex_pty));

    run_app_async(&mut app, pty).await
}

async fn run_app_async(app: &mut App, pty: Arc<Mutex<CodexPty>>) -> Result<()> {
    // Set up terminal (but keep in same screen, not alternate)
    enable_raw_mode()?;

    // Spawn background task to read Codex output
    let pty_reader = Arc::clone(&pty);
    tokio::spawn(async move {
        loop {
            let output = pty_reader.lock().await.read_output().await;
            // Update app.output with streaming content
        }
    });

    // Main event loop
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match handle_key_event_async(app, key, &pty).await? {
                    true => break,
                    false => {}
                }
            }
        }
    }
}
```

### 4. Fix Voice Input to Inject into PTY

```rust
async fn handle_key_event_async(
    app: &mut App,
    key: KeyEvent,
    pty: &Arc<Mutex<CodexPty>>
) -> Result<bool> {
    match key.code {
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Capture voice
            match capture_voice(app) {
                Ok(transcript) => {
                    // Send directly to Codex via PTY
                    pty.lock().await.send_input(&transcript).await?;
                    app.status = "Voice sent to Codex".into();
                }
                Err(err) => {
                    app.status = format!("Voice capture failed: {err:#}");
                }
            }
        }
        KeyCode::Enter => {
            // Send typed input to Codex
            pty.lock().await.send_input(&app.input).await?;
            app.input.clear();
        }
        // ... other keys
    }
}
```

### 5. Handle Streaming Output with ANSI Preservation

```rust
fn update_output_with_ansi(app: &mut App, raw_output: &str) {
    // Parse ANSI codes and convert to ratatui styles
    // Or use a library like `ansi-to-tui` to preserve formatting

    // Add to scrollback buffer
    for line in raw_output.lines() {
        app.output.push(line.to_string());
        if app.output.len() > 500 {
            app.output.remove(0);
        }
    }
}
```

### 6. Optional: Stay in Same Screen (No Alternate Screen)

```rust
fn run_app(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    // DON'T use EnterAlternateScreen - stay in main terminal
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // This keeps everything in the same terminal context
}
```

## Benefits of This Approach

1. **Full Codex Features**: All tools, commands, and features work
2. **Real-time Streaming**: See output as Codex generates it
3. **Proper Formatting**: ANSI colors, progress bars, everything preserved
4. **Seamless Voice**: Voice input goes directly to Codex as if typed
5. **No Mode Switching**: Stay in one interface with everything integrated

## Testing Plan

1. Test basic text input/output with PTY
2. Test voice capture and injection
3. Test Codex tool usage (file operations, web search, etc.)
4. Test ANSI color preservation
5. Test error handling and recovery

## Alternative: Use Existing PTY Helper

If implementing PTY in Rust is too complex, leverage the Python PTY helper that already exists:
- Spawn `python run_in_pty.py codex` as the subprocess
- Communicate with it via pipes
- This gives you PTY handling without implementing it yourself

## Next Steps

1. Add the PTY dependencies to Cargo.toml
2. Create the PTY manager module
3. Convert main loop to async
4. Test with simple Codex commands first
5. Add streaming output handling
6. Fix voice injection to go through PTY