//! VoxTerm overlay - voice input for the Codex CLI (or another AI CLI via --backend).
//!
//! Runs the selected CLI in a PTY and intercepts hotkeys for voice capture. Transcripts
//! are injected as keystrokes, preserving the native TUI.
//!
//! # Architecture
//!
//! - Input thread: reads stdin, intercepts Ctrl+R/V/Q
//! - PTY reader: forwards CLI output to terminal
//! - Writer thread: serializes output to avoid interleaving
//! - Voice worker: background audio capture and STT

mod audio_meter;
mod banner;
mod buttons;
mod color_mode;
mod config;
mod help;
mod hud;
mod icons;
mod input;
mod progress;
mod prompt;
mod session_stats;
mod settings;
mod status_line;
mod status_style;
mod theme;
mod theme_picker;
mod transcript;
mod voice_control;
mod writer;

use anyhow::{anyhow, Result};
use clap::Parser;
use crossbeam_channel::{bounded, select, Sender};
use crossterm::terminal::size as terminal_size;
use std::collections::VecDeque;
use std::env;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use voxterm::pty_session::PtyOverlaySession;
use voxterm::{
    audio, doctor::base_doctor_report, init_logging, log_debug, log_file_path,
    terminal_restore::TerminalRestoreGuard, VoiceCaptureSource, VoiceCaptureTrigger,
    VoiceJobMessage,
};

use crate::banner::{format_ascii_banner, format_minimal_banner, format_startup_banner, BannerConfig};
use crate::buttons::{ButtonAction, ButtonRegistry};
use crate::config::{HudRightPanel, HudStyle, OverlayConfig, VoiceSendMode};
use crate::help::{
    format_help_overlay, help_overlay_height, help_overlay_inner_width_for_terminal,
    help_overlay_width_for_terminal, HELP_OVERLAY_FOOTER,
};
use crate::hud::HudRegistry;
use crate::input::{spawn_input_thread, InputEvent};
use crate::prompt::{
    resolve_prompt_log, resolve_prompt_regex, should_auto_trigger, PromptLogger, PromptTracker,
};
use crate::session_stats::{format_session_stats, SessionStats};
use crate::settings::{
    format_settings_overlay, settings_overlay_height, settings_overlay_inner_width_for_terminal,
    settings_overlay_width_for_terminal, SettingsItem, SettingsMenuState, SettingsView,
    SETTINGS_OVERLAY_FOOTER,
};
use crate::status_line::{
    get_button_positions, status_banner_height, Pipeline, RecordingState, StatusLineState,
    VoiceMode, METER_HISTORY_MAX,
};
use crate::theme::Theme;
use crate::theme_picker::{
    format_theme_picker, theme_picker_height, theme_picker_inner_width_for_terminal,
    theme_picker_total_width_for_terminal, THEME_OPTIONS, THEME_PICKER_FOOTER,
    THEME_PICKER_OPTION_START_ROW,
};
use crate::transcript::{
    deliver_transcript, push_pending_transcript, transcript_ready, try_flush_pending,
    PendingTranscript, TranscriptIo, TranscriptSession,
};
use crate::voice_control::{
    handle_voice_message, start_voice_capture, VoiceManager, VoiceMessageContext,
};
use crate::writer::{send_enhanced_status, set_status, spawn_writer_thread, WriterMessage};

/// Flag set by SIGWINCH handler to trigger terminal resize.
static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Max pending messages for the output writer thread.
const WRITER_CHANNEL_CAPACITY: usize = 512;

/// Max pending input events before backpressure.
const INPUT_CHANNEL_CAPACITY: usize = 256;

const METER_UPDATE_MS: u64 = 80;
const PREVIEW_CLEAR_MS: u64 = 3000;
const TRANSCRIPT_PREVIEW_MAX: usize = 60;
const EVENT_LOOP_IDLE_MS: u64 = 50;
const THEME_PICKER_NUMERIC_TIMEOUT_MS: u64 = 350;
const RECORDING_DURATION_UPDATE_MS: u64 = 200;
const PROCESSING_SPINNER_TICK_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayMode {
    None,
    Help,
    ThemePicker,
    Settings,
}

/// Signal handler for terminal resize events.
///
/// Sets a flag that the main loop checks to update PTY dimensions.
/// Only uses atomic operations (async-signal-safe).
extern "C" fn handle_sigwinch(_: libc::c_int) {
    SIGWINCH_RECEIVED.store(true, Ordering::SeqCst);
}

fn main() -> Result<()> {
    let mut config = OverlayConfig::parse();
    let sound_on_complete = resolve_sound_flag(config.app.sounds, config.app.sound_on_complete);
    let sound_on_error = resolve_sound_flag(config.app.sounds, config.app.sound_on_error);
    let backend = config.resolve_backend();
    let backend_label = backend.label.clone();
    let mut theme = config.theme_for_backend(&backend_label);
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
    let mut prompt_tracker = PromptTracker::new(
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
    let is_wrapper = env::var("VOXTERM_WRAPPER")
        .map(|v| v == "1")
        .unwrap_or(false);
    let no_startup_banner = env::var("VOXTERM_NO_STARTUP_BANNER").is_ok();
    let skip_banner = should_skip_banner(is_wrapper, no_startup_banner);
    if !skip_banner {
        let use_color = theme != Theme::None;
        match terminal_size() {
            Ok((cols, _)) if cols >= 66 => {
                // Wide terminal: show centered ASCII art with tagline
                print!("{}", format_ascii_banner(use_color, cols));
                let _ = io::stdout().flush();
            }
            Ok((cols, _)) if use_minimal_banner(cols) => {
                // Narrow terminal: minimal one-line banner
                print!("{}", format_minimal_banner(theme));
                let _ = io::stdout().flush();
            }
            _ => {
                // Medium terminal: show config banner
                print!("{}", format_startup_banner(&banner_config, theme));
                let _ = io::stdout().flush();
            }
        }
    }

    let mut session = PtyOverlaySession::new(
        &backend.command,
        &working_dir,
        &backend.args,
        &config.app.term_value,
    )?;

    let terminal_guard = TerminalRestoreGuard::new();
    terminal_guard.enable_raw_mode()?;

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
    let mut voice_manager = VoiceManager::new(config.app.clone());
    let live_meter = voice_manager.meter();
    let mut auto_voice_enabled = config.auto_voice;
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
    let mut theme_picker_selected = theme_index_from_theme(theme);
    let mut theme_picker_digits = String::new();
    let mut theme_picker_digit_deadline: Option<Instant> = None;
    let mut last_auto_trigger_at: Option<Instant> = None;
    let mut last_enter_at: Option<Instant> = None;
    let mut pending_transcripts: VecDeque<PendingTranscript> = VecDeque::new();
    let mut status_clear_deadline: Option<Instant> = None;
    let mut current_status: Option<String> = None;
    let mut recording_started_at: Option<Instant> = None;
    let mut last_recording_update = Instant::now();
    let mut last_recording_duration = 0.0_f32;
    let mut processing_spinner_index = 0usize;
    let mut last_processing_tick = Instant::now();
    let mut last_heartbeat_tick = Instant::now();
    let mut overlay_mode = OverlayMode::None;
    let mut settings_menu = SettingsMenuState::new();
    let mut meter_levels: VecDeque<f32> = VecDeque::with_capacity(METER_HISTORY_MAX);
    let mut last_meter_update = Instant::now();
    let mut preview_clear_deadline: Option<Instant> = None;
    let mut session_stats = SessionStats::new();

    if auto_voice_enabled {
        set_status(
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
            "Auto-voice enabled",
            Some(Duration::from_secs(2)),
        );
        if voice_manager.is_idle() {
            if let Err(err) = start_voice_capture(
                &mut voice_manager,
                VoiceCaptureTrigger::Auto,
                &writer_tx,
                &mut status_clear_deadline,
                &mut current_status,
                &mut status_state,
            ) {
                log_debug(&format!("auto voice capture failed: {err:#}"));
            } else {
                let now = Instant::now();
                last_auto_trigger_at = Some(now);
                recording_started_at = Some(now);
                reset_capture_visuals(
                    &mut status_state,
                    &mut preview_clear_deadline,
                    &mut last_meter_update,
                );
            }
        }
    }

    let mut running = true;
    while running {
        select! {
            recv(input_rx) -> event => {
                match event {
                    Ok(evt) => {
                        if overlay_mode != OverlayMode::None {
                            match (overlay_mode, evt) {
                                (_, InputEvent::Exit) => running = false,
                                (mode, InputEvent::ToggleHudStyle) => {
                                    update_hud_style(
                                        &mut status_state,
                                        &writer_tx,
                                        &mut status_clear_deadline,
                                        &mut current_status,
                                        1,
                                    );
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        mode,
                                        status_state.hud_style,
                                    );
                                    if mode == OverlayMode::Settings {
                                        let cols = resolved_cols(terminal_cols);
                                        show_settings_overlay(
                                            &writer_tx,
                                            theme,
                                            cols,
                                            &settings_menu,
                                            &config,
                                            &status_state,
                                            &backend_label,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::SettingsToggle) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    if status_state.mouse_enabled {
                                        update_button_registry(
                                            &button_registry,
                                            &status_state,
                                            overlay_mode,
                                            terminal_cols,
                                            theme,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::HelpToggle) => {
                                    overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    let content = format_help_overlay(theme, cols as usize);
                                    let height = help_overlay_height();
                                    let _ = writer_tx.send(WriterMessage::ShowOverlay {
                                        content,
                                        height,
                                    });
                                }
                                (OverlayMode::Settings, InputEvent::ThemePicker) => {
                                    overlay_mode = OverlayMode::ThemePicker;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    theme_picker_selected = theme_index_from_theme(theme);
                                    theme_picker_digits.clear();
                                    theme_picker_digit_deadline = None;
                                    show_theme_picker_overlay(
                                        &writer_tx,
                                        theme,
                                        theme_picker_selected,
                                        cols,
                                    );
                                }
                                (OverlayMode::Settings, InputEvent::EnterKey) => {
                                    let mut should_redraw = false;
                                    match settings_menu.selected_item() {
                                        SettingsItem::AutoVoice => {
                                            toggle_auto_voice(
                                                &mut auto_voice_enabled,
                                                &mut voice_manager,
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                                &mut status_state,
                                                &mut last_auto_trigger_at,
                                                &mut recording_started_at,
                                                &mut preview_clear_deadline,
                                                &mut last_meter_update,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::SendMode => {
                                            toggle_send_mode(
                                                &mut config,
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                                &mut status_state,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::Sensitivity => {}
                                        SettingsItem::Theme => {
                                            theme = apply_theme_selection(
                                                &mut config,
                                                cycle_theme(theme, 1),
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                                &mut status_state,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudStyle => {
                                            update_hud_style(
                                                &mut status_state,
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                                1,
                                            );
                                            update_pty_winsize(
                                                &mut session,
                                                &mut terminal_rows,
                                                &mut terminal_cols,
                                                overlay_mode,
                                                status_state.hud_style,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudPanel => {
                                            update_hud_panel(
                                                &mut config,
                                                &mut status_state,
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                                1,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudAnimate => {
                                            toggle_hud_panel_recording_only(
                                                &mut config,
                                                &mut status_state,
                                                &writer_tx,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::Mouse => {
                                            toggle_mouse(
                                                &mut status_state,
                                                &writer_tx,
                                                &button_registry,
                                                overlay_mode,
                                                terminal_cols,
                                                theme,
                                                &mut status_clear_deadline,
                                                &mut current_status,
                                            );
                                            should_redraw = true;
                                        }
                                        SettingsItem::Backend | SettingsItem::Pipeline => {}
                                        SettingsItem::Close => {
                                            overlay_mode = OverlayMode::None;
                                            let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                            update_pty_winsize(
                                                &mut session,
                                                &mut terminal_rows,
                                                &mut terminal_cols,
                                                overlay_mode,
                                                status_state.hud_style,
                                            );
                                        }
                                        SettingsItem::Quit => running = false,
                                    }
                                    if overlay_mode == OverlayMode::Settings && should_redraw {
                                        let cols = resolved_cols(terminal_cols);
                                        show_settings_overlay(
                                            &writer_tx,
                                            theme,
                                            cols,
                                            &settings_menu,
                                            &config,
                                            &status_state,
                                            &backend_label,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::Bytes(bytes)) => {
                                    if bytes == [0x1b] {
                                        overlay_mode = OverlayMode::None;
                                        let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                        update_pty_winsize(
                                            &mut session,
                                            &mut terminal_rows,
                                            &mut terminal_cols,
                                            overlay_mode,
                                            status_state.hud_style,
                                        );
                                    } else {
                                        let mut should_redraw = false;
                                        for key in parse_arrow_keys(&bytes) {
                                            match key {
                                                ArrowKey::Up => {
                                                    settings_menu.move_up();
                                                    should_redraw = true;
                                                }
                                                ArrowKey::Down => {
                                                    settings_menu.move_down();
                                                    should_redraw = true;
                                                }
                                                ArrowKey::Left => match settings_menu.selected_item() {
                                                    SettingsItem::AutoVoice => {
                                                        toggle_auto_voice(
                                                            &mut auto_voice_enabled,
                                                            &mut voice_manager,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                            &mut last_auto_trigger_at,
                                                            &mut recording_started_at,
                                                            &mut preview_clear_deadline,
                                                            &mut last_meter_update,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::SendMode => {
                                                        toggle_send_mode(
                                                            &mut config,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Sensitivity => {
                                                        adjust_sensitivity(
                                                            &mut voice_manager,
                                                            -5.0,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Theme => {
                                                        theme = apply_theme_selection(
                                                            &mut config,
                                                            cycle_theme(theme, -1),
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudStyle => {
                                                        update_hud_style(
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            -1,
                                                        );
                                                        update_pty_winsize(
                                                            &mut session,
                                                            &mut terminal_rows,
                                                            &mut terminal_cols,
                                                            overlay_mode,
                                                            status_state.hud_style,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudPanel => {
                                                        update_hud_panel(
                                                            &mut config,
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            -1,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudAnimate => {
                                                        toggle_hud_panel_recording_only(
                                                            &mut config,
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Mouse => {
                                                        toggle_mouse(
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &button_registry,
                                                            overlay_mode,
                                                            terminal_cols,
                                                            theme,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Backend
                                                    | SettingsItem::Pipeline
                                                    | SettingsItem::Close
                                                    | SettingsItem::Quit => {}
                                                },
                                                ArrowKey::Right => match settings_menu.selected_item() {
                                                    SettingsItem::AutoVoice => {
                                                        toggle_auto_voice(
                                                            &mut auto_voice_enabled,
                                                            &mut voice_manager,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                            &mut last_auto_trigger_at,
                                                            &mut recording_started_at,
                                                            &mut preview_clear_deadline,
                                                            &mut last_meter_update,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::SendMode => {
                                                        toggle_send_mode(
                                                            &mut config,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Sensitivity => {
                                                        adjust_sensitivity(
                                                            &mut voice_manager,
                                                            5.0,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Theme => {
                                                        theme = apply_theme_selection(
                                                            &mut config,
                                                            cycle_theme(theme, 1),
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            &mut status_state,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudStyle => {
                                                        update_hud_style(
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            1,
                                                        );
                                                        update_pty_winsize(
                                                            &mut session,
                                                            &mut terminal_rows,
                                                            &mut terminal_cols,
                                                            overlay_mode,
                                                            status_state.hud_style,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudPanel => {
                                                        update_hud_panel(
                                                            &mut config,
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                            1,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudAnimate => {
                                                        toggle_hud_panel_recording_only(
                                                            &mut config,
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Mouse => {
                                                        toggle_mouse(
                                                            &mut status_state,
                                                            &writer_tx,
                                                            &button_registry,
                                                            overlay_mode,
                                                            terminal_cols,
                                                            theme,
                                                            &mut status_clear_deadline,
                                                            &mut current_status,
                                                        );
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Backend
                                                    | SettingsItem::Pipeline
                                                    | SettingsItem::Close
                                                    | SettingsItem::Quit => {}
                                                },
                                            }
                                        }
                                        if should_redraw {
                                            let cols = resolved_cols(terminal_cols);
                                            show_settings_overlay(
                                                &writer_tx,
                                                theme,
                                                cols,
                                                &settings_menu,
                                                &config,
                                                &status_state,
                                                &backend_label,
                                            );
                                        }
                                    }
                                }
                                (OverlayMode::Help, InputEvent::HelpToggle) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                }
                                (OverlayMode::Help, InputEvent::SettingsToggle) => {
                                    overlay_mode = OverlayMode::Settings;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    show_settings_overlay(
                                        &writer_tx,
                                        theme,
                                        cols,
                                        &settings_menu,
                                        &config,
                                        &status_state,
                                        &backend_label,
                                    );
                                }
                                (OverlayMode::Help, InputEvent::ThemePicker) => {
                                    overlay_mode = OverlayMode::ThemePicker;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    theme_picker_selected = theme_index_from_theme(theme);
                                    theme_picker_digits.clear();
                                    theme_picker_digit_deadline = None;
                                    show_theme_picker_overlay(
                                        &writer_tx,
                                        theme,
                                        theme_picker_selected,
                                        cols,
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::HelpToggle) => {
                                    overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    let content = format_help_overlay(theme, cols as usize);
                                    let height = help_overlay_height();
                                    let _ =
                                        writer_tx.send(WriterMessage::ShowOverlay { content, height });
                                }
                                (OverlayMode::ThemePicker, InputEvent::SettingsToggle) => {
                                    overlay_mode = OverlayMode::Settings;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    show_settings_overlay(
                                        &writer_tx,
                                        theme,
                                        cols,
                                        &settings_menu,
                                        &config,
                                        &status_state,
                                        &backend_label,
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::ThemePicker) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    theme_picker_digits.clear();
                                    theme_picker_digit_deadline = None;
                                }
                                (OverlayMode::ThemePicker, InputEvent::EnterKey) => {
                                    if apply_theme_picker_index(
                                        theme_picker_selected,
                                        &mut theme,
                                        &mut config,
                                        &writer_tx,
                                        &mut status_clear_deadline,
                                        &mut current_status,
                                        &mut status_state,
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        &mut overlay_mode,
                                    ) {
                                        theme_picker_selected = theme_index_from_theme(theme);
                                    }
                                    theme_picker_digits.clear();
                                    theme_picker_digit_deadline = None;
                                }
                                (OverlayMode::ThemePicker, InputEvent::Bytes(bytes)) => {
                                    if bytes == [0x1b] {
                                        overlay_mode = OverlayMode::None;
                                        let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                        update_pty_winsize(
                                            &mut session,
                                            &mut terminal_rows,
                                            &mut terminal_cols,
                                            overlay_mode,
                                            status_state.hud_style,
                                        );
                                        theme_picker_digits.clear();
                                        theme_picker_digit_deadline = None;
                                    } else if let Some(keys) = parse_arrow_keys_only(&bytes) {
                                        let mut moved = false;
                                        let total = THEME_OPTIONS.len();
                                        for key in keys {
                                            let direction = match key {
                                                ArrowKey::Up | ArrowKey::Left => -1,
                                                ArrowKey::Down | ArrowKey::Right => 1,
                                            };
                                            if direction != 0 && total > 0 {
                                                let next = (theme_picker_selected as i32 + direction)
                                                    .rem_euclid(total as i32) as usize;
                                                if next != theme_picker_selected {
                                                    theme_picker_selected = next;
                                                    moved = true;
                                                }
                                            }
                                        }
                                        if moved {
                                            let cols = resolved_cols(terminal_cols);
                                            show_theme_picker_overlay(
                                                &writer_tx,
                                                theme,
                                                theme_picker_selected,
                                                cols,
                                            );
                                        }
                                        theme_picker_digits.clear();
                                        theme_picker_digit_deadline = None;
                                    } else {
                                        let digits: String = bytes
                                            .iter()
                                            .filter(|b| b.is_ascii_digit())
                                            .map(|b| *b as char)
                                            .collect();
                                        if !digits.is_empty() {
                                            theme_picker_digits.push_str(&digits);
                                            if theme_picker_digits.len() > 3 {
                                                theme_picker_digits.clear();
                                            }
                                            let now = Instant::now();
                                            theme_picker_digit_deadline = Some(
                                                now + Duration::from_millis(THEME_PICKER_NUMERIC_TIMEOUT_MS),
                                            );
                                            if let Some(idx) = theme_picker_parse_index(
                                                &theme_picker_digits,
                                                THEME_OPTIONS.len(),
                                            ) {
                                                if !theme_picker_has_longer_match(
                                                    &theme_picker_digits,
                                                    THEME_OPTIONS.len(),
                                                ) {
                                                    if apply_theme_picker_index(
                                                        idx,
                                                        &mut theme,
                                                        &mut config,
                                                        &writer_tx,
                                                        &mut status_clear_deadline,
                                                        &mut current_status,
                                                        &mut status_state,
                                                        &mut session,
                                                        &mut terminal_rows,
                                                        &mut terminal_cols,
                                                        &mut overlay_mode,
                                                    ) {
                                                        theme_picker_selected = theme_index_from_theme(theme);
                                                    }
                                                    theme_picker_digits.clear();
                                                    theme_picker_digit_deadline = None;
                                                }
                                            }
                                        }
                                    }
                                }
                                (_, _) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                        status_state.hud_style,
                                    );
                                    if status_state.mouse_enabled {
                                        update_button_registry(
                                            &button_registry,
                                            &status_state,
                                            overlay_mode,
                                            terminal_cols,
                                            theme,
                                        );
                                    }
                                }
                            }
                            continue;
                        }
                        match evt {
                            InputEvent::HelpToggle => {
                                status_state.hud_button_focus = None;
                                overlay_mode = OverlayMode::Help;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                    status_state.hud_style,
                                );
                                let cols = resolved_cols(terminal_cols);
                                let content = format_help_overlay(theme, cols as usize);
                                let height = help_overlay_height();
                                let _ =
                                    writer_tx.send(WriterMessage::ShowOverlay { content, height });
                            }
                            InputEvent::ThemePicker => {
                                status_state.hud_button_focus = None;
                                overlay_mode = OverlayMode::ThemePicker;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                    status_state.hud_style,
                                );
                                let cols = resolved_cols(terminal_cols);
                                theme_picker_selected = theme_index_from_theme(theme);
                                theme_picker_digits.clear();
                                theme_picker_digit_deadline = None;
                                show_theme_picker_overlay(
                                    &writer_tx,
                                    theme,
                                    theme_picker_selected,
                                    cols,
                                );
                            }
                            InputEvent::SettingsToggle => {
                                status_state.hud_button_focus = None;
                                overlay_mode = OverlayMode::Settings;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                    status_state.hud_style,
                                );
                                let cols = resolved_cols(terminal_cols);
                                show_settings_overlay(
                                    &writer_tx,
                                    theme,
                                    cols,
                                    &settings_menu,
                                    &config,
                                    &status_state,
                                    &backend_label,
                                );
                            }
                            InputEvent::ToggleHudStyle => {
                                update_hud_style(
                                    &mut status_state,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    1,
                                );
                                status_state.hud_button_focus = None;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                    status_state.hud_style,
                                );
                                if status_state.mouse_enabled {
                                    update_button_registry(
                                        &button_registry,
                                        &status_state,
                                        overlay_mode,
                                        terminal_cols,
                                        theme,
                                    );
                                }
                            }
                            InputEvent::Bytes(bytes) => {
                                if status_state.mouse_enabled {
                                    if let Some(keys) = parse_arrow_keys_only(&bytes) {
                                        let mut moved = false;
                                        for key in keys {
                                            let direction = match key {
                                                ArrowKey::Left => -1,
                                                ArrowKey::Right => 1,
                                                _ => 0,
                                            };
                                            if direction != 0
                                                && advance_hud_button_focus(
                                                    &mut status_state,
                                                    overlay_mode,
                                                    terminal_cols,
                                                    theme,
                                                    direction,
                                                )
                                            {
                                                moved = true;
                                            }
                                        }
                                        if moved {
                                            send_enhanced_status_with_buttons(
                                                &writer_tx,
                                                &button_registry,
                                                &status_state,
                                                overlay_mode,
                                                terminal_cols,
                                                theme,
                                            );
                                            continue;
                                        }
                                    }
                                }

                                status_state.hud_button_focus = None;
                                if let Err(err) = session.send_bytes(&bytes) {
                                    log_debug(&format!("failed to write to PTY: {err:#}"));
                                    running = false;
                                }
                            }
                            InputEvent::VoiceTrigger => {
                                if let Err(err) = start_voice_capture(
                                    &mut voice_manager,
                                    VoiceCaptureTrigger::Manual,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                ) {
                                    set_status(
                                        &writer_tx,
                                        &mut status_clear_deadline,
                                        &mut current_status,
                                        &mut status_state,
                                        "Voice capture failed (see log)",
                                        Some(Duration::from_secs(2)),
                                    );
                                    log_debug(&format!("voice capture failed: {err:#}"));
                                } else {
                                    recording_started_at = Some(Instant::now());
                                    reset_capture_visuals(
                                        &mut status_state,
                                        &mut preview_clear_deadline,
                                        &mut last_meter_update,
                                    );
                                }
                            }
                            InputEvent::ToggleAutoVoice => {
                                toggle_auto_voice(
                                    &mut auto_voice_enabled,
                                    &mut voice_manager,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                    &mut last_auto_trigger_at,
                                    &mut recording_started_at,
                                    &mut preview_clear_deadline,
                                    &mut last_meter_update,
                                );
                                if status_state.mouse_enabled {
                                    update_button_registry(
                                        &button_registry,
                                        &status_state,
                                        overlay_mode,
                                        terminal_cols,
                                        theme,
                                    );
                                }
                            }
                            InputEvent::ToggleSendMode => {
                                toggle_send_mode(
                                    &mut config,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                );
                                if status_state.mouse_enabled {
                                    update_button_registry(
                                        &button_registry,
                                        &status_state,
                                        overlay_mode,
                                        terminal_cols,
                                        theme,
                                    );
                                }
                            }
                            InputEvent::IncreaseSensitivity => {
                                adjust_sensitivity(
                                    &mut voice_manager,
                                    5.0,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                );
                            }
                            InputEvent::DecreaseSensitivity => {
                                adjust_sensitivity(
                                    &mut voice_manager,
                                    -5.0,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                );
                            }
                            InputEvent::EnterKey => {
                                if status_state.mouse_enabled {
                                    if let Some(action) = status_state.hud_button_focus {
                                        status_state.hud_button_focus = None;
                                        if action == ButtonAction::ThemePicker {
                                            theme_picker_selected = theme_index_from_theme(theme);
                                            theme_picker_digits.clear();
                                            theme_picker_digit_deadline = None;
                                        }
                                        handle_button_action(
                                            action,
                                            &mut overlay_mode,
                                            &mut settings_menu,
                                            &mut config,
                                            &mut status_state,
                                            &mut auto_voice_enabled,
                                            &mut voice_manager,
                                            &mut session,
                                            &writer_tx,
                                            &mut status_clear_deadline,
                                            &mut current_status,
                                            &mut recording_started_at,
                                            &mut preview_clear_deadline,
                                            &mut last_meter_update,
                                            &mut last_auto_trigger_at,
                                            &mut terminal_rows,
                                            &mut terminal_cols,
                                            &backend_label,
                                            theme,
                                            &button_registry,
                                        );
                                        send_enhanced_status_with_buttons(
                                            &writer_tx,
                                            &button_registry,
                                            &status_state,
                                            overlay_mode,
                                            terminal_cols,
                                            theme,
                                        );
                                        continue;
                                    }
                                }
                                // In insert mode, Enter stops capture early and sends what was recorded
                                if config.voice_send_mode == VoiceSendMode::Insert && !voice_manager.is_idle() {
                                    if voice_manager.active_source() == Some(VoiceCaptureSource::Python) {
                                        let _ = voice_manager.cancel_capture();
                                        status_state.recording_state = RecordingState::Idle;
                                        recording_started_at = None;
                                        set_status(
                                            &writer_tx,
                                            &mut status_clear_deadline,
                                            &mut current_status,
                                            &mut status_state,
                                            "Capture cancelled (python fallback cannot stop early)",
                                            Some(Duration::from_secs(3)),
                                        );
                                    } else {
                                        voice_manager.request_early_stop();
                                        status_state.recording_state = RecordingState::Processing;
                                        processing_spinner_index = 0;
                                        last_processing_tick = Instant::now();
                                        set_status(
                                            &writer_tx,
                                            &mut status_clear_deadline,
                                            &mut current_status,
                                            &mut status_state,
                                            "Processing",
                                            None,
                                        );
                                    }
                                } else {
                                    // Forward Enter to PTY
                                    if let Err(err) = session.send_bytes(&[0x0d]) {
                                        log_debug(&format!("failed to write Enter to PTY: {err:#}"));
                                        running = false;
                                    } else {
                                        last_enter_at = Some(Instant::now());
                                    }
                                }
                            }
                            InputEvent::Exit => {
                                running = false;
                            }
                            InputEvent::MouseClick { x, y } => {
                                // Only process clicks if mouse is enabled
                                if !status_state.mouse_enabled {
                                    continue;
                                }

                                // Overlay click handling (close button + theme selection)
                                if overlay_mode != OverlayMode::None {
                                    let overlay_height = match overlay_mode {
                                        OverlayMode::Help => help_overlay_height(),
                                        OverlayMode::ThemePicker => theme_picker_height(),
                                        OverlayMode::Settings => settings_overlay_height(),
                                        OverlayMode::None => 0,
                                    };
                                    if overlay_height == 0 || terminal_rows == 0 {
                                        continue;
                                    }
                                    let overlay_top_y =
                                        terminal_rows.saturating_sub(overlay_height as u16).saturating_add(1);
                                    if y < overlay_top_y || y > terminal_rows {
                                        continue;
                                    }
                                    let overlay_row = (y - overlay_top_y) as usize + 1;
                                    let cols = resolved_cols(terminal_cols) as usize;

                                    let (overlay_width, inner_width, footer_title) = match overlay_mode {
                                        OverlayMode::Help => (
                                            help_overlay_width_for_terminal(cols),
                                            help_overlay_inner_width_for_terminal(cols),
                                            HELP_OVERLAY_FOOTER,
                                        ),
                                        OverlayMode::ThemePicker => (
                                            theme_picker_total_width_for_terminal(cols),
                                            theme_picker_inner_width_for_terminal(cols),
                                            THEME_PICKER_FOOTER,
                                        ),
                                        OverlayMode::Settings => (
                                            settings_overlay_width_for_terminal(cols),
                                            settings_overlay_inner_width_for_terminal(cols),
                                            SETTINGS_OVERLAY_FOOTER,
                                        ),
                                        OverlayMode::None => (0, 0, ""),
                                    };

                                    if overlay_width == 0 || x as usize > overlay_width {
                                        continue;
                                    }

                                    let footer_row = overlay_height.saturating_sub(1);
                                    if overlay_row == footer_row {
                                        let title_len = footer_title.chars().count();
                                        let left_pad = inner_width.saturating_sub(title_len) / 2;
                                        let close_prefix = footer_title
                                            .split('·')
                                            .next()
                                            .unwrap_or(footer_title)
                                            .trim_end();
                                        let close_len = close_prefix.chars().count();
                                        let close_start = 2usize.saturating_add(left_pad);
                                        let close_end = close_start.saturating_add(close_len.saturating_sub(1));
                                        if (x as usize) >= close_start && (x as usize) <= close_end {
                                            overlay_mode = OverlayMode::None;
                                            let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                            apply_pty_winsize(
                                                &mut session,
                                                terminal_rows,
                                                terminal_cols,
                                                overlay_mode,
                                                status_state.hud_style,
                                            );
                                            if status_state.mouse_enabled {
                                                update_button_registry(
                                                    &button_registry,
                                                    &status_state,
                                                    overlay_mode,
                                                    terminal_cols,
                                                    theme,
                                                );
                                            }
                                        }
                                        continue;
                                    }

                                    if overlay_mode == OverlayMode::ThemePicker {
                                        let options_start = THEME_PICKER_OPTION_START_ROW;
                                        let options_end =
                                            options_start.saturating_add(THEME_OPTIONS.len().saturating_sub(1));
                                        if overlay_row >= options_start
                                            && overlay_row <= options_end
                                            && x > 1
                                            && (x as usize) < overlay_width
                                        {
                                            let idx = overlay_row.saturating_sub(options_start);
                                            if let Some((_, name, _)) = THEME_OPTIONS.get(idx) {
                                                if let Some(requested) = Theme::from_name(name) {
                                                    theme = apply_theme_selection(
                                                        &mut config,
                                                        requested,
                                                        &writer_tx,
                                                        &mut status_clear_deadline,
                                                        &mut current_status,
                                                        &mut status_state,
                                                    );
                                                    overlay_mode = OverlayMode::None;
                                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                                    apply_pty_winsize(
                                                        &mut session,
                                                        terminal_rows,
                                                        terminal_cols,
                                                        overlay_mode,
                                                        status_state.hud_style,
                                                    );
                                                    if status_state.mouse_enabled {
                                                        update_button_registry(
                                                            &button_registry,
                                                            &status_state,
                                                            overlay_mode,
                                                            terminal_cols,
                                                            theme,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                }

                                if let Some(action) = button_registry.find_at(x, y, terminal_rows) {
                                    if action == ButtonAction::ThemePicker {
                                        theme_picker_selected = theme_index_from_theme(theme);
                                        theme_picker_digits.clear();
                                        theme_picker_digit_deadline = None;
                                    }
                                    handle_button_action(
                                        action,
                                        &mut overlay_mode,
                                        &mut settings_menu,
                                        &mut config,
                                        &mut status_state,
                                        &mut auto_voice_enabled,
                                        &mut voice_manager,
                                        &mut session,
                                        &writer_tx,
                                        &mut status_clear_deadline,
                                        &mut current_status,
                                        &mut recording_started_at,
                                        &mut preview_clear_deadline,
                                        &mut last_meter_update,
                                        &mut last_auto_trigger_at,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        &backend_label,
                                        theme,
                                        &button_registry,
                                    );
                                    status_state.hud_button_focus = None;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            recv(session.output_rx) -> chunk => {
                match chunk {
                    Ok(data) => {
                        let now = Instant::now();
                        prompt_tracker.feed_output(&data);
                        {
                            let mut io = TranscriptIo {
                                session: &mut session,
                                writer_tx: &writer_tx,
                                status_clear_deadline: &mut status_clear_deadline,
                                current_status: &mut current_status,
                                status_state: &mut status_state,
                            };
                            try_flush_pending(
                                &mut pending_transcripts,
                                &prompt_tracker,
                                &mut last_enter_at,
                                &mut io,
                                now,
                                transcript_idle_timeout,
                            );
                        }
                        if writer_tx.send(WriterMessage::PtyOutput(data)).is_err() {
                            running = false;
                        }
                        drain_voice_messages(
                            &mut voice_manager,
                            &config,
                            &mut session,
                            &writer_tx,
                            &mut status_clear_deadline,
                            &mut current_status,
                            &mut status_state,
                            &mut session_stats,
                            &mut pending_transcripts,
                            &mut prompt_tracker,
                            &mut last_enter_at,
                            now,
                            transcript_idle_timeout,
                            &mut recording_started_at,
                            &mut preview_clear_deadline,
                            &mut last_meter_update,
                            &mut last_auto_trigger_at,
                            auto_voice_enabled,
                            sound_on_complete,
                            sound_on_error,
                        );
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            default(Duration::from_millis(EVENT_LOOP_IDLE_MS)) => {
                if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                    if let Ok((cols, rows)) = terminal_size() {
                        terminal_cols = cols;
                        terminal_rows = rows;
                        apply_pty_winsize(&mut session, rows, cols, overlay_mode, status_state.hud_style);
                        let _ = writer_tx.send(WriterMessage::Resize { rows, cols });
                        if status_state.mouse_enabled {
                            update_button_registry(
                                &button_registry,
                                &status_state,
                                overlay_mode,
                                terminal_cols,
                                theme,
                            );
                        }
                        match overlay_mode {
                            OverlayMode::Help => {
                                let content = format_help_overlay(theme, cols as usize);
                                let height = help_overlay_height();
                                let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
                            }
                            OverlayMode::ThemePicker => {
                                show_theme_picker_overlay(
                                    &writer_tx,
                                    theme,
                                    theme_picker_selected,
                                    cols,
                                );
                            }
                            OverlayMode::Settings => {
                                show_settings_overlay(
                                    &writer_tx,
                                    theme,
                                    cols,
                                    &settings_menu,
                                    &config,
                                    &status_state,
                                    &backend_label,
                                );
                            }
                            OverlayMode::None => {}
                        }
                    }
                }

                let now = Instant::now();
                if overlay_mode != OverlayMode::ThemePicker {
                    theme_picker_digits.clear();
                    theme_picker_digit_deadline = None;
                } else if let Some(deadline) = theme_picker_digit_deadline {
                    if now >= deadline {
                        if let Some(idx) = theme_picker_parse_index(
                            &theme_picker_digits,
                            THEME_OPTIONS.len(),
                        ) {
                            if apply_theme_picker_index(
                                idx,
                                &mut theme,
                                &mut config,
                                &writer_tx,
                                &mut status_clear_deadline,
                                &mut current_status,
                                &mut status_state,
                                &mut session,
                                &mut terminal_rows,
                                &mut terminal_cols,
                                &mut overlay_mode,
                            ) {
                                theme_picker_selected = theme_index_from_theme(theme);
                            }
                        }
                        theme_picker_digits.clear();
                        theme_picker_digit_deadline = None;
                    }
                }

                if status_state.recording_state == RecordingState::Recording {
                    if let Some(start) = recording_started_at {
                        if now.duration_since(last_recording_update)
                            >= Duration::from_millis(RECORDING_DURATION_UPDATE_MS)
                        {
                            let duration = now.duration_since(start).as_secs_f32();
                            if (duration - last_recording_duration).abs() >= 0.1 {
                                status_state.recording_duration = Some(duration);
                                last_recording_duration = duration;
                                send_enhanced_status_with_buttons(
                                    &writer_tx,
                                    &button_registry,
                                    &status_state,
                                    overlay_mode,
                                    terminal_cols,
                                    theme,
                                );
                            }
                            last_recording_update = now;
                        }
                    }
                }

                if status_state.recording_state == RecordingState::Recording {
                    if now.duration_since(last_meter_update) >= Duration::from_millis(meter_update_ms)
                    {
                        let level = live_meter.level_db();
                        meter_levels.push_back(level);
                        if meter_levels.len() > METER_HISTORY_MAX {
                            meter_levels.pop_front();
                        }
                        status_state.meter_db = Some(level);
                        status_state.meter_levels.clear();
                        status_state
                            .meter_levels
                            .extend(meter_levels.iter().copied());
                        last_meter_update = now;
                        send_enhanced_status_with_buttons(
                            &writer_tx,
                            &button_registry,
                            &status_state,
                            overlay_mode,
                            terminal_cols,
                            theme,
                        );
                    }
                }

                if status_state.recording_state == RecordingState::Processing
                    && now.duration_since(last_processing_tick)
                        >= Duration::from_millis(PROCESSING_SPINNER_TICK_MS)
                {
                    let spinner = progress::SPINNER_BRAILLE
                        [processing_spinner_index % progress::SPINNER_BRAILLE.len()];
                    status_state.message = format!("Processing {spinner}");
                    processing_spinner_index = processing_spinner_index.wrapping_add(1);
                    last_processing_tick = now;
                    send_enhanced_status_with_buttons(
                        &writer_tx,
                        &button_registry,
                        &status_state,
                        overlay_mode,
                        terminal_cols,
                        theme,
                    );
                }

                if status_state.hud_right_panel == HudRightPanel::Heartbeat {
                    let animate = !status_state.hud_right_panel_recording_only
                        || status_state.recording_state == RecordingState::Recording;
                    if animate && now.duration_since(last_heartbeat_tick) >= Duration::from_secs(1)
                    {
                        last_heartbeat_tick = now;
                        send_enhanced_status_with_buttons(
                            &writer_tx,
                            &button_registry,
                            &status_state,
                            overlay_mode,
                            terminal_cols,
                            theme,
                        );
                    }
                }
                prompt_tracker.on_idle(now, auto_idle_timeout);

                drain_voice_messages(
                    &mut voice_manager,
                    &config,
                    &mut session,
                    &writer_tx,
                    &mut status_clear_deadline,
                    &mut current_status,
                    &mut status_state,
                    &mut session_stats,
                    &mut pending_transcripts,
                    &mut prompt_tracker,
                    &mut last_enter_at,
                    now,
                    transcript_idle_timeout,
                    &mut recording_started_at,
                    &mut preview_clear_deadline,
                    &mut last_meter_update,
                    &mut last_auto_trigger_at,
                    auto_voice_enabled,
                    sound_on_complete,
                    sound_on_error,
                );

                {
                    let mut io = TranscriptIo {
                        session: &mut session,
                        writer_tx: &writer_tx,
                        status_clear_deadline: &mut status_clear_deadline,
                        current_status: &mut current_status,
                        status_state: &mut status_state,
                    };
                    try_flush_pending(
                        &mut pending_transcripts,
                        &prompt_tracker,
                        &mut last_enter_at,
                        &mut io,
                        now,
                        transcript_idle_timeout,
                    );
                }

                if auto_voice_enabled
                    && voice_manager.is_idle()
                    && should_auto_trigger(
                        &prompt_tracker,
                        now,
                        auto_idle_timeout,
                        last_auto_trigger_at,
                    )
                {
                    if let Err(err) = start_voice_capture(
                        &mut voice_manager,
                        VoiceCaptureTrigger::Auto,
                        &writer_tx,
                        &mut status_clear_deadline,
                        &mut current_status,
                        &mut status_state,
                    ) {
                        log_debug(&format!("auto voice capture failed: {err:#}"));
                    } else {
                        last_auto_trigger_at = Some(now);
                        recording_started_at = Some(now);
                        reset_capture_visuals(
                            &mut status_state,
                            &mut preview_clear_deadline,
                            &mut last_meter_update,
                        );
                    }
                }

                if let Some(deadline) = preview_clear_deadline {
                    if now >= deadline {
                        preview_clear_deadline = None;
                        if status_state.transcript_preview.is_some() {
                            status_state.transcript_preview = None;
                            send_enhanced_status_with_buttons(
                                &writer_tx,
                                &button_registry,
                                &status_state,
                                overlay_mode,
                                terminal_cols,
                                theme,
                            );
                        }
                    }
                }

                if let Some(deadline) = status_clear_deadline {
                    if now >= deadline {
                        status_clear_deadline = None;
                        current_status = None;
                        status_state.message.clear();
                        // Don't repeatedly set "Auto-voice enabled" - the mode indicator shows it
                        send_enhanced_status_with_buttons(
                            &writer_tx,
                            &button_registry,
                            &status_state,
                            overlay_mode,
                            terminal_cols,
                            theme,
                        );
                    }
                }
            }
        }
    }

    let _ = writer_tx.send(WriterMessage::ClearStatus);
    let _ = writer_tx.send(WriterMessage::Shutdown);
    terminal_guard.restore();
    let stats_output = format_session_stats(&session_stats, theme);
    if should_print_stats(&stats_output) {
        print!("{stats_output}");
        let _ = io::stdout().flush();
    }
    log_debug("=== VoxTerm Overlay Exiting ===");
    Ok(())
}

fn resolved_cols(cached: u16) -> u16 {
    if cached == 0 {
        terminal_size().map(|(c, _)| c).unwrap_or(80)
    } else {
        cached
    }
}

fn resolved_rows(cached: u16) -> u16 {
    if cached == 0 {
        terminal_size().map(|(_, r)| r).unwrap_or(24)
    } else {
        cached
    }
}

fn reserved_rows_for_mode(mode: OverlayMode, cols: u16, hud_style: HudStyle) -> usize {
    match mode {
        OverlayMode::None => status_banner_height(cols as usize, hud_style),
        OverlayMode::Help => help_overlay_height(),
        OverlayMode::ThemePicker => theme_picker_height(),
        OverlayMode::Settings => settings_overlay_height(),
    }
}

fn apply_pty_winsize(
    session: &mut PtyOverlaySession,
    rows: u16,
    cols: u16,
    mode: OverlayMode,
    hud_style: HudStyle,
) {
    if rows == 0 || cols == 0 {
        return;
    }
    let reserved = reserved_rows_for_mode(mode, cols, hud_style) as u16;
    let pty_rows = rows.saturating_sub(reserved).max(1);
    let _ = session.set_winsize(pty_rows, cols);
}

fn update_pty_winsize(
    session: &mut PtyOverlaySession,
    terminal_rows: &mut u16,
    terminal_cols: &mut u16,
    mode: OverlayMode,
    hud_style: HudStyle,
) {
    let rows = resolved_rows(*terminal_rows);
    let cols = resolved_cols(*terminal_cols);
    *terminal_rows = rows;
    *terminal_cols = cols;
    apply_pty_winsize(session, rows, cols, mode, hud_style);
}

fn show_settings_overlay(
    writer_tx: &Sender<WriterMessage>,
    theme: Theme,
    cols: u16,
    settings_menu: &SettingsMenuState,
    config: &OverlayConfig,
    status_state: &StatusLineState,
    backend_label: &str,
) {
    let view = SettingsView {
        selected: settings_menu.selected,
        auto_voice_enabled: status_state.auto_voice_enabled,
        send_mode: config.voice_send_mode,
        sensitivity_db: status_state.sensitivity_db,
        theme,
        hud_style: status_state.hud_style,
        hud_right_panel: config.hud_right_panel,
        hud_right_panel_recording_only: config.hud_right_panel_recording_only,
        mouse_enabled: status_state.mouse_enabled,
        backend_label,
        pipeline: status_state.pipeline,
    };
    let content = format_settings_overlay(&view, cols as usize);
    let height = settings_overlay_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

fn show_theme_picker_overlay(
    writer_tx: &Sender<WriterMessage>,
    theme: Theme,
    selected_idx: usize,
    cols: u16,
) {
    let content = format_theme_picker(theme, selected_idx, cols as usize);
    let height = theme_picker_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

fn parse_arrow_keys(bytes: &[u8]) -> Vec<ArrowKey> {
    let mut keys = Vec::new();
    let mut idx: usize = 0;
    while idx.checked_add(2).map_or(false, |next| next < bytes.len()) {
        if bytes[idx] == 0x1b && (bytes[idx + 1] == b'[' || bytes[idx + 1] == b'O') {
            match bytes[idx + 2] {
                b'A' => keys.push(ArrowKey::Up),
                b'B' => keys.push(ArrowKey::Down),
                b'C' => keys.push(ArrowKey::Right),
                b'D' => keys.push(ArrowKey::Left),
                _ => {}
            }
            idx = idx.saturating_add(3);
        } else {
            idx = idx.saturating_add(1);
        }
    }
    keys
}

fn parse_arrow_keys_only(bytes: &[u8]) -> Option<Vec<ArrowKey>> {
    if bytes.is_empty() {
        return None;
    }
    let mut keys = Vec::new();
    let mut idx: usize = 0;
    while idx.checked_add(2).map_or(false, |next| next < bytes.len()) {
        if bytes[idx] == 0x1b && (bytes[idx + 1] == b'[' || bytes[idx + 1] == b'O') {
            let key = match bytes[idx + 2] {
                b'A' => ArrowKey::Up,
                b'B' => ArrowKey::Down,
                b'C' => ArrowKey::Right,
                b'D' => ArrowKey::Left,
                _ => return None,
            };
            keys.push(key);
            idx = idx.saturating_add(3);
        } else {
            return None;
        }
    }
    if idx == bytes.len() {
        Some(keys)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn toggle_auto_voice(
    auto_voice_enabled: &mut bool,
    voice_manager: &mut VoiceManager,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    last_auto_trigger_at: &mut Option<Instant>,
    recording_started_at: &mut Option<Instant>,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
) {
    *auto_voice_enabled = !*auto_voice_enabled;
    status_state.auto_voice_enabled = *auto_voice_enabled;
    status_state.voice_mode = if *auto_voice_enabled {
        VoiceMode::Auto
    } else {
        VoiceMode::Manual
    };
    let msg = if *auto_voice_enabled {
        "Auto-voice enabled"
    } else if voice_manager.cancel_capture() {
        status_state.recording_state = RecordingState::Idle;
        *recording_started_at = None;
        "Auto-voice disabled (capture cancelled)"
    } else {
        "Auto-voice disabled"
    };
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        msg,
        Some(Duration::from_secs(2)),
    );
    if *auto_voice_enabled && voice_manager.is_idle() {
        if let Err(err) = start_voice_capture(
            voice_manager,
            VoiceCaptureTrigger::Auto,
            writer_tx,
            status_clear_deadline,
            current_status,
            status_state,
        ) {
            log_debug(&format!("auto voice capture failed: {err:#}"));
        } else {
            let now = Instant::now();
            *last_auto_trigger_at = Some(now);
            *recording_started_at = Some(now);
            reset_capture_visuals(status_state, preview_clear_deadline, last_meter_update);
        }
    }
}

fn toggle_send_mode(
    config: &mut OverlayConfig,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
) {
    config.voice_send_mode = match config.voice_send_mode {
        VoiceSendMode::Auto => VoiceSendMode::Insert,
        VoiceSendMode::Insert => VoiceSendMode::Auto,
    };
    status_state.send_mode = config.voice_send_mode;
    let msg = match config.voice_send_mode {
        VoiceSendMode::Auto => "Send mode: auto (sends Enter)",
        VoiceSendMode::Insert => "Send mode: insert (press Enter to send)",
    };
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        msg,
        Some(Duration::from_secs(3)),
    );
}

fn adjust_sensitivity(
    voice_manager: &mut VoiceManager,
    delta_db: f32,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
) {
    let threshold_db = voice_manager.adjust_sensitivity(delta_db);
    status_state.sensitivity_db = threshold_db;
    let direction = if delta_db >= 0.0 {
        "less sensitive"
    } else {
        "more sensitive"
    };
    let msg = format!("Mic sensitivity: {threshold_db:.0} dB ({direction})");
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        &msg,
        Some(Duration::from_secs(3)),
    );
}

fn cycle_hud_right_panel(current: HudRightPanel, direction: i32) -> HudRightPanel {
    const OPTIONS: &[HudRightPanel] = &[
        HudRightPanel::Ribbon,
        HudRightPanel::Dots,
        HudRightPanel::Heartbeat,
        HudRightPanel::Off,
    ];
    let len = OPTIONS.len() as i32;
    let idx = OPTIONS
        .iter()
        .position(|panel| *panel == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    OPTIONS[next]
}

fn update_hud_panel(
    config: &mut OverlayConfig,
    status_state: &mut StatusLineState,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    direction: i32,
) {
    config.hud_right_panel = cycle_hud_right_panel(config.hud_right_panel, direction);
    status_state.hud_right_panel = config.hud_right_panel;
    let label = format!("HUD right panel: {}", config.hud_right_panel);
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        &label,
        Some(Duration::from_secs(2)),
    );
}

fn cycle_hud_style(current: HudStyle, direction: i32) -> HudStyle {
    const OPTIONS: &[HudStyle] = &[HudStyle::Full, HudStyle::Minimal, HudStyle::Hidden];
    let len = OPTIONS.len() as i32;
    let idx = OPTIONS
        .iter()
        .position(|style| *style == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    OPTIONS[next]
}

fn update_hud_style(
    status_state: &mut StatusLineState,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    direction: i32,
) {
    status_state.hud_style = cycle_hud_style(status_state.hud_style, direction);
    let label = format!("HUD style: {}", status_state.hud_style);
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        &label,
        Some(Duration::from_secs(2)),
    );
}

fn resolve_sound_flag(global: bool, specific: bool) -> bool {
    global || specific
}

fn should_skip_banner(is_wrapper: bool, no_startup_banner: bool) -> bool {
    is_wrapper || no_startup_banner
}

fn use_minimal_banner(cols: u16) -> bool {
    cols < 60
}

fn should_print_stats(stats_output: &str) -> bool {
    !stats_output.is_empty()
}

fn toggle_hud_panel_recording_only(
    config: &mut OverlayConfig,
    status_state: &mut StatusLineState,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
) {
    config.hud_right_panel_recording_only = !config.hud_right_panel_recording_only;
    status_state.hud_right_panel_recording_only = config.hud_right_panel_recording_only;
    let label = if config.hud_right_panel_recording_only {
        "HUD right panel: recording-only"
    } else {
        "HUD right panel: always on"
    };
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        label,
        Some(Duration::from_secs(2)),
    );
}

fn toggle_mouse(
    status_state: &mut StatusLineState,
    writer_tx: &Sender<WriterMessage>,
    button_registry: &ButtonRegistry,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
) {
    status_state.mouse_enabled = !status_state.mouse_enabled;
    if status_state.mouse_enabled {
        // Enable mouse mode and update button positions
        let _ = writer_tx.send(WriterMessage::EnableMouse);
        update_button_registry(button_registry, status_state, overlay_mode, terminal_cols, theme);
        set_status(
            writer_tx,
            status_clear_deadline,
            current_status,
            status_state,
            "Mouse: ON - click HUD buttons",
            Some(Duration::from_secs(2)),
        );
    } else {
        // Disable mouse mode and clear button positions
        let _ = writer_tx.send(WriterMessage::DisableMouse);
        button_registry.clear();
        status_state.hud_button_focus = None;
        set_status(
            writer_tx,
            status_clear_deadline,
            current_status,
            status_state,
            "Mouse: OFF",
            Some(Duration::from_secs(2)),
        );
    }
}

fn update_button_registry(
    registry: &ButtonRegistry,
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) {
    registry.clear();
    if overlay_mode != OverlayMode::None {
        registry.set_hud_offset(0);
        return;
    }
    let banner_height = status_banner_height(terminal_cols as usize, status_state.hud_style);
    registry.set_hud_offset(banner_height as u16);
    let positions = get_button_positions(status_state, theme, terminal_cols as usize);
    for pos in positions {
        registry.register(pos.start_x, pos.end_x, pos.row, pos.action);
    }
}

fn visible_button_actions(
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) -> Vec<ButtonAction> {
    if overlay_mode != OverlayMode::None {
        return Vec::new();
    }
    let mut positions = get_button_positions(status_state, theme, terminal_cols as usize);
    positions.sort_by_key(|pos| pos.start_x);
    positions.into_iter().map(|pos| pos.action).collect()
}

fn advance_hud_button_focus(
    status_state: &mut StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
    direction: i32,
) -> bool {
    let actions = visible_button_actions(status_state, overlay_mode, terminal_cols, theme);
    if actions.is_empty() {
        if status_state.hud_button_focus.is_some() {
            status_state.hud_button_focus = None;
            return true;
        }
        return false;
    }

    let current = status_state.hud_button_focus;
    let mut idx = current
        .and_then(|action| actions.iter().position(|candidate| *candidate == action));

    if idx.is_none() {
        idx = if direction >= 0 { Some(0) } else { Some(actions.len().saturating_sub(1)) };
    } else {
        let len = actions.len() as i32;
        let next = (idx.unwrap() as i32 + direction).rem_euclid(len) as usize;
        idx = Some(next);
    }

    let next_action = actions[idx.unwrap()];
    if status_state.hud_button_focus == Some(next_action) {
        return false;
    }
    status_state.hud_button_focus = Some(next_action);
    true
}

fn send_enhanced_status_with_buttons(
    writer_tx: &Sender<WriterMessage>,
    button_registry: &ButtonRegistry,
    status_state: &StatusLineState,
    overlay_mode: OverlayMode,
    terminal_cols: u16,
    theme: Theme,
) {
    send_enhanced_status(writer_tx, status_state);
    if status_state.mouse_enabled {
        update_button_registry(button_registry, status_state, overlay_mode, terminal_cols, theme);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_button_action(
    action: ButtonAction,
    overlay_mode: &mut OverlayMode,
    settings_menu: &mut SettingsMenuState,
    config: &mut OverlayConfig,
    status_state: &mut StatusLineState,
    auto_voice_enabled: &mut bool,
    voice_manager: &mut VoiceManager,
    session: &mut PtyOverlaySession,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    recording_started_at: &mut Option<Instant>,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
    last_auto_trigger_at: &mut Option<Instant>,
    terminal_rows: &mut u16,
    terminal_cols: &mut u16,
    backend_label: &str,
    theme: Theme,
    button_registry: &ButtonRegistry,
) {
    if *overlay_mode != OverlayMode::None {
        return;
    }

    match action {
        ButtonAction::VoiceTrigger => {
            if let Err(err) = start_voice_capture(
                voice_manager,
                VoiceCaptureTrigger::Manual,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
            ) {
                set_status(
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                    "Voice capture failed (see log)",
                    Some(Duration::from_secs(2)),
                );
                log_debug(&format!("voice capture failed: {err:#}"));
            } else {
                *recording_started_at = Some(Instant::now());
                reset_capture_visuals(status_state, preview_clear_deadline, last_meter_update);
            }
        }
        ButtonAction::ToggleAutoVoice => {
            toggle_auto_voice(
                auto_voice_enabled,
                voice_manager,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                last_auto_trigger_at,
                recording_started_at,
                preview_clear_deadline,
                last_meter_update,
            );
        }
        ButtonAction::ToggleSendMode => {
            toggle_send_mode(
                config,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
            );
        }
        ButtonAction::SettingsToggle => {
            *overlay_mode = OverlayMode::Settings;
            update_pty_winsize(
                session,
                terminal_rows,
                terminal_cols,
                *overlay_mode,
                status_state.hud_style,
            );
            let cols = resolved_cols(*terminal_cols);
            show_settings_overlay(
                writer_tx,
                theme,
                cols,
                settings_menu,
                config,
                status_state,
                backend_label,
            );
        }
        ButtonAction::ToggleHudStyle => {
            update_hud_style(
                status_state,
                writer_tx,
                status_clear_deadline,
                current_status,
                1,
            );
            update_pty_winsize(
                session,
                terminal_rows,
                terminal_cols,
                *overlay_mode,
                status_state.hud_style,
            );
        }
        ButtonAction::HudBack => {
            update_hud_style(
                status_state,
                writer_tx,
                status_clear_deadline,
                current_status,
                -1,
            );
            update_pty_winsize(
                session,
                terminal_rows,
                terminal_cols,
                *overlay_mode,
                status_state.hud_style,
            );
        }
        ButtonAction::HelpToggle => {
            *overlay_mode = OverlayMode::Help;
            update_pty_winsize(
                session,
                terminal_rows,
                terminal_cols,
                *overlay_mode,
                status_state.hud_style,
            );
            let cols = resolved_cols(*terminal_cols);
            let content = format_help_overlay(theme, cols as usize);
            let height = help_overlay_height();
            let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
        }
        ButtonAction::ThemePicker => {
            *overlay_mode = OverlayMode::ThemePicker;
            update_pty_winsize(
                session,
                terminal_rows,
                terminal_cols,
                *overlay_mode,
                status_state.hud_style,
            );
            let cols = resolved_cols(*terminal_cols);
            show_theme_picker_overlay(
                writer_tx,
                theme,
                theme_index_from_theme(theme),
                cols,
            );
        }
    }

    if status_state.mouse_enabled {
        update_button_registry(button_registry, status_state, *overlay_mode, *terminal_cols, theme);
    }
}

#[allow(clippy::too_many_arguments)]
fn drain_voice_messages<S: TranscriptSession>(
    voice_manager: &mut VoiceManager,
    config: &OverlayConfig,
    session: &mut S,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    session_stats: &mut SessionStats,
    pending_transcripts: &mut VecDeque<PendingTranscript>,
    prompt_tracker: &mut PromptTracker,
    last_enter_at: &mut Option<Instant>,
    now: Instant,
    transcript_idle_timeout: Duration,
    recording_started_at: &mut Option<Instant>,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
    last_auto_trigger_at: &mut Option<Instant>,
    auto_voice_enabled: bool,
    sound_on_complete: bool,
    sound_on_error: bool,
) {
    let Some(message) = voice_manager.poll_message() else {
        return;
    };
    let rearm_auto = matches!(
        message,
        VoiceJobMessage::Empty { .. } | VoiceJobMessage::Error(_)
    );
    match message {
        VoiceJobMessage::Transcript {
            text,
            source,
            metrics,
        } => {
            update_last_latency(status_state, *recording_started_at, metrics.as_ref(), now);
            let ready =
                transcript_ready(prompt_tracker, *last_enter_at, now, transcript_idle_timeout);
            if auto_voice_enabled {
                prompt_tracker.note_activity(now);
            }
            status_state.recording_state = RecordingState::Idle;
            status_state.pipeline = match source {
                VoiceCaptureSource::Native => Pipeline::Rust,
                VoiceCaptureSource::Python => Pipeline::Python,
            };
            let preview = format_transcript_preview(&text, TRANSCRIPT_PREVIEW_MAX);
            if preview.is_empty() {
                status_state.transcript_preview = None;
                *preview_clear_deadline = None;
            } else {
                status_state.transcript_preview = Some(preview);
                *preview_clear_deadline = Some(now + Duration::from_millis(PREVIEW_CLEAR_MS));
            }
            let drop_note = metrics
                .as_ref()
                .filter(|metrics| metrics.frames_dropped > 0)
                .map(|metrics| format!("dropped {} frames", metrics.frames_dropped));
            let duration_secs = metrics
                .as_ref()
                .map(|metrics| metrics.speech_ms as f32 / 1000.0)
                .unwrap_or(0.0);
            session_stats.record_transcript(duration_secs);
            let drop_suffix = drop_note
                .as_ref()
                .map(|note| format!(", {note}"))
                .unwrap_or_default();
            if ready && pending_transcripts.is_empty() {
                let mut io = TranscriptIo {
                    session,
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                };
                let sent_newline = deliver_transcript(
                    &text,
                    source.label(),
                    config.voice_send_mode,
                    &mut io,
                    0,
                    drop_note.as_deref(),
                );
                if sent_newline {
                    *last_enter_at = Some(now);
                }
            } else {
                let dropped = push_pending_transcript(
                    pending_transcripts,
                    PendingTranscript {
                        text,
                        source,
                        mode: config.voice_send_mode,
                    },
                );
                status_state.queue_depth = pending_transcripts.len();
                if dropped {
                    set_status(
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                        "Transcript queue full (oldest dropped)",
                        Some(Duration::from_secs(2)),
                    );
                }
                if ready {
                    let mut io = TranscriptIo {
                        session,
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                    };
                    try_flush_pending(
                        pending_transcripts,
                        prompt_tracker,
                        last_enter_at,
                        &mut io,
                        now,
                        transcript_idle_timeout,
                    );
                } else if !dropped {
                    let status = format!(
                        "Transcript queued ({}{})",
                        pending_transcripts.len(),
                        drop_suffix
                    );
                    set_status(
                        writer_tx,
                        status_clear_deadline,
                        current_status,
                        status_state,
                        &status,
                        None,
                    );
                }
            }
            if auto_voice_enabled
                && config.voice_send_mode == VoiceSendMode::Insert
                && pending_transcripts.is_empty()
                && voice_manager.is_idle()
            {
                if let Err(err) = start_voice_capture(
                    voice_manager,
                    VoiceCaptureTrigger::Auto,
                    writer_tx,
                    status_clear_deadline,
                    current_status,
                    status_state,
                ) {
                    log_debug(&format!("auto voice capture failed: {err:#}"));
                } else {
                    *last_auto_trigger_at = Some(now);
                    *recording_started_at = Some(now);
                    reset_capture_visuals(status_state, preview_clear_deadline, last_meter_update);
                }
            }
            if sound_on_complete {
                let _ = writer_tx.send(WriterMessage::Bell { count: 1 });
            }
        }
        VoiceJobMessage::Empty { source, metrics } => {
            update_last_latency(status_state, *recording_started_at, metrics.as_ref(), now);
            let mut ctx = VoiceMessageContext {
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
            };
            handle_voice_message(
                VoiceJobMessage::Empty { source, metrics },
                &mut ctx,
            );
        }
        other => {
            if sound_on_error && matches!(other, VoiceJobMessage::Error(_)) {
                let _ = writer_tx.send(WriterMessage::Bell { count: 2 });
            }
            let mut ctx = VoiceMessageContext {
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
            };
            handle_voice_message(
                other,
                &mut ctx,
            );
        }
    }
    if auto_voice_enabled && rearm_auto {
        prompt_tracker.note_activity(now);
    }
    if status_state.recording_state != RecordingState::Recording {
        *recording_started_at = None;
    }
}

fn cycle_theme(current: Theme, direction: i32) -> Theme {
    let len = THEME_OPTIONS.len() as i32;
    if len == 0 {
        return current;
    }
    let idx = THEME_OPTIONS
        .iter()
        .position(|(theme, _, _)| *theme == current)
        .unwrap_or(0) as i32;
    let next = (idx + direction).rem_euclid(len) as usize;
    THEME_OPTIONS[next].0
}

fn theme_index_from_theme(theme: Theme) -> usize {
    THEME_OPTIONS
        .iter()
        .position(|(candidate, _, _)| *candidate == theme)
        .unwrap_or(0)
}

fn apply_theme_selection(
    config: &mut OverlayConfig,
    requested: Theme,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
) -> Theme {
    config.theme_name = Some(requested.to_string());
    let (resolved, note) = resolve_theme_choice(config, requested);
    let _ = writer_tx.send(WriterMessage::SetTheme(resolved));
    let mut status = if resolved == Theme::None && requested != Theme::None {
        "Theme set: none".to_string()
    } else {
        format!("Theme set: {}", requested)
    };
    if let Some(note) = note {
        status = format!("{status} ({note})");
    }
    set_status(
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
        &status,
        Some(Duration::from_secs(2)),
    );
    resolved
}

fn theme_picker_parse_index(digits: &str, total: usize) -> Option<usize> {
    if digits.is_empty() {
        return None;
    }
    let value: usize = digits.parse().ok()?;
    if value == 0 || value > total {
        return None;
    }
    Some(value - 1)
}

fn theme_picker_has_longer_match(prefix: &str, total: usize) -> bool {
    if prefix.is_empty() {
        return false;
    }
    (1..=total).any(|idx| {
        let label = idx.to_string();
        label.len() > prefix.len() && label.starts_with(prefix)
    })
}

#[allow(clippy::too_many_arguments)]
fn apply_theme_picker_index(
    idx: usize,
    theme: &mut Theme,
    config: &mut OverlayConfig,
    writer_tx: &Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut StatusLineState,
    session: &mut PtyOverlaySession,
    terminal_rows: &mut u16,
    terminal_cols: &mut u16,
    overlay_mode: &mut OverlayMode,
) -> bool {
    let Some((_, name, _)) = THEME_OPTIONS.get(idx) else {
        return false;
    };
    let Some(requested) = Theme::from_name(name) else {
        return false;
    };
    *theme = apply_theme_selection(
        config,
        requested,
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
    );
    *overlay_mode = OverlayMode::None;
    let _ = writer_tx.send(WriterMessage::ClearOverlay);
    update_pty_winsize(
        session,
        terminal_rows,
        terminal_cols,
        *overlay_mode,
        status_state.hud_style,
    );
    true
}

fn resolve_theme_choice(config: &OverlayConfig, requested: Theme) -> (Theme, Option<&'static str>) {
    if config.no_color || std::env::var("NO_COLOR").is_ok() {
        return (Theme::None, Some("colors disabled"));
    }
    let mode = config.color_mode();
    if !mode.supports_color() {
        return (Theme::None, Some("no color support"));
    }
    if !mode.supports_truecolor() {
        let resolved = requested.fallback_for_ansi();
        if resolved != requested {
            return (resolved, Some("ansi fallback"));
        }
        return (resolved, None);
    }
    (requested, None)
}

fn reset_capture_visuals(
    status_state: &mut StatusLineState,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
) {
    status_state.transcript_preview = None;
    *preview_clear_deadline = None;
    *last_meter_update = Instant::now();
}

fn update_last_latency(
    status_state: &mut StatusLineState,
    recording_started_at: Option<Instant>,
    metrics: Option<&voxterm::audio::CaptureMetrics>,
    now: Instant,
) {
    let Some(started_at) = recording_started_at else { return; };
    let Some(elapsed) = now.checked_duration_since(started_at) else { return; };
    let elapsed_ms = elapsed.as_millis().min(u128::from(u32::MAX)) as u32;
    let latency_ms = match metrics {
        Some(metrics) if metrics.transcribe_ms > 0 => {
            metrics.transcribe_ms.min(u64::from(u32::MAX)) as u32
        }
        Some(metrics) => {
            let capture_ms = metrics.capture_ms.min(u64::from(u32::MAX)) as u32;
            elapsed_ms.saturating_sub(capture_ms)
        }
        None => elapsed_ms,
    };
    status_state.last_latency_ms = Some(latency_ms);
}

fn format_transcript_preview(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut collapsed = String::new();
    let mut last_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() || ch.is_ascii_control() {
            if !last_space {
                collapsed.push(' ');
                last_space = true;
            }
        } else {
            collapsed.push(ch);
            last_space = false;
        }
    }
    let cleaned = collapsed.trim();
    let max_len = max_len.max(4);
    if cleaned.chars().count() > max_len {
        let keep = max_len.saturating_sub(3);
        let prefix: String = cleaned.chars().take(keep).collect();
        format!("{prefix}...")
    } else {
        cleaned.to_string()
    }
}

fn list_input_devices() -> Result<()> {
    // Support VOXTERM_TEST_DEVICES for testing
    let devices = if let Ok(raw) = std::env::var("VOXTERM_TEST_DEVICES") {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        }
    } else {
        audio::Recorder::list_devices().unwrap_or_else(|err| {
            eprintln!("Failed to list audio input devices: {err}");
            Vec::new()
        })
    };

    if devices.is_empty() {
        println!("No audio input devices detected.");
    } else {
        println!("Available audio input devices:");
        for name in devices {
            println!("  - {name}");
        }
    }
    Ok(())
}

fn install_sigwinch_handler() -> Result<()> {
    unsafe {
        // SAFETY: handle_sigwinch is an extern "C" signal handler with no side effects
        // beyond flipping an atomic flag, which is async-signal-safe.
        let handler = handle_sigwinch as *const () as libc::sighandler_t;
        if libc::signal(libc::SIGWINCH, handler) == libc::SIG_ERR {
            log_debug("failed to install SIGWINCH handler");
            return Err(anyhow!("failed to install SIGWINCH handler"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn sigwinch_handler_sets_flag() {
        SIGWINCH_RECEIVED.store(false, Ordering::SeqCst);
        handle_sigwinch(0);
        assert!(SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst));
    }

    #[test]
    fn install_sigwinch_handler_installs_handler() {
        SIGWINCH_RECEIVED.store(false, Ordering::SeqCst);
        install_sigwinch_handler().expect("install sigwinch handler");
        unsafe {
            // SAFETY: raising SIGWINCH in-process is used for test validation only.
            libc::raise(libc::SIGWINCH);
        }
        for _ in 0..20 {
            if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("SIGWINCH was not received");
    }

    #[test]
    fn resolve_sound_flag_prefers_global() {
        assert!(!resolve_sound_flag(false, false));
        assert!(resolve_sound_flag(true, false));
        assert!(resolve_sound_flag(false, true));
        assert!(resolve_sound_flag(true, true));
    }

    #[test]
    fn should_skip_banner_matches_flags() {
        assert!(!should_skip_banner(false, false));
        assert!(should_skip_banner(true, false));
        assert!(should_skip_banner(false, true));
        assert!(should_skip_banner(true, true));
    }

    #[test]
    fn use_minimal_banner_threshold() {
        assert!(use_minimal_banner(59));
        assert!(!use_minimal_banner(60));
    }

    #[test]
    fn should_print_stats_requires_non_empty() {
        assert!(!should_print_stats(""));
        assert!(should_print_stats("stats"));
    }

    #[test]
    fn resolved_cols_rows_use_cache() {
        assert_eq!(resolved_cols(123), 123);
        assert_eq!(resolved_rows(45), 45);
    }

    #[test]
    fn reserved_rows_for_mode_matches_helpers() {
        let cols = 80;
        assert_eq!(
            reserved_rows_for_mode(OverlayMode::None, cols, HudStyle::Full),
            status_banner_height(cols as usize, HudStyle::Full)
        );
        assert_eq!(
            reserved_rows_for_mode(OverlayMode::Help, cols, HudStyle::Full),
            help_overlay_height()
        );
        assert_eq!(
            reserved_rows_for_mode(OverlayMode::ThemePicker, cols, HudStyle::Full),
            theme_picker_height()
        );
        assert_eq!(
            reserved_rows_for_mode(OverlayMode::Settings, cols, HudStyle::Full),
            settings_overlay_height()
        );
    }

    #[test]
    fn parse_arrow_keys_reads_sequences() {
        let bytes = [
            0x1b,
            b'[',
            b'A',
            0x1b,
            b'[',
            b'B',
            b'x',
            0x1b,
            b'O',
            b'C',
            0x1b,
            b'[',
            b'D',
        ];
        let keys = parse_arrow_keys(&bytes);
        assert_eq!(
            keys,
            vec![ArrowKey::Up, ArrowKey::Down, ArrowKey::Right, ArrowKey::Left]
        );
        assert!(parse_arrow_keys(&[0x1b, b'[']).is_empty());
    }

    #[test]
    fn toggle_send_mode_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();

        toggle_send_mode(
            &mut config,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
        );
        assert_eq!(config.voice_send_mode, VoiceSendMode::Insert);
        assert_eq!(status_state.send_mode, VoiceSendMode::Insert);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("insert"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        toggle_send_mode(
            &mut config,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
        );
        assert_eq!(config.voice_send_mode, VoiceSendMode::Auto);
        assert_eq!(status_state.send_mode, VoiceSendMode::Auto);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("auto"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn adjust_sensitivity_updates_threshold_and_message() {
        let config = OverlayConfig::parse_from(["test-app"]);
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();

        adjust_sensitivity(
            &mut voice_manager,
            5.0,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
        );
        let up_threshold = status_state.sensitivity_db;
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("less sensitive"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        adjust_sensitivity(
            &mut voice_manager,
            -5.0,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
        );
        assert!(status_state.sensitivity_db < up_threshold);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("more sensitive"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn cycle_hud_right_panel_wraps() {
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Ribbon, 1),
            HudRightPanel::Dots
        );
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Ribbon, -1),
            HudRightPanel::Off
        );
        assert_eq!(
            cycle_hud_right_panel(HudRightPanel::Off, 1),
            HudRightPanel::Ribbon
        );
    }

    #[test]
    fn update_hud_panel_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();

        update_hud_panel(
            &mut config,
            &mut status_state,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            1,
        );
        assert_eq!(config.hud_right_panel, HudRightPanel::Dots);
        assert_eq!(status_state.hud_right_panel, HudRightPanel::Dots);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("HUD right panel"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn cycle_hud_style_wraps() {
        assert_eq!(cycle_hud_style(HudStyle::Full, 1), HudStyle::Minimal);
        assert_eq!(cycle_hud_style(HudStyle::Full, -1), HudStyle::Hidden);
        assert_eq!(cycle_hud_style(HudStyle::Hidden, 1), HudStyle::Full);
    }

    #[test]
    fn update_hud_style_updates_state_and_status() {
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();

        update_hud_style(
            &mut status_state,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            1,
        );
        assert_eq!(status_state.hud_style, HudStyle::Minimal);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("HUD style"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn show_settings_overlay_sends_overlay() {
        let config = OverlayConfig::parse_from(["test-app"]);
        let settings_menu = SettingsMenuState::new();
        let mut status_state = StatusLineState::new();
        status_state.pipeline = Pipeline::Rust;
        let (writer_tx, writer_rx) = bounded(4);
        show_settings_overlay(
            &writer_tx,
            Theme::Coral,
            80,
            &settings_menu,
            &config,
            &status_state,
            "codex",
        );
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("overlay message") {
            WriterMessage::ShowOverlay { height, .. } => {
                assert_eq!(height, settings_overlay_height());
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn toggle_auto_voice_updates_state_and_status() {
        let mut config = OverlayConfig::parse_from(["test-app"]);
        config.app.no_python_fallback = true;
        let mut voice_manager = VoiceManager::new(config.app.clone());
        let (writer_tx, writer_rx) = bounded(4);
        let mut status_clear_deadline = None;
        let mut current_status = None;
        let mut status_state = StatusLineState::new();
        let mut auto_voice_enabled = false;
        let mut last_auto_trigger_at = None;
        let mut recording_started_at = None;
        let mut preview_clear_deadline = None;
        let mut last_meter_update = Instant::now();

        toggle_auto_voice(
            &mut auto_voice_enabled,
            &mut voice_manager,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
            &mut last_auto_trigger_at,
            &mut recording_started_at,
            &mut preview_clear_deadline,
            &mut last_meter_update,
        );
        assert!(auto_voice_enabled);
        assert_eq!(status_state.voice_mode, VoiceMode::Auto);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("Auto-voice enabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }

        toggle_auto_voice(
            &mut auto_voice_enabled,
            &mut voice_manager,
            &writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut status_state,
            &mut last_auto_trigger_at,
            &mut recording_started_at,
            &mut preview_clear_deadline,
            &mut last_meter_update,
        );
        assert!(!auto_voice_enabled);
        assert_eq!(status_state.voice_mode, VoiceMode::Manual);
        match writer_rx.recv_timeout(Duration::from_millis(200)).expect("status message") {
            WriterMessage::EnhancedStatus(state) => {
                assert!(state.message.contains("Auto-voice disabled"));
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[cfg(all(unix, feature = "mutants"))]
    #[test]
    fn apply_pty_winsize_updates_session_size() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        let rows = 30;
        let cols = 100;
        apply_pty_winsize(
            &mut session,
            rows,
            cols,
            OverlayMode::None,
            HudStyle::Full,
        );
        let reserved = reserved_rows_for_mode(OverlayMode::None, cols, HudStyle::Full) as u16;
        let expected_rows = rows.saturating_sub(reserved).max(1);
        let (set_rows, set_cols) = session.test_winsize();
        assert_eq!(set_cols, cols);
        assert_eq!(set_rows, expected_rows);

        let before = session.test_winsize();
        apply_pty_winsize(
            &mut session,
            0,
            cols,
            OverlayMode::None,
            HudStyle::Full,
        );
        assert_eq!(session.test_winsize(), before);
    }

    #[cfg(unix)]
    #[test]
    fn update_pty_winsize_updates_cached_dimensions() {
        let mut session =
            PtyOverlaySession::new("cat", ".", &[], "xterm-256color").expect("pty session");
        let mut rows = 0;
        let mut cols = 0;
        update_pty_winsize(
            &mut session,
            &mut rows,
            &mut cols,
            OverlayMode::None,
            HudStyle::Full,
        );
        assert!(rows > 0);
        assert!(cols > 0);
    }
}
