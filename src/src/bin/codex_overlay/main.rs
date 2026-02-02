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

use crate::banner::{format_minimal_banner, format_startup_banner, BannerConfig};
use crate::config::{OverlayConfig, VoiceSendMode};
use crate::help::{format_help_overlay, help_overlay_height};
use crate::hud::HudRegistry;
use crate::input::{spawn_input_thread, InputEvent};
use crate::prompt::{
    resolve_prompt_log, resolve_prompt_regex, should_auto_trigger, PromptLogger, PromptTracker,
};
use crate::session_stats::{format_session_stats, SessionStats};
use crate::settings::{
    format_settings_overlay, settings_overlay_height, SettingsItem, SettingsMenuState, SettingsView,
};
use crate::status_line::{
    status_banner_height, Pipeline, RecordingState, StatusLineState, VoiceMode,
};
use crate::theme::Theme;
use crate::theme_picker::{format_theme_picker, theme_picker_height, THEME_OPTIONS};
use crate::transcript::{
    deliver_transcript, push_pending_transcript, transcript_ready, try_flush_pending,
    PendingTranscript, TranscriptIo, TranscriptSession,
};
use crate::voice_control::{handle_voice_message, start_voice_capture, VoiceManager};
use crate::writer::{send_enhanced_status, set_status, spawn_writer_thread, WriterMessage};

/// Flag set by SIGWINCH handler to trigger terminal resize.
static SIGWINCH_RECEIVED: AtomicBool = AtomicBool::new(false);

/// Max pending messages for the output writer thread.
const WRITER_CHANNEL_CAPACITY: usize = 512;

/// Max pending input events before backpressure.
const INPUT_CHANNEL_CAPACITY: usize = 256;

const METER_HISTORY_MAX: usize = 24;
const METER_UPDATE_MS: u64 = 80;
const PREVIEW_CLEAR_MS: u64 = 3000;
const TRANSCRIPT_PREVIEW_MAX: usize = 60;

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
    let sound_on_complete = config.app.sounds || config.app.sound_on_complete;
    let sound_on_error = config.app.sounds || config.app.sound_on_error;
    let mut theme = config.theme();
    if config.app.doctor {
        let mut report = base_doctor_report(&config.app, "voxterm");
        let backend = config.resolve_backend();
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

    // Resolve backend command and args
    let backend = config.resolve_backend();
    let backend_label = backend.label.clone();

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
    let skip_banner = env::var("VOXTERM_WRAPPER")
        .map(|v| v == "1")
        .unwrap_or(false)
        || env::var("VOXTERM_NO_STARTUP_BANNER").is_ok();
    if !skip_banner {
        let banner = match terminal_size() {
            Ok((cols, _)) if cols < 60 => format_minimal_banner(theme),
            _ => format_startup_banner(&banner_config, theme),
        };
        print!("{banner}");
        let _ = io::stdout().flush();
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

    let mut terminal_cols = 0u16;
    let mut terminal_rows = 0u16;
    if let Ok((cols, rows)) = terminal_size() {
        terminal_cols = cols;
        terminal_rows = rows;
        apply_pty_winsize(&mut session, rows, cols, OverlayMode::None);
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
    status_state.voice_mode = if auto_voice_enabled {
        VoiceMode::Auto
    } else {
        VoiceMode::Manual
    };
    status_state.pipeline = Pipeline::Rust;
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
                    &mut meter_levels,
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
                                (OverlayMode::Settings, InputEvent::SettingsToggle) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                    );
                                }
                                (OverlayMode::Settings, InputEvent::HelpToggle) => {
                                    overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
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
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    let content = format_theme_picker(theme, cols as usize);
                                    let height = theme_picker_height();
                                    let _ = writer_tx.send(WriterMessage::ShowOverlay {
                                        content,
                                        height,
                                    });
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
                                                &mut meter_levels,
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
                                        SettingsItem::Backend | SettingsItem::Pipeline => {}
                                        SettingsItem::Close => {
                                            overlay_mode = OverlayMode::None;
                                            let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                            update_pty_winsize(
                                                &mut session,
                                                &mut terminal_rows,
                                                &mut terminal_cols,
                                                overlay_mode,
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
                                                            &mut meter_levels,
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
                                                            &mut meter_levels,
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
                                    );
                                }
                                (OverlayMode::Help, InputEvent::SettingsToggle) => {
                                    overlay_mode = OverlayMode::Settings;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
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
                                    );
                                    let cols = resolved_cols(terminal_cols);
                                    let content = format_theme_picker(theme, cols as usize);
                                    let height = theme_picker_height();
                                    let _ =
                                        writer_tx.send(WriterMessage::ShowOverlay { content, height });
                                }
                                (OverlayMode::ThemePicker, InputEvent::HelpToggle) => {
                                    overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
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
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::EnterKey) => {
                                    overlay_mode = OverlayMode::None;
                                    let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut session,
                                        &mut terminal_rows,
                                        &mut terminal_cols,
                                        overlay_mode,
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::Bytes(bytes)) => {
                                    if bytes.contains(&0x1b) {
                                        overlay_mode = OverlayMode::None;
                                        let _ = writer_tx.send(WriterMessage::ClearOverlay);
                                        update_pty_winsize(
                                            &mut session,
                                            &mut terminal_rows,
                                            &mut terminal_cols,
                                            overlay_mode,
                                        );
                                    } else if let Some(idx) =
                                        bytes.iter().find_map(|b| theme_index_from_byte(*b))
                                    {
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
                                                update_pty_winsize(
                                                    &mut session,
                                                    &mut terminal_rows,
                                                    &mut terminal_cols,
                                                    overlay_mode,
                                                );
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
                                    );
                                }
                            }
                            continue;
                        }
                        match evt {
                            InputEvent::HelpToggle => {
                                overlay_mode = OverlayMode::Help;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                );
                                let cols = resolved_cols(terminal_cols);
                                let content = format_help_overlay(theme, cols as usize);
                                let height = help_overlay_height();
                                let _ =
                                    writer_tx.send(WriterMessage::ShowOverlay { content, height });
                            }
                            InputEvent::ThemePicker => {
                                overlay_mode = OverlayMode::ThemePicker;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
                                );
                                let cols = resolved_cols(terminal_cols);
                                let content = format_theme_picker(theme, cols as usize);
                                let height = theme_picker_height();
                                let _ =
                                    writer_tx.send(WriterMessage::ShowOverlay { content, height });
                            }
                            InputEvent::SettingsToggle => {
                                overlay_mode = OverlayMode::Settings;
                                update_pty_winsize(
                                    &mut session,
                                    &mut terminal_rows,
                                    &mut terminal_cols,
                                    overlay_mode,
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
                            InputEvent::Bytes(bytes) => {
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
                                        &mut meter_levels,
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
                                    &mut meter_levels,
                                    &mut preview_clear_deadline,
                                    &mut last_meter_update,
                                );
                            }
                            InputEvent::ToggleSendMode => {
                                toggle_send_mode(
                                    &mut config,
                                    &writer_tx,
                                    &mut status_clear_deadline,
                                    &mut current_status,
                                    &mut status_state,
                                );
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
                                        status_state.recording_duration = None;
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
                            &mut meter_levels,
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
            default(Duration::from_millis(50)) => {
                if SIGWINCH_RECEIVED.swap(false, Ordering::SeqCst) {
                    if let Ok((cols, rows)) = terminal_size() {
                        terminal_cols = cols;
                        terminal_rows = rows;
                        apply_pty_winsize(&mut session, rows, cols, overlay_mode);
                        let _ = writer_tx.send(WriterMessage::Resize { rows, cols });
                        match overlay_mode {
                            OverlayMode::Help => {
                                let content = format_help_overlay(theme, cols as usize);
                                let height = help_overlay_height();
                                let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
                            }
                            OverlayMode::ThemePicker => {
                                let content = format_theme_picker(theme, cols as usize);
                                let height = theme_picker_height();
                                let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
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
                if status_state.recording_state == RecordingState::Recording {
                    if let Some(start) = recording_started_at {
                        if now.duration_since(last_recording_update) >= Duration::from_millis(200) {
                            let duration = now.duration_since(start).as_secs_f32();
                            if (duration - last_recording_duration).abs() >= 0.1 {
                                status_state.recording_duration = Some(duration);
                                last_recording_duration = duration;
                                send_enhanced_status(&writer_tx, &status_state);
                            }
                            last_recording_update = now;
                        }
                    }
                } else if status_state.recording_duration.is_some() {
                    status_state.recording_duration = None;
                    last_recording_duration = 0.0;
                    send_enhanced_status(&writer_tx, &status_state);
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
                        send_enhanced_status(&writer_tx, &status_state);
                    }
                } else if !meter_levels.is_empty() || status_state.meter_db.is_some() {
                    meter_levels.clear();
                    status_state.meter_levels.clear();
                    status_state.meter_db = None;
                    send_enhanced_status(&writer_tx, &status_state);
                }

                if status_state.recording_state == RecordingState::Processing
                    && now.duration_since(last_processing_tick) >= Duration::from_millis(120)
                {
                    let spinner = progress::SPINNER_BRAILLE
                        [processing_spinner_index % progress::SPINNER_BRAILLE.len()];
                    status_state.message = format!("Processing {spinner}");
                    processing_spinner_index = processing_spinner_index.wrapping_add(1);
                    last_processing_tick = now;
                    send_enhanced_status(&writer_tx, &status_state);
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
                    &mut meter_levels,
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
                            &mut meter_levels,
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
                            send_enhanced_status(&writer_tx, &status_state);
                        }
                    }
                }

                if let Some(deadline) = status_clear_deadline {
                    if now >= deadline {
                        status_clear_deadline = None;
                        current_status = None;
                        status_state.message.clear();
                        // Don't repeatedly set "Auto-voice enabled" - the mode indicator shows it
                        send_enhanced_status(&writer_tx, &status_state);
                    }
                }
            }
        }
    }

    let _ = writer_tx.send(WriterMessage::ClearStatus);
    let _ = writer_tx.send(WriterMessage::Shutdown);
    terminal_guard.restore();
    let stats_output = format_session_stats(&session_stats, theme);
    if !stats_output.is_empty() {
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

fn reserved_rows_for_mode(mode: OverlayMode, cols: u16) -> usize {
    match mode {
        OverlayMode::None => status_banner_height(cols as usize),
        OverlayMode::Help => help_overlay_height(),
        OverlayMode::ThemePicker => theme_picker_height(),
        OverlayMode::Settings => settings_overlay_height(),
    }
}

fn apply_pty_winsize(session: &mut PtyOverlaySession, rows: u16, cols: u16, mode: OverlayMode) {
    if rows == 0 || cols == 0 {
        return;
    }
    let reserved = reserved_rows_for_mode(mode, cols) as u16;
    let pty_rows = rows.saturating_sub(reserved).max(1);
    let _ = session.set_winsize(pty_rows, cols);
}

fn update_pty_winsize(
    session: &mut PtyOverlaySession,
    terminal_rows: &mut u16,
    terminal_cols: &mut u16,
    mode: OverlayMode,
) {
    let rows = resolved_rows(*terminal_rows);
    let cols = resolved_cols(*terminal_cols);
    *terminal_rows = rows;
    *terminal_cols = cols;
    apply_pty_winsize(session, rows, cols, mode);
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
        backend_label,
        pipeline: status_state.pipeline,
    };
    let content = format_settings_overlay(&view, cols as usize);
    let height = settings_overlay_height();
    let _ = writer_tx.send(WriterMessage::ShowOverlay { content, height });
}

#[derive(Debug, Clone, Copy)]
enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

fn parse_arrow_keys(bytes: &[u8]) -> Vec<ArrowKey> {
    let mut keys = Vec::new();
    let mut idx = 0;
    while idx + 2 < bytes.len() {
        if bytes[idx] == 0x1b && (bytes[idx + 1] == b'[' || bytes[idx + 1] == b'O') {
            match bytes[idx + 2] {
                b'A' => keys.push(ArrowKey::Up),
                b'B' => keys.push(ArrowKey::Down),
                b'C' => keys.push(ArrowKey::Right),
                b'D' => keys.push(ArrowKey::Left),
                _ => {}
            }
            idx += 3;
        } else {
            idx += 1;
        }
    }
    keys
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
    meter_levels: &mut VecDeque<f32>,
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
            reset_capture_visuals(
                meter_levels,
                status_state,
                preview_clear_deadline,
                last_meter_update,
            );
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
    meter_levels: &mut VecDeque<f32>,
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
            if let Some(started_at) = *recording_started_at {
                if let Some(elapsed) = now.checked_duration_since(started_at) {
                    let ms = elapsed.as_millis().min(u128::from(u32::MAX)) as u32;
                    status_state.last_latency_ms = Some(ms);
                }
            }
            let ready =
                transcript_ready(prompt_tracker, *last_enter_at, now, transcript_idle_timeout);
            if auto_voice_enabled {
                prompt_tracker.note_activity(now);
            }
            status_state.recording_state = RecordingState::Idle;
            status_state.recording_duration = None;
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
                    reset_capture_visuals(
                        meter_levels,
                        status_state,
                        preview_clear_deadline,
                        last_meter_update,
                    );
                }
            }
            if sound_on_complete {
                let _ = writer_tx.send(WriterMessage::Bell { count: 1 });
            }
        }
        VoiceJobMessage::Empty { source, metrics } => {
            if let Some(started_at) = *recording_started_at {
                if let Some(elapsed) = now.checked_duration_since(started_at) {
                    let ms = elapsed.as_millis().min(u128::from(u32::MAX)) as u32;
                    status_state.last_latency_ms = Some(ms);
                }
            }
            handle_voice_message(
                VoiceJobMessage::Empty { source, metrics },
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
            );
        }
        other => {
            if sound_on_error && matches!(other, VoiceJobMessage::Error(_)) {
                let _ = writer_tx.send(WriterMessage::Bell { count: 2 });
            }
            handle_voice_message(
                other,
                config,
                session,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
                session_stats,
                auto_voice_enabled,
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

fn theme_index_from_byte(byte: u8) -> Option<usize> {
    if (b'1'..=b'9').contains(&byte) {
        Some((byte - b'1') as usize)
    } else {
        None
    }
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
    meter_levels: &mut VecDeque<f32>,
    status_state: &mut StatusLineState,
    preview_clear_deadline: &mut Option<Instant>,
    last_meter_update: &mut Instant,
) {
    meter_levels.clear();
    status_state.meter_levels.clear();
    status_state.meter_db = None;
    status_state.transcript_preview = None;
    *preview_clear_deadline = None;
    *last_meter_update = Instant::now();
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
}
