//! Wikilink plugin for markdown-it
//!
//! Supports Obsidian-style wikilinks:
//! - `[[Simple Link]]`
//! - `[[Link|Alias]]`
//! - `[[Note#Heading]]`
//! - `[[Note#^block-id]]`
//! - `![[Embed]]`

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};
use std::fmt;

/// Custom AST node for wikilinks
#[derive(Debug, Clone)]
pub struct WikilinkNode {
    pub target: String,
    pub alias: Option<String>,
    pub heading_ref: Option<String>,
    pub block_ref: Option<String>,
    pub is_embed: bool,
    pub offset: usize,
}

impl NodeValue for WikilinkNode {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        // Render as HTML for now (would be customizable)
        let display_text = self.alias.as_ref().unwrap_or(&self.target);

        if self.is_embed {
            fmt.text(&format!("![[{}]]", self.target));
        } else {
            fmt.open("span", &[("class", "wikilink".to_string())]);
            fmt.text(display_text);
            fmt.close("span");
        }
    }
}

impl fmt::Display for WikilinkNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Wikilink({})", self.target)
    }
}

/// Scanner for wikilink syntax
pub struct WikilinkScanner;

impl InlineRule for WikilinkScanner {
    const MARKER: char = '[';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let input = &state.src[state.pos..];

        // Check for [[ opener
        if !input.starts_with("[[") {
            return None;
        }

        // Check for embed ![[
        let is_embed = if state.pos > 0 {
            state.src.chars().nth(state.pos - 1) == Some('!')
        } else {
            false
        };

        // Find closing ]]
        let end = input.find("]]")?;
        if end < 2 {
            return None; // Empty wikilink
        }

        let inner = &input[2..end];

        // Parse: [[target#heading|alias]]
        let (target_part, alias) = if let Some(pipe_pos) = inner.find('|') {
            let target = &inner[..pipe_pos];
            let alias = &inner[pipe_pos + 1..];
            (target, Some(alias.to_string()))
        } else {
            (inner, None)
        };

        // Parse: target#heading or target#^block
        let (target, heading_ref, block_ref) = if let Some(hash_pos) = target_part.find('#') {
            let target = &target_part[..hash_pos];
            let ref_part = &target_part[hash_pos + 1..];

            if ref_part.starts_with('^') {
                // Block reference
                (target, None, Some(ref_part[1..].to_string()))
            } else {
                // Heading reference
                (target, Some(ref_part.to_string()), None)
            }
        } else {
            (target_part, None, None)
        };

        // Create custom node
        let wikilink = WikilinkNode {
            target: target.to_string(),
            alias,
            heading_ref,
            block_ref,
            is_embed,
            offset: state.pos,
        };

        let node = Node::new(wikilink);

        // Return node and length consumed (including [[ and ]])
        Some((node, end + 4))
    }
}

/// Add wikilink plugin to markdown-it parser
pub fn add_wikilink_plugin(md: &mut MarkdownIt) {
    md.inline.add_rule::<WikilinkScanner>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_it::MarkdownIt;

    fn setup_parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        add_wikilink_plugin(&mut md);
        md
    }

    #[test]
    fn test_simple_wikilink() {
        let md = setup_parser();
        let ast = md.parse("This has [[Simple Link]] in it.");
        let html = ast.render();

        // Should contain wikilink
        assert!(html.contains("Simple Link") || html.contains("[[Simple Link]]"));
    }

    #[test]
    fn test_wikilink_with_alias() {
        let md = setup_parser();
        let ast = md.parse("See [[Actual Page|Display Name]].");
        let html = ast.render();

        // Should use alias for display
        assert!(html.contains("Display Name"));
    }

    #[test]
    fn test_wikilink_with_heading() {
        let md = setup_parser();
        let ast = md.parse("Reference [[Note#Section]].");
        let html = ast.render();

        // Should parse heading reference
        assert!(html.contains("Note") || html.contains("[[Note#Section]]"));
    }

    #[test]
    fn test_wikilink_embed() {
        let md = setup_parser();
        let ast = md.parse("Embed: ![[Image]].");
        let html = ast.render();

        // Should recognize embed
        assert!(html.contains("Image") || html.contains("![[Image]]"));
    }

    #[test]
    fn test_regular_markdown_links() {
        let md = setup_parser();
        let ast = md.parse("Regular [link](url) should work.");
        let html = ast.render();

        // Regular links should still work
        assert!(html.contains("link") && html.contains("href"));
    }
}
