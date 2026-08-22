//! Inline markdown link syntax extension
//!
//! This module implements support for standard markdown inline links:
//! - Basic links: `[text](url)`
//! - Links with titles: `[text](url "title")`
//! - External links: `[text](https://example.com)`
//! - Relative links: `[text](./relative/path.md)`

use super::error::ParseError;
use super::types::{InlineLink, NoteContent};

use regex::Regex;
use std::sync::LazyLock;

static LINK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\[([^\]]+)\]\(([^\s)]+)(?:\s+"([^"]+)")?\)"#).expect("inline link regex")
});

/// Inline link syntax extension
#[derive(Debug, Clone, Copy, Default)]
pub struct InlineLinkExtension;

impl InlineLinkExtension {
    /// Create a new inline link extension
    pub fn new() -> Self {
        Self
    }
}

impl InlineLinkExtension {
    pub(super) fn can_handle(&self, content: &str) -> bool {
        // Quick check for link pattern before expensive regex
        content.contains("](")
    }

    pub(super) fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let errors = Vec::new();

        // Extract all inline links
        for cap in LINK_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let text = cap.get(1).unwrap().as_str().trim();
            let url = cap.get(2).unwrap().as_str().trim();
            let title = cap.get(3).map(|m| m.as_str().trim());
            let offset = full_match.start();

            // Create inline link
            let inline_link = if let Some(title_str) = title {
                InlineLink::with_title(
                    text.to_string(),
                    url.to_string(),
                    title_str.to_string(),
                    offset,
                )
            } else {
                InlineLink::new(text.to_string(), url.to_string(), offset)
            };

            doc_content.inline_links.push(inline_link);
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_link_detection() {
        let extension = InlineLinkExtension::new();

        assert!(extension.can_handle("This has a [link](url) reference"));
        assert!(extension.can_handle("Check out [Rust](https://rust-lang.org)"));
        assert!(!extension.can_handle("Regular text without links"));
        assert!(!extension.can_handle("Wikilink [[note]] is not handled"));
    }

    #[test]
    fn test_basic_inline_link_parsing() {
        let extension = InlineLinkExtension::new();
        let content = "Check out [Rust](https://rust-lang.org) for more info.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 1);

        let link = &doc_content.inline_links[0];
        assert_eq!(link.text, "Rust");
        assert_eq!(link.url, "https://rust-lang.org");
        assert_eq!(link.title, None);
        assert!(link.is_external());
    }

    #[test]
    fn test_inline_link_with_title() {
        let extension = InlineLinkExtension::new();
        let content =
            r#"Visit [Rust](https://rust-lang.org "The Rust Programming Language") today!"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 1);

        let link = &doc_content.inline_links[0];
        assert_eq!(link.text, "Rust");
        assert_eq!(link.url, "https://rust-lang.org");
        assert_eq!(
            link.title,
            Some("The Rust Programming Language".to_string())
        );
        assert!(link.is_external());
    }

    #[test]
    fn test_multiple_inline_links() {
        let extension = InlineLinkExtension::new();
        let content =
            r#"Check [Rust](https://rust-lang.org) and [GitHub](https://github.com) for more."#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 2);

        let link1 = &doc_content.inline_links[0];
        assert_eq!(link1.text, "Rust");
        assert_eq!(link1.url, "https://rust-lang.org");

        let link2 = &doc_content.inline_links[1];
        assert_eq!(link2.text, "GitHub");
        assert_eq!(link2.url, "https://github.com");
    }

    #[test]
    fn test_relative_links() {
        let extension = InlineLinkExtension::new();
        let content = "See [other note](./notes/other.md) for details.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 1);

        let link = &doc_content.inline_links[0];
        assert_eq!(link.text, "other note");
        assert_eq!(link.url, "./notes/other.md");
        assert!(link.is_relative());
        assert!(!link.is_external());
    }

    #[test]
    fn test_mixed_link_types() {
        let extension = InlineLinkExtension::new();
        let content = r#"
# Documentation

External: [Rust](https://rust-lang.org "Official Site")
Relative: [Guide](./guide.md)
Another: [API](https://docs.rs)
"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 3);

        // Verify external links
        let external_links: Vec<_> = doc_content
            .inline_links
            .iter()
            .filter(|l| l.is_external())
            .collect();
        assert_eq!(external_links.len(), 2);

        // Verify relative links
        let relative_links: Vec<_> = doc_content
            .inline_links
            .iter()
            .filter(|l| l.is_relative())
            .collect();
        assert_eq!(relative_links.len(), 1);
    }

    #[test]
    fn test_link_offset_tracking() {
        let extension = InlineLinkExtension::new();
        let content = "Start [first](url1) middle [second](url2) end";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 2);

        // First link should appear before second
        assert!(doc_content.inline_links[0].offset < doc_content.inline_links[1].offset);
    }

    #[test]
    fn test_empty_content() {
        let extension = InlineLinkExtension::new();
        let content = "";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 0);
    }

    #[test]
    fn test_no_links() {
        let extension = InlineLinkExtension::new();
        let content = "This is plain text with [[wikilinks]] but no inline links.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.inline_links.len(), 0);
    }
}
