use crate::node::{col, row, styled, Node};
use crate::style::{Color, Style};
use similar::{ChangeTag, TextDiff};

/// Colors for diff rendering
#[derive(Debug, Clone, Copy)]
pub struct DiffColors {
    pub delete: Style,
    pub insert: Style,
    pub context: Style,
    pub hunk_header: Style,
}

impl Default for DiffColors {
    fn default() -> Self {
        Self {
            delete: Style::new().fg(Color::Red),
            insert: Style::new().fg(Color::Green),
            context: Style::new().fg(Color::DarkGray),
            hunk_header: Style::new().fg(Color::Cyan),
        }
    }
}

pub fn diff_to_node(old: &str, new: &str, context_lines: usize, colors: &DiffColors) -> Node {
    if old == new {
        return Node::Empty;
    }

    let diff = TextDiff::from_lines(old, new);
    let mut nodes: Vec<Node> = Vec::new();

    let delete_style = colors.delete;
    let insert_style = colors.insert;
    let context_style = colors.context;
    let hunk_header_style = colors.hunk_header;

    let mut in_hunk = false;
    let mut hunk_lines: Vec<Node> = Vec::new();
    let mut context_buffer: Vec<(usize, String)> = Vec::new();
    let mut pending_context: Vec<(usize, String)> = Vec::new();

    for (idx, change) in diff.iter_all_changes().enumerate() {
        let tag = change.tag();
        let line_content = change.value().trim_end_matches('\n');

        match tag {
            ChangeTag::Equal => {
                if in_hunk {
                    if context_lines > 0 && pending_context.len() < context_lines {
                        pending_context.push((idx, line_content.to_string()));
                    } else {
                        flush_hunk(&mut nodes, &mut hunk_lines, hunk_header_style);
                        in_hunk = false;
                        pending_context.clear();
                    }
                }
                if context_lines > 0 {
                    context_buffer.push((idx, line_content.to_string()));
                    if context_buffer.len() > context_lines {
                        context_buffer.remove(0);
                    }
                }
            }
            ChangeTag::Delete | ChangeTag::Insert => {
                if !in_hunk {
                    in_hunk = true;
                    for (_, ctx_line) in &context_buffer {
                        hunk_lines.push(styled(format!(" {}", ctx_line), context_style));
                    }
                } else {
                    for (_, ctx_line) in pending_context.drain(..) {
                        hunk_lines.push(styled(format!(" {}", ctx_line), context_style));
                    }
                }

                let (prefix, style) = match tag {
                    ChangeTag::Delete => ("-", delete_style),
                    ChangeTag::Insert => ("+", insert_style),
                    _ => unreachable!(),
                };
                hunk_lines.push(styled(format!("{}{}", prefix, line_content), style));
            }
        }
    }

    if in_hunk {
        flush_hunk(&mut nodes, &mut hunk_lines, hunk_header_style);
    }

    if nodes.is_empty() {
        Node::Empty
    } else {
        col(nodes)
    }
}

fn flush_hunk(nodes: &mut Vec<Node>, hunk_lines: &mut Vec<Node>, _header_style: Style) {
    if hunk_lines.is_empty() {
        return;
    }
    nodes.append(hunk_lines);
}

pub fn diff_to_node_with_word_highlight(old: &str, new: &str, colors: &DiffColors) -> Node {
    if old == new {
        return Node::Empty;
    }

    let line_diff = TextDiff::from_lines(old, new);
    let mut nodes: Vec<Node> = Vec::new();

    let delete_style = colors.delete;
    let insert_style = colors.insert;

    for change in line_diff.iter_all_changes() {
        let line_content = change.value().trim_end_matches('\n');

        match change.tag() {
            ChangeTag::Equal => {}
            ChangeTag::Delete => {
                nodes.push(row([
                    styled("-", delete_style),
                    styled(line_content, delete_style),
                ]));
            }
            ChangeTag::Insert => {
                nodes.push(row([
                    styled("+", insert_style),
                    styled(line_content, insert_style),
                ]));
            }
        }
    }

    if nodes.is_empty() {
        Node::Empty
    } else {
        col(nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_to_string;

    fn has_red_ansi(s: &str) -> bool {
        s.contains("\x1b[31m") || s.contains("\x1b[38;5;9m")
    }

    fn has_green_ansi(s: &str) -> bool {
        s.contains("\x1b[32m") || s.contains("\x1b[38;5;10m")
    }

    #[test]
    fn diff_to_node_identical_returns_empty() {
        let result = diff_to_node("same\n", "same\n", 0, &DiffColors::default());
        assert!(matches!(result, Node::Empty));
    }

    #[test]
    fn diff_to_node_deletion_is_red() {
        let result = diff_to_node("old line\n", "", 0, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(
            has_red_ansi(&output),
            "Deletion should be red. Got:\n{}",
            output.escape_debug()
        );
        assert!(output.contains("-old line"));
    }

    #[test]
    fn diff_to_node_insertion_is_green() {
        let result = diff_to_node("", "new line\n", 0, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(
            has_green_ansi(&output),
            "Insertion should be green. Got:\n{}",
            output.escape_debug()
        );
        assert!(output.contains("+new line"));
    }

    #[test]
    fn diff_to_node_modification_shows_both() {
        let result = diff_to_node("old\n", "new\n", 0, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(output.contains("-old"));
        assert!(output.contains("+new"));
    }

    #[test]
    fn diff_to_node_with_context() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let result = diff_to_node(old, new, 1, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(output.contains(" line1"), "Should show context before");
    }

    #[test]
    fn diff_to_node_multiline_changes() {
        let old = "a\nb\nc\n";
        let new = "a\nB\nC\n";
        let result = diff_to_node(old, new, 0, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(output.contains("-b"));
        assert!(output.contains("-c"));
        assert!(output.contains("+B"));
        assert!(output.contains("+C"));
    }

    #[test]
    fn diff_preserves_indentation() {
        let old = "    indented line\n";
        let new = "    modified line\n";
        let result = diff_to_node(old, new, 0, &DiffColors::default());
        let output = render_to_string(&result, 80);

        assert!(
            output.contains("-    indented"),
            "Should preserve indentation in deletion"
        );
        assert!(
            output.contains("+    modified"),
            "Should preserve indentation in insertion"
        );
    }
}
