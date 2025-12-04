//! Test utilities for parser-related tests.
//!
//! This module provides shared test helpers for parsing notes in tests.
//! Enable with the `test-utils` feature.
//!
//! # Usage
//!
//! In `Cargo.toml`:
//! ```toml
//! [dev-dependencies]
//! crucible-parser = { path = "../crucible-parser", features = ["test-utils"] }
//! ```
//!
//! In tests:
//! ```rust,ignore
//! use crucible_parser::test_utils::parse_note;
//!
//! #[tokio::test]
//! async fn test_parsing() {
//!     let parsed = parse_note("# Hello", "test.md").await.unwrap();
//!     assert!(!parsed.headings.is_empty());
//! }
//! ```

use crate::{CrucibleParser, MarkdownParser, ParsedNote};
use std::path::Path;

/// Parse markdown content into a ParsedNote for testing.
///
/// This is a convenience wrapper around `CrucibleParser::parse_content` that
/// simplifies test code by handling parser creation and error boxing.
///
/// # Arguments
///
/// * `content` - The markdown content to parse
/// * `path` - The virtual file path (used for context in error messages)
///
/// # Returns
///
/// A `Result` containing the parsed note or a boxed error.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_parser::test_utils::parse_note;
///
/// #[tokio::test]
/// async fn test_wikilinks() {
///     let content = "Link to [[other-note]]";
///     let parsed = parse_note(content, "test.md").await.unwrap();
///     assert_eq!(parsed.wikilinks.len(), 1);
/// }
/// ```
pub async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_note_helper() {
        let content = "# Test Heading\n\nSome content with [[wikilink]].";
        let parsed = parse_note(content, "test.md").await.unwrap();

        assert!(!parsed.content.headings.is_empty());
        assert_eq!(parsed.content.headings[0].text, "Test Heading");
        assert_eq!(parsed.wikilinks.len(), 1);
        assert_eq!(parsed.wikilinks[0].target, "wikilink");
    }

    #[tokio::test]
    async fn test_parse_note_with_frontmatter() {
        let content = r#"---
title: Test Note
tags: [test, example]
---

# Content

Some text here.
"#;
        let parsed = parse_note(content, "note.md").await.unwrap();

        assert!(parsed.frontmatter.is_some());
        let frontmatter = parsed.frontmatter.unwrap();
        assert!(frontmatter.raw.contains("title: Test Note"));
    }
}
