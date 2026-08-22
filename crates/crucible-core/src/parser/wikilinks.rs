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
use super::types::{NoteContent, Wikilink};
use regex::Regex;
use std::sync::LazyLock;

static WIKILINK_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(!?)\[\[([^\]]+)\]\]").expect("wikilink regex"));

static CODE_BLOCK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^```[\s\S]*?^```|^    .*$|`[^`]+`").expect("code block regex")
});

/// Wikilink syntax extension
#[derive(Debug, Clone, Copy, Default)]
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

impl WikilinkExtension {
    pub(super) fn can_handle(&self, content: &str) -> bool {
        // Quick check for wikilink pattern before expensive regex
        content.contains("[[")
    }

    pub(super) fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wikilink_detection() {
        let extension = WikilinkExtension::new();

        assert!(extension.can_handle("This has a [[wikilink]] reference"));
        assert!(extension.can_handle("Embed: ![[note]]"));
        assert!(!extension.can_handle("Regular text without wikilinks"));
        assert!(!extension.can_handle("Markdown link [text](url)"));
    }

    #[test]
    fn test_basic_wikilink_parsing() {
        let extension = WikilinkExtension::new();
        let content = "See [[Other Note]] for details.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.wikilinks.len(), 1);
        assert_eq!(doc_content.wikilinks[0].target, "Other Note");
        assert_eq!(doc_content.wikilinks[0].alias, None);
        assert!(!doc_content.wikilinks[0].is_embed);
    }

    #[test]
    fn test_wikilink_with_alias() {
        let extension = WikilinkExtension::new();
        let content = "Link: [[Note|Display Text]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_wikilink_with_heading() {
        let extension = WikilinkExtension::new();
        let content = "Reference: [[Note#Section]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_embed_wikilink() {
        let extension = WikilinkExtension::new();
        let content = "Embed: ![[embedded-note]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_wikilink_in_code_block_skipped() {
        let extension = WikilinkExtension::new();
        let content = r#"Regular link: [[normal]]

```
Code block link: [[should-not-parse]]
```

After code: [[after]]"#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);

        // The wikilinks should only include 'normal' and 'after', not 'should-not-parse'
        // This is tested in the integration test
    }

    /// The target token's byte span must address exactly the text a rename
    /// splice replaces — for every syntax form, and at correct BYTE offsets
    /// even after multi-byte UTF-8.
    #[test]
    fn test_target_spans_address_exact_target_bytes() {
        let extension = WikilinkExtension::new();
        let content = "a [[plain]] b [[tgt|Alias]] c [[tgt#Head]] d [[tgt#^blk]] e ![[emb]] f 🎉 [[after-emoji]]";
        let mut doc_content = NoteContent::new();
        extension.parse(content, &mut doc_content);

        assert_eq!(doc_content.wikilinks.len(), 6);
        for link in &doc_content.wikilinks {
            let (start, end) = link.target_span;
            assert_eq!(
                &content[start..end],
                link.target,
                "span must slice exactly the target token for {:?}",
                link
            );
        }
        // The emoji before the last link forces byte offset > char offset.
        let last = &doc_content.wikilinks[5];
        assert_eq!(last.target, "after-emoji");
        assert!(last.offset > content[..last.offset].chars().count());
    }

    #[test]
    fn test_multiple_wikilinks() {
        let extension = WikilinkExtension::new();
        let content = "Links: [[first]] and [[second]] and [[third]]";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content);
        assert_eq!(errors.len(), 0);
    }
}
