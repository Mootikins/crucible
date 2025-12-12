//! Viewport rendering for chat TUI
//!
//! Handles layout and rendering of the inline viewport.

use super::app::ChatApp;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

/// Render the complete chat viewport
pub fn render_chat_viewport(app: &mut ChatApp, frame: &mut Frame) {
    let area = frame.area();
    let input_height = app.input.height(area.width);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(input_height), // Input area
            Constraint::Length(1),            // Separator
            Constraint::Length(1),            // Status bar
            Constraint::Min(0),               // Remaining space (for popup overlay)
        ])
        .split(area);

    // Render input
    app.input.render(frame, chunks[0]);

    // Render separator
    render_separator(frame, chunks[1]);

    // Render status bar
    render_status_bar(app, frame, chunks[2]);

    // Render completion popup if active (overlays other content)
    if let Some(ref completion) = app.completion {
        super::widgets::popup::render_completion_popup(frame, area, completion);
    }
}

/// Render the separator line
fn render_separator(frame: &mut Frame, area: Rect) {
    let sep =
        Paragraph::new("─".repeat(area.width as usize)).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, area);
}

/// Render the status bar
fn render_status_bar(app: &ChatApp, frame: &mut Frame, area: Rect) {
    let mode_style = match app.mode {
        super::app::ChatMode::Plan => Style::default().fg(Color::Cyan),
        super::app::ChatMode::Act => Style::default().fg(Color::Green),
        super::app::ChatMode::Auto => Style::default().fg(Color::Yellow),
    };

    let (status_icon, status_text) = if app.is_streaming {
        ("⟳", "Streaming...")
    } else {
        ("●", "Ready")
    };

    let spans = vec![
        Span::styled(
            format!("[{}]", app.mode.name()),
            mode_style.add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(status_icon, mode_style),
        Span::raw(" "),
        Span::raw(status_text),
        Span::raw(" | "),
        Span::styled("/help", Style::default().fg(Color::DarkGray)),
    ];

    let status = Paragraph::new(Line::from(spans));
    frame.render_widget(status, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_render_does_not_panic() {
        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = ChatApp::new();

        terminal
            .draw(|frame| {
                render_chat_viewport(&mut app, frame);
            })
            .unwrap();
    }

    #[test]
    fn test_render_with_streaming() {
        let backend = TestBackend::new(80, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = ChatApp::new();
        app.set_streaming(true);

        terminal
            .draw(|frame| {
                render_chat_viewport(&mut app, frame);
            })
            .unwrap();
    }
}
