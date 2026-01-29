//! Subagent rendering component.
//!
//! Renders subagent executions with status (running/completed/failed),
//! prompt preview, and result summary or error message.

use crate::tui::oil::node::{col, row, scrollback, styled, text, Node, BRAILLE_SPINNER_FRAMES};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::utils::truncate_first_line;
use crate::tui::oil::viewport_cache::{CachedSubagent, SubagentStatus};
use std::time::Duration;

use super::tool_render::format_elapsed;

/// Render a subagent with status indicator and prompt preview.
pub fn render_subagent(subagent: &CachedSubagent, spinner_frame: usize) -> Node {
    let (icon, icon_style) = match subagent.status {
        SubagentStatus::Running => {
            let frame = BRAILLE_SPINNER_FRAMES[spinner_frame % BRAILLE_SPINNER_FRAMES.len()];
            (format!(" {} ", frame), Style::new().fg(colors::TEXT_ACCENT))
        }
        SubagentStatus::Completed => (" ✓ ".to_string(), Style::new().fg(colors::SUCCESS)),
        SubagentStatus::Failed => (" ✗ ".to_string(), Style::new().fg(colors::ERROR)),
    };

    let prompt_preview = truncate_first_line(&subagent.prompt, 60, true);

    let status_text = match subagent.status {
        SubagentStatus::Running => {
            let elapsed = subagent.elapsed();
            format_elapsed_display(elapsed)
        }
        SubagentStatus::Completed => subagent
            .summary
            .as_ref()
            .map(|s| format!(" → {}", truncate_first_line(s, 50, true)))
            .unwrap_or_default(),
        SubagentStatus::Failed => subagent
            .error
            .as_ref()
            .map(|e| format!(" → {}", truncate_first_line(e, 50, true)))
            .unwrap_or_default(),
    };

    let status_style = match subagent.status {
        SubagentStatus::Running => styles::dim(),
        SubagentStatus::Completed => styles::muted(),
        SubagentStatus::Failed => styles::error(),
    };

    let header = row([
        styled(icon, icon_style),
        styled("subagent", Style::new().fg(colors::TEXT_PRIMARY)),
        styled(format!(" {}", prompt_preview), styles::muted()),
        styled(status_text, status_style),
    ]);

    scrollback(subagent.id.to_string(), [col([text(""), header])])
}

fn format_elapsed_display(elapsed: Duration) -> String {
    format!("  {}", format_elapsed(elapsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_plain_text;
    use std::sync::Arc;

    #[test]
    fn render_subagent_running() {
        let mut subagent = CachedSubagent::new("sub-1", "Analyze the code");
        subagent.status = SubagentStatus::Running;
        let node = render_subagent(&subagent, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("subagent"));
        assert!(plain.contains("Analyze the code"));
    }

    #[test]
    fn render_subagent_completed() {
        let mut subagent = CachedSubagent::new("sub-1", "Analyze the code");
        subagent.status = SubagentStatus::Completed;
        subagent.summary = Some(Arc::from("Analysis complete"));
        let node = render_subagent(&subagent, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✓"));
        assert!(plain.contains("Analysis complete"));
    }

    #[test]
    fn render_subagent_failed() {
        let mut subagent = CachedSubagent::new("sub-1", "Analyze the code");
        subagent.status = SubagentStatus::Failed;
        subagent.error = Some(Arc::from("Connection timeout"));
        let node = render_subagent(&subagent, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✗"));
        assert!(plain.contains("Connection timeout"));
    }

    #[test]
    fn render_subagent_truncates_long_prompt() {
        let long_prompt = "a".repeat(100);
        let subagent = CachedSubagent::new("sub-1", &long_prompt);
        let node = render_subagent(&subagent, 0);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("…"));
    }
}
