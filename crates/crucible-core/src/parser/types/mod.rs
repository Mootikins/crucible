//! Core data types for parsed markdown notes
//!
//! # Type Ownership (DEPENDENCY INVERSION)
//!
//! This module contains the **canonical definitions** of all parser-related types.
//! This is the single source of truth for parser types in the Crucible system.
//!
//! ## Canonical Location
//!
//! - **Parser Types**: This module (`crate::parser::types`)
//! - **Hash Types**: This module (`BlockHash`)
//! - **AST Types**: This module (parser implementation detail)
//!
//! ## Import Guidelines
//!
//! Import from the canonical location `crate::parser` or
//! `crate::parser::types`.

mod block_hash;
mod blocks;
mod callout;
mod content;
mod frontmatter;
mod inline_metadata;
mod links;
mod lists;
mod parsed_note;
mod task;
mod workflow;

// Re-export ParseError from parser error module
pub use crate::parser::error::ParseError;

// Re-export all types for public API compatibility
pub use block_hash::BlockHash;
pub use blocks::{Block, BlockKind, Blockquote, HorizontalRule, Table};
pub use callout::{Callout, CalloutType, LatexExpression};
pub use content::{CodeBlock, Heading, NoteContent, Paragraph};
pub use frontmatter::{Frontmatter, FrontmatterFormat};
pub use inline_metadata::{extract_inline_metadata, InlineMetadata};
pub use links::{FootnoteDefinition, FootnoteMap, FootnoteReference, InlineLink, Tag, Wikilink};
pub use lists::{
    CheckboxStatus, ListBlock, ListItem, ListMarkerStyle, ListStats, ListType, TaskStatus,
};
pub use parsed_note::{ParsedNote, ParsedNoteBuilder, ParsedNoteMetadata};
pub use task::{TaskFile, TaskGraph, TaskItem};
pub use workflow::{
    extract_yaml_frontmatter, Gate, ValidationEntry, WorkflowDoc, WorkflowParseWarning,
    WorkflowStep,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wikilink_parse() {
        let link = Wikilink::parse("Note A", 10, false);
        assert_eq!(link.target, "Note A");
        assert_eq!(link.alias, None);
        assert!(!link.is_embed);

        let link = Wikilink::parse("Note B|My Alias", 20, false);
        assert_eq!(link.target, "Note B");
        assert_eq!(link.alias, Some("My Alias".to_string()));

        let link = Wikilink::parse("Note#heading", 30, false);
        assert_eq!(link.target, "Note");
        assert_eq!(link.heading_ref, Some("heading".to_string()));

        let link = Wikilink::parse("Note#^block", 40, false);
        assert_eq!(link.target, "Note");
        assert_eq!(link.block_ref, Some("block".to_string()));
    }

    #[test]
    fn test_tag_nested() {
        let tag = Tag::new("project/ai/llm", 10);
        assert_eq!(tag.path.len(), 3);
        assert_eq!(tag.root(), "project");
        assert_eq!(tag.leaf(), "llm");
        assert!(tag.is_nested());
        assert_eq!(tag.parent(), Some("project/ai".to_string()));
    }

    #[test]
    fn test_frontmatter_yaml() {
        let yaml = "title: Test Note\ntags: [ai, rust]";
        let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

        assert_eq!(fm.get_string("title"), Some("Test Note".to_string()));
        assert_eq!(
            fm.get_array("tags"),
            Some(vec!["ai".to_string(), "rust".to_string()])
        );
    }

    #[test]
    fn test_heading_id_generation() {
        let heading = Heading::new(1, "Hello World!", 0);
        assert_eq!(heading.id, Some("hello-world".to_string()));

        let heading = Heading::new(2, "API Reference (v2)", 10);
        assert_eq!(heading.id, Some("api-reference-v2".to_string()));
    }

    #[test]
    fn test_document_content_word_count() {
        let content = NoteContent::new().with_plain_text("Hello world test".to_string());
        assert_eq!(content.word_count, 3);
        assert_eq!(content.char_count, 16);
    }

    #[test]
    fn test_parsed_note_all_tags() {
        let mut doc = ParsedNote::new(PathBuf::from("test.md"));
        doc.tags = vec![Tag::new("rust", 0), Tag::new("ai", 10)];

        let yaml = "tags: [project, parsing]";
        doc.frontmatter = Some(Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml));

        let all_tags = doc.all_tags();
        assert_eq!(all_tags.len(), 4);
        assert!(all_tags.contains(&"rust".to_string()));
        assert!(all_tags.contains(&"project".to_string()));
    }
}
