// Widget rendering for the TUI
//
// This module contains the rendering logic for each UI section.
// Widgets are stateless - they receive immutable references to App state
// and render to a ratatui Frame.

mod header;
mod logs;
mod repl;

use crate::tui::app::App;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};

pub use header::render_header;
pub use logs::render_logs;
pub use repl::render_repl;

/// Main render function
///
/// Splits the terminal into three sections and delegates to specialized renderers:
/// - Header (1 line, fixed)
/// - Logs (70% of remaining space)
/// - REPL (30% of remaining space)
///
/// Only renders sections that are marked dirty to optimize performance.
pub fn render(app: &mut App, frame: &mut Frame) {
    // Main layout: Header | Logs | REPL
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                       // Header
            Constraint::Percentage(app.config.log_split_ratio), // Logs
            Constraint::Min(5),                          // REPL (at least 5 lines)
        ])
        .split(frame.area());

    // Render sections
    render_header(app, frame, chunks[0]);
    render_logs(app, frame, chunks[1]);
    render_repl(app, frame, chunks[2]);
}
