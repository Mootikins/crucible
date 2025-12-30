//! TUI rendering for the Zellij plugin
//!
//! This module provides testable rendering functions for the inbox plugin UI.

use crate::{Inbox, Status};

/// Box-drawing characters
pub mod chars {
    pub const TOP_LEFT: char = '┌';
    pub const TOP_RIGHT: char = '┐';
    pub const BOTTOM_LEFT: char = '└';
    pub const BOTTOM_RIGHT: char = '┘';
    pub const HORIZONTAL: char = '─';
    pub const VERTICAL: char = '│';
    pub const SELECTED: char = '▶';
    pub const ELLIPSIS: char = '…';
}

/// Render options for the TUI
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub width: usize,
    pub height: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 50,
            height: 20,
        }
    }
}

/// Render the inbox TUI to a string buffer (simple API)
pub fn render_tui(inbox: &Inbox, selected: usize, width: usize) -> String {
    render_tui_full(inbox, selected, RenderOptions { width, height: 100 })
}

/// Render the inbox TUI with full options
pub fn render_tui_full(inbox: &Inbox, selected: usize, opts: RenderOptions) -> String {
    let mut output = String::new();
    let width = opts.width.max(20); // Minimum width, no max
    let height = opts.height.max(5); // Minimum height for header + 1 item + footer

    // Reserve lines: 1 top border + 1 empty + 1 help + 1 bottom border = 4
    let content_height = height.saturating_sub(4);

    // Title
    let title = " Agent Inbox ";
    render_top_border(&mut output, title, width);

    if inbox.is_empty() {
        output.push(chars::VERTICAL);
        output.push_str("  (no items)\n");
    } else {
        let overflow = render_items_with_height(&mut output, inbox, selected, width, content_height);
        if overflow {
            // Show overflow indicator
            output.push(chars::VERTICAL);
            output.push_str(&format!(" {} more below\n", chars::ELLIPSIS));
        }
    }

    // Footer
    output.push(chars::VERTICAL);
    output.push('\n');
    output.push(chars::VERTICAL);
    output.push_str(" j/k:nav  Enter:focus  esc:close\n");
    render_bottom_border(&mut output, width);

    output
}

fn render_top_border(output: &mut String, title: &str, width: usize) {
    let title_len = title.len();
    let padding = width.saturating_sub(title_len + 2);
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    output.push(chars::TOP_LEFT);
    for _ in 0..left_pad {
        output.push(chars::HORIZONTAL);
    }
    output.push_str(title);
    for _ in 0..right_pad {
        output.push(chars::HORIZONTAL);
    }
    output.push(chars::TOP_RIGHT);
    output.push('\n');
}

fn render_bottom_border(output: &mut String, width: usize) {
    output.push(chars::BOTTOM_LEFT);
    for _ in 0..width.saturating_sub(2) {
        output.push(chars::HORIZONTAL);
    }
    output.push(chars::BOTTOM_RIGHT);
    output.push('\n');
}

/// Render items with height constraint. Returns true if content was truncated.
fn render_items_with_height(
    output: &mut String,
    inbox: &Inbox,
    selected: usize,
    width: usize,
    max_lines: usize,
) -> bool {
    let mut lines_used = 0;
    let mut current_status: Option<Status> = None;
    let mut current_project: Option<&str> = None;
    let mut truncated = false;

    for (idx, item) in inbox.items.iter().enumerate() {
        // Check if we need section header
        let need_section = current_status != Some(item.status);
        let need_project = current_project != Some(&item.project);

        // Calculate lines needed for this item
        let lines_needed = 1 + if need_section { 1 } else { 0 } + if need_project { 1 } else { 0 };

        // Check if we have room (need at least 1 line for overflow indicator)
        if lines_used + lines_needed > max_lines.saturating_sub(1) && idx < inbox.items.len() - 1 {
            truncated = true;
            break;
        }

        // Section header (status change)
        if need_section {
            current_status = Some(item.status);
            current_project = None;
            output.push(chars::VERTICAL);
            output.push(' ');
            output.push_str(item.status.section_name());
            output.push('\n');
            lines_used += 1;
        }

        // Project header
        if need_project {
            current_project = Some(&item.project);
            output.push(chars::VERTICAL);
            output.push_str("   ");
            // Truncate project name if needed
            let max_proj_len = width.saturating_sub(5);
            let proj: String = item.project.chars().take(max_proj_len).collect();
            output.push_str(&proj);
            output.push('\n');
            lines_used += 1;
        }

        // Item line
        output.push(chars::VERTICAL);
        output.push(' ');
        if idx == selected {
            output.push(chars::SELECTED);
        } else {
            output.push(' ');
        }
        output.push(' ');

        // Truncate text to fit width
        let max_text_len = width.saturating_sub(6);
        let text: String = item.text.chars().take(max_text_len).collect();
        output.push_str(&text);
        output.push('\n');
        lines_used += 1;
    }

    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InboxItem;

    fn sample_inbox() -> Inbox {
        Inbox {
            items: vec![
                InboxItem {
                    text: "claude-code: Auth question".to_string(),
                    pane_id: 42,
                    project: "crucible".to_string(),
                    status: Status::Waiting,
                },
                InboxItem {
                    text: "claude-code: Review PR".to_string(),
                    pane_id: 17,
                    project: "k3s".to_string(),
                    status: Status::Waiting,
                },
                InboxItem {
                    text: "indexer: Processing files".to_string(),
                    pane_id: 5,
                    project: "crucible".to_string(),
                    status: Status::Working,
                },
            ],
        }
    }

    #[test]
    fn render_empty_inbox() {
        let inbox = Inbox::new();
        let output = render_tui(&inbox, 0, 40);

        assert!(output.contains("Agent Inbox"));
        assert!(output.contains("(no items)"));
        assert!(output.contains("j/k:nav"));
    }

    #[test]
    fn render_with_items_shows_sections() {
        let inbox = sample_inbox();
        let output = render_tui(&inbox, 0, 50);

        // Check sections
        assert!(output.contains("Waiting for Input"));
        assert!(output.contains("Background"));

        // Check projects
        assert!(output.contains("crucible"));
        assert!(output.contains("k3s"));

        // Check items
        assert!(output.contains("Auth question"));
        assert!(output.contains("Review PR"));
        assert!(output.contains("Processing files"));
    }

    #[test]
    fn render_selection_marker() {
        let inbox = sample_inbox();

        // First item selected
        let output = render_tui(&inbox, 0, 50);
        let lines: Vec<&str> = output.lines().collect();
        let auth_line = lines.iter().find(|l| l.contains("Auth question")).unwrap();
        assert!(auth_line.contains('▶'), "First item should have selection marker");

        // Second item selected
        let output = render_tui(&inbox, 1, 50);
        let lines: Vec<&str> = output.lines().collect();
        let review_line = lines.iter().find(|l| l.contains("Review PR")).unwrap();
        assert!(review_line.contains('▶'), "Second item should have selection marker");
    }

    #[test]
    fn render_respects_width() {
        let inbox = Inbox {
            items: vec![InboxItem {
                text: "This is a very long text that should be truncated when width is small".to_string(),
                pane_id: 1,
                project: "test".to_string(),
                status: Status::Waiting,
            }],
        };

        let narrow = render_tui(&inbox, 0, 30);
        let wide = render_tui(&inbox, 0, 80);

        // Narrow should truncate
        assert!(!narrow.contains("truncated when width is small"));
        // Wide should show more
        assert!(wide.contains("truncated"));
    }

    #[test]
    fn render_box_drawing() {
        let inbox = Inbox::new();
        let output = render_tui(&inbox, 0, 40);

        // Check corners
        assert!(output.contains('┌'));
        assert!(output.contains('┐'));
        assert!(output.contains('└'));
        assert!(output.contains('┘'));

        // Check borders
        assert!(output.contains('─'));
        assert!(output.contains('│'));
    }

    #[test]
    fn snapshot_empty() {
        let inbox = Inbox::new();
        let output = render_tui(&inbox, 0, 40);

        let expected = "\
┌──────────── Agent Inbox ─────────────┐
│  (no items)
│
│ j/k:nav  Enter:focus  esc:close
└──────────────────────────────────────┘
";
        assert_eq!(output, expected);
    }

    #[test]
    fn snapshot_with_items() {
        let inbox = Inbox {
            items: vec![
                InboxItem {
                    text: "claude: Question".to_string(),
                    pane_id: 42,
                    project: "myproject".to_string(),
                    status: Status::Waiting,
                },
            ],
        };
        let output = render_tui(&inbox, 0, 40);

        let expected = "\
┌──────────── Agent Inbox ─────────────┐
│ Waiting for Input
│   myproject
│ ▶ claude: Question
│
│ j/k:nav  Enter:focus  esc:close
└──────────────────────────────────────┘
";
        assert_eq!(output, expected);
    }

    #[test]
    fn render_with_height_truncation() {
        let inbox = Inbox {
            items: vec![
                InboxItem {
                    text: "item1".to_string(),
                    pane_id: 1,
                    project: "proj".to_string(),
                    status: Status::Waiting,
                },
                InboxItem {
                    text: "item2".to_string(),
                    pane_id: 2,
                    project: "proj".to_string(),
                    status: Status::Waiting,
                },
                InboxItem {
                    text: "item3".to_string(),
                    pane_id: 3,
                    project: "proj".to_string(),
                    status: Status::Waiting,
                },
            ],
        };

        // Height 8: header(1) + section(1) + project(1) + items need 3 = 6, footer needs 3
        // With height 8, we should truncate
        let opts = RenderOptions { width: 40, height: 8 };
        let output = render_tui_full(&inbox, 0, opts);

        // Should show truncation indicator
        assert!(output.contains('…'), "Should show truncation indicator");
        assert!(output.contains("item1"));
        // May or may not contain item3 depending on space
    }

    #[test]
    fn render_full_width() {
        let inbox = Inbox {
            items: vec![InboxItem {
                text: "A very long message that should not be truncated at 80 chars anymore".to_string(),
                pane_id: 1,
                project: "test".to_string(),
                status: Status::Waiting,
            }],
        };

        let output = render_tui(&inbox, 0, 100);
        assert!(output.contains("truncated at 80"));
    }
}
