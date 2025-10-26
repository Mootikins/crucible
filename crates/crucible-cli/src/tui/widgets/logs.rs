// Logs widget rendering
//
// Renders the scrollable log window with color-coded log levels.

use crate::tui::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

/// Render the logs window
pub fn render_logs(app: &App, frame: &mut Frame, area: Rect) {
    // Calculate visible range (accounting for scroll offset)
    let visible_height = area.height.saturating_sub(2) as usize; // Account for borders
    let total_logs = app.logs.len();
    let scroll_offset = app.log_scroll.offset.min(total_logs);

    // Get log entries to display
    let log_items: Vec<ListItem> = app
        .logs
        .entries()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|entry| {
            // Color-code by level
            let level_style = match entry.level {
                tracing::Level::ERROR => Style::default().fg(Color::Red),
                tracing::Level::WARN => Style::default().fg(Color::Yellow),
                tracing::Level::INFO => Style::default().fg(Color::Green),
                tracing::Level::DEBUG => Style::default().fg(Color::Cyan),
                tracing::Level::TRACE => Style::default().fg(Color::Gray),
            };

            // Format: "12:34:56 INFO  Indexed file.md (23ms)"
            let timestamp = entry.timestamp.format("%H:%M:%S");
            let spans = vec![
                Span::styled(
                    format!("{} ", timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:<5} ", entry.level), level_style),
                Span::raw(&entry.message),
            ];

            ListItem::new(Line::from(spans))
        })
        .collect();

    // Build title with scroll indicator
    let title = if scroll_offset > 0 {
        format!(
            " Logs ({}/{} buffered, ↑{}) ",
            log_items.len(),
            total_logs,
            scroll_offset
        )
    } else {
        format!(" Logs ({} buffered) ", total_logs)
    };

    let logs_list = List::new(log_items).block(Block::default().borders(Borders::TOP).title(title));

    frame.render_widget(logs_list, area);
}
