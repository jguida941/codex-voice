//! Minimal `ratatui` front-end that mirrors the logic from `main.rs` so integration
//! tests can drive the UI without pulling in all of `main`.

use crate::log_debug;
use crate::utf8_safe::window_by_columns;
use crate::voice::VoiceCaptureTrigger;
use crate::App;
use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::{
    event, execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
    Terminal,
};
use std::io;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Configure the terminal, run the drawing loop, and tear everything down.
pub fn run_app(app: &mut App) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = app_loop(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    result
}

/// Core event/render loop for the test-friendly UI entrypoint.
fn app_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    // Initial render to show UI immediately on startup
    terminal.draw(|frame| crate::ui::draw(frame, app))?;

    loop {
        app.poll_codex_job()?;
        app.poll_voice_job()?;
        app.drain_persistent_output();

        let has_active_job = app.has_active_jobs();
        if has_active_job {
            app.update_codex_spinner();
        }

        let poll_duration = if has_active_job {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(100)
        };

        // Always draw when there's an active job to show spinner animation
        let mut should_draw = app.take_redraw_request() || has_active_job;
        let mut should_quit = false;

        if event::poll(poll_duration)? {
            match event::read()? {
                Event::Key(key) => {
                    // Handle key BEFORE drawing to avoid input lag
                    should_quit = handle_key_event(app, key)?;
                    should_draw = true;
                }
                Event::Resize(_, _) => {
                    // Terminal resize requires immediate redraw
                    should_draw = true;
                }
                _ => {} // Ignore other events
            }
        }

        if should_draw {
            terminal.draw(|frame| crate::ui::draw(frame, app))?;
        }

        if should_quit {
            break;
        }
    }
    Ok(())
}

/// Interpret keystrokes into modifications to the shared `App` state.
fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    log_debug(&format!(
        "Key event: {:?} with modifiers: {:?}",
        key.code, key.modifiers
    ));

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        if app.cancel_codex_job_if_active() {
            return Ok(false);
        }
        return Ok(true);
    }

    match key.code {
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            log_debug("Ctrl+R pressed, starting voice capture");
            app.start_voice_capture(VoiceCaptureTrigger::Manual)?;
        }
        KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.toggle_voice_mode()?;
        }
        KeyCode::Enter => {
            app.send_current_input()?;
        }
        KeyCode::Backspace => app.backspace_input(),
        KeyCode::Esc => {
            if !app.cancel_codex_job_if_active() {
                app.clear_input();
            }
        }
        KeyCode::Char(c) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                app.push_input_char(c);
            }
        }
        KeyCode::Delete => app.clear_input(),
        KeyCode::Up => app.scroll_up(),
        KeyCode::Down => app.scroll_down(),
        KeyCode::PageUp => app.page_up(),
        KeyCode::PageDown => app.page_down(),
        KeyCode::Home => app.scroll_to_top(),
        KeyCode::End => app.scroll_to_bottom(),
        _ => {}
    }

    Ok(false)
}

/// Render scrollback, prompt, and status bars.
pub fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    // Split the screen into output, input, and status regions.
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(frame.size());

    let output_text = if app.output_lines().is_empty() {
        Text::from("No Codex output yet. Press Ctrl+R to capture voice or type and press Enter.")
    } else {
        let lines: Vec<Line> = app
            .output_lines()
            .iter()
            .map(|s| {
                if s.trim().is_empty() && !s.is_empty() {
                    return Line::from("");
                }

                let owned_line = s.to_string();
                let cleaned = owned_line
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                    .collect::<String>();
                let mut final_text = cleaned
                    .trim_matches(|c: char| {
                        c.is_control() || (c as u32) < 32 || c == '\u{200B}' || c == '\u{FEFF}'
                    })
                    .to_string();
                if final_text.is_empty() {
                    final_text = " ".to_string();
                }

                let safe_text = final_text
                    .chars()
                    .map(|c| {
                        if c.width().unwrap_or(0) == 0 && c != '\n' && c != '\t' {
                            ' '
                        } else {
                            c
                        }
                    })
                    .collect::<String>();
                let safe_text = safe_text.replace('`', "'");

                const MAX_LINE_WIDTH: usize = 500;
                let trimmed = window_by_columns(&safe_text, 0, MAX_LINE_WIDTH);
                let mut final_safe_text = trimmed.to_string();
                if UnicodeWidthStr::width(safe_text.as_str()) > UnicodeWidthStr::width(trimmed) {
                    final_safe_text.push('…');
                }
                Line::from(final_safe_text)
            })
            .collect();
        Text::from(lines)
    };

    // Theme colors - Vibrant red
    let border_color = Color::Rgb(255, 90, 90); // Vibrant red accent
    let title_color = Color::Rgb(255, 110, 110); // Bright red for titles
    let dim_border = Color::Rgb(130, 70, 70); // Dimmer border for less important areas
    let output_text_color = Color::Rgb(210, 205, 200); // Soft white for output
    let input_text_color = Color::Rgb(255, 220, 100); // Warm yellow for input
    let status_text_color = Color::Rgb(160, 150, 150); // Dimmer for status

    // CRITICAL: Disable text wrapping entirely to avoid ratatui's underflow bug
    // The bug in tui/src/wrapping.rs:21 causes integer underflow when calculating
    // slice positions, leading to "byte index 18446... out of bounds" panics.
    // Until ratatui fixes this, we must avoid text wrapping completely.
    let output_block = Paragraph::new(output_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " Codex Output ",
                    Style::default()
                        .fg(title_color)
                        .add_modifier(Modifier::BOLD),
                )),
        )
        .style(Style::default().fg(output_text_color))
        .scroll((app.get_scroll_offset(), 0));
    frame.render_widget(output_block, chunks[0]);

    // CRITICAL FIX: Sanitize the input text before rendering to prevent crashes
    // from terminal control sequences like "0;0;0u" that can appear in the input buffer
    let sanitized_input = app.sanitized_input_text();
    let input_block = Paragraph::new(sanitized_input.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(Span::styled(
                    " Prompt ",
                    Style::default()
                        .fg(title_color)
                        .add_modifier(Modifier::BOLD),
                ))
                .title_bottom(Line::from(vec![
                    Span::styled(
                        " Ctrl+R ",
                        Style::default()
                            .fg(input_text_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("voice  ", Style::default().fg(dim_border)),
                    Span::styled(
                        "Ctrl+V ",
                        Style::default()
                            .fg(input_text_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("toggle ", Style::default().fg(dim_border)),
                ])),
        )
        .style(Style::default().fg(input_text_color));
    frame.render_widget(input_block, chunks[1]);

    let status_block = Paragraph::new(app.status_text())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(dim_border))
                .title(Span::styled(
                    " Status ",
                    Style::default().fg(status_text_color),
                )),
        )
        .style(Style::default().fg(status_text_color));
    frame.render_widget(status_block, chunks[2]);

    let inner_width = chunks[1].width.saturating_sub(2);
    let input_width =
        UnicodeWidthStr::width(sanitized_input.as_str()).min(u16::MAX as usize) as u16;
    let cursor_offset = input_width.min(inner_width);
    let cursor_x = chunks[1].x.saturating_add(1).saturating_add(cursor_offset);
    let cursor_y = chunks[1].y + 1;
    frame.set_cursor(cursor_x, cursor_y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use clap::Parser;

    fn test_app() -> App {
        let mut config = AppConfig::parse_from(["test-app"]);
        config.persistent_codex = false;
        App::new(config)
    }

    #[test]
    fn handle_key_event_appends_and_backspaces() {
        let mut app = test_app();
        handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
        )
        .expect("key event");
        assert_eq!(app.sanitized_input_text(), "a");

        handle_key_event(
            &mut app,
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        )
        .expect("key event");
        assert_eq!(app.sanitized_input_text(), "");
    }
}
