//! Common test utilities for SurrealDB tests
//!
//! This module provides shared utilities, test data creators, and helpers
//! to reduce duplication across all SurrealDB test files.

use chrono::Utc;
use crucible_core::parser::{
    DocumentContent, Frontmatter, FrontmatterFormat, Heading, ParsedDocument, Tag,
};
use crucible_surrealdb::embedding_config::DocumentEmbedding;
use std::path::PathBuf;

/// Document creation utilities for testing
pub struct DocumentTestUtils;

impl DocumentTestUtils {
    /// Create a basic test ParsedDocument for testing
    pub fn create_basic_parsed_document() -> ParsedDocument {
        let mut doc = ParsedDocument::new(PathBuf::from("test.md"));
        doc.content.plain_text = "This is a test document content.".to_string();
        doc.content_hash = "test-hash-123".to_string();
        doc.file_size = 100;
        doc.parsed_at = Utc::now();

        // Add some tags
        doc.tags.push(Tag::new("test", 0));
        doc.tags.push(Tag::new("document", 5));

        doc
    }

    /// Create a test ParsedDocument with frontmatter
    pub fn create_parsed_document_with_frontmatter(
        path: &str,
        title: &str,
        tags: Vec<&str>,
    ) -> ParsedDocument {
        let mut doc = ParsedDocument::new(PathBuf::from(path));

        // Add frontmatter
        let frontmatter = Frontmatter::new(
            format!(
                r#"title: "{}"
tags: [{}]
author: "Test Author"
created: "2024-01-01""#,
                title,
                tags.iter()
                    .map(|tag| format!("\"{}\"", tag))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            FrontmatterFormat::Yaml,
        );
        doc.frontmatter = Some(frontmatter);

        // Add content
        doc.content = DocumentContent::new()
            .with_plain_text(format!("This is a test document with title: {}", title));
        doc.content.add_heading(Heading::new(1, title, 0));

        doc
    }

    /// Create a test ParsedDocument with wikilinks
    pub fn create_parsed_document_with_wikilinks(path: &str, links: Vec<&str>) -> ParsedDocument {
        let mut doc = ParsedDocument::new(PathBuf::from(path));

        let mut content = String::from("# Document with Links\n\n");
        for link in &links {
            content.push_str(&format!("See [[{}]] for more information.\n", link));
        }

        doc.content.plain_text = content.clone();
        doc.content_hash = format!("hash-{}", links.len());
        doc.file_size = content.len() as u64;
        doc.parsed_at = Utc::now();

        doc
    }

    /// Create a test ParsedDocument with tags
    pub fn create_parsed_document_with_tags(path: &str, tags: Vec<&str>) -> ParsedDocument {
        let mut doc = ParsedDocument::new(PathBuf::from(path));

        let mut content = String::from("# Document with Tags\n\n");
        for tag in &tags {
            content.push_str(&format!("This content is related to #{}.\n", tag));
            doc.tags.push(Tag::new(*tag, content.len() - tag.len() - 1));
        }

        doc.content.plain_text = content.clone();
        doc.content_hash = format!("hash-{}", tags.len());
        doc.file_size = content.len() as u64;
        doc.parsed_at = Utc::now();

        doc
    }

    /// Create a test ParsedDocument with embeds
    pub fn create_parsed_document_with_embeds(path: &str, embeds: Vec<&str>) -> ParsedDocument {
        let mut doc = ParsedDocument::new(PathBuf::from(path));

        let mut content = String::from("# Document with Embeds\n\n");
        for embed in &embeds {
            content.push_str(&format!("![[{}]]\n", embed));
        }

        doc.content.plain_text = content.clone();
        doc.content_hash = format!("hash-{}", embeds.len());
        doc.file_size = content.len() as u64;
        doc.parsed_at = Utc::now();

        doc
    }
}

/// Embedding creation utilities for testing
pub struct EmbeddingTestUtils;

impl EmbeddingTestUtils {
    /// Create a test DocumentEmbedding for a document
    pub fn create_document_embedding(document_id: &str, dimensions: usize) -> DocumentEmbedding {
        let vector: Vec<f32> = (0..dimensions)
            .map(|i| ((i as f32 * 0.1) % 1.0).cos())
            .collect();

        DocumentEmbedding {
            document_id: document_id.to_string(),
            chunk_id: None,
            vector,
            embedding_model: "test-model".to_string(),
            created_at: Utc::now(),
            chunk_size: dimensions,
            chunk_position: Some(0),
        }
    }

    /// Create a test DocumentEmbedding for a chunk
    pub fn create_chunk_embedding(
        document_id: &str,
        chunk_id: &str,
        position: usize,
        dimensions: usize,
    ) -> DocumentEmbedding {
        let vector: Vec<f32> = (0..dimensions)
            .map(|i| ((i as f32 * 0.2 + position as f32) % 1.0).sin())
            .collect();

        DocumentEmbedding {
            document_id: document_id.to_string(),
            chunk_id: Some(chunk_id.to_string()),
            vector,
            embedding_model: "test-model".to_string(),
            created_at: Utc::now(),
            chunk_size: dimensions,
            chunk_position: Some(position),
        }
    }

    /// Create a batch of test embeddings
    pub fn create_embedding_batch(
        document_ids: &[&str],
        embeddings_per_doc: usize,
        dimensions: usize,
    ) -> Vec<DocumentEmbedding> {
        let mut embeddings = Vec::new();

        for (doc_index, &document_id) in document_ids.iter().enumerate() {
            for chunk_index in 0..embeddings_per_doc {
                let position = doc_index * embeddings_per_doc + chunk_index;
                let embedding = Self::create_chunk_embedding(
                    document_id,
                    &format!("chunk-{}", chunk_index),
                    position,
                    dimensions,
                );
                embeddings.push(embedding);
            }
        }

        embeddings
    }

    /// Create a test embedding vector with deterministic seed
    pub fn create_test_embedding(seed: u32, dimensions: usize) -> Vec<f32> {
        (0..dimensions)
            .map(|i| {
                let base = (seed + i as u32) as f32;
                (base * 0.12345).sin() * 0.5 + 0.5
            })
            .collect()
    }
}

/// Assertion utilities for testing embeddings
pub struct EmbeddingAssertions;

impl EmbeddingAssertions {
    /// Assert that two embeddings are approximately equal within tolerance
    pub fn assert_embeddings_approx_eq(
        embedding1: &DocumentEmbedding,
        embedding2: &DocumentEmbedding,
        tolerance: f32,
    ) {
        assert_eq!(embedding1.document_id, embedding2.document_id);
        assert_eq!(embedding1.chunk_id, embedding2.chunk_id);
        assert_eq!(embedding1.chunk_position, embedding2.chunk_position);
        assert_eq!(embedding1.vector.len(), embedding2.vector.len());

        for (i, (v1, v2)) in embedding1
            .vector
            .iter()
            .zip(embedding2.vector.iter())
            .enumerate()
        {
            assert!(
                (v1 - v2).abs() < tolerance,
                "Vector values at index {} differ by more than tolerance {}: {} vs {}",
                i,
                tolerance,
                v1,
                v2
            );
        }
    }
}
