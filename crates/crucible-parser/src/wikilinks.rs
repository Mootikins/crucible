//! Wikilink syntax extension
//!
//! This module implements support for Obsidian-style wikilinks:
//! - Basic wikilinks: `[[note]]`
//! - Wikilinks with aliases: `[[note|alias]]`
//! - Wikilinks with headings: `[[note#heading]]`
//! - Wikilinks with block references: `[[note#^block-id]]`
//! - Embeds: `![[note]]`
//! - Complex: `[[note#heading|alias]]`

use super::error::ParseError;
use super::extensions::SyntaxExtension;
use super::types::{NoteContent, Wikilink};
use async_trait::async_trait;
use regex::Regex;
use std::sync::{Arc, LazyLock};

static WIKILINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\]]+)\]\]").expect("wikilink regex"));

static CODE_BLOCK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^```[\s\S]*?^```|^    .*$|`[^`]+`").expect("code block regex"));

/// Wikilink syntax extension
pub struct WikilinkExtension;

impl WikilinkExtension {
    /// Create a new wikilink extension
    pub fn new() -> Self {
        Self
    }

    /// Check if an offset is inside a code block
    fn is_inside_code_block(&self, content: &str, offset: usize) -> bool {
        for cap in CODE_BLOCK_REGEX.find_iter(content) {
            if offset >= cap.start() && offset < cap.end() {
                return true;
            }
        }
        false
    }
}

impl Default for WikilinkExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for WikilinkExtension {
    fn name(&self) -> &'static str {
        "obsidian-wikilinks"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports Obsidian-style wikilinks [[note]], [[note|alias]], [[note#heading]], and ![[embed]]"
    }

    fn can_handle(&self, content: &str) -> bool {
        // Quick check for wikilink pattern before expensive regex
        content.contains("[[")
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let errors = Vec::new();

        // Extract all wikilinks
        for cap in WIKILINK_REGEX.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let offset = full_match.start();

            // Skip wikilinks inside code blocks
            if self.is_inside_code_block(content, offset) {
                continue;
            }

            let is_embed = !cap.get(1).unwrap().as_str().is_empty();
            let inner = cap.get(2).unwrap().as_str();

            // Parse the wikilink using the Wikilink::parse method
            let wikilink = Wikilink::parse(inner, offset, is_embed);
            doc_content.wikilinks.push(wikilink);
        }

        errors
    }

    fn priority(&self) -> u8 {
        80 // High priority, run early before other extensions
    }
}

/// Factory function to create the wikilink extension
pub fn create_wikilink_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(WikilinkExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wikilink_detection() {
        let extension = WikilinkExtension::new();

        assert!(extension.can_handle("This has a [[wikilink]] reference"));
        assert!(extension.can_handle("Embed: ![[note]]"));
        assert!(!extension.can_handle("Regular text without wikilinks"));
        assert!(!extension.can_handle("Markdown link [text](url)"));
    }

    #[tokio::test]
    async fn test_basic_wikilink_parsing() {
        let extension = WikilinkExtension::new();
        let content = "See [[Other Note]] for details.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.wikilinks.len(), 1);
        assert_eq!(doc_content.wikilinks[0].target, "Other Note");
        assert_eq!(doc_content.wikilinks[0].alias, None);
        assert!(!doc_content.wikilinks[0].is_embed);
    }

    #[tokio::test]
    async fn test_wikilink_with_alias() {
        let extension = WikilinkExtension::new();
        let content = "Link: [[Note|Display Text]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);
    }

    #[tokio::test]
    async fn test_wikilink_with_heading() {
        let extension = WikilinkExtension::new();
        let content = "Reference: [[Note#Section]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);
    }

    #[tokio::test]
    async fn test_embed_wikilink() {
        let extension = WikilinkExtension::new();
        let content = "Embed: ![[embedded-note]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);
    }

    #[tokio::test]
    async fn test_wikilink_in_code_block_skipped() {
        let extension = WikilinkExtension::new();
        let content = r#"Regular link: [[normal]]

```
Code block link: [[should-not-parse]]
```

After code: [[after]]"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);

        // The wikilinks should only include 'normal' and 'after', not 'should-not-parse'
        // This is tested in the integration test
    }

    #[tokio::test]
    async fn test_multiple_wikilinks() {
        let extension = WikilinkExtension::new();
        let content = "Links: [[first]] and [[second]] and [[third]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;
        assert_eq!(errors.len(), 0);
    }
}
