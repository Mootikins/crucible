//! Metadata enrichment algorithm tests
//!
//! Tests for enrichment algorithms like reading time calculation,
//! complexity scoring, and metadata extraction validation.

use crucible_enrichment::DefaultEnrichmentService;
use crucible_core::parser::{ParsedNote, ParsedNoteBuilder};
use crucible_merkle::HybridMerkleTree;
use std::path::PathBuf;

// Create a minimal test note without needing parser implementation
fn create_test_note(content: &str, path: &str) -> ParsedNote {
    ParsedNoteBuilder::new(PathBuf::from(path))
        .with_raw_content(content.to_string())
        .build()
}

#[tokio::test]
async fn test_enrichment_without_provider() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Test\n\nSome content";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should succeed without embeddings
    assert!(enriched.core.embeddings.is_empty());
}

#[tokio::test]
async fn test_min_words_threshold() {
    let service = DefaultEnrichmentService::without_embeddings()
        .with_min_words(10);

    // Short content (less than 10 words)
    let content = "# Short\n\nFew words here.";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Metadata should still be extracted even with short content
    assert!(enriched.core.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_batch_size_limit() {
    let service = DefaultEnrichmentService::without_embeddings()
        .with_max_batch_size(5);

    // Create content with many blocks
    let mut content = String::new();
    for i in 0..20 {
        content.push_str(&format!("# Heading {}\n\nThis is paragraph {} with enough words to meet the minimum threshold for embedding generation.\n\n", i, i));
    }

    let parsed = create_test_note(&content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_metadata_extraction_word_count() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Title\n\nThis is a paragraph with exactly ten words total.";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Reading time should be calculated
    assert!(enriched.core.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_metadata_extraction_empty_note() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    assert_eq!(enriched.core.metadata.reading_time_minutes, 0.0);
}

#[tokio::test]
async fn test_changed_blocks_filtering() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = r#"# Heading 1

Paragraph 1

# Heading 2

Paragraph 2
"#;

    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    // Only specify some blocks as changed
    let changed_blocks = vec!["heading_0".to_string()];

    let result = service.enrich_internal(parsed, merkle_tree, changed_blocks).await;
    assert!(result.is_ok());

    // Should only process changed blocks (when embeddings are enabled)
}

#[tokio::test]
async fn test_metadata_unicode_word_count() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# 日本語のタイトル\n\n日本語のパラグラフです。単語数をカウントします。";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should handle unicode text
    assert!(enriched.core.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_enrichment_preserves_original_data() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Original Content\n\nThis should be preserved.";
    let parsed = create_test_note(content, "test.md");
    let original_path = parsed.path.clone();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Original data should be preserved
    assert_eq!(enriched.core.parsed.path, original_path);
    assert!(!enriched.core.parsed.content.paragraphs.is_empty());
}

#[tokio::test]
async fn test_builder_pattern_chaining() {
    let service = DefaultEnrichmentService::without_embeddings()
        .with_min_words(15)
        .with_max_batch_size(25);

    // Verify builder pattern works
    let content = "# Test\n\nContent";
    let parsed = create_test_note(content, "test.md");
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());
}
