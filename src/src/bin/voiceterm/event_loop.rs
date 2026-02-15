//! Core runtime loop that coordinates PTY output, input events, and voice jobs.

#[cfg(test)]
use std::cell::Cell;
use std::io::{self, ErrorKind};
use std::time::{Duration, Instant};

use crossbeam_channel::{never, select, TryRecvError, TrySendError};
use crossterm::terminal::size as terminal_size;
use voiceterm::{log_debug, VoiceCaptureSource, VoiceCaptureTrigger};

use crate::arrow_keys::{is_arrow_escape_noise, parse_arrow_keys, parse_arrow_keys_only, ArrowKey};
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

#[cfg(test)]
type TrySendHook = fn(&[u8]) -> io::Result<usize>;
#[cfg(test)]
type TakeSigwinchHook = fn() -> bool;
#[cfg(test)]
type TerminalSizeHook = fn() -> io::Result<(u16, u16)>;
#[cfg(test)]
type StartCaptureHook = fn(
    &mut crate::voice_control::VoiceManager,
    VoiceCaptureTrigger,
    &crossbeam_channel::Sender<WriterMessage>,
    &mut Option<Instant>,
    &mut Option<String>,
    &mut crate::status_line::StatusLineState,
) -> anyhow::Result<()>;
#[cfg(test)]
thread_local! {
    static TRY_SEND_HOOK: Cell<Option<TrySendHook>> = const { Cell::new(None) };
    static TAKE_SIGWINCH_HOOK: Cell<Option<TakeSigwinchHook>> = const { Cell::new(None) };
    static TERMINAL_SIZE_HOOK: Cell<Option<TerminalSizeHook>> = const { Cell::new(None) };
    static START_CAPTURE_HOOK: Cell<Option<StartCaptureHook>> = const { Cell::new(None) };
}

#[cfg(test)]
fn set_try_send_hook(hook: Option<TrySendHook>) {
    TRY_SEND_HOOK.with(|slot| slot.set(hook));
}

#[cfg(test)]
fn set_take_sigwinch_hook(hook: Option<TakeSigwinchHook>) {
    TAKE_SIGWINCH_HOOK.with(|slot| slot.set(hook));
}

#[cfg(test)]
fn set_terminal_size_hook(hook: Option<TerminalSizeHook>) {
    TERMINAL_SIZE_HOOK.with(|slot| slot.set(hook));
}

#[cfg(test)]
fn set_start_capture_hook(hook: Option<StartCaptureHook>) {
    START_CAPTURE_HOOK.with(|slot| slot.set(hook));
}

fn try_send_pty_bytes(
    session: &mut voiceterm::pty_session::PtyOverlaySession,
    bytes: &[u8],
) -> io::Result<usize> {
    #[cfg(test)]
    {
        if let Some(hook) = TRY_SEND_HOOK.with(|slot| slot.get()) {
            return hook(bytes);
        }
    }
    session.try_send_bytes(bytes)
}

fn take_sigwinch_flag() -> bool {
    #[cfg(test)]
    {
        if let Some(hook) = TAKE_SIGWINCH_HOOK.with(|slot| slot.get()) {
            return hook();
        }
    }
    take_sigwinch()
}

fn read_terminal_size() -> io::Result<(u16, u16)> {
    #[cfg(test)]
    {
        if let Some(hook) = TERMINAL_SIZE_HOOK.with(|slot| slot.get()) {
            return hook();
        }
    }
    terminal_size()
}

fn start_voice_capture_with_hook(
    voice_manager: &mut crate::voice_control::VoiceManager,
    trigger: VoiceCaptureTrigger,
    writer_tx: &crossbeam_channel::Sender<WriterMessage>,
    status_clear_deadline: &mut Option<Instant>,
    current_status: &mut Option<String>,
    status_state: &mut crate::status_line::StatusLineState,
) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        if let Some(hook) = START_CAPTURE_HOOK.with(|slot| slot.get()) {
            return hook(
                voice_manager,
                trigger,
                writer_tx,
                status_clear_deadline,
                current_status,
                status_state,
            );
        }
    }
    start_voice_capture(
        voice_manager,
        trigger,
        writer_tx,
        status_clear_deadline,
        current_status,
        status_state,
    )
}

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
            try_send_pty_bytes(&mut deps.session, &front[state.pending_pty_input_offset..])
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
        match try_send_pty_bytes(&mut deps.session, &bytes) {
            Ok(written) => {
                let Some(remaining) = bytes.get(written..) else {
                    log_debug("PTY write returned an out-of-range byte count");
                    return false;
                };
                if !remaining.is_empty() {
                    state.pending_pty_input.push_back(remaining.to_vec());
                    state.pending_pty_input_bytes = state
                        .pending_pty_input_bytes
                        .saturating_add(remaining.len());
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
    if take_sigwinch_flag() {
        if let Ok((cols, rows)) = read_terminal_size() {
            // JetBrains terminals can emit SIGWINCH without a geometry delta.
            // Skip no-op resize work so we avoid redraw flicker and redundant backend SIGWINCH.
            let size_changed = state.terminal_cols != cols || state.terminal_rows != rows;
            if size_changed {
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
        &deps.voice_macros,
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
        if let Err(err) = start_voice_capture_with_hook(
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

fn flush_pending_output_or_continue(state: &mut EventLoopState, deps: &EventLoopDeps) -> bool {
    if state.pending_pty_output.is_none() {
        return true;
    }
    let flush_ok = flush_pending_pty_output(state, deps);
    flush_ok || state.pending_pty_output.is_some()
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
        if !flush_pending_output_or_continue(state, deps) {
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
                                        SettingsItem::Macros => {
                                            settings_ctx.toggle_macros_enabled();
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
                                        SettingsItem::HudBorders => {
                                            settings_ctx.update_hud_border_style(1);
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
                                        SettingsItem::Latency => {
                                            settings_ctx.cycle_latency_display(1);
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
                                                    SettingsItem::Macros => {
                                                        settings_ctx.toggle_macros_enabled();
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
                                                    SettingsItem::HudBorders => {
                                                        settings_ctx.update_hud_border_style(-1);
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
                                                    SettingsItem::Latency => {
                                                        settings_ctx.cycle_latency_display(-1);
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
                                                    SettingsItem::Macros => {
                                                        settings_ctx.toggle_macros_enabled();
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
                                                    SettingsItem::HudBorders => {
                                                        settings_ctx.update_hud_border_style(1);
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
                                                    SettingsItem::Latency => {
                                                        settings_ctx.cycle_latency_display(1);
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
                                if state.suppress_startup_escape_input
                                    && is_arrow_escape_noise(&bytes)
                                {
                                    continue;
                                }
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
                        if !data.is_empty() {
                            state.suppress_startup_escape_input = false;
                        }
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
                            &deps.voice_macros,
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use crossbeam_channel::{bounded, Receiver, Sender};
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::io;
    use voiceterm::pty_session::PtyOverlaySession;

    use crate::buttons::ButtonRegistry;
    use crate::config::OverlayConfig;
    use crate::prompt::{PromptLogger, PromptTracker};
    use crate::session_stats::SessionStats;
    use crate::settings::SettingsMenuState;
    use crate::status_line::{Pipeline, StatusLineState, VoiceMode};
    use crate::theme::Theme;
    use crate::theme_ops::theme_index_from_theme;
    use crate::voice_control::VoiceManager;
    use crate::voice_macros::VoiceMacros;

    thread_local! {
        static HOOK_CALLS: Cell<usize> = const { Cell::new(0) };
        static START_CAPTURE_CALLS: Cell<usize> = const { Cell::new(0) };
    }

    struct TrySendHookGuard;

    impl Drop for TrySendHookGuard {
        fn drop(&mut self) {
            set_try_send_hook(None);
            HOOK_CALLS.with(|calls| calls.set(0));
        }
    }

    fn install_try_send_hook(hook: TrySendHook) -> TrySendHookGuard {
        set_try_send_hook(Some(hook));
        HOOK_CALLS.with(|calls| calls.set(0));
        TrySendHookGuard
    }

    struct SigwinchHookGuard;

    impl Drop for SigwinchHookGuard {
        fn drop(&mut self) {
            set_take_sigwinch_hook(None);
            set_terminal_size_hook(None);
        }
    }

    fn install_sigwinch_hooks(
        take_hook: TakeSigwinchHook,
        size_hook: TerminalSizeHook,
    ) -> SigwinchHookGuard {
        set_take_sigwinch_hook(Some(take_hook));
        set_terminal_size_hook(Some(size_hook));
        SigwinchHookGuard
    }

    struct StartCaptureHookGuard;

    impl Drop for StartCaptureHookGuard {
        fn drop(&mut self) {
            set_start_capture_hook(None);
            START_CAPTURE_CALLS.with(|calls| calls.set(0));
        }
    }

    fn install_start_capture_hook(hook: StartCaptureHook) -> StartCaptureHookGuard {
        set_start_capture_hook(Some(hook));
        START_CAPTURE_CALLS.with(|calls| calls.set(0));
        StartCaptureHookGuard
    }

    fn hook_would_block(_: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::WouldBlock, "hook would block"))
    }

    fn hook_interrupted(_: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::Interrupted, "hook interrupted"))
    }

    fn hook_broken_pipe(_: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(ErrorKind::BrokenPipe, "hook broken pipe"))
    }

    fn hook_non_empty_full_write(bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            Err(io::Error::new(
                ErrorKind::BrokenPipe,
                "unexpected empty write",
            ))
        } else {
            Ok(bytes.len())
        }
    }

    fn hook_one_byte(bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            Ok(0)
        } else {
            Ok(1)
        }
    }

    fn hook_partial_then_would_block(bytes: &[u8]) -> io::Result<usize> {
        HOOK_CALLS.with(|calls| {
            let call_index = calls.get();
            calls.set(call_index + 1);
            if call_index == 0 {
                if bytes.is_empty() {
                    Ok(0)
                } else {
                    Ok(1)
                }
            } else {
                Err(io::Error::new(ErrorKind::WouldBlock, "hook would block"))
            }
        })
    }

    fn hook_take_sigwinch_true() -> bool {
        true
    }

    fn hook_take_sigwinch_false() -> bool {
        false
    }

    fn hook_terminal_size_80x24() -> io::Result<(u16, u16)> {
        Ok((80, 24))
    }

    fn hook_start_capture_count(
        _: &mut crate::voice_control::VoiceManager,
        _: VoiceCaptureTrigger,
        _: &crossbeam_channel::Sender<WriterMessage>,
        _: &mut Option<Instant>,
        _: &mut Option<String>,
        _: &mut crate::status_line::StatusLineState,
    ) -> anyhow::Result<()> {
        START_CAPTURE_CALLS.with(|calls| calls.set(calls.get() + 1));
        Ok(())
    }

    fn hook_start_capture_err(
        _: &mut crate::voice_control::VoiceManager,
        _: VoiceCaptureTrigger,
        _: &crossbeam_channel::Sender<WriterMessage>,
        _: &mut Option<Instant>,
        _: &mut Option<String>,
        _: &mut crate::status_line::StatusLineState,
    ) -> anyhow::Result<()> {
        Err(anyhow::anyhow!("forced start capture failure"))
    }

    fn build_harness(
        cmd: &str,
        args: &[&str],
        writer_capacity: usize,
    ) -> (
        EventLoopState,
        EventLoopTimers,
        EventLoopDeps,
        Receiver<WriterMessage>,
        Sender<InputEvent>,
    ) {
        let config = OverlayConfig::parse_from(["voiceterm"]);
        let mut status_state = StatusLineState::new();
        let auto_voice_enabled = config.auto_voice;
        status_state.sensitivity_db = config.app.voice_vad_threshold_db;
        status_state.auto_voice_enabled = auto_voice_enabled;
        status_state.send_mode = config.voice_send_mode;
        status_state.latency_display = config.latency_display;
        status_state.macros_enabled = true;
        status_state.hud_right_panel = config.hud_right_panel;
        status_state.hud_border_style = config.hud_border_style;
        status_state.hud_right_panel_recording_only = config.hud_right_panel_recording_only;
        status_state.hud_style = config.hud_style;
        status_state.voice_mode = if auto_voice_enabled {
            VoiceMode::Auto
        } else {
            VoiceMode::Manual
        };
        status_state.pipeline = Pipeline::Rust;
        status_state.mouse_enabled = true;

        let theme = Theme::Codex;
        let prompt_tracker = PromptTracker::new(None, true, PromptLogger::new(None));
        let voice_manager = VoiceManager::new(config.app.clone());
        let live_meter = voice_manager.meter();
        let arg_vec: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let session = PtyOverlaySession::new(cmd, ".", &arg_vec, "xterm-256color")
            .expect("start pty session");

        let (writer_tx, writer_rx) = bounded(writer_capacity);
        let (input_tx, input_rx) = bounded(16);

        let state = EventLoopState {
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
            terminal_rows: 24,
            terminal_cols: 80,
            last_recording_duration: 0.0,
            processing_spinner_index: 0,
            pending_pty_output: None,
            pending_pty_input: VecDeque::new(),
            pending_pty_input_offset: 0,
            pending_pty_input_bytes: 0,
            suppress_startup_escape_input: false,
        };

        let now = Instant::now();
        let timers = EventLoopTimers {
            theme_picker_digit_deadline: None,
            status_clear_deadline: None,
            preview_clear_deadline: None,
            last_auto_trigger_at: None,
            last_enter_at: None,
            recording_started_at: None,
            last_recording_update: now,
            last_processing_tick: now,
            last_heartbeat_tick: now,
            last_meter_update: now,
        };

        let deps = EventLoopDeps {
            session,
            voice_manager,
            writer_tx,
            input_rx,
            button_registry: ButtonRegistry::new(),
            backend_label: "test".to_string(),
            sound_on_complete: false,
            sound_on_error: false,
            live_meter,
            meter_update_ms: 50,
            auto_idle_timeout: Duration::from_millis(300),
            transcript_idle_timeout: Duration::from_millis(100),
            voice_macros: VoiceMacros::default(),
        };

        (state, timers, deps, writer_rx, input_tx)
    }

    #[test]
    fn flush_pending_pty_output_returns_true_when_empty() {
        let (mut state, _timers, deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        assert!(flush_pending_pty_output(&mut state, &deps));
    }

    #[test]
    fn flush_pending_pty_output_requeues_when_writer_is_full() {
        let (mut state, _timers, deps, _writer_rx, _input_tx) = build_harness("cat", &[], 1);
        deps.writer_tx
            .try_send(WriterMessage::ClearStatus)
            .expect("fill bounded writer channel");
        state.pending_pty_output = Some(vec![1, 2, 3]);

        assert!(!flush_pending_pty_output(&mut state, &deps));
        assert_eq!(state.pending_pty_output, Some(vec![1, 2, 3]));
    }

    #[test]
    fn flush_pending_pty_output_returns_false_when_writer_is_disconnected() {
        let (mut state, _timers, deps, writer_rx, _input_tx) = build_harness("cat", &[], 8);
        drop(writer_rx);
        state.pending_pty_output = Some(vec![9, 8, 7]);

        assert!(!flush_pending_pty_output(&mut state, &deps));
        assert!(
            state.pending_pty_output.is_none(),
            "disconnected writes should not keep stale pending output"
        );
    }

    #[test]
    fn flush_pending_pty_input_empty_queue_resets_counters() {
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input_offset = 3;
        state.pending_pty_input_bytes = 9;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert_eq!(state.pending_pty_input_offset, 0);
        assert_eq!(state.pending_pty_input_bytes, 0);
    }

    #[test]
    fn flush_pending_pty_input_pops_front_when_offset_reaches_chunk_end() {
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(vec![1, 2]);
        state.pending_pty_input.push_back(vec![3]);
        state.pending_pty_input_offset = 2;
        state.pending_pty_input_bytes = 1;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert!(state.pending_pty_input.is_empty());
        assert_eq!(state.pending_pty_input_offset, 0);
        assert_eq!(state.pending_pty_input_bytes, 0);
    }

    #[test]
    fn event_loop_constants_match_expected_limits() {
        assert_eq!(METER_DB_FLOOR, -60.0);
        assert_eq!(PTY_INPUT_MAX_BUFFER_BYTES, 256 * 1024);
    }

    #[test]
    fn flush_pending_pty_input_treats_would_block_as_retryable() {
        let _hook = install_try_send_hook(hook_would_block);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(vec![1, 2, 3]);
        state.pending_pty_input_bytes = 3;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert_eq!(state.pending_pty_input_offset, 0);
        assert_eq!(state.pending_pty_input_bytes, 3);
        assert_eq!(state.pending_pty_input.len(), 1);
    }

    #[test]
    fn flush_pending_pty_input_treats_interrupted_as_retryable() {
        let _hook = install_try_send_hook(hook_interrupted);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(vec![7, 8]);
        state.pending_pty_input_bytes = 2;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert_eq!(state.pending_pty_input_offset, 0);
        assert_eq!(state.pending_pty_input_bytes, 2);
        assert_eq!(state.pending_pty_input.len(), 1);
    }

    #[test]
    fn flush_pending_pty_input_returns_false_for_non_retry_errors() {
        let _hook = install_try_send_hook(hook_broken_pipe);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(vec![7, 8]);
        state.pending_pty_input_bytes = 2;

        assert!(!flush_pending_pty_input(&mut state, &mut deps));
    }

    #[test]
    fn flush_pending_pty_input_does_not_write_empty_slice_when_offset_is_at_chunk_end() {
        let _hook = install_try_send_hook(hook_non_empty_full_write);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(vec![1, 2]);
        state.pending_pty_input_offset = 2;
        state.pending_pty_input_bytes = 0;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert!(state.pending_pty_input.is_empty());
    }

    #[test]
    fn flush_pending_pty_input_drains_many_single_byte_chunks_within_attempt_budget() {
        let _hook = install_try_send_hook(hook_one_byte);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        for _ in 0..12 {
            state.pending_pty_input.push_back(vec![b'x']);
        }
        state.pending_pty_input_bytes = 12;

        assert!(flush_pending_pty_input(&mut state, &mut deps));
        assert!(
            state.pending_pty_input.is_empty(),
            "single-byte writes should clear this queue within 16 attempts"
        );
        assert_eq!(state.pending_pty_input_bytes, 0);
    }

    #[test]
    fn write_or_queue_pty_input_queues_remainder_after_partial_write() {
        let _hook = install_try_send_hook(hook_partial_then_would_block);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let bytes = vec![1, 2, 3, 4];

        assert!(write_or_queue_pty_input(&mut state, &mut deps, bytes));
        assert_eq!(state.pending_pty_input_bytes, 3);
        assert_eq!(state.pending_pty_input.len(), 1);
        assert_eq!(state.pending_pty_input.front(), Some(&vec![2, 3, 4]));
    }

    #[test]
    fn write_or_queue_pty_input_queues_all_bytes_on_would_block() {
        let _hook = install_try_send_hook(hook_would_block);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let bytes = vec![1, 2, 3];

        assert!(write_or_queue_pty_input(
            &mut state,
            &mut deps,
            bytes.clone()
        ));
        assert_eq!(state.pending_pty_input_bytes, bytes.len());
        assert_eq!(state.pending_pty_input.front(), Some(&bytes));
    }

    #[test]
    fn write_or_queue_pty_input_returns_false_on_non_retryable_error() {
        let _hook = install_try_send_hook(hook_broken_pipe);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);

        assert!(!write_or_queue_pty_input(
            &mut state,
            &mut deps,
            vec![1, 2, 3]
        ));
        assert!(
            state.pending_pty_input.is_empty(),
            "non-retry errors should not queue bytes"
        );
        assert_eq!(state.pending_pty_input_bytes, 0);
    }

    #[test]
    fn write_or_queue_pty_input_returns_true_for_live_session() {
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let bytes = b"hello".to_vec();

        assert!(write_or_queue_pty_input(&mut state, &mut deps, bytes));
        assert_eq!(state.pending_pty_input_offset, 0);
        assert!(
            state.pending_pty_input_bytes <= 5,
            "live writes may flush immediately but should not overcount pending bytes"
        );
    }

    #[test]
    fn run_periodic_tasks_clears_theme_digits_outside_picker_mode() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.overlay_mode = OverlayMode::None;
        state.theme_picker_digits = "12".to_string();
        timers.theme_picker_digit_deadline = Some(Instant::now() + Duration::from_secs(1));

        run_periodic_tasks(&mut state, &mut timers, &mut deps, Instant::now());
        assert!(state.theme_picker_digits.is_empty());
        assert!(timers.theme_picker_digit_deadline.is_none());
    }

    #[test]
    fn take_sigwinch_flag_uses_installed_hook_value() {
        let _hooks = install_sigwinch_hooks(hook_take_sigwinch_false, hook_terminal_size_80x24);
        assert!(!take_sigwinch_flag());
    }

    #[test]
    fn start_voice_capture_with_hook_propagates_hook_error() {
        let _capture = install_start_capture_hook(hook_start_capture_err);
        let (mut state, _timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let mut status_clear_deadline = None;
        let mut current_status = None;

        let result = start_voice_capture_with_hook(
            &mut deps.voice_manager,
            VoiceCaptureTrigger::Auto,
            &deps.writer_tx,
            &mut status_clear_deadline,
            &mut current_status,
            &mut state.status_state,
        );
        assert!(result.is_err());
    }

    #[test]
    fn run_periodic_tasks_sigwinch_no_size_change_skips_resize_messages() {
        let _hooks = install_sigwinch_hooks(hook_take_sigwinch_true, hook_terminal_size_80x24);
        let (mut state, mut timers, mut deps, writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.terminal_cols = 80;
        state.terminal_rows = 24;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, Instant::now());
        assert!(
            writer_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "no resize event expected when geometry is unchanged"
        );
    }

    #[test]
    fn run_periodic_tasks_sigwinch_single_dimension_change_triggers_resize() {
        let _hooks = install_sigwinch_hooks(hook_take_sigwinch_true, hook_terminal_size_80x24);
        let (mut state, mut timers, mut deps, writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.terminal_cols = 80;
        state.terminal_rows = 1;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, Instant::now());
        let msg = writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("resize message");
        match msg {
            WriterMessage::Resize { rows, cols } => {
                assert_eq!(rows, 24);
                assert_eq!(cols, 80);
            }
            other => panic!("unexpected writer message: {other:?}"),
        }
        assert_eq!(state.terminal_cols, 80);
        assert_eq!(state.terminal_rows, 24);
    }

    #[test]
    fn run_periodic_tasks_updates_recording_duration() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Recording;
        timers.recording_started_at = Some(now - Duration::from_secs(2));
        timers.last_recording_update =
            now - Duration::from_millis(RECORDING_DURATION_UPDATE_MS + 5);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert!(state.status_state.recording_duration.is_some());
        assert!(state.last_recording_duration > 1.0);
        assert_eq!(timers.last_recording_update, now);
    }

    #[test]
    fn run_periodic_tasks_keeps_theme_digits_when_picker_deadline_not_reached() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.overlay_mode = OverlayMode::ThemePicker;
        state.theme_picker_digits = "12".to_string();
        timers.theme_picker_digit_deadline = Some(now + Duration::from_secs(1));

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(state.theme_picker_digits, "12");
        assert_eq!(
            timers.theme_picker_digit_deadline,
            Some(now + Duration::from_secs(1))
        );
    }

    #[test]
    fn run_periodic_tasks_skips_recording_update_when_delta_is_too_small() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Recording;
        timers.recording_started_at = Some(now - Duration::from_secs(2));
        timers.last_recording_update =
            now - Duration::from_millis(RECORDING_DURATION_UPDATE_MS + 5);
        state.last_recording_duration = 2.05;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert!(state.status_state.recording_duration.is_none());
        assert_eq!(state.last_recording_duration, 2.05);
        assert_eq!(timers.last_recording_update, now);
    }

    #[test]
    fn run_periodic_tasks_does_not_update_meter_when_not_recording() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Idle;
        timers.last_meter_update = now - Duration::from_secs(1);
        let prior_update = timers.last_meter_update;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert!(state.status_state.meter_db.is_none());
        assert_eq!(timers.last_meter_update, prior_update);
    }

    #[test]
    fn run_periodic_tasks_keeps_meter_history_at_cap_when_prefill_is_one_under() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Recording;
        state.meter_levels = VecDeque::from(vec![-30.0; METER_HISTORY_MAX - 1]);
        timers.last_meter_update = now - Duration::from_millis(500);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(state.meter_levels.len(), METER_HISTORY_MAX);
    }

    #[test]
    fn run_periodic_tasks_updates_meter_and_caps_history() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Recording;
        state.meter_levels = VecDeque::from(vec![-30.0; METER_HISTORY_MAX]);
        timers.last_meter_update = now - Duration::from_millis(500);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(state.meter_levels.len(), METER_HISTORY_MAX);
        assert!(state.status_state.meter_db.is_some());
        assert_eq!(timers.last_meter_update, now);
    }

    #[test]
    fn run_periodic_tasks_does_not_advance_spinner_when_not_processing() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Idle;
        timers.last_processing_tick = now - Duration::from_secs(1);
        let prior_tick = timers.last_processing_tick;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(state.processing_spinner_index, 0);
        assert_eq!(timers.last_processing_tick, prior_tick);
    }

    #[test]
    fn run_periodic_tasks_advances_processing_spinner() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.recording_state = RecordingState::Processing;
        timers.last_processing_tick = now - Duration::from_millis(PROCESSING_SPINNER_TICK_MS + 5);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert!(state.status_state.message.starts_with("Processing "));
        assert_eq!(state.processing_spinner_index, 1);
        assert_eq!(timers.last_processing_tick, now);
    }

    #[test]
    fn run_periodic_tasks_spinner_uses_modulo_for_frame_selection() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        let start_index = 5;
        let expected = progress::SPINNER_BRAILLE[start_index % progress::SPINNER_BRAILLE.len()];
        state.status_state.recording_state = RecordingState::Processing;
        state.processing_spinner_index = start_index;
        timers.last_processing_tick = now - Duration::from_secs(1);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(state.status_state.message, format!("Processing {expected}"));
    }

    #[test]
    fn run_periodic_tasks_heartbeat_respects_recording_only_gate() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.hud_right_panel = HudRightPanel::Heartbeat;
        state.status_state.hud_right_panel_recording_only = true;
        state.status_state.recording_state = RecordingState::Idle;
        timers.last_heartbeat_tick = now - Duration::from_secs(2);
        let prior_tick = timers.last_heartbeat_tick;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(timers.last_heartbeat_tick, prior_tick);
    }

    #[test]
    fn run_periodic_tasks_heartbeat_animates_when_recording_only_is_disabled() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.hud_right_panel = HudRightPanel::Heartbeat;
        state.status_state.hud_right_panel_recording_only = false;
        state.status_state.recording_state = RecordingState::Idle;
        timers.last_heartbeat_tick = now - Duration::from_secs(2);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(timers.last_heartbeat_tick, now);
    }

    #[test]
    fn run_periodic_tasks_heartbeat_requires_full_interval() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.hud_right_panel = HudRightPanel::Heartbeat;
        state.status_state.hud_right_panel_recording_only = false;
        state.status_state.recording_state = RecordingState::Recording;
        timers.last_heartbeat_tick = now - Duration::from_millis(500);
        let prior_tick = timers.last_heartbeat_tick;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(timers.last_heartbeat_tick, prior_tick);
    }

    #[test]
    fn run_periodic_tasks_heartbeat_only_runs_for_heartbeat_panel() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.hud_right_panel = HudRightPanel::Ribbon;
        state.status_state.hud_right_panel_recording_only = false;
        state.status_state.recording_state = RecordingState::Recording;
        timers.last_heartbeat_tick = now - Duration::from_secs(2);
        let prior_tick = timers.last_heartbeat_tick;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert_eq!(timers.last_heartbeat_tick, prior_tick);
    }

    #[test]
    fn run_periodic_tasks_clears_preview_and_status_at_deadline() {
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        let now = Instant::now();
        state.status_state.transcript_preview = Some("preview".to_string());
        state.current_status = Some("busy".to_string());
        state.status_state.message = "busy".to_string();
        timers.preview_clear_deadline = Some(now);
        timers.status_clear_deadline = Some(now);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, now);
        assert!(timers.preview_clear_deadline.is_none());
        assert!(state.status_state.transcript_preview.is_none());
        assert!(timers.status_clear_deadline.is_none());
        assert!(state.current_status.is_none());
        assert!(state.status_state.message.is_empty());
    }

    #[test]
    fn run_periodic_tasks_does_not_start_auto_voice_when_disabled() {
        let _capture = install_start_capture_hook(hook_start_capture_count);
        let _sigwinch = install_sigwinch_hooks(hook_take_sigwinch_false, hook_terminal_size_80x24);
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.auto_voice_enabled = false;
        deps.auto_idle_timeout = Duration::ZERO;

        run_periodic_tasks(&mut state, &mut timers, &mut deps, Instant::now());
        START_CAPTURE_CALLS.with(|calls| assert_eq!(calls.get(), 0));
        assert!(timers.last_auto_trigger_at.is_none());
    }

    #[test]
    fn run_periodic_tasks_does_not_start_auto_voice_when_trigger_not_ready() {
        let _capture = install_start_capture_hook(hook_start_capture_count);
        let _sigwinch = install_sigwinch_hooks(hook_take_sigwinch_false, hook_terminal_size_80x24);
        let (mut state, mut timers, mut deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.auto_voice_enabled = true;
        deps.auto_idle_timeout = Duration::from_secs(60);

        run_periodic_tasks(&mut state, &mut timers, &mut deps, Instant::now());
        START_CAPTURE_CALLS.with(|calls| assert_eq!(calls.get(), 0));
        assert!(timers.last_auto_trigger_at.is_none());
    }

    #[test]
    fn flush_pending_output_or_continue_handles_no_pending_output() {
        let (mut state, _timers, deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        assert!(flush_pending_output_or_continue(&mut state, &deps));
    }

    #[test]
    fn flush_pending_output_or_continue_keeps_running_when_output_requeues() {
        let (mut state, _timers, deps, _writer_rx, _input_tx) = build_harness("cat", &[], 1);
        deps.writer_tx
            .try_send(WriterMessage::ClearStatus)
            .expect("fill bounded writer channel");
        state.pending_pty_output = Some(b"abc".to_vec());

        assert!(flush_pending_output_or_continue(&mut state, &deps));
        assert_eq!(state.pending_pty_output, Some(b"abc".to_vec()));
    }

    #[test]
    fn flush_pending_output_or_continue_stops_when_writer_disconnected_and_output_drained() {
        let (mut state, _timers, deps, writer_rx, _input_tx) = build_harness("cat", &[], 8);
        drop(writer_rx);
        state.pending_pty_output = Some(b"abc".to_vec());

        assert!(!flush_pending_output_or_continue(&mut state, &deps));
        assert!(state.pending_pty_output.is_none());
    }

    #[test]
    fn flush_pending_output_or_continue_keeps_running_when_flush_succeeds() {
        let (mut state, _timers, deps, _writer_rx, _input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_output = Some(b"ok".to_vec());

        assert!(flush_pending_output_or_continue(&mut state, &deps));
        assert!(state.pending_pty_output.is_none());
    }

    #[test]
    fn run_event_loop_flushes_pending_input_before_exit() {
        let (mut state, mut timers, mut deps, _writer_rx, input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_input.push_back(b"hello".to_vec());
        state.pending_pty_input_bytes = 5;
        input_tx.send(InputEvent::Exit).expect("queue exit event");

        run_event_loop(&mut state, &mut timers, &mut deps);
        assert!(state.pending_pty_input.is_empty());
        assert_eq!(state.pending_pty_input_offset, 0);
        assert_eq!(state.pending_pty_input_bytes, 0);
    }

    #[test]
    fn run_event_loop_flushes_pending_output_even_when_writer_is_disconnected() {
        let (mut state, mut timers, mut deps, writer_rx, input_tx) = build_harness("cat", &[], 8);
        drop(writer_rx);
        state.pending_pty_output = Some(b"leftover".to_vec());
        input_tx.send(InputEvent::Exit).expect("queue exit event");

        run_event_loop(&mut state, &mut timers, &mut deps);
        assert!(
            state.pending_pty_output.is_none(),
            "pending output should be consumed even when writer is disconnected"
        );
    }

    #[test]
    fn run_event_loop_flushes_pending_output_on_success_path() {
        let (mut state, mut timers, mut deps, writer_rx, input_tx) = build_harness("cat", &[], 8);
        state.pending_pty_output = Some(b"ok".to_vec());
        input_tx.send(InputEvent::Exit).expect("queue exit event");

        run_event_loop(&mut state, &mut timers, &mut deps);
        assert!(state.pending_pty_output.is_none());
        match writer_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("writer output")
        {
            WriterMessage::PtyOutput(bytes) => assert_eq!(bytes, b"ok".to_vec()),
            other => panic!("unexpected writer message: {other:?}"),
        }
    }

    #[test]
    fn run_event_loop_processes_multiple_input_events_before_exit() {
        let (mut state, mut timers, mut deps, _writer_rx, input_tx) = build_harness("cat", &[], 8);
        let initial_auto_voice = state.auto_voice_enabled;
        input_tx
            .send(InputEvent::ToggleAutoVoice)
            .expect("queue first auto-voice toggle");
        input_tx
            .send(InputEvent::ToggleAutoVoice)
            .expect("queue second auto-voice toggle");
        input_tx.send(InputEvent::Exit).expect("queue exit event");

        run_event_loop(&mut state, &mut timers, &mut deps);
        assert!(
            state.auto_voice_enabled == initial_auto_voice,
            "both toggles should run before exit so auto-voice returns to its initial value"
        );
        assert!(
            state.status_state.auto_voice_enabled == initial_auto_voice,
            "status and runtime auto-voice state should stay aligned"
        );
    }

    #[test]
    fn run_event_loop_does_not_run_periodic_before_first_tick() {
        let (mut state, mut timers, mut deps, _writer_rx, input_tx) = build_harness("cat", &[], 8);
        state.overlay_mode = OverlayMode::None;
        state.theme_picker_digits = "12".to_string();
        input_tx.send(InputEvent::Exit).expect("queue exit event");

        run_event_loop(&mut state, &mut timers, &mut deps);
        assert_eq!(state.theme_picker_digits, "12");
    }
}
