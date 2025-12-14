//! Core data types for parsed markdown notes
//!
//! # Type Ownership (DEPENDENCY INVERSION)
//!
//! This module contains the **canonical definitions** of all parser-related types.
//! This is the single source of truth for parser types in the Crucible system.
//!
//! ## Canonical Location
//!
//! - **Parser Types**: This module (`crucible_core::parser::types`)
//! - **Hash Types**: This module (`BlockHash`)
//! - **AST Types**: This module (parser implementation detail)
//!
//! ## Import Guidelines
//!
//! Import from the canonical location `crucible_core::parser` or
//! `crucible_core::parser::types`. The `crucible_parser` crate re-exports
//! these types for implementation purposes.

mod ast;
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

// Re-export ParseError from parser error module
pub use crate::parser::error::ParseError;

// Re-export all types for public API compatibility
pub use ast::{ASTBlock, ASTBlockMetadata, ASTBlockType};
pub use block_hash::BlockHash;
pub use blocks::{Blockquote, HorizontalRule, Table};
pub use callout::{Callout, LatexExpression};
pub use content::{CodeBlock, Heading, NoteContent, Paragraph};
pub use frontmatter::{Frontmatter, FrontmatterFormat};
pub use inline_metadata::{extract_inline_metadata, InlineMetadata};
pub use links::{FootnoteDefinition, FootnoteMap, FootnoteReference, InlineLink, Tag, Wikilink};
pub use lists::{CheckboxStatus, ListBlock, ListItem, ListMarkerStyle, ListStats, ListType, TaskStatus};
pub use parsed_note::{ParsedNote, ParsedNoteBuilder, ParsedNoteMetadata};
pub use task::{TaskFile, TaskGraph, TaskItem};

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

    #[test]
    fn test_ast_block_creation() {
        let metadata = ASTBlockMetadata::heading(1, Some("test-heading".to_string()));
        let block = ASTBlock::new(
            ASTBlockType::Heading,
            "Test Heading".to_string(),
            0,
            12,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Heading);
        assert_eq!(block.content, "Test Heading");
        assert_eq!(block.start_offset, 0);
        assert_eq!(block.end_offset, 12);
        assert_eq!(block.length(), 12);
        assert_eq!(block.content_length(), 12);
        assert!(!block.is_empty());
        assert_eq!(block.type_name(), "heading");
        assert!(block.is_heading_level(1));
        assert!(!block.is_heading_level(2));
    }

    #[test]
    fn test_ast_block_code_creation() {
        let metadata = ASTBlockMetadata::code(Some("rust".to_string()), 3);
        let block = ASTBlock::new(
            ASTBlockType::Code,
            "let x = 42;".to_string(),
            20,
            31,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Code);
        assert_eq!(block.content, "let x = 42;");
        assert!(block.is_code_language("rust"));
        assert!(!block.is_code_language("python"));
        assert_eq!(block.type_name(), "code");
    }

    #[test]
    fn test_ast_block_callout_creation() {
        let metadata =
            ASTBlockMetadata::callout("note".to_string(), Some("Important Note".to_string()), true);
        let block = ASTBlock::new(
            ASTBlockType::Callout,
            "This is an important note".to_string(),
            50,
            78,
            metadata,
        );

        assert_eq!(block.block_type, ASTBlockType::Callout);
        assert!(block.is_callout_type("note"));
        assert!(!block.is_callout_type("warning"));
        assert_eq!(block.type_name(), "callout");
    }

    #[test]
    fn test_ast_block_hash_computation() {
        let content = "Test content";
        let metadata = ASTBlockMetadata::generic();
        let block1 = ASTBlock::new(
            ASTBlockType::Paragraph,
            content.to_string(),
            0,
            content.len(),
            metadata.clone(),
        );

        let block2 = ASTBlock::new(
            ASTBlockType::Paragraph,
            content.to_string(),
            10,
            10 + content.len(),
            metadata,
        );

        // Same content should produce same hash
        assert_eq!(block1.block_hash, block2.block_hash);
        assert_eq!(block1.block_hash.len(), 64); // BLAKE3 produces 32-byte hash = 64 hex chars
    }

    #[test]
    fn test_ast_block_with_explicit_hash() {
        let metadata = ASTBlockMetadata::generic();
        let block = ASTBlock::with_hash(
            ASTBlockType::Paragraph,
            "Test content".to_string(),
            0,
            12,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262".to_string(),
            metadata,
        );

        assert_eq!(
            block.block_hash,
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn test_ast_block_type_display() {
        let metadata = ASTBlockMetadata::generic();

        let paragraph = ASTBlock::new(
            ASTBlockType::Paragraph,
            "Test".to_string(),
            0,
            4,
            metadata.clone(),
        );
        assert_eq!(paragraph.type_name(), "paragraph");

        let heading = ASTBlock::new(
            ASTBlockType::Heading,
            "Test".to_string(),
            0,
            4,
            metadata.clone(),
        );
        assert_eq!(heading.type_name(), "heading");

        let code = ASTBlock::new(ASTBlockType::Code, "Test".to_string(), 0, 4, metadata);
        assert_eq!(code.type_name(), "code");
    }

    #[test]
    fn test_ast_block_metadata_creation() {
        // Test all metadata creation methods
        let heading_meta = ASTBlockMetadata::heading(2, Some("test".to_string()));
        if let ASTBlockMetadata::Heading { level, id } = heading_meta {
            assert_eq!(level, 2);
            assert_eq!(id, Some("test".to_string()));
        } else {
            panic!("Expected heading metadata");
        }

        let code_meta = ASTBlockMetadata::code(Some("rust".to_string()), 10);
        if let ASTBlockMetadata::Code {
            language,
            line_count,
        } = code_meta
        {
            assert_eq!(language, Some("rust".to_string()));
            assert_eq!(line_count, 10);
        } else {
            panic!("Expected code metadata");
        }

        let list_meta = ASTBlockMetadata::list(ListType::Ordered, 5);
        if let ASTBlockMetadata::List {
            list_type,
            item_count,
        } = list_meta
        {
            assert_eq!(list_type, ListType::Ordered);
            assert_eq!(item_count, 5);
        } else {
            panic!("Expected list metadata");
        }

        let callout_meta =
            ASTBlockMetadata::callout("warning".to_string(), Some("Watch out".to_string()), true);
        if let ASTBlockMetadata::Callout {
            callout_type,
            title,
            is_standard_type,
        } = callout_meta
        {
            assert_eq!(callout_type, "warning");
            assert_eq!(title, Some("Watch out".to_string()));
            assert!(is_standard_type);
        } else {
            panic!("Expected callout metadata");
        }

        let latex_meta = ASTBlockMetadata::latex(true);
        if let ASTBlockMetadata::Latex { is_block } = latex_meta {
            assert!(is_block);
        } else {
            panic!("Expected latex metadata");
        }

        let generic_meta = ASTBlockMetadata::generic();
        matches!(generic_meta, ASTBlockMetadata::Generic);
    }

    #[test]
    fn test_parsed_note_with_block_hashes() {
        let mut doc = ParsedNote::new(PathBuf::from("test.md"));

        // Create some test block hashes
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let hash2 = BlockHash::new([
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c,
            0x3d, 0x3e, 0x3f, 0x40,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        // Test adding block hashes
        doc.add_block_hash(hash1);
        doc.add_block_hash(hash2);

        assert_eq!(doc.block_hash_count(), 2);
        assert!(doc.has_block_hashes());
        assert_eq!(doc.block_hashes[0], hash1);
        assert_eq!(doc.block_hashes[1], hash2);

        // Test setting Merkle root
        doc = doc.with_merkle_root(Some(merkle_root));
        assert!(doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), Some(merkle_root));

        // Test clearing hash data
        doc.clear_hash_data();
        assert!(!doc.has_block_hashes());
        assert!(!doc.has_merkle_root());
        assert_eq!(doc.block_hash_count(), 0);
    }

    #[test]
    fn test_parsed_note_builder_with_hashes() {
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        let doc = ParsedNote::builder(PathBuf::from("test.md"))
            .with_block_hashes(vec![hash1])
            .with_merkle_root(Some(merkle_root))
            .build();

        assert!(doc.has_block_hashes());
        assert_eq!(doc.block_hash_count(), 1);
        assert_eq!(doc.block_hashes[0], hash1);
        assert!(doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), Some(merkle_root));
    }

    #[test]
    fn test_parsed_note_backward_compatibility() {
        use chrono::Utc;

        // Test that legacy constructor still works with empty hash fields
        let path = PathBuf::from("test.md");
        let frontmatter = None;
        let wikilinks = vec![];
        let tags = vec![];
        let content = NoteContent::new();
        let parsed_at = Utc::now();
        let content_hash = "test_hash".to_string();
        let file_size = 1024;

        let doc = ParsedNote::legacy(
            path.clone(),
            frontmatter,
            wikilinks,
            tags,
            content,
            parsed_at,
            content_hash.clone(),
            file_size,
        );

        // Legacy documents should have empty hash fields
        assert!(!doc.has_block_hashes());
        assert_eq!(doc.block_hash_count(), 0);
        assert!(!doc.has_merkle_root());
        assert_eq!(doc.get_merkle_root(), None);

        // But other fields should still work
        assert_eq!(doc.path, path);
        assert_eq!(doc.content_hash, content_hash);
        assert_eq!(doc.file_size, file_size);
    }

    #[test]
    fn test_parsed_note_serialization_with_hashes() {
        let hash1 = BlockHash::new([
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ]);

        let merkle_root = BlockHash::new([
            0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e,
            0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x5b, 0x5c,
            0x5d, 0x5e, 0x5f, 0x60,
        ]);

        let original_doc = ParsedNote::builder(PathBuf::from("test.md"))
            .with_block_hashes(vec![hash1])
            .with_merkle_root(Some(merkle_root))
            .build();

        // Test JSON serialization
        let json = serde_json::to_string(&original_doc).expect("Failed to serialize");
        let deserialized_doc: ParsedNote =
            serde_json::from_str(&json).expect("Failed to deserialize");

        // Verify the fields are preserved
        assert!(deserialized_doc.has_block_hashes());
        assert_eq!(deserialized_doc.block_hash_count(), 1);
        assert_eq!(deserialized_doc.block_hashes[0], hash1);
        assert!(deserialized_doc.has_merkle_root());
        assert_eq!(deserialized_doc.get_merkle_root(), Some(merkle_root));
        assert_eq!(deserialized_doc.path, original_doc.path);
    }
}
