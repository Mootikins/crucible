//! Tag plugin for markdown-it (#tag, #nested/tag)
//!
//! Implements inline tag syntax similar to Obsidian tags:
//! - `#tag` - Simple tag
//! - `#nested/tag` - Nested tag
//!
//! Tags are extracted during parsing and added to the AST.

use markdown_it::parser::inline::{InlineRule, InlineState};
use markdown_it::{MarkdownIt, Node, NodeValue, Renderer};

/// Custom AST node for tags
#[derive(Debug, Clone)]
pub struct TagNode {
    pub name: String,
    pub offset: usize,
}

impl NodeValue for TagNode {
    fn render(&self, _node: &Node, fmt: &mut dyn Renderer) {
        // Render as HTML span with tag class
        fmt.open("span", &[("class", "tag".to_string())]);
        fmt.text(&format!("#{}", self.name));
        fmt.close("span");
    }
}

/// Tag scanner - matches #tag and #nested/tag patterns
pub struct TagScanner;

impl InlineRule for TagScanner {
    const MARKER: char = '#';

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let input = &state.src[state.pos..];

        // Must start with #
        if !input.starts_with('#') {
            return None;
        }

        // Don't match if preceded by alphanumeric (e.g., "test#tag" shouldn't match)
        if state.pos > 0 {
            if let Some(prev_char) = state.src.chars().nth(state.pos - 1) {
                if prev_char.is_alphanumeric() || prev_char == '_' {
                    return None;
                }
            }
        }

        // Find the end of the tag
        let mut end = 1; // Start after #
        let chars: Vec<char> = input.chars().collect();

        while end < chars.len() {
            let ch = chars[end];
            // Tags can contain alphanumeric, underscores, hyphens, and forward slashes
            if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '/' {
                end += 1;
            } else {
                break;
            }
        }

        // Need at least one character after #
        if end <= 1 {
            return None;
        }

        let tag_name = &input[1..end];

        // Validate tag structure:
        // - No empty path components
        // - No consecutive slashes
        // - No leading/trailing slashes
        if tag_name.is_empty()
            || tag_name.starts_with('/')
            || tag_name.ends_with('/')
            || tag_name.contains("//")
        {
            return None;
        }

        // Create tag node
        let tag = TagNode {
            name: tag_name.to_string(),
            offset: state.pos,
        };

        let node = Node::new(tag);
        // Return node and number of characters consumed (including #)
        Some((node, end))
    }
}

/// Add tag plugin to markdown-it parser
pub fn add_tag_plugin(md: &mut MarkdownIt) {
    md.inline.add_rule::<TagScanner>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tags(input: &str) -> Vec<String> {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        add_tag_plugin(&mut md);

        let ast = md.parse(input);
        let mut tags = Vec::new();

        fn walk(node: &Node, tags: &mut Vec<String>) {
            if let Some(tag) = node.cast::<TagNode>() {
                tags.push(tag.name.clone());
            }
            for child in &node.children {
                walk(child, tags);
            }
        }

        walk(&ast, &mut tags);
        tags
    }

    #[test]
    fn test_simple_tag() {
        let tags = parse_tags("This has #tag in it");
        assert_eq!(tags, vec!["tag"]);
    }

    #[test]
    fn test_nested_tag() {
        let tags = parse_tags("This has #nested/tag/path");
        assert_eq!(tags, vec!["nested/tag/path"]);
    }

    #[test]
    fn test_multiple_tags() {
        let tags = parse_tags("#tag1 and #tag2 and #nested/tag3");
        assert_eq!(tags, vec!["tag1", "tag2", "nested/tag3"]);
    }

    #[test]
    fn test_tag_with_underscores() {
        let tags = parse_tags("#my_tag and #nested_tag/sub_tag");
        assert_eq!(tags, vec!["my_tag", "nested_tag/sub_tag"]);
    }

    #[test]
    fn test_tag_with_hyphens() {
        let tags = parse_tags("#my-tag and #nested-tag/sub-tag");
        assert_eq!(tags, vec!["my-tag", "nested-tag/sub-tag"]);
    }

    #[test]
    fn test_not_a_tag_in_word() {
        // "test#tag" should not match because # is not preceded by whitespace
        let tags = parse_tags("test#tag");
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_not_a_tag_just_hash() {
        let tags = parse_tags("Just # alone");
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_not_a_tag_invalid_slash() {
        let tags = parse_tags("#/invalid #also/invalid/ #and//invalid");
        assert_eq!(tags.len(), 0);
    }
}
