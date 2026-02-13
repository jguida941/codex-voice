use std::io::ErrorKind;
use std::time::{Duration, Instant};

use crossbeam_channel::{never, select, TryRecvError, TrySendError};
use crossterm::terminal::size as terminal_size;
use voxterm::{log_debug, VoiceCaptureSource, VoiceCaptureTrigger};

use crate::arrow_keys::{parse_arrow_keys, parse_arrow_keys_only, ArrowKey};
use crate::button_handlers::{
    advance_hud_button_focus, send_enhanced_status_with_buttons, update_button_registry,
    ButtonActionContext,
};
use crate::buttons::ButtonAction;
use crate::config::{HudRightPanel, VoiceSendMode};
use crate::event_state::{EventLoopDeps, EventLoopState, EventLoopTimers};
use crate::help::{
    help_overlay_height, help_overlay_inner_width_for_terminal, help_overlay_width_for_terminal,
    HELP_OVERLAY_FOOTER,
};
use crate::input::InputEvent;
use crate::overlays::{
    show_help_overlay, show_settings_overlay, show_theme_picker_overlay, OverlayMode,
};
use crate::progress;
use crate::prompt::should_auto_trigger;
use crate::settings::{
    settings_overlay_height, settings_overlay_inner_width_for_terminal,
    settings_overlay_width_for_terminal, SettingsItem, SETTINGS_OVERLAY_FOOTER,
};
use crate::settings_handlers::SettingsActionContext;
use crate::status_line::{RecordingState, METER_HISTORY_MAX};
use crate::terminal::{apply_pty_winsize, resolved_cols, take_sigwinch, update_pty_winsize};
use crate::theme::Theme;
use crate::theme_ops::{
    apply_theme_picker_index, apply_theme_selection, theme_index_from_theme,
    theme_picker_has_longer_match, theme_picker_parse_index,
};
use crate::theme_picker::{
    theme_picker_height, theme_picker_inner_width_for_terminal,
    theme_picker_total_width_for_terminal, THEME_OPTIONS, THEME_PICKER_FOOTER,
    THEME_PICKER_OPTION_START_ROW,
};
use crate::transcript::{try_flush_pending, TranscriptIo};
use crate::voice_control::{
    clear_capture_metrics, drain_voice_messages, reset_capture_visuals, start_voice_capture,
};
use crate::writer::{set_status, WriterMessage};

const EVENT_LOOP_IDLE_MS: u64 = 50;
const THEME_PICKER_NUMERIC_TIMEOUT_MS: u64 = 350;
const RECORDING_DURATION_UPDATE_MS: u64 = 200;
const PROCESSING_SPINNER_TICK_MS: u64 = 120;
const METER_DB_FLOOR: f32 = -60.0;
const PTY_OUTPUT_BATCH_CHUNKS: usize = 8;
const PTY_INPUT_FLUSH_ATTEMPTS: usize = 16;
const PTY_INPUT_MAX_BUFFER_BYTES: usize = 256 * 1024;

fn flush_pending_pty_output(state: &mut EventLoopState, deps: &EventLoopDeps) -> bool {
    let Some(pending) = state.pending_pty_output.take() else {
        return true;
    };
    match deps.writer_tx.try_send(WriterMessage::PtyOutput(pending)) {
        Ok(()) => true,
        Err(TrySendError::Full(WriterMessage::PtyOutput(bytes))) => {
            state.pending_pty_output = Some(bytes);
            false
        }
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Disconnected(_)) => false,
    }
}

fn flush_pending_pty_input(state: &mut EventLoopState, deps: &mut EventLoopDeps) -> bool {
    for _ in 0..PTY_INPUT_FLUSH_ATTEMPTS {
        let Some(front_len) = state.pending_pty_input.front().map(Vec::len) else {
            state.pending_pty_input_offset = 0;
            state.pending_pty_input_bytes = 0;
            return true;
        };
        if state.pending_pty_input_offset >= front_len {
            state.pending_pty_input.pop_front();
            state.pending_pty_input_offset = 0;
            continue;
        }
        let write_result = {
            let Some(front) = state.pending_pty_input.front() else {
                state.pending_pty_input_offset = 0;
                state.pending_pty_input_bytes = 0;
                return true;
            };
            deps.session
                .try_send_bytes(&front[state.pending_pty_input_offset..])
                .map(|written| (written, front.len()))
        };
        match write_result {
            Ok((written, front_len)) => {
                state.pending_pty_input_bytes =
                    state.pending_pty_input_bytes.saturating_sub(written);
                state.pending_pty_input_offset += written;
                if state.pending_pty_input_offset >= front_len {
                    state.pending_pty_input.pop_front();
                    state.pending_pty_input_offset = 0;
                }
            }
            Err(err) => {
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::Interrupted {
                    break;
                }
                log_debug(&format!("failed to flush PTY input queue: {err}"));
                return false;
            }
        }
    }
    true
}

fn write_or_queue_pty_input(
    state: &mut EventLoopState,
    deps: &mut EventLoopDeps,
    bytes: Vec<u8>,
) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if state.pending_pty_input.is_empty() {
        match deps.session.try_send_bytes(&bytes) {
            Ok(written) => {
                if written < bytes.len() {
                    state.pending_pty_input.push_back(bytes[written..].to_vec());
                    state.pending_pty_input_bytes = state
                        .pending_pty_input_bytes
                        .saturating_add(bytes.len() - written);
                }
            }
            Err(err) => {
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::Interrupted {
                    state.pending_pty_input_bytes =
                        state.pending_pty_input_bytes.saturating_add(bytes.len());
                    state.pending_pty_input.push_back(bytes);
                } else {
                    log_debug(&format!("failed to write to PTY: {err}"));
                    return false;
                }
            }
        }
    } else {
        state.pending_pty_input_bytes = state.pending_pty_input_bytes.saturating_add(bytes.len());
        state.pending_pty_input.push_back(bytes);
    }
    flush_pending_pty_input(state, deps)
}

fn run_periodic_tasks(
    state: &mut EventLoopState,
    timers: &mut EventLoopTimers,
    deps: &mut EventLoopDeps,
    now: Instant,
) {
    if take_sigwinch() {
        if let Ok((cols, rows)) = terminal_size() {
            state.terminal_cols = cols;
            state.terminal_rows = rows;
            apply_pty_winsize(
                &mut deps.session,
                rows,
                cols,
                state.overlay_mode,
                state.status_state.hud_style,
            );
            let _ = deps.writer_tx.send(WriterMessage::Resize { rows, cols });
            if state.status_state.mouse_enabled {
                update_button_registry(
                    &deps.button_registry,
                    &state.status_state,
                    state.overlay_mode,
                    state.terminal_cols,
                    state.theme,
                );
            }
            match state.overlay_mode {
                OverlayMode::Help => {
                    show_help_overlay(&deps.writer_tx, state.theme, cols);
                }
                OverlayMode::ThemePicker => {
                    show_theme_picker_overlay(
                        &deps.writer_tx,
                        state.theme,
                        state.theme_picker_selected,
                        cols,
                    );
                }
                OverlayMode::Settings => {
                    show_settings_overlay(
                        &deps.writer_tx,
                        state.theme,
                        cols,
                        &state.settings_menu,
                        &state.config,
                        &state.status_state,
                        &deps.backend_label,
                    );
                }
                OverlayMode::None => {}
            }
        }
    }

    if state.overlay_mode != OverlayMode::ThemePicker {
        state.theme_picker_digits.clear();
        timers.theme_picker_digit_deadline = None;
    } else if let Some(deadline) = timers.theme_picker_digit_deadline {
        if now >= deadline {
            if let Some(idx) =
                theme_picker_parse_index(&state.theme_picker_digits, THEME_OPTIONS.len())
            {
                if apply_theme_picker_index(
                    idx,
                    &mut state.theme,
                    &mut state.config,
                    &deps.writer_tx,
                    &mut timers.status_clear_deadline,
                    &mut state.current_status,
                    &mut state.status_state,
                    &mut deps.session,
                    &mut state.terminal_rows,
                    &mut state.terminal_cols,
                    &mut state.overlay_mode,
                ) {
                    state.theme_picker_selected = theme_index_from_theme(state.theme);
                }
            }
            state.theme_picker_digits.clear();
            timers.theme_picker_digit_deadline = None;
        }
    }

    if state.status_state.recording_state == RecordingState::Recording {
        if let Some(start) = timers.recording_started_at {
            if now.duration_since(timers.last_recording_update)
                >= Duration::from_millis(RECORDING_DURATION_UPDATE_MS)
            {
                let duration = now.duration_since(start).as_secs_f32();
                if (duration - state.last_recording_duration).abs() >= 0.1 {
                    state.status_state.recording_duration = Some(duration);
                    state.last_recording_duration = duration;
                    send_enhanced_status_with_buttons(
                        &deps.writer_tx,
                        &deps.button_registry,
                        &state.status_state,
                        state.overlay_mode,
                        state.terminal_cols,
                        state.theme,
                    );
                }
                timers.last_recording_update = now;
            }
        }
    }

    if state.status_state.recording_state == RecordingState::Recording
        && now.duration_since(timers.last_meter_update)
            >= Duration::from_millis(deps.meter_update_ms)
    {
        let level = deps.live_meter.level_db().max(METER_DB_FLOOR);
        state.meter_levels.push_back(level);
        if state.meter_levels.len() > METER_HISTORY_MAX {
            state.meter_levels.pop_front();
        }
        state.status_state.meter_db = Some(level);
        state.status_state.meter_levels.clear();
        state
            .status_state
            .meter_levels
            .extend(state.meter_levels.iter().copied());
        timers.last_meter_update = now;
        send_enhanced_status_with_buttons(
            &deps.writer_tx,
            &deps.button_registry,
            &state.status_state,
            state.overlay_mode,
            state.terminal_cols,
            state.theme,
        );
    }

    if state.status_state.recording_state == RecordingState::Processing
        && now.duration_since(timers.last_processing_tick)
            >= Duration::from_millis(PROCESSING_SPINNER_TICK_MS)
    {
        let spinner = progress::SPINNER_BRAILLE
            [state.processing_spinner_index % progress::SPINNER_BRAILLE.len()];
        state.status_state.message = format!("Processing {spinner}");
        state.processing_spinner_index = state.processing_spinner_index.wrapping_add(1);
        timers.last_processing_tick = now;
        send_enhanced_status_with_buttons(
            &deps.writer_tx,
            &deps.button_registry,
            &state.status_state,
            state.overlay_mode,
            state.terminal_cols,
            state.theme,
        );
    }

    if state.status_state.hud_right_panel == HudRightPanel::Heartbeat {
        let animate = !state.status_state.hud_right_panel_recording_only
            || state.status_state.recording_state == RecordingState::Recording;
        if animate && now.duration_since(timers.last_heartbeat_tick) >= Duration::from_secs(1) {
            timers.last_heartbeat_tick = now;
            send_enhanced_status_with_buttons(
                &deps.writer_tx,
                &deps.button_registry,
                &state.status_state,
                state.overlay_mode,
                state.terminal_cols,
                state.theme,
            );
        }
    }
    state.prompt_tracker.on_idle(now, deps.auto_idle_timeout);

    drain_voice_messages(
        &mut deps.voice_manager,
        &state.config,
        &mut deps.session,
        &deps.writer_tx,
        &mut timers.status_clear_deadline,
        &mut state.current_status,
        &mut state.status_state,
        &mut state.session_stats,
        &mut state.pending_transcripts,
        &mut state.prompt_tracker,
        &mut timers.last_enter_at,
        now,
        deps.transcript_idle_timeout,
        &mut timers.recording_started_at,
        &mut timers.preview_clear_deadline,
        &mut timers.last_meter_update,
        &mut timers.last_auto_trigger_at,
        state.auto_voice_enabled,
        deps.sound_on_complete,
        deps.sound_on_error,
    );

    {
        let mut io = TranscriptIo {
            session: &mut deps.session,
            writer_tx: &deps.writer_tx,
            status_clear_deadline: &mut timers.status_clear_deadline,
            current_status: &mut state.current_status,
            status_state: &mut state.status_state,
        };
        try_flush_pending(
            &mut state.pending_transcripts,
            &state.prompt_tracker,
            &mut timers.last_enter_at,
            &mut io,
            now,
            deps.transcript_idle_timeout,
        );
    }

    if state.auto_voice_enabled
        && deps.voice_manager.is_idle()
        && should_auto_trigger(
            &state.prompt_tracker,
            now,
            deps.auto_idle_timeout,
            timers.last_auto_trigger_at,
        )
    {
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
            timers.last_auto_trigger_at = Some(now);
            timers.recording_started_at = Some(now);
            reset_capture_visuals(
                &mut state.status_state,
                &mut timers.preview_clear_deadline,
                &mut timers.last_meter_update,
            );
        }
    }

    if let Some(deadline) = timers.preview_clear_deadline {
        if now >= deadline {
            timers.preview_clear_deadline = None;
            if state.status_state.transcript_preview.is_some() {
                state.status_state.transcript_preview = None;
                send_enhanced_status_with_buttons(
                    &deps.writer_tx,
                    &deps.button_registry,
                    &state.status_state,
                    state.overlay_mode,
                    state.terminal_cols,
                    state.theme,
                );
            }
        }
    }

    if let Some(deadline) = timers.status_clear_deadline {
        if now >= deadline {
            timers.status_clear_deadline = None;
            state.current_status = None;
            state.status_state.message.clear();
            // Don't repeatedly set "Auto-voice enabled" - the mode indicator shows it
            send_enhanced_status_with_buttons(
                &deps.writer_tx,
                &deps.button_registry,
                &state.status_state,
                state.overlay_mode,
                state.terminal_cols,
                state.theme,
            );
        }
    }
}

pub(crate) fn run_event_loop(
    state: &mut EventLoopState,
    timers: &mut EventLoopTimers,
    deps: &mut EventLoopDeps,
) {
    let mut running = true;
    let tick_interval = Duration::from_millis(EVENT_LOOP_IDLE_MS);
    let mut last_periodic_tick = Instant::now();
    while running {
        if !flush_pending_pty_input(state, deps) {
            running = false;
            continue;
        }
        if state.pending_pty_output.is_some()
            && !flush_pending_pty_output(state, deps)
            && state.pending_pty_output.is_none()
        {
            running = false;
        }
        let now = Instant::now();
        if now.duration_since(last_periodic_tick) >= tick_interval {
            run_periodic_tasks(state, timers, deps, now);
            last_periodic_tick = now;
        }
        let output_guard = if state.pending_pty_output.is_some() {
            Some(never::<Vec<u8>>())
        } else {
            None
        };
        let input_guard = if state.pending_pty_input_bytes >= PTY_INPUT_MAX_BUFFER_BYTES {
            Some(never::<InputEvent>())
        } else {
            None
        };
        let input_rx = input_guard.as_ref().unwrap_or(&deps.input_rx);
        let output_rx = output_guard.as_ref().unwrap_or(&deps.session.output_rx);
        select! {
            recv(input_rx) -> event => {
                match event {
                    Ok(evt) => {
                        if state.overlay_mode != OverlayMode::None {
                            match (state.overlay_mode, evt) {
                                (_, InputEvent::Exit) => running = false,
                                (mode, InputEvent::ToggleHudStyle) => {
                                    let mut settings_ctx = SettingsActionContext::new(
                                        &mut state.config,
                                        &mut state.status_state,
                                        &mut state.auto_voice_enabled,
                                        &mut deps.voice_manager,
                                        &deps.writer_tx,
                                        &mut timers.status_clear_deadline,
                                        &mut state.current_status,
                                        &mut timers.last_auto_trigger_at,
                                        &mut timers.recording_started_at,
                                        &mut timers.preview_clear_deadline,
                                        &mut timers.last_meter_update,
                                        &deps.button_registry,
                                        mode,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        &mut state.theme,
                                        Some(&mut deps.session),
                                    );
                                    settings_ctx.update_hud_style(1);
                                    if mode == OverlayMode::Settings {
                                        let cols = resolved_cols(state.terminal_cols);
                                        show_settings_overlay(
                                            &deps.writer_tx,
                                            state.theme,
                                            cols,
                                            &state.settings_menu,
                                            &state.config,
                                            &state.status_state,
                                            &deps.backend_label,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::SettingsToggle) => {
                                    state.overlay_mode = OverlayMode::None;
                                    let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    if state.status_state.mouse_enabled {
                                        update_button_registry(
                                            &deps.button_registry,
                                            &state.status_state,
                                            state.overlay_mode,
                                            state.terminal_cols,
                                            state.theme,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::HelpToggle) => {
                                    state.overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    show_help_overlay(&deps.writer_tx, state.theme, cols);
                                }
                                (OverlayMode::Settings, InputEvent::ThemePicker) => {
                                    state.overlay_mode = OverlayMode::ThemePicker;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    state.theme_picker_selected = theme_index_from_theme(state.theme);
                                    state.theme_picker_digits.clear();
                                    timers.theme_picker_digit_deadline = None;
                                    show_theme_picker_overlay(
                                        &deps.writer_tx,
                                        state.theme,
                                        state.theme_picker_selected,
                                        cols,
                                    );
                                }
                                (OverlayMode::Settings, InputEvent::EnterKey) => {
                                    let mut should_redraw = false;
                                    let mut settings_ctx = SettingsActionContext::new(
                                        &mut state.config,
                                        &mut state.status_state,
                                        &mut state.auto_voice_enabled,
                                        &mut deps.voice_manager,
                                        &deps.writer_tx,
                                        &mut timers.status_clear_deadline,
                                        &mut state.current_status,
                                        &mut timers.last_auto_trigger_at,
                                        &mut timers.recording_started_at,
                                        &mut timers.preview_clear_deadline,
                                        &mut timers.last_meter_update,
                                        &deps.button_registry,
                                        state.overlay_mode,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        &mut state.theme,
                                        Some(&mut deps.session),
                                    );
                                    match state.settings_menu.selected_item() {
                                        SettingsItem::AutoVoice => {
                                            settings_ctx.toggle_auto_voice();
                                            should_redraw = true;
                                        }
                                        SettingsItem::SendMode => {
                                            settings_ctx.toggle_send_mode();
                                            should_redraw = true;
                                        }
                                        SettingsItem::Sensitivity => {}
                                        SettingsItem::Theme => {
                                            settings_ctx.cycle_theme(1);
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudStyle => {
                                            settings_ctx.update_hud_style(1);
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudPanel => {
                                            settings_ctx.update_hud_panel(1);
                                            should_redraw = true;
                                        }
                                        SettingsItem::HudAnimate => {
                                            settings_ctx.toggle_hud_panel_recording_only();
                                            should_redraw = true;
                                        }
                                        SettingsItem::Mouse => {
                                            settings_ctx.toggle_mouse();
                                            should_redraw = true;
                                        }
                                        SettingsItem::Backend | SettingsItem::Pipeline => {}
                                        SettingsItem::Close => {
                                            state.overlay_mode = OverlayMode::None;
                                            let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                            update_pty_winsize(
                                                &mut deps.session,
                                                &mut state.terminal_rows,
                                                &mut state.terminal_cols,
                                                state.overlay_mode,
                                                state.status_state.hud_style,
                                            );
                                        }
                                        SettingsItem::Quit => running = false,
                                    }
                                    if state.overlay_mode == OverlayMode::Settings && should_redraw {
                                        let cols = resolved_cols(state.terminal_cols);
                                        show_settings_overlay(
                                            &deps.writer_tx,
                                            state.theme,
                                            cols,
                                            &state.settings_menu,
                                            &state.config,
                                            &state.status_state,
                                            &deps.backend_label,
                                        );
                                    }
                                }
                                (OverlayMode::Settings, InputEvent::Bytes(bytes)) => {
                                    if bytes == [0x1b] {
                                        state.overlay_mode = OverlayMode::None;
                                        let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                        update_pty_winsize(
                                            &mut deps.session,
                                            &mut state.terminal_rows,
                                            &mut state.terminal_cols,
                                            state.overlay_mode,
                                            state.status_state.hud_style,
                                        );
                                    } else {
                                        let mut should_redraw = false;
                                        let mut settings_ctx = SettingsActionContext::new(
                                            &mut state.config,
                                            &mut state.status_state,
                                            &mut state.auto_voice_enabled,
                                            &mut deps.voice_manager,
                                            &deps.writer_tx,
                                            &mut timers.status_clear_deadline,
                                            &mut state.current_status,
                                            &mut timers.last_auto_trigger_at,
                                            &mut timers.recording_started_at,
                                            &mut timers.preview_clear_deadline,
                                            &mut timers.last_meter_update,
                                            &deps.button_registry,
                                            state.overlay_mode,
                                            &mut state.terminal_rows,
                                            &mut state.terminal_cols,
                                            &mut state.theme,
                                            Some(&mut deps.session),
                                        );
                                        for key in parse_arrow_keys(&bytes) {
                                            match key {
                                                ArrowKey::Up => {
                                                    state.settings_menu.move_up();
                                                    should_redraw = true;
                                                }
                                                ArrowKey::Down => {
                                                    state.settings_menu.move_down();
                                                    should_redraw = true;
                                                }
                                                ArrowKey::Left => match state.settings_menu.selected_item() {
                                                    SettingsItem::AutoVoice => {
                                                        settings_ctx.toggle_auto_voice();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::SendMode => {
                                                        settings_ctx.toggle_send_mode();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Sensitivity => {
                                                        settings_ctx.adjust_sensitivity(-5.0);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Theme => {
                                                        settings_ctx.cycle_theme(-1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudStyle => {
                                                        settings_ctx.update_hud_style(-1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudPanel => {
                                                        settings_ctx.update_hud_panel(-1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudAnimate => {
                                                        settings_ctx.toggle_hud_panel_recording_only();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Mouse => {
                                                        settings_ctx.toggle_mouse();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Backend
                                                    | SettingsItem::Pipeline
                                                    | SettingsItem::Close
                                                    | SettingsItem::Quit => {}
                                                },
                                                ArrowKey::Right => match state.settings_menu.selected_item() {
                                                    SettingsItem::AutoVoice => {
                                                        settings_ctx.toggle_auto_voice();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::SendMode => {
                                                        settings_ctx.toggle_send_mode();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Sensitivity => {
                                                        settings_ctx.adjust_sensitivity(5.0);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Theme => {
                                                        settings_ctx.cycle_theme(1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudStyle => {
                                                        settings_ctx.update_hud_style(1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudPanel => {
                                                        settings_ctx.update_hud_panel(1);
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::HudAnimate => {
                                                        settings_ctx.toggle_hud_panel_recording_only();
                                                        should_redraw = true;
                                                    }
                                                    SettingsItem::Mouse => {
                                                        settings_ctx.toggle_mouse();
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
                                            let cols = resolved_cols(state.terminal_cols);
                                            show_settings_overlay(
                                                &deps.writer_tx,
                                                state.theme,
                                                cols,
                                                &state.settings_menu,
                                                &state.config,
                                                &state.status_state,
                                                &deps.backend_label,
                                            );
                                        }
                                    }
                                }
                                (OverlayMode::Help, InputEvent::HelpToggle) => {
                                    state.overlay_mode = OverlayMode::None;
                                    let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                }
                                (OverlayMode::Help, InputEvent::SettingsToggle) => {
                                    state.overlay_mode = OverlayMode::Settings;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    show_settings_overlay(
                                        &deps.writer_tx,
                                        state.theme,
                                        cols,
                                        &state.settings_menu,
                                        &state.config,
                                        &state.status_state,
                                        &deps.backend_label,
                                    );
                                }
                                (OverlayMode::Help, InputEvent::ThemePicker) => {
                                    state.overlay_mode = OverlayMode::ThemePicker;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    state.theme_picker_selected = theme_index_from_theme(state.theme);
                                    state.theme_picker_digits.clear();
                                    timers.theme_picker_digit_deadline = None;
                                    show_theme_picker_overlay(
                                        &deps.writer_tx,
                                        state.theme,
                                        state.theme_picker_selected,
                                        cols,
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::HelpToggle) => {
                                    state.overlay_mode = OverlayMode::Help;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    show_help_overlay(&deps.writer_tx, state.theme, cols);
                                }
                                (OverlayMode::ThemePicker, InputEvent::SettingsToggle) => {
                                    state.overlay_mode = OverlayMode::Settings;
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    let cols = resolved_cols(state.terminal_cols);
                                    show_settings_overlay(
                                        &deps.writer_tx,
                                        state.theme,
                                        cols,
                                        &state.settings_menu,
                                        &state.config,
                                        &state.status_state,
                                        &deps.backend_label,
                                    );
                                }
                                (OverlayMode::ThemePicker, InputEvent::ThemePicker) => {
                                    state.overlay_mode = OverlayMode::None;
                                    let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    state.theme_picker_digits.clear();
                                    timers.theme_picker_digit_deadline = None;
                                }
                                (OverlayMode::ThemePicker, InputEvent::EnterKey) => {
                                    if apply_theme_picker_index(
                                        state.theme_picker_selected,
                                        &mut state.theme,
                                        &mut state.config,
                                        &deps.writer_tx,
                                        &mut timers.status_clear_deadline,
                                        &mut state.current_status,
                                        &mut state.status_state,
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        &mut state.overlay_mode,
                                    ) {
                                        state.theme_picker_selected = theme_index_from_theme(state.theme);
                                    }
                                    state.theme_picker_digits.clear();
                                    timers.theme_picker_digit_deadline = None;
                                }
                                (OverlayMode::ThemePicker, InputEvent::Bytes(bytes)) => {
                                    if bytes == [0x1b] {
                                        state.overlay_mode = OverlayMode::None;
                                        let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                        update_pty_winsize(
                                            &mut deps.session,
                                            &mut state.terminal_rows,
                                            &mut state.terminal_cols,
                                            state.overlay_mode,
                                            state.status_state.hud_style,
                                        );
                                        state.theme_picker_digits.clear();
                                        timers.theme_picker_digit_deadline = None;
                                    } else if let Some(keys) = parse_arrow_keys_only(&bytes) {
                                        let mut moved = false;
                                        let total = THEME_OPTIONS.len();
                                        for key in keys {
                                            let direction = match key {
                                                ArrowKey::Up | ArrowKey::Left => -1,
                                                ArrowKey::Down | ArrowKey::Right => 1,
                                            };
                                            if direction != 0 && total > 0 {
                                                let next = (state.theme_picker_selected as i32 + direction)
                                                    .rem_euclid(total as i32) as usize;
                                                if next != state.theme_picker_selected {
                                                    state.theme_picker_selected = next;
                                                    moved = true;
                                                }
                                            }
                                        }
                                        if moved {
                                            let cols = resolved_cols(state.terminal_cols);
                                            show_theme_picker_overlay(
                                                &deps.writer_tx,
                                                state.theme,
                                                state.theme_picker_selected,
                                                cols,
                                            );
                                        }
                                        state.theme_picker_digits.clear();
                                        timers.theme_picker_digit_deadline = None;
                                    } else {
                                        let digits: String = bytes
                                            .iter()
                                            .filter(|b| b.is_ascii_digit())
                                            .map(|b| *b as char)
                                            .collect();
                                        if !digits.is_empty() {
                                            state.theme_picker_digits.push_str(&digits);
                                            if state.theme_picker_digits.len() > 3 {
                                                state.theme_picker_digits.clear();
                                            }
                                            let now = Instant::now();
                                            timers.theme_picker_digit_deadline = Some(
                                                now + Duration::from_millis(THEME_PICKER_NUMERIC_TIMEOUT_MS),
                                            );
                                            if let Some(idx) = theme_picker_parse_index(
                                                &state.theme_picker_digits,
                                                THEME_OPTIONS.len(),
                                            ) {
                                                if !theme_picker_has_longer_match(
                                                    &state.theme_picker_digits,
                                                    THEME_OPTIONS.len(),
                                                ) {
                                                    if apply_theme_picker_index(
                                                        idx,
                                                        &mut state.theme,
                                                        &mut state.config,
                                                        &deps.writer_tx,
                                                        &mut timers.status_clear_deadline,
                                                        &mut state.current_status,
                                                        &mut state.status_state,
                                                        &mut deps.session,
                                                        &mut state.terminal_rows,
                                                        &mut state.terminal_cols,
                                                        &mut state.overlay_mode,
                                                    ) {
                                                        state.theme_picker_selected = theme_index_from_theme(state.theme);
                                                    }
                                                    state.theme_picker_digits.clear();
                                                    timers.theme_picker_digit_deadline = None;
                                                }
                                            }
                                        }
                                    }
                                }
                                (_, _) => {
                                    state.overlay_mode = OverlayMode::None;
                                    let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                    update_pty_winsize(
                                        &mut deps.session,
                                        &mut state.terminal_rows,
                                        &mut state.terminal_cols,
                                        state.overlay_mode,
                                        state.status_state.hud_style,
                                    );
                                    if state.status_state.mouse_enabled {
                                        update_button_registry(
                                            &deps.button_registry,
                                            &state.status_state,
                                            state.overlay_mode,
                                            state.terminal_cols,
                                            state.theme,
                                        );
                                    }
                                }
                            }
                            continue;
                        }
                        match evt {
                            InputEvent::HelpToggle => {
                                state.status_state.hud_button_focus = None;
                                state.overlay_mode = OverlayMode::Help;
                                update_pty_winsize(
                                    &mut deps.session,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    state.overlay_mode,
                                    state.status_state.hud_style,
                                );
                                let cols = resolved_cols(state.terminal_cols);
                                show_help_overlay(&deps.writer_tx, state.theme, cols);
                            }
                            InputEvent::ThemePicker => {
                                state.status_state.hud_button_focus = None;
                                state.overlay_mode = OverlayMode::ThemePicker;
                                update_pty_winsize(
                                    &mut deps.session,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    state.overlay_mode,
                                    state.status_state.hud_style,
                                );
                                let cols = resolved_cols(state.terminal_cols);
                                state.theme_picker_selected = theme_index_from_theme(state.theme);
                                state.theme_picker_digits.clear();
                                timers.theme_picker_digit_deadline = None;
                                show_theme_picker_overlay(
                                    &deps.writer_tx,
                                    state.theme,
                                    state.theme_picker_selected,
                                    cols,
                                );
                            }
                            InputEvent::SettingsToggle => {
                                state.status_state.hud_button_focus = None;
                                state.overlay_mode = OverlayMode::Settings;
                                update_pty_winsize(
                                    &mut deps.session,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    state.overlay_mode,
                                    state.status_state.hud_style,
                                );
                                let cols = resolved_cols(state.terminal_cols);
                                show_settings_overlay(
                                    &deps.writer_tx,
                                    state.theme,
                                    cols,
                                    &state.settings_menu,
                                    &state.config,
                                    &state.status_state,
                                    &deps.backend_label,
                                );
                            }
                            InputEvent::ToggleHudStyle => {
                                let mut settings_ctx = SettingsActionContext::new(
                                    &mut state.config,
                                    &mut state.status_state,
                                    &mut state.auto_voice_enabled,
                                    &mut deps.voice_manager,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut timers.last_auto_trigger_at,
                                    &mut timers.recording_started_at,
                                    &mut timers.preview_clear_deadline,
                                    &mut timers.last_meter_update,
                                    &deps.button_registry,
                                    state.overlay_mode,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    &mut state.theme,
                                    Some(&mut deps.session),
                                );
                                settings_ctx.update_hud_style(1);
                                state.status_state.hud_button_focus = None;
                                if state.status_state.mouse_enabled {
                                    update_button_registry(
                                        &deps.button_registry,
                                        &state.status_state,
                                        state.overlay_mode,
                                        state.terminal_cols,
                                        state.theme,
                                    );
                                }
                            }
                            InputEvent::Bytes(bytes) => {
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
                                                &mut state.status_state,
                                                state.overlay_mode,
                                                state.terminal_cols,
                                                state.theme,
                                                direction,
                                            )
                                        {
                                            moved = true;
                                        }
                                    }
                                    if moved {
                                        send_enhanced_status_with_buttons(
                                            &deps.writer_tx,
                                            &deps.button_registry,
                                            &state.status_state,
                                            state.overlay_mode,
                                            state.terminal_cols,
                                            state.theme,
                                        );
                                        continue;
                                    }
                                }

                                state.status_state.hud_button_focus = None;
                                if !write_or_queue_pty_input(state, deps, bytes) {
                                    running = false;
                                }
                            }
                            InputEvent::VoiceTrigger => {
                                if let Err(err) = start_voice_capture(
                                    &mut deps.voice_manager,
                                    VoiceCaptureTrigger::Manual,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut state.status_state,
                                ) {
                                    set_status(
                                        &deps.writer_tx,
                                        &mut timers.status_clear_deadline,
                                        &mut state.current_status,
                                        &mut state.status_state,
                                        "Voice capture failed (see log)",
                                        Some(Duration::from_secs(2)),
                                    );
                                    log_debug(&format!("voice capture failed: {err:#}"));
                                } else {
                                    timers.recording_started_at = Some(Instant::now());
                                    reset_capture_visuals(
                                        &mut state.status_state,
                                        &mut timers.preview_clear_deadline,
                                        &mut timers.last_meter_update,
                                    );
                                }
                            }
                            InputEvent::ToggleAutoVoice => {
                                let mut settings_ctx = SettingsActionContext::new(
                                    &mut state.config,
                                    &mut state.status_state,
                                    &mut state.auto_voice_enabled,
                                    &mut deps.voice_manager,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut timers.last_auto_trigger_at,
                                    &mut timers.recording_started_at,
                                    &mut timers.preview_clear_deadline,
                                    &mut timers.last_meter_update,
                                    &deps.button_registry,
                                    state.overlay_mode,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    &mut state.theme,
                                    Some(&mut deps.session),
                                );
                                settings_ctx.toggle_auto_voice();
                                if state.status_state.mouse_enabled {
                                    update_button_registry(
                                        &deps.button_registry,
                                        &state.status_state,
                                        state.overlay_mode,
                                        state.terminal_cols,
                                        state.theme,
                                    );
                                }
                            }
                            InputEvent::ToggleSendMode => {
                                let mut settings_ctx = SettingsActionContext::new(
                                    &mut state.config,
                                    &mut state.status_state,
                                    &mut state.auto_voice_enabled,
                                    &mut deps.voice_manager,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut timers.last_auto_trigger_at,
                                    &mut timers.recording_started_at,
                                    &mut timers.preview_clear_deadline,
                                    &mut timers.last_meter_update,
                                    &deps.button_registry,
                                    state.overlay_mode,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    &mut state.theme,
                                    Some(&mut deps.session),
                                );
                                settings_ctx.toggle_send_mode();
                                if state.status_state.mouse_enabled {
                                    update_button_registry(
                                        &deps.button_registry,
                                        &state.status_state,
                                        state.overlay_mode,
                                        state.terminal_cols,
                                        state.theme,
                                    );
                                }
                            }
                            InputEvent::IncreaseSensitivity => {
                                let mut settings_ctx = SettingsActionContext::new(
                                    &mut state.config,
                                    &mut state.status_state,
                                    &mut state.auto_voice_enabled,
                                    &mut deps.voice_manager,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut timers.last_auto_trigger_at,
                                    &mut timers.recording_started_at,
                                    &mut timers.preview_clear_deadline,
                                    &mut timers.last_meter_update,
                                    &deps.button_registry,
                                    state.overlay_mode,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    &mut state.theme,
                                    Some(&mut deps.session),
                                );
                                settings_ctx.adjust_sensitivity(5.0);
                            }
                            InputEvent::DecreaseSensitivity => {
                                let mut settings_ctx = SettingsActionContext::new(
                                    &mut state.config,
                                    &mut state.status_state,
                                    &mut state.auto_voice_enabled,
                                    &mut deps.voice_manager,
                                    &deps.writer_tx,
                                    &mut timers.status_clear_deadline,
                                    &mut state.current_status,
                                    &mut timers.last_auto_trigger_at,
                                    &mut timers.recording_started_at,
                                    &mut timers.preview_clear_deadline,
                                    &mut timers.last_meter_update,
                                    &deps.button_registry,
                                    state.overlay_mode,
                                    &mut state.terminal_rows,
                                    &mut state.terminal_cols,
                                    &mut state.theme,
                                    Some(&mut deps.session),
                                );
                                settings_ctx.adjust_sensitivity(-5.0);
                            }
                            InputEvent::EnterKey => {
                                if let Some(action) = state.status_state.hud_button_focus {
                                    state.status_state.hud_button_focus = None;
                                    if action == ButtonAction::ThemePicker {
                                        state.theme_picker_selected = theme_index_from_theme(state.theme);
                                        state.theme_picker_digits.clear();
                                        timers.theme_picker_digit_deadline = None;
                                    }
                                    {
                                        let mut button_ctx = ButtonActionContext::new(
                                            &mut state.overlay_mode,
                                            &mut state.settings_menu,
                                            &mut state.config,
                                            &mut state.status_state,
                                            &mut state.auto_voice_enabled,
                                            &mut deps.voice_manager,
                                            &mut deps.session,
                                            &deps.writer_tx,
                                            &mut timers.status_clear_deadline,
                                            &mut state.current_status,
                                            &mut timers.recording_started_at,
                                            &mut timers.preview_clear_deadline,
                                            &mut timers.last_meter_update,
                                            &mut timers.last_auto_trigger_at,
                                            &mut state.terminal_rows,
                                            &mut state.terminal_cols,
                                            &deps.backend_label,
                                            &mut state.theme,
                                            &deps.button_registry,
                                        );
                                        button_ctx.handle_action(action);
                                    }
                                    send_enhanced_status_with_buttons(
                                        &deps.writer_tx,
                                        &deps.button_registry,
                                        &state.status_state,
                                        state.overlay_mode,
                                        state.terminal_cols,
                                        state.theme,
                                    );
                                    continue;
                                }
                                // In insert mode, Enter stops capture early and sends what was recorded
                                if state.config.voice_send_mode == VoiceSendMode::Insert && !deps.voice_manager.is_idle() {
                                    if deps.voice_manager.active_source() == Some(VoiceCaptureSource::Python) {
                                        let _ = deps.voice_manager.cancel_capture();
                                        state.status_state.recording_state = RecordingState::Idle;
                                        clear_capture_metrics(&mut state.status_state);
                                        timers.recording_started_at = None;
                                        set_status(
                                            &deps.writer_tx,
                                            &mut timers.status_clear_deadline,
                                            &mut state.current_status,
                                            &mut state.status_state,
                                            "Capture cancelled (python fallback cannot stop early)",
                                            Some(Duration::from_secs(3)),
                                        );
                                    } else {
                                        deps.voice_manager.request_early_stop();
                                        state.status_state.recording_state = RecordingState::Processing;
                                        clear_capture_metrics(&mut state.status_state);
                                        state.processing_spinner_index = 0;
                                        timers.last_processing_tick = Instant::now();
                                        set_status(
                                            &deps.writer_tx,
                                            &mut timers.status_clear_deadline,
                                            &mut state.current_status,
                                            &mut state.status_state,
                                            "Processing",
                                            None,
                                        );
                                    }
                                } else {
                                    // Forward Enter to PTY
                                    if !write_or_queue_pty_input(state, deps, vec![0x0d]) {
                                        running = false;
                                    } else {
                                        timers.last_enter_at = Some(Instant::now());
                                    }
                                }
                            }
                            InputEvent::Exit => {
                                running = false;
                            }
                            InputEvent::MouseClick { x, y } => {
                                // Only process clicks if mouse is enabled
                                if !state.status_state.mouse_enabled {
                                    continue;
                                }

                                // Overlay click handling (close button + state.theme selection)
                                if state.overlay_mode != OverlayMode::None {
                                    let overlay_height = match state.overlay_mode {
                                        OverlayMode::Help => help_overlay_height(),
                                        OverlayMode::ThemePicker => theme_picker_height(),
                                        OverlayMode::Settings => settings_overlay_height(),
                                        OverlayMode::None => 0,
                                    };
                                    if overlay_height == 0 || state.terminal_rows == 0 {
                                        continue;
                                    }
                                    let overlay_top_y =
                                        state.terminal_rows.saturating_sub(overlay_height as u16).saturating_add(1);
                                    if y < overlay_top_y || y > state.terminal_rows {
                                        continue;
                                    }
                                    let overlay_row = (y - overlay_top_y) as usize + 1;
                                    let cols = resolved_cols(state.terminal_cols) as usize;

                                    let (overlay_width, inner_width, footer_title) = match state.overlay_mode {
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
                                            state.overlay_mode = OverlayMode::None;
                                            let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                            apply_pty_winsize(
                                                &mut deps.session,
                                                state.terminal_rows,
                                                state.terminal_cols,
                                                state.overlay_mode,
                                                state.status_state.hud_style,
                                            );
                                            if state.status_state.mouse_enabled {
                                                update_button_registry(
                                                    &deps.button_registry,
                                                    &state.status_state,
                                                    state.overlay_mode,
                                                    state.terminal_cols,
                                                    state.theme,
                                                );
                                            }
                                        }
                                        continue;
                                    }

                                    if state.overlay_mode == OverlayMode::ThemePicker {
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
                                                    state.theme = apply_theme_selection(
                                                        &mut state.config,
                                                        requested,
                                                        &deps.writer_tx,
                                                        &mut timers.status_clear_deadline,
                                                        &mut state.current_status,
                                                        &mut state.status_state,
                                                    );
                                                    state.overlay_mode = OverlayMode::None;
                                                    let _ = deps.writer_tx.send(WriterMessage::ClearOverlay);
                                                    apply_pty_winsize(
                                                        &mut deps.session,
                                                        state.terminal_rows,
                                                        state.terminal_cols,
                                                        state.overlay_mode,
                                                        state.status_state.hud_style,
                                                    );
                                                    if state.status_state.mouse_enabled {
                                                        update_button_registry(
                                                            &deps.button_registry,
                                                            &state.status_state,
                                                            state.overlay_mode,
                                                            state.terminal_cols,
                                                            state.theme,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                }

                                if let Some(action) = deps.button_registry.find_at(x, y, state.terminal_rows) {
                                    if action == ButtonAction::ThemePicker {
                                        state.theme_picker_selected = theme_index_from_theme(state.theme);
                                        state.theme_picker_digits.clear();
                                        timers.theme_picker_digit_deadline = None;
                                    }
                                    {
                                        let mut button_ctx = ButtonActionContext::new(
                                            &mut state.overlay_mode,
                                            &mut state.settings_menu,
                                            &mut state.config,
                                            &mut state.status_state,
                                            &mut state.auto_voice_enabled,
                                            &mut deps.voice_manager,
                                            &mut deps.session,
                                            &deps.writer_tx,
                                            &mut timers.status_clear_deadline,
                                            &mut state.current_status,
                                            &mut timers.recording_started_at,
                                            &mut timers.preview_clear_deadline,
                                            &mut timers.last_meter_update,
                                            &mut timers.last_auto_trigger_at,
                                            &mut state.terminal_rows,
                                            &mut state.terminal_cols,
                                            &deps.backend_label,
                                            &mut state.theme,
                                            &deps.button_registry,
                                        );
                                        button_ctx.handle_action(action);
                                    }
                                    state.status_state.hud_button_focus = None;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            recv(output_rx) -> chunk => {
                match chunk {
                    Ok(mut data) => {
                        let mut output_disconnected = false;
                        for _ in 0..PTY_OUTPUT_BATCH_CHUNKS {
                            match deps.session.output_rx.try_recv() {
                                Ok(next) => data.extend_from_slice(&next),
                                Err(TryRecvError::Empty) => break,
                                Err(TryRecvError::Disconnected) => {
                                    output_disconnected = true;
                                    break;
                                }
                            }
                        }
                        let now = Instant::now();
                        state.prompt_tracker.feed_output(&data);
                        {
                            let mut io = TranscriptIo {
                                session: &mut deps.session,
                                writer_tx: &deps.writer_tx,
                                status_clear_deadline: &mut timers.status_clear_deadline,
                                current_status: &mut state.current_status,
                                status_state: &mut state.status_state,
                            };
                            try_flush_pending(
                                &mut state.pending_transcripts,
                                &state.prompt_tracker,
                                &mut timers.last_enter_at,
                                &mut io,
                                now,
                                deps.transcript_idle_timeout,
                            );
                        }
                        match deps.writer_tx.try_send(WriterMessage::PtyOutput(data)) {
                            Ok(()) => {}
                            Err(TrySendError::Full(WriterMessage::PtyOutput(bytes))) => {
                                state.pending_pty_output = Some(bytes);
                            }
                            Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => {
                                running = false;
                            }
                        }
                        drain_voice_messages(
                            &mut deps.voice_manager,
                            &state.config,
                            &mut deps.session,
                            &deps.writer_tx,
                            &mut timers.status_clear_deadline,
                            &mut state.current_status,
                            &mut state.status_state,
                            &mut state.session_stats,
                            &mut state.pending_transcripts,
                            &mut state.prompt_tracker,
                            &mut timers.last_enter_at,
                            now,
                            deps.transcript_idle_timeout,
                            &mut timers.recording_started_at,
                            &mut timers.preview_clear_deadline,
                            &mut timers.last_meter_update,
                            &mut timers.last_auto_trigger_at,
                            state.auto_voice_enabled,
                            deps.sound_on_complete,
                            deps.sound_on_error,
                        );
                        if output_disconnected && state.pending_pty_output.is_none() {
                            running = false;
                        }
                    }
                    Err(_) => {
                        running = false;
                    }
                }
            }
            default(tick_interval) => {}
        }
    }
}
