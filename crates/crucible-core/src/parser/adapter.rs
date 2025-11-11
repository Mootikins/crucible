//! Adapter to convert parsed documents into SurrealDB records
//!
//! This module provides functionality to transform ParsedNote instances
//! into SurrealDB-compatible data structures for indexing.

use crucible_parser::types::ParsedNote;
use anyhow::Result;
use serde_json::{Map, Value};

/// Adapter for converting parsed documents to SurrealDB records
pub struct SurrealDBAdapter {
    /// Whether to extract full content or just excerpts
    include_full_content: bool,
    /// Maximum content length for excerpts (bytes)
    max_excerpt_length: usize,
}

impl SurrealDBAdapter {
    /// Create a new adapter with default settings
    pub fn new() -> Self {
        Self {
            include_full_content: false,
            max_excerpt_length: 1000,
        }
    }

    /// Create adapter that includes full content
    pub fn with_full_content(mut self) -> Self {
        self.include_full_content = true;
        self
    }

    /// Set maximum excerpt length
    pub fn with_max_excerpt(mut self, max_length: usize) -> Self {
        self.max_excerpt_length = max_length;
        self
    }

    /// Convert a parsed note into a SurrealDB note record
    ///
    /// Returns a JSON object suitable for insertion into the `notes` table
    pub fn to_note_record(&self, doc: &ParsedNote) -> Result<Value> {
        let mut record = Map::new();

        // Path: string representation of PathBuf
        record.insert(
            "path".to_string(),
            Value::String(doc.path.to_string_lossy().to_string()),
        );

        // Title: from frontmatter, first heading, or filename
        record.insert("title".to_string(), Value::String(doc.title()));

        // Word count
        record.insert(
            "word_count".to_string(),
            Value::Number(doc.content.word_count.into()),
        );

        // Content: full or truncated based on configuration
        let content = if self.include_full_content {
            doc.content.plain_text.clone()
        } else {
            // Extract excerpt: first paragraph or up to max_excerpt_length
            // Split on double newline to get first paragraph
            let first_paragraph = doc.content.plain_text.split("\n\n").next().unwrap_or("");

            // Truncate to max_excerpt_length bytes if needed
            let bytes = first_paragraph.as_bytes();
            if bytes.len() <= self.max_excerpt_length {
                first_paragraph.to_string()
            } else {
                // Find valid UTF-8 boundary at or before max_excerpt_length
                let mut end = self.max_excerpt_length;
                while end > 0 && !first_paragraph.is_char_boundary(end) {
                    end -= 1;
                }
                first_paragraph[..end].to_string()
            }
        };
        record.insert("content".to_string(), Value::String(content));

        // Metadata: frontmatter properties excluding "title" and "tags"
        let mut metadata = Map::new();
        if let Some(frontmatter) = &doc.frontmatter {
            for (key, value) in frontmatter.properties() {
                if key != "title" && key != "tags" {
                    metadata.insert(key.clone(), value.clone());
                }
            }
        }
        record.insert("metadata".to_string(), Value::Object(metadata));

        // Tags: combined from frontmatter and inline, deduplicated and sorted
        let all_tags: Vec<Value> = doc.all_tags().into_iter().map(Value::String).collect();
        record.insert("tags".to_string(), Value::Array(all_tags));

        Ok(Value::Object(record))
    }

    /// Extract wikilink edges from a note
    ///
    /// Returns a vec of (source_path, target_path, context) tuples
    pub fn to_wikilink_edges(&self, doc: &ParsedNote) -> Result<Vec<(String, String, String)>> {
        let source_path = doc.path.to_string_lossy().to_string();
        let mut edges = Vec::new();

        for wikilink in &doc.wikilinks {
            // Extract surrounding context (~50 chars on each side)
            let context = self.extract_context(&doc.content.plain_text, wikilink.offset, 50);

            edges.push((source_path.clone(), wikilink.target.clone(), context));
        }

        Ok(edges)
    }

    /// Extract tag associations from a note
    ///
    /// Returns a vec of (note_path, tag_name) tuples
    pub fn to_tag_associations(&self, doc: &ParsedNote) -> Result<Vec<(String, String)>> {
        let note_path = doc.path.to_string_lossy().to_string();
        let mut associations = Vec::new();

        for tag in doc.all_tags() {
            associations.push((note_path.clone(), tag));
        }

        Ok(associations)
    }

    /// Extract context around a given offset in the text
    ///
    /// Returns a substring centered on the offset, up to max_chars on each side
    fn extract_context(&self, text: &str, offset: usize, max_chars: usize) -> String {
        let text_len = text.len();

        // Find start position (max_chars before offset, or start of text)
        let start = offset.saturating_sub(max_chars);

        // Find end position (max_chars after offset, or end of text)
        let end = std::cmp::min(offset + max_chars, text_len);

        // Ensure we're on valid UTF-8 boundaries
        let mut actual_start = start;
        while actual_start > 0 && !text.is_char_boundary(actual_start) {
            actual_start -= 1;
        }

        let mut actual_end = end;
        while actual_end < text_len && !text.is_char_boundary(actual_end) {
            actual_end += 1;
        }

        text[actual_start..actual_end].to_string()
    }
}

impl Default for SurrealDBAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_parser::types::{
        NoteContent, FootnoteMap, Frontmatter, FrontmatterFormat, Heading, Tag, Wikilink,
    };

    fn create_test_document() -> ParsedNote {
        use chrono::Utc;
        use std::path::PathBuf;

        // Create frontmatter using the raw string format
        let frontmatter_raw = "title: Test Note\ntags: [project, ai]\nstatus: active".to_string();

        ParsedNote {
            path: PathBuf::from("Projects/test.md"),
            frontmatter: Some(Frontmatter::new(frontmatter_raw, FrontmatterFormat::Yaml)),
            content: NoteContent {
                plain_text: "Test\n\nThis is a test note with link and tag.".to_string(),
                word_count: 10,
                char_count: 47,
                paragraphs: vec![],
                lists: vec![],
                inline_links: Vec::new(),
                headings: vec![Heading {
                    level: 1,
                    text: "Test".to_string(),
                    offset: 0,
                    id: Some("test".to_string()),
                }],
                code_blocks: vec![],
                latex_expressions: vec![],
                callouts: vec![],
                blockquotes: vec![],
                footnotes: FootnoteMap::new(),
                tables: vec![],
                horizontal_rules: vec![],
            },
            wikilinks: vec![Wikilink {
                target: "link".to_string(),
                alias: None,
                offset: 37,
                heading_ref: None,
                block_ref: None,
                is_embed: false,
            }],
            tags: vec![Tag {
                name: "tag".to_string(),
                path: vec!["tag".to_string()],
                offset: 48,
            }],
            inline_links: Vec::new(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: Utc::now(),
            content_hash: "test_hash_123".to_string(),
            file_size: 100,
            parse_errors: Vec::new(),
            block_hashes: vec![],
            merkle_root: None,
        }
    }

    #[test]
    fn test_note_record_basic_fields() {
        let adapter = SurrealDBAdapter::new();
        let doc = create_test_document();

        let record = adapter
            .to_note_record(&doc)
            .expect("Should convert to record");

        // Check basic fields
        assert_eq!(record["path"], "Projects/test.md");
        assert_eq!(record["title"], "Test Note");
        assert_eq!(record["word_count"], 10);

        // Should NOT include full content by default
        assert!(!record["content"]
            .as_str()
            .unwrap()
            .contains("This is a test note"));
        assert!(record["content"].as_str().unwrap().len() <= 1000);
    }

    #[test]
    fn test_note_record_with_full_content() {
        let adapter = SurrealDBAdapter::new().with_full_content();
        let doc = create_test_document();

        let record = adapter
            .to_note_record(&doc)
            .expect("Should convert to record");

        // Should include full plain_text content when configured
        assert_eq!(
            record["content"],
            "Test\n\nThis is a test note with link and tag."
        );
    }

    #[test]
    fn test_note_record_frontmatter() {
        let adapter = SurrealDBAdapter::new();
        let doc = create_test_document();

        let record = adapter
            .to_note_record(&doc)
            .expect("Should convert to record");

        // Check frontmatter extraction
        let metadata = record["metadata"]
            .as_object()
            .expect("Should have metadata");
        assert_eq!(metadata["status"], "active");

        // Tags from frontmatter should be in tags array
        let tags = record["tags"].as_array().expect("Should have tags array");
        assert!(tags.contains(&Value::String("project".to_string())));
        assert!(tags.contains(&Value::String("ai".to_string())));
    }

    #[test]
    fn test_wikilink_edges_extraction() {
        let adapter = SurrealDBAdapter::new();
        let doc = create_test_document();

        let edges = adapter
            .to_wikilink_edges(&doc)
            .expect("Should extract edges");

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].0, "Projects/test.md"); // source
        assert_eq!(edges[0].1, "link"); // target
        assert!(edges[0].2.contains("test note")); // context
    }

    #[test]
    fn test_tag_associations_extraction() {
        let adapter = SurrealDBAdapter::new();
        let doc = create_test_document();

        let associations = adapter
            .to_tag_associations(&doc)
            .expect("Should extract tags");

        // Should have tags from both frontmatter and content
        assert_eq!(associations.len(), 3); // project, ai, tag

        assert!(associations.contains(&("Projects/test.md".to_string(), "project".to_string())));
        assert!(associations.contains(&("Projects/test.md".to_string(), "ai".to_string())));
        assert!(associations.contains(&("Projects/test.md".to_string(), "tag".to_string())));
    }

    #[test]
    fn test_excerpt_length_limit() {
        let adapter = SurrealDBAdapter::new().with_max_excerpt(50);

        let mut doc = create_test_document();
        doc.content.plain_text = "a".repeat(200);
        doc.content.char_count = 200;

        let record = adapter
            .to_note_record(&doc)
            .expect("Should convert to record");

        // Content should be truncated to max excerpt length
        assert!(record["content"].as_str().unwrap().len() <= 50);
    }

    #[test]
    fn test_no_frontmatter_document() {
        use chrono::Utc;
        use std::path::PathBuf;

        let adapter = SurrealDBAdapter::new();

        let doc = ParsedNote {
            path: PathBuf::from("simple.md"),
            frontmatter: None,
            content: NoteContent {
                plain_text: "Simple content".to_string(),
                word_count: 2,
                char_count: 14,
                headings: vec![],
                code_blocks: vec![],
                paragraphs: vec![],
                lists: vec![],
                inline_links: Vec::new(),
                latex_expressions: vec![],
                callouts: vec![],
                blockquotes: vec![],
                footnotes: FootnoteMap::new(),
                tables: vec![],
                horizontal_rules: vec![],
            },
            wikilinks: vec![],
            tags: vec![],
            inline_links: Vec::new(),
            callouts: Vec::new(),
            latex_expressions: Vec::new(),
            footnotes: FootnoteMap::new(),
            parsed_at: Utc::now(),
            content_hash: "simple_hash".to_string(),
            file_size: 14,
            parse_errors: Vec::new(),
            block_hashes: vec![],
            merkle_root: None,
        };

        let record = adapter
            .to_note_record(&doc)
            .expect("Should handle no frontmatter");

        assert_eq!(record["path"], "simple.md");
        assert!(record["metadata"].as_object().unwrap().is_empty());
        assert_eq!(record["tags"].as_array().unwrap().len(), 0);
    }
}
