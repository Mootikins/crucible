//! Rendering for TUI

//!
//! Draws the TUI widget to the terminal using ratatui.
//!
//! This module renders only the bottom widget (streaming area, input, status).
//! Completed messages are printed to terminal scrollback, not rendered here.

use crate::tui::state::TuiState;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

/// Render the TUI widget to the terminal
///
/// This renders only the bottom widget area:
/// - Streaming response area (shows content being received)
/// - Input prompt
/// - Status line
///
/// Completed messages go to terminal scrollback, not rendered here.
/// Note: Popup rendering is handled by RatatuiView, not this function.
pub fn render(frame: &mut Frame, state: &TuiState) {
    let constraints = vec![
        Constraint::Min(3),    // Streaming area
        Constraint::Length(3), // Input area
        Constraint::Length(1), // Status bar
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(frame.area());

    render_streaming(frame, chunks[0], state);
    render_input(frame, chunks[1], state);
    render_status(frame, chunks[2], state);
}

/// Render the streaming response area
///
/// Shows content currently being streamed from the agent.
/// Empty when not streaming.
fn render_streaming(frame: &mut Frame, area: Rect, state: &TuiState) {
    let lines: Vec<Line> = if let Some(ref streaming) = state.streaming {
        // Show streaming content with cursor
        vec![Line::from(vec![
            Span::styled(
                "Assistant: ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(streaming.content()),
            Span::styled(" \u{258B}", Style::default().fg(Color::Green)), // Block cursor
        ])]
    } else {
        // Empty when not streaming
        Vec::new()
    };

    let streaming_widget = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Streaming"))
        .wrap(Wrap { trim: false });

    frame.render_widget(streaming_widget, area);
}

fn render_input(frame: &mut Frame, area: Rect, state: &TuiState) {
    let mode_str = match state.mode_id() {
        "plan" => "[Plan]",
        "act" => "[Act]",
        "auto" => "[Auto]",
        _ => "[Unknown]",
    };

    let mode_style = match state.mode_id() {
        "plan" => Style::default().fg(Color::Cyan),
        "act" => Style::default().fg(Color::Yellow),
        "auto" => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::Gray),
    };

    let input_line = Line::from(vec![
        Span::styled(mode_str, mode_style),
        Span::raw(" > "),
        Span::raw(state.input()),
    ]);

    let input =
        Paragraph::new(input_line).block(Block::default().borders(Borders::ALL).title("Input"));

    frame.render_widget(input, area);

    // Position cursor
    let cursor_x = area.x + mode_str.len() as u16 + 4 + state.cursor() as u16;
    let cursor_y = area.y + 1;
    frame.set_cursor_position((cursor_x, cursor_y));
}

fn render_status(frame: &mut Frame, area: Rect, state: &TuiState) {
    let mode_str = match state.mode_id() {
        "plan" => "Plan",
        "act" => "Act",
        "auto" => "Auto",
        _ => &state.mode_name, // mode_name is still a field on TuiState
    };

    // Check for status error first - display prominently in red
    if let Some(ref error) = state.status_error {
        let error_text = format!("Mode: {} \u{2502} \u{26A0} Error: {}", mode_str, error);
        let status = Paragraph::new(error_text).style(Style::default().fg(Color::Red));
        frame.render_widget(status, area);
        return;
    }

    let pending_count = state.pending_tools.iter().filter(|t| !t.completed).count();

    let status_text = if pending_count > 0 {
        format!(
            "Mode: {} \u{2502} \u{23F3} {} tools pending",
            mode_str, pending_count
        )
    } else {
        format!("Mode: {} \u{2502} Ready", mode_str)
    };

    let status = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));

    frame.render_widget(status, area);
}
