// Header widget rendering
//
// Renders the status bar showing vault info and statistics.

use crate::tui::app::App;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Paragraph,
    Frame,
};

/// Render the header status bar
///
/// Format: "Crucible v0.1.0 | /path/to/vault | SurrealDB | 43 docs | 2.3MB"
pub fn render_header(app: &App, frame: &mut Frame, area: Rect) {
    let status = &app.status;

    // Format vault path (shorten if too long)
    let vault_path = status
        .vault_path
        .to_string_lossy()
        .chars()
        .take(40)
        .collect::<String>();

    // Format database size
    let db_size = format_bytes(status.db_size);

    // Build status text
    let text = format!(
        "Crucible v{} | {} | {} | {} docs | {}",
        env!("CARGO_PKG_VERSION"),
        vault_path,
        status.db_type,
        status.doc_count,
        db_size,
    );

    let header = Paragraph::new(text).style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(header, area);
}

/// Format bytes as human-readable size
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }
}
