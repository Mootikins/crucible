// REPL widget rendering
//
// Renders the REPL input area and result display.

use crate::tui::{app::App, events::ReplResult};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Row, Table, Wrap},
    Frame,
};

/// Render the REPL area
///
/// Splits into two sections:
/// - Result display (70%)
/// - Input prompt (30%)
pub fn render_repl(app: &App, frame: &mut Frame, area: Rect) {
    // Split REPL area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70), // Result area
            Constraint::Percentage(30), // Input area
        ])
        .split(area);

    // Render last result if any
    if let Some(result) = &app.last_repl_result {
        render_result(result, frame, chunks[0]);
    } else {
        // Show welcome message
        render_welcome(frame, chunks[0]);
    }

    // Render input area
    render_input(&app.repl, frame, chunks[1]);

    // Position cursor
    if app.repl.is_idle() {
        position_cursor(&app.repl, chunks[1], frame);
    }
}

/// Render REPL result
fn render_result(result: &ReplResult, frame: &mut Frame, area: Rect) {
    match result {
        ReplResult::Success { output, duration } => {
            let text = format!("{}\n\n({}ms)", output, duration.as_millis());
            let widget = Paragraph::new(text)
                .block(Block::default().borders(Borders::TOP).title(" Result "))
                .wrap(Wrap { trim: false });

            frame.render_widget(widget, area);
        }

        ReplResult::Error { message } => {
            let widget = Paragraph::new(message.clone())
                .style(Style::default().fg(Color::Red))
                .block(Block::default().borders(Borders::TOP).title(" Error "));

            frame.render_widget(widget, area);
        }

        ReplResult::Table { headers, rows } => {
            render_table(headers, rows, frame, area);
        }
    }
}

/// Render welcome message when no result
fn render_welcome(frame: &mut Frame, area: Rect) {
    let text = "Welcome to Crucible REPL!\n\nType :help for commands or enter SurrealQL queries.";
    let widget = Paragraph::new(text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP).title(" Result "));

    frame.render_widget(widget, area);
}

/// Render tabular result
fn render_table(headers: &[String], rows: &[Vec<String>], frame: &mut Frame, area: Rect) {
    // Create header row
    let header_cells = headers
        .iter()
        .map(|h| {
            Span::styled(
                h.clone(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect::<Vec<_>>();

    // Create data rows
    let data_rows: Vec<Row> = rows
        .iter()
        .map(|row| Row::new(row.iter().map(|cell| cell.as_str()).collect::<Vec<_>>()))
        .collect();

    // Calculate column widths (simple equal distribution)
    let col_count = headers.len() as u16;
    let col_width = if col_count > 0 {
        area.width.saturating_sub(2) / col_count
    } else {
        10
    };
    let widths = vec![Constraint::Length(col_width); headers.len()];

    let table = Table::new(data_rows, widths)
        .header(Row::new(header_cells).style(Style::default().fg(Color::Yellow)))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" Result ({} rows) ", rows.len())),
        );

    frame.render_widget(table, area);
}

/// Render REPL input area
fn render_input(repl: &crate::tui::repl_state::ReplState, frame: &mut Frame, area: Rect) {
    let prompt = "> ";
    let input_text = format!("{}{}", prompt, repl.input());

    // Build title based on execution state
    let title = if repl.is_executing() {
        " Executing... "
    } else {
        " REPL (SurrealQL | :help) "
    };

    // Style differently if executing
    let style = if repl.is_executing() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let widget = Paragraph::new(input_text)
        .style(style)
        .block(Block::default().borders(Borders::TOP).title(title));

    frame.render_widget(widget, area);
}

/// Position cursor in input area
fn position_cursor(repl: &crate::tui::repl_state::ReplState, area: Rect, frame: &mut Frame) {
    let prompt_len = 2; // "> "
    let cursor_x = area.x + prompt_len as u16 + repl.cursor() as u16;
    let cursor_y = area.y + 1; // Account for block border

    // Ensure cursor is within bounds
    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        frame.set_cursor_position((cursor_x, cursor_y));
    }
}
