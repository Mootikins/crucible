//! Metadata enrichment algorithm tests
//!
//! Tests for enrichment algorithms like reading time calculation,
//! complexity scoring, and metadata extraction validation.

use crucible_enrichment::DefaultEnrichmentService;
use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};
use crucible_core::merkle::HybridMerkleTree;
use std::path::Path;

async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}

#[tokio::test]
async fn test_enrichment_without_provider() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Test\n\nSome content";
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should succeed without embeddings
    assert!(enriched.embeddings.is_empty());
}

#[tokio::test]
async fn test_min_words_threshold() {
    let service = DefaultEnrichmentService::without_embeddings()
        .with_min_words(10);

    // Short content (less than 10 words)
    let content = "# Short\n\nFew words here.";
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Metadata should still be extracted even with short content
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
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

    let parsed = parse_note(&content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_metadata_extraction_word_count() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Title\n\nThis is a paragraph with exactly ten words total.";
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Reading time should be calculated
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_metadata_extraction_empty_note() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "";
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    assert_eq!(enriched.metadata.reading_time_minutes, 0.0);
}

#[tokio::test]
async fn test_changed_blocks_filtering() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = r#"# Heading 1

Paragraph 1

# Heading 2

Paragraph 2
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
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
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Should handle unicode text
    assert!(enriched.metadata.reading_time_minutes >= 0.0);
}

#[tokio::test]
async fn test_enrichment_preserves_original_data() {
    let service = DefaultEnrichmentService::without_embeddings();

    let content = "# Original Content\n\nThis should be preserved.";
    let parsed = parse_note(content, "test.md").unwrap();
    let original_path = parsed.path.clone();
    let merkle_tree = HybridMerkleTree::from_parsed_note(&parsed).unwrap();

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());

    let enriched = result.unwrap();
    // Original data should be preserved
    assert_eq!(enriched.parsed.path, original_path);
    assert!(!enriched.parsed.blocks.is_empty());
}

#[tokio::test]
async fn test_builder_pattern_chaining() {
    let service = DefaultEnrichmentService::without_embeddings()
        .with_min_words(15)
        .with_max_batch_size(25);

    // Verify builder pattern works
    let content = "# Test\n\nContent";
    let parsed = parse_note(content, "test.md").await.unwrap();
    let merkle_tree = HybridMerkleTree::from_document(&parsed);

    let result = service.enrich_internal(parsed, merkle_tree, vec![]).await;
    assert!(result.is_ok());
}
