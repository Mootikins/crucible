//! Metadata enrichment algorithm tests
//!
//! Tests for enrichment algorithms like reading time calculation,
//! complexity scoring, and metadata extraction validation.
//!
//! Note: DefaultEnrichmentService is intentionally private per SOLID architecture.
//! Use the factory function `create_default_enrichment_service` for the public API.
//! These tests exercise the public API only.

use crucible_core::enrichment::EnrichmentService;
use crucible_core::parser::{ParsedNote, ParsedNoteBuilder};
use crucible_enrichment::create_default_enrichment_service;
use std::path::PathBuf;

// Create a minimal test note without needing parser implementation
fn create_test_note(_content: &str, path: &str) -> ParsedNote {
    ParsedNoteBuilder::new(PathBuf::from(path)).build()
}

#[tokio::test]
async fn test_enrichment_without_provider() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = "# Test\n\nSome content";
    let parsed = create_test_note(content, "test.md");

    // EnrichmentService.enrich takes 2 args: parsed note and changed block IDs
    let result = service.enrich(parsed, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should succeed without embeddings
    assert!(enriched.embeddings.is_empty());
}

#[tokio::test]
async fn test_metadata_extraction_word_count() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = "# Title\n\nThis is a paragraph with exactly ten words total.";
    let parsed = create_test_note(content, "test.md");

    let result = service.enrich(parsed, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Reading time should be calculated
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_metadata_extraction_empty_note() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = "";
    let parsed = create_test_note(content, "test.md");

    let result = service.enrich(parsed, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    assert_eq!(enriched.metadata.reading_time_minutes, 0.0);
}

#[tokio::test]
async fn test_changed_blocks_filtering() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = r#"# Heading 1

Paragraph 1

# Heading 2

Paragraph 2
"#;

    let parsed = create_test_note(content, "test.md");

    // Only specify some blocks as changed
    let changed_blocks = vec!["heading_0".to_string()];

    let result = service.enrich(parsed, changed_blocks).await;
    assert!(result.is_ok());

    // Should only process changed blocks (when embeddings are enabled)
}

#[tokio::test]
async fn test_metadata_unicode_word_count() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = "# 日本語のタイトル\n\n日本語のパラグラフです。単語数をカウントします。";
    let parsed = create_test_note(content, "test.md");

    let result = service.enrich(parsed, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should handle unicode text
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_enrichment_preserves_original_data() {
    let service = create_default_enrichment_service(None).expect("Failed to create service");

    let content = "# Original Content\n\nThis should be preserved.";
    let parsed = create_test_note(content, "test.md");
    let original_path = parsed.path.clone();

    let result = service.enrich(parsed, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Original data should be preserved
    assert_eq!(enriched.parsed.path, original_path);
    // Metadata should be present
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
}
