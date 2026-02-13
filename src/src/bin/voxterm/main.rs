//! VoxTerm overlay entrypoint so PTY, HUD, and voice control start as one runtime.
//!
//! Runs the selected CLI in a PTY and intercepts hotkeys for voice capture. Transcripts
//! are injected as keystrokes, preserving the native TUI.
//!
//! # Architecture
//!
//! - Input thread: reads stdin, intercepts overlay shortcut keys
//! - PTY reader: forwards CLI output to terminal
//! - Writer thread: serializes output to avoid interleaving
//! - Voice worker: background audio capture and STT

mod arrow_keys;
mod audio_meter;
mod banner;
mod button_handlers;
mod buttons;
mod cli_utils;
mod color_mode;
mod config;
mod event_loop;
mod event_state;
mod help;
mod hud;
mod icons;
mod input;
mod overlays;
mod progress;
mod prompt;
mod session_stats;
mod settings;
mod settings_handlers;
mod status_line;
mod status_style;
mod terminal;
mod theme;
mod theme_ops;
mod theme_picker;
mod transcript;
mod voice_control;
mod writer;

pub(crate) use overlays::OverlayMode;

use anyhow::Result;
use clap::Parser;
use crossbeam_channel::bounded;
use crossterm::terminal::size as terminal_size;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use voxterm::pty_session::PtyOverlaySession;
use voxterm::{
    auth::run_login_command, doctor::base_doctor_report, init_logging, log_debug, log_file_path,
    terminal_restore::TerminalRestoreGuard, VoiceCaptureTrigger,
};

use crate::banner::{should_skip_banner, show_startup_splash, BannerConfig};
use crate::button_handlers::send_enhanced_status_with_buttons;
use crate::buttons::ButtonRegistry;
use crate::cli_utils::{list_input_devices, resolve_sound_flag, should_print_stats};
use crate::config::{HudStyle, OverlayConfig};
use crate::event_loop::run_event_loop;
use crate::event_state::{EventLoopDeps, EventLoopState, EventLoopTimers};
use crate::hud::HudRegistry;
use crate::input::spawn_input_thread;
use crate::prompt::{resolve_prompt_log, resolve_prompt_regex, PromptLogger, PromptTracker};
use crate::session_stats::{format_session_stats, SessionStats};
use crate::settings::SettingsMenuState;
use crate::status_line::{Pipeline, StatusLineState, VoiceMode, METER_HISTORY_MAX};
use crate::terminal::{apply_pty_winsize, install_sigwinch_handler};
use crate::theme_ops::theme_index_from_theme;
use crate::voice_control::{reset_capture_visuals, start_voice_capture, VoiceManager};
use crate::writer::{set_status, spawn_writer_thread, WriterMessage};

/// Max pending messages for the output writer thread.
const WRITER_CHANNEL_CAPACITY: usize = 512;

/// Max pending input events before backpressure.
const INPUT_CHANNEL_CAPACITY: usize = 256;

const METER_UPDATE_MS: u64 = 80;
fn main() -> Result<()> {
    let mut config = OverlayConfig::parse();
    let sound_on_complete = resolve_sound_flag(config.app.sounds, config.app.sound_on_complete);
    let sound_on_error = resolve_sound_flag(config.app.sounds, config.app.sound_on_error);
    let backend = config.resolve_backend();
    let backend_label = backend.label.clone();
    let theme = config.theme_for_backend(&backend_label);
    if config.app.doctor {
        let mut report = base_doctor_report(&config.app, "voxterm");
        report.section("Overlay");
        report.push_kv("backend", backend.label);
        let mut command = vec![backend.command];
        command.extend(backend.args);
        report.push_kv("backend_command", command.join(" "));
        report.push_kv(
            "prompt_regex",
            config.prompt_regex.as_deref().unwrap_or("auto"),
        );
        report.push_kv(
            "prompt_log",
            config
                .prompt_log
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "disabled".to_string()),
        );
        report.push_kv("theme", config.theme_name.as_deref().unwrap_or("coral"));
        report.push_kv("no_color", config.no_color);
        report.push_kv("auto_voice", config.auto_voice);
        report.push_kv(
            "voice_send_mode",
            format!("{:?}", config.voice_send_mode).to_lowercase(),
        );
        println!("{}", report.render());
        return Ok(());
    }
    if config.app.list_input_devices {
        list_input_devices()?;
        return Ok(());
    }

    if config.app.mic_meter {
        audio_meter::run_mic_meter(&config.app, theme)?;
        return Ok(());
    }

    config.app.validate()?;
    init_logging(&config.app);
    let log_path = log_file_path();
    log_debug("=== VoxTerm Overlay Started ===");
    log_debug(&format!("Log file: {log_path:?}"));

    if config.login {
        log_debug(&format!("Running login for backend: {}", backend.label));
        run_login_command(&backend.command)
            .map_err(|err| anyhow::anyhow!("{} login failed: {err}", backend.label))?;
    }

    install_sigwinch_handler()?;

    let working_dir = env::var("VOXTERM_CWD")
        .ok()
        .or_else(|| {
            env::current_dir()
                .ok()
                .map(|dir| dir.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| ".".to_string());

    // Backend command and args already resolved

    let prompt_log_path = if config.app.no_logs {
        None
    } else {
        resolve_prompt_log(&config)
    };
    let prompt_logger = PromptLogger::new(prompt_log_path);
    let prompt_regex = resolve_prompt_regex(&config, backend.prompt_pattern.as_deref())?;
    let prompt_tracker = PromptTracker::new(
        prompt_regex.regex,
        prompt_regex.allow_auto_learn,
        prompt_logger,
    );

    let banner_config = BannerConfig {
        auto_voice: config.auto_voice,
        theme: theme.to_string(),
        pipeline: "Rust".to_string(),
        sensitivity_db: config.app.voice_vad_threshold_db,
        backend: backend.label.clone(),
    };
    let no_startup_banner = env::var("VOXTERM_NO_STARTUP_BANNER").is_ok();
    let skip_banner = should_skip_banner(no_startup_banner);

    if !skip_banner {
        show_startup_splash(&banner_config, theme)?;
    }

    let terminal_guard = TerminalRestoreGuard::new();
    terminal_guard.enable_raw_mode()?;

    let mut session = PtyOverlaySession::new(
        &backend.command,
        &working_dir,
        &backend.args,
        &config.app.term_value,
    )?;

    let (writer_tx, writer_rx) = bounded(WRITER_CHANNEL_CAPACITY);
    let _writer_handle = spawn_writer_thread(writer_rx);

    // Set the color theme for the status line
    let _ = writer_tx.send(WriterMessage::SetTheme(theme));

    // Button registry for tracking clickable button positions (mouse is on by default)
    let button_registry = ButtonRegistry::new();

    // Compute initial HUD style (handle --minimal-hud shorthand)
    let initial_hud_style = if config.minimal_hud {
        HudStyle::Minimal
    } else {
        config.hud_style
    };

    let mut terminal_cols = 0u16;
    let mut terminal_rows = 0u16;
    if let Ok((cols, rows)) = terminal_size() {
        terminal_cols = cols;
        terminal_rows = rows;
        apply_pty_winsize(
            &mut session,
            rows,
            cols,
            OverlayMode::None,
            initial_hud_style,
        );
        let _ = writer_tx.send(WriterMessage::Resize { rows, cols });
    }

    let (input_tx, input_rx) = bounded(INPUT_CHANNEL_CAPACITY);
    let _input_handle = spawn_input_thread(input_tx);

    let auto_idle_timeout = Duration::from_millis(config.auto_voice_idle_ms.max(100));
    let transcript_idle_timeout = Duration::from_millis(config.transcript_idle_ms.max(50));
    let hud_registry = HudRegistry::with_defaults();
    let meter_update_ms = hud_registry
        .min_tick_interval()
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(METER_UPDATE_MS);
    let voice_manager = VoiceManager::new(config.app.clone());
    let live_meter = voice_manager.meter();
    let auto_voice_enabled = config.auto_voice;
    let mut status_state = StatusLineState::new();
    status_state.sensitivity_db = config.app.voice_vad_threshold_db;
    status_state.auto_voice_enabled = auto_voice_enabled;
    status_state.send_mode = config.voice_send_mode;
    status_state.hud_right_panel = config.hud_right_panel;
    status_state.hud_right_panel_recording_only = config.hud_right_panel_recording_only;
    status_state.hud_style = initial_hud_style;
    status_state.voice_mode = if auto_voice_enabled {
        VoiceMode::Auto
    } else {
        VoiceMode::Manual
    };
    status_state.pipeline = Pipeline::Rust;
    status_state.mouse_enabled = true; // Mouse enabled by default for clickable buttons
    let _ = writer_tx.send(WriterMessage::EnableMouse);
    let mut state = EventLoopState {
        config,
        status_state,
        auto_voice_enabled,
        theme,
        overlay_mode: OverlayMode::None,
        settings_menu: SettingsMenuState::new(),
        meter_levels: VecDeque::with_capacity(METER_HISTORY_MAX),
        theme_picker_selected: theme_index_from_theme(theme),
        theme_picker_digits: String::new(),
        current_status: None,
        pending_transcripts: VecDeque::new(),
        session_stats: SessionStats::new(),
        prompt_tracker,
        terminal_rows,
        terminal_cols,
        last_recording_duration: 0.0_f32,
        processing_spinner_index: 0,
        pending_pty_output: None,
        pending_pty_input: VecDeque::new(),
        pending_pty_input_offset: 0,
        pending_pty_input_bytes: 0,
    };
    let mut timers = EventLoopTimers {
        theme_picker_digit_deadline: None,
        status_clear_deadline: None,
        preview_clear_deadline: None,
        last_auto_trigger_at: None,
        last_enter_at: None,
        recording_started_at: None,
        last_recording_update: Instant::now(),
        last_processing_tick: Instant::now(),
        last_heartbeat_tick: Instant::now(),
        last_meter_update: Instant::now(),
    };
    let mut deps = EventLoopDeps {
        session,
        voice_manager,
        writer_tx,
        input_rx,
        button_registry,
        backend_label,
        sound_on_complete,
        sound_on_error,
        live_meter,
        meter_update_ms,
        auto_idle_timeout,
        transcript_idle_timeout,
    };

    if state.auto_voice_enabled {
        set_status(
            &deps.writer_tx,
            &mut timers.status_clear_deadline,
            &mut state.current_status,
            &mut state.status_state,
            "Auto-voice enabled",
            Some(Duration::from_secs(2)),
        );
        if deps.voice_manager.is_idle() {
            if let Err(err) = start_voice_capture(
                &mut deps.voice_manager,
                VoiceCaptureTrigger::Auto,
                &deps.writer_tx,
                &mut timers.status_clear_deadline,
                &mut state.current_status,
                &mut state.status_state,
            ) {
                log_debug(&format!("auto voice capture failed: {err:#}"));
            } else {
                let now = Instant::now();
                timers.last_auto_trigger_at = Some(now);
                timers.recording_started_at = Some(now);
                reset_capture_visuals(
                    &mut state.status_state,
                    &mut timers.preview_clear_deadline,
                    &mut timers.last_meter_update,
                );
            }
        }
    }

    // Ensure the HUD/launcher is visible immediately, before any user input arrives.
    send_enhanced_status_with_buttons(
        &deps.writer_tx,
        &deps.button_registry,
        &state.status_state,
        state.overlay_mode,
        state.terminal_cols,
        state.theme,
    );

    run_event_loop(&mut state, &mut timers, &mut deps);

    let _ = deps.writer_tx.send(WriterMessage::ClearStatus);
    let _ = deps.writer_tx.send(WriterMessage::Shutdown);
    terminal_guard.restore();
    let stats_output = format_session_stats(&state.session_stats, state.theme);
    if should_print_stats(&stats_output) {
        print!("{stats_output}");
        let _ = io::stdout().flush();
    }
    log_debug("=== VoxTerm Overlay Exiting ===");
    Ok(())
}
