//! Callout plugin for markdown-it (Obsidian-style callouts)
//!
//! Implements block-level callout syntax:
//! ```text
//! > [!note] Optional title
//! > Content line 1
//! > Content line 2
//! ```
//!
//! Supported callout types: note, tip, warning, danger, info, question, success, failure, etc.

use markdown_it::parser::block::{BlockRule, BlockState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};
use regex::Regex;
use std::sync::OnceLock;

/// Custom AST node for callouts
#[derive(Debug, Clone)]
pub struct CalloutNode {
    pub callout_type: String,
    pub title: Option<String>,
    pub content: String,
    pub offset: usize,
}

impl NodeValue for CalloutNode {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        // Render as HTML with callout class
        fmt.open(
            "div",
            &[
                ("class", format!("callout callout-{}", self.callout_type)),
                (
                    "data-callout",
                    self.callout_type.clone(),
                ),
            ],
        );

        // Title section
        if let Some(title) = &self.title {
            fmt.open("div", &[("class", "callout-title".to_string())]);
            fmt.text(title);
            fmt.close("div");
        }

        // Content section
        fmt.open("div", &[("class", "callout-content".to_string())]);
        fmt.text(&self.content);
        fmt.close("div");

        fmt.close("div");
    }
}

/// Callout scanner - matches > [!type] patterns
pub struct CalloutScanner;

fn callout_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Match: > [!type] optional title
        Regex::new(r"^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+(.*))?$").unwrap()
    })
}

impl BlockRule for CalloutScanner {
    fn check(state: &mut BlockState) -> Option<()> {
        let line = state.get_line(state.line);
        if callout_regex().is_match(&line) {
            Some(())
        } else {
            None
        }
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let start_line = state.line;
        let first_line = state.get_line(start_line);

        // Parse the first line to extract type and title
        let caps = callout_regex().captures(&first_line)?;
        let callout_type = caps.get(1)?.as_str().to_string();
        let title = caps.get(2).map(|m| m.as_str().trim().to_string());

        // Calculate offset (start of the block)
        let offset = start_line; // Use line number as offset for now

        // Collect content lines (subsequent lines starting with >)
        let mut content_lines = Vec::new();
        let mut current_line = start_line + 1;

        // Use state.line_max which is the proper bound for iteration
        while current_line < state.line_max {
            let line = state.get_line(current_line);
            let line_str = &*line;

            // Check if line starts with >
            if let Some(stripped) = line_str.strip_prefix('>') {
                // Remove leading space if present
                let content = stripped.strip_prefix(' ').unwrap_or(stripped);
                content_lines.push(content.to_string());
                current_line += 1;
            } else if line_str.trim().is_empty() {
                // Empty line ends the callout (unless followed by a continuation > line)
                // Check if next line continues the callout (but NOT if it's a new callout)
                if current_line + 1 < state.line_max {
                    let next_line = state.get_line(current_line + 1);
                    // Continue only if next line starts with > but is NOT a new callout
                    if next_line.starts_with('>') && !callout_regex().is_match(&next_line) {
                        // Skip empty line but continue callout
                        content_lines.push(String::new());
                        current_line += 1;
                        continue;
                    }
                }
                break;
            } else {
                // Line doesn't start with >, end of callout
                break;
            }
        }

        let content = content_lines.join("\n");

        // Create callout node
        let callout = CalloutNode {
            callout_type,
            title,
            content,
            offset,
        };

        let mut node = Node::new(callout);
        // get_map expects end_line to be a valid index, use current_line - 1
        // since current_line points past the last consumed line
        let end_line = if current_line > start_line {
            current_line - 1
        } else {
            start_line
        };
        node.srcmap = state.get_map(start_line, end_line);

        // Return the node and number of lines consumed
        let lines_consumed = current_line - start_line;
        Some((node, lines_consumed))
    }
}

/// Add callout plugin to markdown-it parser
///
/// Note: Callout rules must be added BEFORE the cmark blockquote rule,
/// otherwise blockquotes will consume the `>` lines first.
pub fn add_callout_plugin(md: &mut MarkdownIt) {
    // Add rule before blockquote so callouts take priority over regular blockquotes
    md.block
        .add_rule::<CalloutScanner>()
        .before::<markdown_it::plugins::cmark::block::blockquote::BlockquoteScanner>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_callouts(input: &str) -> Vec<(String, Option<String>, String)> {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        add_callout_plugin(&mut md);

        let ast = md.parse(input);
        let mut callouts = Vec::new();

        fn walk(node: &Node, callouts: &mut Vec<(String, Option<String>, String)>) {
            if let Some(callout) = node.cast::<CalloutNode>() {
                callouts.push((
                    callout.callout_type.clone(),
                    callout.title.clone(),
                    callout.content.clone(),
                ));
            }
            for child in &node.children {
                walk(child, callouts);
            }
        }

        walk(&ast, &mut callouts);
        callouts
    }

    #[test]
    fn test_simple_callout() {
        let input = "> [!note]\n> This is a note";
        let callouts = parse_callouts(input);
        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].0, "note");
        assert_eq!(callouts[0].1, None);
        assert_eq!(callouts[0].2, "This is a note");
    }

    #[test]
    fn test_callout_with_title() {
        let input = "> [!warning] Important Warning\n> Be careful!";
        let callouts = parse_callouts(input);
        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].0, "warning");
        assert_eq!(callouts[0].1, Some("Important Warning".to_string()));
        assert_eq!(callouts[0].2, "Be careful!");
    }

    #[test]
    fn test_multiline_callout() {
        let input = "> [!tip]\n> Line 1\n> Line 2\n> Line 3";
        let callouts = parse_callouts(input);
        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].0, "tip");
        assert_eq!(callouts[0].2, "Line 1\nLine 2\nLine 3");
    }

    #[test]
    fn test_callout_with_hyphen() {
        let input = "> [!my-custom-type]\n> Custom content";
        let callouts = parse_callouts(input);
        assert_eq!(callouts.len(), 1);
        assert_eq!(callouts[0].0, "my-custom-type");
    }

    #[test]
    fn test_multiple_callouts() {
        let input = "> [!note]\n> First\n\n> [!warning]\n> Second";
        let callouts = parse_callouts(input);
        assert_eq!(callouts.len(), 2);
        assert_eq!(callouts[0].0, "note");
        assert_eq!(callouts[1].0, "warning");
    }
}
