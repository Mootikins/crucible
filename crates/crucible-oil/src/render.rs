//! Render functions for Oil node trees.
//!
//! All rendering goes through the Taffy layout engine — the same pipeline
//! used by the production TUI (`FramePlanner`). These convenience wrappers
//! build a standalone layout tree and render in compact mode.

use crate::ansi::{strip_ansi, visual_rows};
use crate::layout::{build_layout_tree, render_layout_tree_compact};
use crate::node::Node;

/// Maximum standalone render height. The CellGrid is allocated at this height;
/// trailing blank lines are trimmed from compact output.
const STANDALONE_HEIGHT: u16 = 500;

#[derive(Debug, Clone, Copy, Default)]
pub struct CursorInfo {
    pub col: u16,
    pub row_from_end: u16,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct RenderResult {
    pub content: String,
    pub cursor: CursorInfo,
}

/// Render a node tree to an ANSI string (compact, no trailing blank lines).
pub fn render_to_string(node: &Node, width: usize) -> String {
    render_with_cursor(node, width).content
}

/// Render a node tree to plain text (ANSI stripped, no carriage returns).
///
/// Raw nodes render as `[raw: WxH]` placeholders since their escape
/// sequences are not meaningful as plain text.
pub fn render_to_plain_text(node: &Node, width: usize) -> String {
    let mut output = String::new();
    render_node_plain_text(node, width, &mut output);
    output
}

fn render_node_plain_text(node: &Node, width: usize, output: &mut String) {
    match node {
        Node::Raw(raw) => {
            output.push_str(&format!(
                "[raw: {}x{}]",
                raw.display_width, raw.display_height
            ));
        }
        Node::Fragment(children) => {
            for child in children {
                render_node_plain_text(child, width, output);
            }
        }
        Node::Static(static_node) => {
            for child in &static_node.children {
                render_node_plain_text(child, width, output);
            }
        }
        Node::Overlay(o) => render_node_plain_text(&o.child, width, output),
        other => {
            let rendered = render_to_string(other, width);
            output.push_str(&strip_ansi(&rendered).replace('\r', ""));
        }
    }
}

/// Render a node tree to an ANSI string with cursor tracking.
pub fn render_with_cursor(node: &Node, width: usize) -> RenderResult {
    if width == 0 {
        return RenderResult {
            content: String::new(),
            cursor: CursorInfo::default(),
        };
    }

    // Raw nodes bypass CellGrid — their content contains escape sequences
    // that must be passed through verbatim (iTerm2 image protocol, etc.).
    if let Node::Raw(raw) = node {
        let mut content = raw.content.clone();
        let pad = width.saturating_sub(raw.display_width as usize);
        if pad > 0 {
            content.push_str(&" ".repeat(pad));
        }
        return RenderResult {
            content,
            cursor: CursorInfo::default(),
        };
    }

    let layout_tree = build_layout_tree(node, width as u16, STANDALONE_HEIGHT);
    // Compact mode strips per-line trailing padding at the CellGrid level
    // (before converting to string), correctly handling styled cells.
    let (full_content, mut cursor_info) = render_layout_tree_compact(&layout_tree);

    // Trim trailing blank lines from the full-height CellGrid output.
    let content = trim_trailing_blank_lines(&full_content);

    // Recalculate row_from_end against the trimmed content
    if cursor_info.visible {
        let full_line_count = full_content.lines().count();
        let line_count = content.lines().count();
        let cursor_abs_row =
            full_line_count.saturating_sub(cursor_info.row_from_end as usize + 1);

        // Adjust for visual wrapping
        let lines: Vec<&str> = content.lines().collect();
        let visual_rows_below: usize = lines
            .get(cursor_abs_row + 1..)
            .unwrap_or(&[])
            .iter()
            .map(|line| visual_rows(line, width))
            .sum();
        cursor_info.row_from_end = visual_rows_below as u16;

        // Clamp if cursor is beyond trimmed content
        if cursor_abs_row >= line_count {
            cursor_info.row_from_end = 0;
        }
    }

    RenderResult {
        content,
        cursor: cursor_info,
    }
}

/// Trim trailing blank lines from CellGrid compact output.
fn trim_trailing_blank_lines(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();

    // Find last line with visible content
    let last_content_idx = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);

    lines[..last_content_idx].join("\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::*;
    use crate::style::{Border, Color, Gap, Padding, Style};
    use insta::assert_snapshot;

    #[test]
    fn test_render_empty_node() {
        let node = Node::Empty;
        let result = render_to_string(&node, 80);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_simple_text() {
        let node = text("Hello, World!");
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Hello, World!");
    }

    #[test]
    fn test_render_styled_text() {
        let style = Style::new().bold().fg(Color::Red);
        let node = styled("Bold Red", style);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Bold Red"));
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn test_render_column() {
        let node = col(vec![text("Line 1"), text("Line 2"), text("Line 3")]);
        let result = render_to_string(&node, 80);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "Line 1");
        assert_eq!(lines[1], "Line 2");
        assert_eq!(lines[2], "Line 3");
    }

    #[test]
    fn test_render_row_simple() {
        let node = row(vec![text("A"), text("B"), text("C")]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("A"));
        assert!(result.contains("B"));
        assert!(result.contains("C"));
    }

    #[test]
    fn test_render_fragment() {
        let node = fragment(vec![text("First"), text("Second"), text("Third")]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
        assert!(result.contains("Third"));
    }

    #[test]
    fn test_render_input_unfocused() {
        let node = Node::Input(InputNode {
            value: "test input".to_string(),
            cursor: 0,
            placeholder: None,
            style: Style::default(),
            focused: false,
        });
        let result = render_to_string(&node, 80);
        assert_eq!(result, "test input");
    }

    #[test]
    fn test_render_input_with_placeholder() {
        let node = Node::Input(InputNode {
            value: String::new(),
            cursor: 0,
            placeholder: Some("Enter text...".to_string()),
            style: Style::default(),
            focused: false,
        });
        let result = render_to_string(&node, 80);
        assert!(result.contains("Enter text..."));
    }

    #[test]
    fn test_render_spinner() {
        let node = Node::Spinner(SpinnerNode {
            label: Some("Loading".to_string()),
            style: Style::default(),
            frame: 0,
            style_variant: None,
        });
        let result = render_to_string(&node, 80);
        assert!(result.contains("Loading"));
        assert!(result.contains("◐"));
    }

    #[test]
    fn test_render_popup_single_item() {
        let items = vec![popup_item("Option 1")];
        let node = popup(items, 0, 5);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Option 1"));
    }

    #[test]
    fn test_render_popup_multiple_items() {
        let items = vec![
            popup_item("Option 1"),
            popup_item("Option 2"),
            popup_item("Option 3"),
        ];
        let node = popup(items, 1, 5);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Option 1"));
        assert!(result.contains("Option 2"));
        assert!(result.contains("Option 3"));
    }

    #[test]
    fn test_render_overlay_node() {
        let node = overlay_from_bottom(text("Overlay content"), 5);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Overlay content"));
    }

    #[test]
    fn test_render_raw_node() {
        let node = raw("\\x1b[31mRed\\x1b[0m", 3, 1);
        let result = render_to_string(&node, 80);
        assert!(result.contains("\\x1b[31mRed\\x1b[0m"));
    }

    #[test]
    fn test_render_nested_col_row() {
        let node = col(vec![
            text("Header"),
            row(vec![text("A"), text("B")]),
            text("Footer"),
        ]);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Header"));
        assert!(result.contains("A"));
        assert!(result.contains("B"));
        assert!(result.contains("Footer"));
    }

    #[test]
    fn test_render_box_with_padding() {
        let boxnode = BoxNode {
            children: vec![text("Content")],
            direction: Direction::Column,
            padding: Padding {
                top: 1,
                bottom: 1,
                left: 2,
                right: 2,
            },
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_render_box_with_border() {
        let boxnode = BoxNode {
            children: vec![text("Content")],
            direction: Direction::Column,
            border: Some(Border::Single),
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        assert!(result.contains("Content"));
    }

    #[test]
    fn test_render_static_node() {
        let node = scrollback("key1", vec![text("Static content")]);
        let result = render_to_string(&node, 80);
        assert_eq!(result, "Static content");
    }

    #[test]
    fn test_cursor_tracking_simple_input() {
        let node = Node::Input(InputNode {
            value: "hello".to_string(),
            cursor: 2,
            placeholder: None,
            style: Style::default(),
            focused: true,
        });
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 2);
        assert_eq!(result.cursor.row_from_end, 0);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_input_at_end() {
        let node = Node::Input(InputNode {
            value: "hello".to_string(),
            cursor: 5,
            placeholder: None,
            style: Style::default(),
            focused: true,
        });
        let result = render_with_cursor(&node, 80);
        assert_eq!(result.cursor.col, 5);
        assert!(result.cursor.visible);
    }

    #[test]
    fn test_cursor_tracking_unfocused_input() {
        let node = Node::Input(InputNode {
            value: "unfocused".to_string(),
            cursor: 3,
            placeholder: None,
            style: Style::default(),
            focused: false,
        });
        let result = render_with_cursor(&node, 80);
        assert!(!result.cursor.visible);
    }

    #[test]
    fn test_render_column_with_custom_gap() {
        let boxnode = BoxNode {
            children: vec![text("A"), text("B"), text("C")],
            direction: Direction::Column,
            gap: Gap { row: 2, column: 0 },
            ..Default::default()
        };
        let node = Node::Box(boxnode);
        let result = render_to_string(&node, 80);
        let lines: Vec<&str> = result.lines().collect();
        // 3 items + 2 gaps of 2 = 3 + 4 = 7 lines
        assert!(lines.len() >= 5, "Expected at least 5 lines with gap=2, got {}", lines.len());
    }

    #[test]
    fn test_render_empty_fragment() {
        let node = fragment(vec![]);
        let result = render_to_string(&node, 80);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_text_embedded_newlines_use_crlf() {
        let node = text("line1\nline2\nline3");
        let result = render_to_string(&node, 200);

        // CellGrid joins lines with \r\n
        assert!(
            result.contains("\r\n"),
            "Expected \\r\\n between lines, got: {:?}",
            result
        );

        let lines: Vec<&str> = result.split("\r\n").collect();
        assert_eq!(lines.len(), 3, "Expected 3 lines separated by \\r\\n");
        assert_eq!(lines[0], "line1");
        assert_eq!(lines[1], "line2");
        assert_eq!(lines[2], "line3");
    }

    #[test]
    fn snapshot_core_text_wrapping() {
        let node = text("rendering primitives should wrap this line without hyphenation");
        let output = render_to_string(&node, 18);
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_core_styled_text_ansi() {
        let node = styled(
            "Styled",
            Style::new()
                .bold()
                .underline()
                .fg(Color::Cyan)
                .bg(Color::DarkGray),
        );
        let output = render_to_string(&node, 80);
        assert_snapshot!(output.escape_debug().to_string());
    }

    #[test]
    fn snapshot_core_row_mixed_sizes() {
        let node = row(vec![
            text("alpha beta"),
            text("gamma delta"),
            text("epsilon"),
        ]);
        let output = render_to_plain_text(&node, 14);
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_core_popup_truncation() {
        let node = popup(
            vec![
                popup_item("Open file")
                    .kind("CMD")
                    .desc("Open a file from the kiln"),
                popup_item("Long command label that truncates")
                    .kind("TOOL")
                    .desc("Description should also truncate when width is tight"),
            ],
            1,
            2,
        );
        let output = render_to_plain_text(&node, 36);
        assert_snapshot!(output);
    }

    #[test]
    fn snapshot_core_raw_padding() {
        let node = raw("[img]", 5, 1);
        let output = render_to_string(&node, 12);
        assert_snapshot!(format!("{output:?}"));
    }
}
