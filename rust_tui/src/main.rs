//! Terminal UI shell for the `codex_voice` pipeline. It mirrors the Python
//! prototype but wraps it in a full-screen experience driven by `ratatui`.

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tempfile::TempDir;
use unicode_width::UnicodeWidthStr;

/// Maximum number of lines to retain in the scrollback buffer.
const OUTPUT_MAX_LINES: usize = 500;

/// Simple debug logging to file (doesn't interfere with TUI)
fn log_debug(msg: &str) {
    use std::fs::OpenOptions;

    let log_path = env::temp_dir().join("codex_voice_tui.log");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

/// Parse CLI arguments, bootstrap the application state, and launch the UI.
fn main() -> Result<()> {
    // Initialize debug logging
    let log_path = env::temp_dir().join("codex_voice_tui.log");
    log_debug("=== Codex Voice TUI Started ===");
    log_debug(&format!("Log file: {:?}", log_path));

    let config = AppConfig::from_args()?;
    let mut app = App::new(config);

    let result = run_app(&mut app);

    log_debug("=== Codex Voice TUI Exiting ===");
    if let Err(ref e) = result {
        log_debug(&format!("Exit with error: {:#}", e));
    }

    result
}

/// Configure the terminal, run the main loop, and restore the screen on exit.
fn run_app(app: &mut App) -> Result<()> {
    enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("failed to enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("failed to init terminal backend")?;

    let result = app_loop(&mut terminal, app);

    disable_raw_mode().context("failed to disable raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)
        .context("failed to leave alternate screen")?;
    terminal.show_cursor().context("failed to show cursor")?;

    result
}

/// Redraw the interface and dispatch input events until the user exits.
fn app_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui(frame, app))?;

        if event::poll(Duration::from_millis(250)).context("failed to poll events")? {
            if let Event::Key(key) = event::read().context("failed to read key event")? {
                if handle_key_event(app, key)? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Render the three main UI regions: output scrollback, input line, and status.
fn ui(frame: &mut ratatui::Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(frame.size());

    let output_text = if app.output.is_empty() {
        Text::from("No Codex output yet. Press Ctrl+R to capture voice or type and press Enter.")
    } else {
        // Convert each output line to a Line, preserving individual lines
        let lines: Vec<Line> = app.output.iter()
            .map(|s| Line::from(s.as_str()))
            .collect();
        Text::from(lines)
    };
    let output = Paragraph::new(output_text)
        .block(Block::default().title("Output").borders(Borders::ALL))
        .wrap(Wrap { trim: true })
        .scroll((app.scroll_offset, 0));
    frame.render_widget(output, chunks[0]);

    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().title("Prompt").borders(Borders::ALL));
    frame.render_widget(input, chunks[1]);

    let status_text = format!(
        "[Ctrl+C] Quit • [F2/Alt+R/Ctrl+R] Voice{} • [↑↓] Scroll • {}",
        if app.voice_enabled { " (auto)" } else { "" },
        app.status
    );
    let status =
        Paragraph::new(status_text).block(Block::default().title("Status").borders(Borders::ALL));
    frame.render_widget(status, chunks[2]);

    // Align the cursor with the visual width of the input, accounting for wide glyphs.
    let cursor_x = chunks[1].x + app.input.width() as u16 + 1;
    let cursor_y = chunks[1].y + 1;
    frame.set_cursor(cursor_x, cursor_y);
}

/// Temporarily exit TUI mode to run a function in normal terminal mode.
/// This prevents terminal corruption when running external commands.
fn with_normal_terminal<F, T>(f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>
{
    // Ensure we flush any pending events before and after voice capture
    // This prevents the Enter key from getting "stuck" in the event queue
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read();  // Clear any pending events
    }

    let result = f()?;

    // Clear events again after voice capture
    while event::poll(Duration::from_millis(0))? {
        let _ = event::read();
    }

    Ok(result)
}

/// Process a key press and update application state; returns true to quit.
fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    // Log all key events for debugging
    log_debug(&format!("Key event: {:?} with modifiers: {:?}", key.code, key.modifiers));

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(true);
    }

    match key.code {
        // Multiple key bindings for voice capture to avoid IDE conflicts
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Use terminal wrapper to prevent corruption during voice capture
            log_debug("Ctrl+R pressed, starting voice capture");
            app.status = "Switching to voice capture mode...".into();

            match with_normal_terminal(|| capture_voice(app)) {
                Ok(transcript) => {
                    log_debug(&format!("Voice capture success: '{}'", transcript));
                    // Clear input first to ensure clean state
                    app.input.clear();
                    // Set the new transcript
                    app.input = transcript.clone();
                    app.status = "Transcript captured; edit and press Enter.".into();
                    log_debug(&format!("Input field now contains: '{}' (len={})",
                                      app.input, app.input.len()));
                }
                Err(err) => {
                    log_debug(&format!("Voice capture failed: {:#}", err));
                    app.status = format!("Voice capture failed: {err:#}");
                }
            }
            log_debug(&format!("After voice capture - Input: '{}', Status: '{}'", app.input, app.status));
        }
        // Alternative: Alt+R (less likely to conflict with IDE)
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::ALT) => {
            log_debug("Alt+R pressed, starting voice capture");
            app.status = "Switching to voice capture mode...".into();

            match with_normal_terminal(|| capture_voice(app)) {
                Ok(transcript) => {
                    log_debug(&format!("Voice capture success (Alt+R): '{}'", transcript));
                    app.input = transcript;
                    app.status = "Transcript captured; edit and press Enter.".into();
                }
                Err(err) => {
                    log_debug(&format!("Voice capture failed: {:#}", err));
                    app.status = format!("Voice capture failed: {err:#}");
                }
            }
        }
        // Another alternative: F2 key (rarely used by IDEs)
        KeyCode::F(2) => {
            log_debug("F2 pressed, starting voice capture");
            app.status = "Recording voice...".into();

            match with_normal_terminal(|| capture_voice(app)) {
                Ok(transcript) => {
                    log_debug(&format!("Voice capture success (F2): '{}'", transcript));
                    app.input = transcript;
                    app.status = "Transcript captured; edit and press Enter.".into();
                }
                Err(err) => {
                    log_debug(&format!("Voice capture failed: {:#}", err));
                    app.status = format!("Voice capture failed: {err:#}");
                }
            }
        }
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.voice_enabled = !app.voice_enabled;
            if app.voice_enabled {
                // Use terminal wrapper for voice capture
                match with_normal_terminal(|| capture_voice(app)) {
                    Ok(transcript) => {
                        app.input = transcript;
                        app.status = "Voice mode enabled; edit and press Enter.".into();
                    }
                    Err(err) => {
                        app.voice_enabled = false;
                        app.status = format!(
                            "Voice mode failed to start; disabled voice mode. Error: {err:#}"
                        );
                    }
                }
            } else {
                app.status = "Voice mode disabled.".into();
            }
        }
        KeyCode::Enter => {
            log_debug(&format!("Enter pressed. Input: '{}' (len={}, trimmed_len={})",
                              app.input, app.input.len(), app.input.trim().len()));

            // Ensure input is properly trimmed
            let trimmed_input = app.input.trim();
            if trimmed_input.is_empty() {
                log_debug("Enter pressed but input is empty after trimming");
                app.status = "Nothing to send; input is empty.".into();
            } else {
                log_debug(&format!("Attempting to send: '{}'", trimmed_input));
                if let Some(output) = send_prompt(app)? {
                    app.append_output(output);
                    log_debug("Prompt sent successfully and output appended");
                } else {
                    log_debug("send_prompt returned None");
                }
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Esc => {
            app.input.clear();
        }
        KeyCode::Char(c) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                app.input.push(c);
            }
        }
        KeyCode::Delete => {
            app.input.clear();
        }
        KeyCode::Up => {
            // Scroll up in output
            if app.scroll_offset > 0 {
                app.scroll_offset = app.scroll_offset.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            // Scroll down in output
            app.scroll_offset = app.scroll_offset.saturating_add(1);
        }
        KeyCode::PageUp => {
            // Scroll up by page
            app.scroll_offset = app.scroll_offset.saturating_sub(10);
        }
        KeyCode::PageDown => {
            // Scroll down by page
            app.scroll_offset = app.scroll_offset.saturating_add(10);
        }
        KeyCode::Home => {
            // Scroll to top
            app.scroll_offset = 0;
        }
        KeyCode::End => {
            // Scroll to bottom
            app.scroll_offset = app.output.len().saturating_sub(10) as u16;
        }
        _ => {}
    }

    Ok(false)
}

/// Dispatch the current prompt to the Codex CLI and format the output lines.
fn send_prompt(app: &mut App) -> Result<Option<Vec<String>>> {
    let prompt = app.input.trim().to_string();
    if prompt.is_empty() {
        app.status = "Nothing to send; prompt is empty.".into();
        return Ok(None);
    }

    app.status = "Calling Codex…".into();
    let codex_output = call_codex(&app.config, &prompt)?;

    let mut lines = Vec::new();
    lines.push(format!("> {}", prompt));
    lines.extend(codex_output.lines().map(|line| line.to_string()));
    app.status = format!("Codex returned {} lines.", codex_output.lines().count());
    app.input.clear();

    if app.voice_enabled {
        // Use terminal wrapper for voice capture in continuous mode
        match with_normal_terminal(|| capture_voice(app)) {
            Ok(transcript) => {
                app.input = transcript;
                app.status = "Voice capture ready; edit the prompt or press Enter to send.".into();
            }
            Err(err) => {
                app.status = format!("Voice capture failed after Codex call: {err:#}");
            }
        }
    }

    Ok(Some(lines))
}

/// Capture audio and return its transcript while updating the status message.
fn capture_voice(app: &mut App) -> Result<String> {
    app.status = format!("Recording voice for {} seconds... (speak now)", app.config.seconds);

    // Wrap voice_to_text to catch and log errors
    let result = match voice_to_text(&app.config) {
        Ok(transcript) => {
            if transcript.is_empty() {
                app.status = "No speech detected. Try again.".into();
                Ok(String::new())
            } else {
                app.status = "Transcript captured! Edit if needed and press Enter to send.".into();
                Ok(transcript)
            }
        }
        Err(e) => {
            let error_msg = format!("Voice capture error: {}", e);
            log_debug(&error_msg);
            app.status = error_msg.clone();
            Err(anyhow::anyhow!(error_msg))
        }
    };

    result
}

/// Record audio and return the Whisper transcript using a scoped temp folder.
fn voice_to_text(config: &AppConfig) -> Result<String> {
    let tmp = TempDir::new().context("failed to create temp dir")?;
    let wav_path = tmp.path().join("audio.wav");
    record_wav(config, &wav_path)?;
    let transcript = transcribe(config, &wav_path, tmp.path())?;
    Ok(transcript)
}

/// Spawn ffmpeg with platform-specific capture flags to produce the WAV file.
fn record_wav(config: &AppConfig, wav_path: &Path) -> Result<()> {
    log_debug(&format!("Starting ffmpeg recording to {:?}", wav_path));

    let mut cmd = Command::new(&config.ffmpeg_cmd);
    cmd.arg("-y");

    // Suppress ffmpeg output to prevent TUI corruption
    cmd.args(["-loglevel", "quiet", "-nostats"]);

    match env::consts::OS {
        "macos" => {
            let device = config.ffmpeg_device.as_deref().unwrap_or(":0");
            cmd.args(["-f", "avfoundation", "-i", device]);
        }
        "linux" => {
            let device = config.ffmpeg_device.as_deref().unwrap_or("default");
            cmd.args(["-f", "pulse", "-i", device]);
        }
        "windows" => {
            let device = config
                .ffmpeg_device
                .as_deref()
                .unwrap_or("audio=Microphone (Default)");
            cmd.args(["-f", "dshow", "-i", device]);
        }
        other => bail!("unsupported OS for ffmpeg capture: {other}"),
    }

    cmd.args([
        "-t",
        &config.seconds.to_string(),
        "-ac",
        "1",
        "-ar",
        "16000",
        "-vn",
    ]);
    cmd.arg(wav_path);

    // Redirect stderr to null to prevent any output from breaking the TUI
    cmd.stderr(Stdio::null());
    cmd.stdout(Stdio::null());

    log_debug(&format!("Running ffmpeg for {} seconds", config.seconds));
    exec_command_silent(&mut cmd).context("ffmpeg capture failed")?;
    log_debug("ffmpeg recording completed successfully");
    Ok(())
}

/// Translate the captured audio into text via either whisper or whisper.cpp.
fn transcribe(config: &AppConfig, wav_path: &Path, tmp_dir: &Path) -> Result<String> {
    log_debug(&format!("Starting transcription of {:?}", wav_path));

    let exe = Path::new(&config.whisper_cmd)
        .file_name()
        .and_then(OsStr::to_str)
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if exe.starts_with("whisper") {
        let mut cmd = Command::new(&config.whisper_cmd);
        cmd.arg(wav_path)
            .args(["--language", &config.lang])
            .args(["--model", &config.whisper_model])
            .args(["--output_format", "txt"])
            .args(["--output_dir", tmp_dir.to_string_lossy().as_ref()]);

        // Suppress whisper output to prevent TUI corruption
        cmd.stderr(Stdio::null());
        cmd.stdout(Stdio::null());

        exec_command_silent(&mut cmd).context("whisper transcription failed")?;

        let txt_path = tmp_dir.join(
            wav_path
                .file_stem()
                .and_then(OsStr::to_str)
                .ok_or_else(|| anyhow::anyhow!("invalid wav file name"))?
                .to_string()
                + ".txt",
        );
        let transcript = fs::read_to_string(&txt_path)
            .with_context(|| format!("failed to read transcript {txt_path:?}"))?;
        log_debug(&format!("Transcription complete: '{}'", transcript.trim()));
        Ok(transcript.trim().to_string())
    } else {
        let model_path = config
            .whisper_model_path
            .as_ref()
            .context("whisper.cpp requires --whisper-model-path")?;
        let base = tmp_dir.join("transcript");
        let mut cmd = Command::new(&config.whisper_cmd);
        cmd.args(["-m", model_path])
            .args(["-f", wav_path.to_string_lossy().as_ref()])
            .args(["-l", &config.lang])
            .args(["-otxt"])
            .args(["-of", base.to_string_lossy().as_ref()]);

        // Suppress whisper.cpp output to prevent TUI corruption
        cmd.stderr(Stdio::null());
        cmd.stdout(Stdio::null());

        exec_command_silent(&mut cmd).context("whisper.cpp transcription failed")?;

        let txt_path = PathBuf::from(format!("{}.txt", base.to_string_lossy()));
        let transcript = fs::read_to_string(&txt_path)
            .with_context(|| format!("failed to read transcript {txt_path:?}"))?;
        log_debug(&format!("Transcription complete (whisper.cpp): '{}'", transcript.trim()));
        Ok(transcript.trim().to_string())
    }
}

/// Run the Codex CLI and return its stdout using PTY for proper interactive support.
fn call_codex(config: &AppConfig, prompt: &str) -> Result<String> {
    // ALWAYS try PTY helper first for full interactive mode
    // This preserves all Codex features (tools, formatting, streaming, etc.)

    // Use the current working directory for Codex (where user actually is)
    let codex_working_dir = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."));

    if config.pty_helper.exists() {
        match try_python_pty(config, prompt, &codex_working_dir)? {
            Some(PtyResult::Success(text)) => return Ok(text),
            Some(PtyResult::Failure(_msg)) => {
                // PTY failed, but continue to fallbacks
                // Don't use eprintln in TUI mode - it corrupts the display
            }
            None => {
                // PTY helper not available
            }
        }
    }

    // Fallback 1: Try running Codex in interactive mode without exec
    let mut interactive_cmd = Command::new(&config.codex_cmd);
    interactive_cmd
        .args(&config.codex_args)
        .arg("-C")
        .arg(&codex_working_dir)
        .env("TERM", &config.term_value)
        .env("CODEX_NONINTERACTIVE", "1") // Hint to Codex that we're piping
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut interactive_child = interactive_cmd
        .spawn()
        .with_context(|| format!("failed to spawn interactive {}", config.codex_cmd))?;

    if let Some(mut stdin) = interactive_child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to codex stdin")?;
        stdin
            .write_all(b"\n")
            .context("failed to write newline")?;
    }

    let interactive_output = interactive_child
        .wait_with_output()
        .context("failed to wait for interactive codex process")?;

    if interactive_output.status.success() {
        return Ok(String::from_utf8_lossy(&interactive_output.stdout).to_string());
    }

    let interactive_stderr = String::from_utf8_lossy(&interactive_output.stderr).to_string();

    // Fallback 2: Use codex exec with full permissions
    let mut exec_cmd = Command::new(&config.codex_cmd);
    exec_cmd
        .arg("exec")
        // Removed --skip-git-repo-check to allow full tool access
        .arg("-C")
        .arg(&codex_working_dir)
        .args(&config.codex_args)
        .env("TERM", &config.term_value)
        .arg("-") // Read from stdin
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut exec_child = exec_cmd
        .spawn()
        .with_context(|| format!("failed to spawn {} exec", config.codex_cmd))?;

    if let Some(mut stdin) = exec_child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to codex exec stdin")?;
    }

    let exec_output = exec_child
        .wait_with_output()
        .context("failed to wait for codex exec process")?;

    if exec_output.status.success() {
        // Using codex exec mode as fallback
        return Ok(String::from_utf8_lossy(&exec_output.stdout).to_string());
    }

    bail!("All codex invocation methods failed:\n\
           PTY Helper: Check if scripts/run_in_pty.py exists\n\
           Interactive: {}\n\
           Exec mode: {}",
          interactive_stderr.trim(),
          String::from_utf8_lossy(&exec_output.stderr).trim())
}

enum PtyResult {
    Success(String),
    Failure(String),
}

fn try_python_pty(config: &AppConfig, prompt: &str, working_dir: &Path) -> Result<Option<PtyResult>> {
    if !config.pty_helper.exists() {
        return Ok(None);
    }

    let mut cmd = Command::new(&config.python_cmd);
    cmd.arg(&config.pty_helper);
    cmd.arg("--stdin");
    cmd.arg(&config.codex_cmd);
    cmd.arg("-C");
    cmd.arg(working_dir);
    cmd.args(&config.codex_args);
    cmd.env("TERM", &config.term_value);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to run PTY helper {}", config.pty_helper.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        let mut payload = prompt.as_bytes().to_vec();
        if !prompt.ends_with('\n') {
            payload.push(b'\n');
        }
        stdin
            .write_all(&payload)
            .context("failed writing prompt to PTY helper")?;
    }

    let output = child
        .wait_with_output()
        .context("failed waiting for PTY helper")?;

    if output.status.success() {
        return Ok(Some(PtyResult::Success(
            String::from_utf8_lossy(&output.stdout).to_string(),
        )));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let msg = if stderr.is_empty() {
        format!(
            "PTY helper exit {}: {}",
            output.status,
            config.pty_helper.display()
        )
    } else {
        format!("PTY helper exit {}: {}", output.status, stderr)
    };
    Ok(Some(PtyResult::Failure(msg)))
}

/// Run a command silently (with stdout/stderr already redirected), checking only exit status.
fn exec_command_silent(cmd: &mut Command) -> Result<()> {
    let cmd_str = format_command(cmd);
    let status = cmd
        .status()
        .with_context(|| format!("failed to run {cmd_str}"))?;
    if !status.success() {
        bail!("command `{cmd_str}` failed with exit status: {}", status);
    }
    Ok(())
}

/// Convert the program and its arguments into a shell-style string.
fn format_command(cmd: &Command) -> String {
    let mut parts = Vec::new();
    parts.push(cmd.get_program().to_string_lossy().to_string());
    for arg in cmd.get_args() {
        parts.push(arg.to_string_lossy().to_string());
    }
    parts.join(" ")
}

/// Runtime configuration derived from CLI flags.
#[derive(Debug, Clone)]
struct AppConfig {
    codex_cmd: String,
    codex_args: Vec<String>,
    python_cmd: String,
    term_value: String,
    pty_helper: PathBuf,
    whisper_cmd: String,
    whisper_model: String,
    whisper_model_path: Option<String>,
    ffmpeg_cmd: String,
    ffmpeg_device: Option<String>,
    seconds: u64,
    lang: String,
}

impl AppConfig {
    /// Parse command-line flags into an `AppConfig`, validating required values.
    fn from_args() -> Result<Self> {
        let mut config = Self::default();
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--codex-cmd" => {
                    config.codex_cmd = args.next().context("missing value for --codex-cmd")?;
                }
                "--codex-arg" => {
                    config
                        .codex_args
                        .push(args.next().context("missing value for --codex-arg")?);
                }
                "--codex-args" => {
                    let value = args.next().context("missing value for --codex-args")?;
                    config
                        .codex_args
                        .extend(value.split_whitespace().map(|s| s.to_string()));
                }
                "--python-cmd" => {
                    config.python_cmd = args.next().context("missing value for --python-cmd")?;
                }
                "--term" => {
                    config.term_value = args.next().context("missing value for --term")?;
                }
                "--pty-helper" => {
                    let value = args.next().context("missing value for --pty-helper")?;
                    config.pty_helper = PathBuf::from(value);
                }
                "--whisper-cmd" => {
                    config.whisper_cmd = args.next().context("missing value for --whisper-cmd")?;
                }
                "--whisper-model" => {
                    config.whisper_model =
                        args.next().context("missing value for --whisper-model")?;
                }
                "--whisper-model-path" => {
                    config.whisper_model_path = Some(
                        args.next()
                            .context("missing value for --whisper-model-path")?,
                    );
                }
                "--ffmpeg-cmd" => {
                    config.ffmpeg_cmd = args.next().context("missing value for --ffmpeg-cmd")?;
                }
                "--ffmpeg-device" => {
                    config.ffmpeg_device =
                        Some(args.next().context("missing value for --ffmpeg-device")?);
                }
                "--seconds" => {
                    let value = args.next().context("missing value for --seconds")?;
                    config.seconds = value.parse().context("invalid integer for --seconds")?;
                }
                "--lang" => {
                    config.lang = args.next().context("missing value for --lang")?;
                }
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => bail!("unrecognized argument: {other}"),
            }
        }
        Ok(config)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            codex_cmd: "codex".to_string(),
            codex_args: Vec::new(),
            python_cmd: "python3".to_string(),
            term_value: env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
            pty_helper: Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/run_in_pty.py"),
            whisper_cmd: "whisper".to_string(),
            whisper_model: "small".to_string(),
            whisper_model_path: None,
            ffmpeg_cmd: "ffmpeg".to_string(),
            ffmpeg_device: None,
            seconds: 5,
            lang: "en".to_string(),
        }
    }
}

/// Print a short usage message and exit when the user asks for help.
fn print_usage() {
    println!(
        "codex-voice-tui\n\
         Usage: rust_tui [options]\n\n\
         Options:\n\
           --codex-cmd <path>            Path to Codex CLI (default: codex)\n\
          --codex-arg <value>           Append a single argument for the Codex CLI (repeatable)\n\
          --codex-args \"<values>\"      Append a whitespace-separated list of Codex CLI args\n\
          --term <name>                 TERM value to export (default: inherit or xterm-256color)\n\
          --python-cmd <path>          Python interpreter for PTY helper (default: python3)\n\
           --pty-helper <path>          Override PTY helper script path\n\
           --whisper-cmd <path>          Path to whisper or whisper.cpp binary (default: whisper)\n\
           --whisper-model <name>        Whisper model name (default: small)\n\
           --whisper-model-path <path>   whisper.cpp model path\n\
           --ffmpeg-cmd <path>           ffmpeg executable (default: ffmpeg)\n\
           --ffmpeg-device <name>        Override audio input device string\n\
           --seconds <n>                 Recording length in seconds (default: 5)\n\
           --lang <code>                 Transcription language (default: en)\n\
           -h, --help                    Show this message\n"
    );
}

/// Mutable UI state backing the event loop and renderer.
struct App {
    config: AppConfig,
    input: String,
    output: Vec<String>,
    status: String,
    voice_enabled: bool,
    scroll_offset: u16,
}

impl App {
    /// Construct a fresh application state with default status messaging.
    fn new(config: AppConfig) -> Self {
        Self {
            config,
            input: String::new(),
            output: Vec::new(),
            status: "Ready.".into(),
            voice_enabled: false,
            scroll_offset: 0,
        }
    }

    /// Append output lines while trimming the scrollback when it grows large.
    fn append_output(&mut self, lines: Vec<String>) {
        self.output.extend(lines);
        if self.output.len() > OUTPUT_MAX_LINES {
            let excess = self.output.len() - OUTPUT_MAX_LINES;
            self.output.drain(0..excess);
        }
        // Auto-scroll to bottom when new content is added
        self.scroll_offset = self.output.len().saturating_sub(10) as u16;
    }
}
