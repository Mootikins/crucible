//! TUI rendering for the Zellij plugin
//!
//! This module provides testable rendering functions for the inbox plugin UI.

use std::str::FromStr;

use crate::{Inbox, Status};

/// Unicode characters for TUI elements
pub mod chars {
    pub const SELECTED: char = '▶';
    pub const ELLIPSIS: char = '…';

    // Checkbox styles
    pub const CHECKBOX_EMPTY: &str = "[ ]";
    pub const CHECKBOX_FILLED: &str = "[✓]";
    pub const CIRCLE_EMPTY: &str = "○";
    pub const CIRCLE_FILLED: &str = "●";
    pub const BULLET: &str = "•";
}

/// ANSI escape codes for styling
pub mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const ITALIC: &str = "\x1b[3m";

    // Colors (using standard 16-color palette for compatibility)
    pub const CYAN: &str = "\x1b[36m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const GREEN: &str = "\x1b[32m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const WHITE: &str = "\x1b[37m";
}

/// Style for checkbox/bullet indicators
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CheckboxStyle {
    /// [ ] and [✓] style
    #[default]
    Brackets,
    /// ○ and ● style
    Circles,
    /// • bullet style (no filled variant)
    Bullets,
    /// No indicator, just indentation
    None,
}

impl FromStr for CheckboxStyle {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "circles" | "circle" | "dots" => Self::Circles,
            "bullets" | "bullet" => Self::Bullets,
            "none" | "off" => Self::None,
            _ => Self::Brackets,
        })
    }
}

impl CheckboxStyle {
    /// Get the indicator string for an item
    pub fn indicator(&self, _selected: bool) -> &'static str {
        match self {
            Self::Brackets => chars::CHECKBOX_EMPTY,
            Self::Circles => chars::CIRCLE_EMPTY,
            Self::Bullets => chars::BULLET,
            Self::None => "",
        }
    }
}

/// Render options for the TUI
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub width: usize,
    pub height: usize,
    pub checkbox_style: CheckboxStyle,
    pub colors: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            width: 50,
            height: 20,
            checkbox_style: CheckboxStyle::default(),
            colors: true,
        }
    }
}

/// Render the inbox TUI to a string buffer (simple API, no colors for tests)
pub fn render_tui(inbox: &Inbox, selected: usize, width: usize) -> String {
    render_tui_full(
        inbox,
        selected,
        RenderOptions {
            width,
            height: 100,
            colors: false,
            ..Default::default()
        },
    )
}

/// Render the inbox TUI with full options
pub fn render_tui_full(inbox: &Inbox, selected: usize, opts: RenderOptions) -> String {
    let mut output = String::new();
    let width = opts.width.max(20);
    let height = opts.height.max(3); // Minimum height for title + 1 item + help

    // Reserve lines: 1 title + 1 help = 2
    let content_height = height.saturating_sub(2);

    // Title (styled)
    if opts.colors {
        output.push_str(ansi::BOLD);
        output.push_str(ansi::CYAN);
    }
    output.push_str("Agent Inbox");
    if opts.colors {
        output.push_str(ansi::RESET);
    }
    output.push('\n');

    if inbox.is_empty() {
        if opts.colors {
            output.push_str(ansi::DIM);
        }
        output.push_str("  (no items)");
        if opts.colors {
            output.push_str(ansi::RESET);
        }
        output.push('\n');
    } else {
        let overflow =
            render_items_with_height(&mut output, inbox, selected, width, content_height, &opts);
        if overflow {
            if opts.colors {
                output.push_str(ansi::DIM);
            }
            output.push_str(&format!("  {} more below", chars::ELLIPSIS));
            if opts.colors {
                output.push_str(ansi::RESET);
            }
            output.push('\n');
        }
    }

    // Footer/help
    if opts.colors {
        output.push_str(ansi::DIM);
    }
    output.push_str("j/k:nav  Enter:focus  q:close");
    if opts.colors {
        output.push_str(ansi::RESET);
    }
    output.push('\n');

    output
}

/// Render items with height constraint. Returns true if content was truncated.
fn render_items_with_height(
    output: &mut String,
    inbox: &Inbox,
    selected: usize,
    width: usize,
    max_lines: usize,
    opts: &RenderOptions,
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
            if opts.colors {
                output.push_str(ansi::BOLD);
                output.push_str(ansi::YELLOW);
            }
            output.push_str(item.status.section_name());
            if opts.colors {
                output.push_str(ansi::RESET);
            }
            output.push('\n');
            lines_used += 1;
        }

        // Project header
        if need_project {
            current_project = Some(&item.project);
            output.push_str("  ");
            if opts.colors {
                output.push_str(ansi::MAGENTA);
            }
            // Truncate project name if needed
            let max_proj_len = width.saturating_sub(4);
            let proj: String = item.project.chars().take(max_proj_len).collect();
            output.push_str(&proj);
            if opts.colors {
                output.push_str(ansi::RESET);
            }
            output.push('\n');
            lines_used += 1;
        }

        // Item line with selection and checkbox
        output.push_str("    ");
        let is_selected = idx == selected;

        // Selection marker
        if is_selected {
            if opts.colors {
                output.push_str(ansi::GREEN);
            }
            output.push(chars::SELECTED);
            output.push(' ');
            if opts.colors {
                output.push_str(ansi::RESET);
            }
        } else {
            // Checkbox/bullet indicator
            let indicator = opts.checkbox_style.indicator(false);
            output.push_str(indicator);
            if !indicator.is_empty() {
                output.push(' ');
            }
        }

        // Item text
        let prefix_len = if is_selected {
            6
        } else {
            4 + opts.checkbox_style.indicator(false).len() + 1
        };
        let max_text_len = width.saturating_sub(prefix_len);
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
        assert!(
            auth_line.contains('▶'),
            "First item should have selection marker"
        );

        // Second item selected
        let output = render_tui(&inbox, 1, 50);
        let lines: Vec<&str> = output.lines().collect();
        let review_line = lines.iter().find(|l| l.contains("Review PR")).unwrap();
        assert!(
            review_line.contains('▶'),
            "Second item should have selection marker"
        );
    }

    #[test]
    fn render_respects_width() {
        let inbox = Inbox {
            items: vec![InboxItem {
                text: "This is a very long text that should be truncated when width is small"
                    .to_string(),
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
    fn render_no_border() {
        let inbox = Inbox::new();
        let output = render_tui(&inbox, 0, 40);

        // No box-drawing characters (borderless for floating window)
        assert!(!output.contains('┌'));
        assert!(!output.contains('┘'));
        assert!(!output.contains('│'));
    }

    #[test]
    fn snapshot_empty() {
        let inbox = Inbox::new();
        let output = render_tui(&inbox, 0, 40);

        let expected = "\
Agent Inbox
  (no items)
j/k:nav  Enter:focus  q:close
";
        assert_eq!(output, expected);
    }

    #[test]
    fn snapshot_with_items() {
        let inbox = Inbox {
            items: vec![InboxItem {
                text: "claude: Question".to_string(),
                pane_id: 42,
                project: "myproject".to_string(),
                status: Status::Waiting,
            }],
        };
        let output = render_tui(&inbox, 0, 40);

        let expected = "\
Agent Inbox
Waiting for Input
  myproject
    ▶ claude: Question
j/k:nav  Enter:focus  q:close
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

        // Height 6: title(1) + section(1) + project(1) + items(3) + help(1) = 8 needed
        // With height 6, we should truncate
        let opts = RenderOptions {
            width: 40,
            height: 6,
            colors: false,
            ..Default::default()
        };
        let output = render_tui_full(&inbox, 0, opts);

        // Should show truncation indicator
        assert!(output.contains('…'), "Should show truncation indicator");
        assert!(output.contains("item1"));
    }

    #[test]
    fn render_full_width() {
        let inbox = Inbox {
            items: vec![InboxItem {
                text: "A very long message that should not be truncated at 80 chars anymore"
                    .to_string(),
                pane_id: 1,
                project: "test".to_string(),
                status: Status::Waiting,
            }],
        };

        let output = render_tui(&inbox, 0, 100);
        assert!(output.contains("truncated at 80"));
    }

    #[test]
    fn checkbox_style_from_str() {
        assert_eq!(
            "circles".parse::<CheckboxStyle>().unwrap(),
            CheckboxStyle::Circles
        );
        assert_eq!(
            "bullets".parse::<CheckboxStyle>().unwrap(),
            CheckboxStyle::Bullets
        );
        assert_eq!(
            "none".parse::<CheckboxStyle>().unwrap(),
            CheckboxStyle::None
        );
        assert_eq!(
            "anything".parse::<CheckboxStyle>().unwrap(),
            CheckboxStyle::Brackets
        );
    }

    #[test]
    fn checkbox_style_indicators() {
        assert_eq!(CheckboxStyle::Brackets.indicator(false), "[ ]");
        assert_eq!(CheckboxStyle::Circles.indicator(false), "○");
        assert_eq!(CheckboxStyle::Bullets.indicator(false), "•");
        assert_eq!(CheckboxStyle::None.indicator(false), "");
    }

    #[test]
    fn render_with_circles_style() {
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
            ],
        };

        let opts = RenderOptions {
            width: 40,
            height: 20,
            colors: false,
            checkbox_style: CheckboxStyle::Circles,
        };
        let output = render_tui_full(&inbox, 0, opts);

        // Selected item has arrow, not circle
        assert!(output.contains('▶'));
        // Non-selected item has circle
        assert!(output.contains('○'));
    }
}
