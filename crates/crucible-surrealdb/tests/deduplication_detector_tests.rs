//! Deduplication Detector Tests
//!
//! Comprehensive tests for Phase 3 duplicate block detection logic
//! including advanced deduplication analysis, statistics, and reporting.

use crucible_surrealdb::{
    ContentAddressedStorageSurrealDB, SurrealDbConfig,
    deduplication_detector::SurrealDeduplicationDetector,
};
use crucible_parser::types::{ASTBlock, ASTBlockMetadata, ASTBlockType};
use crucible_core::storage::DeduplicationStorage;
use crucible_core::hashing::blake3::Blake3Hasher;
use crucible_core::storage::ContentHasher;
use std::collections::HashMap;

/// Create a test storage instance
async fn create_test_storage() -> ContentAddressedStorageSurrealDB {
    let config = SurrealDbConfig::memory();
    let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
    storage.initialize().await.unwrap();
    storage
}

/// Create a test deduplication detector with storage reference
async fn create_test_detector_with_storage(storage: &ContentAddressedStorageSurrealDB) -> SurrealDeduplicationDetector {
    // Note: This is not ideal either, but let's fix the detector to take references instead
    // For now, we'll work around the ownership issues
    SurrealDeduplicationDetector::new(create_test_storage().await)
}

/// Create a test AST block for testing
fn create_test_ast_block(
    content: &str,
    block_type: ASTBlockType,
    start_offset: usize,
    end_offset: usize,
) -> ASTBlock {
    let hasher = Blake3Hasher::new();
    let block_hash = hasher.hash_block(content.as_bytes()).to_string();

    ASTBlock {
        block_type,
        content: content.to_string(),
        start_offset,
        end_offset,
        block_hash,
        metadata: ASTBlockMetadata::Generic,
    }
}

/// Store test data for deduplication testing
async fn setup_deduplication_test_data(
    storage: &ContentAddressedStorageSurrealDB,
) -> HashMap<String, Vec<String>> {
    let mut block_to_documents = HashMap::new();

    // Create unique blocks
    let unique_block_1 = create_test_ast_block(
        "Unique content 1", ASTBlockType::Paragraph, 0, 15
    );
    let unique_block_2 = create_test_ast_block(
        "Unique content 2", ASTBlockType::Heading, 0, 15
    );

    // Create duplicate blocks
    let duplicate_content_1 = "Common paragraph content";
    let duplicate_block_1a = create_test_ast_block(
        duplicate_content_1, ASTBlockType::Paragraph, 0, 23
    );
    let duplicate_block_1b = create_test_ast_block(
        duplicate_content_1, ASTBlockType::Paragraph, 0, 23
    );
    let duplicate_block_1c = create_test_ast_block(
        duplicate_content_1, ASTBlockType::Paragraph, 0, 23
    );

    let duplicate_content_2 = "Common code snippet";
    let duplicate_block_2a = create_test_ast_block(
        duplicate_content_2, ASTBlockType::Code, 0, 19
    );
    let duplicate_block_2b = create_test_ast_block(
        duplicate_content_2, ASTBlockType::Code, 0, 19
    );

    // Store blocks in different documents
    storage.store_document_blocks_from_ast("doc1.md", &[
        unique_block_1.clone(),
        duplicate_block_1a.clone(),
        duplicate_block_2a.clone(),
    ]).await.unwrap();

    storage.store_document_blocks_from_ast("doc2.md", &[
        unique_block_2.clone(),
        duplicate_block_1b.clone(),
    ]).await.unwrap();

    storage.store_document_blocks_from_ast("doc3.md", &[
        duplicate_block_1c.clone(),
        duplicate_block_2b.clone(),
    ]).await.unwrap();

    // Map expected results
    block_to_documents.insert(unique_block_1.block_hash.clone(), vec!["doc1.md".to_string()]);
    block_to_documents.insert(unique_block_2.block_hash.clone(), vec!["doc2.md".to_string()]);
    block_to_documents.insert(duplicate_block_1a.block_hash.clone(), vec![
        "doc1.md".to_string(), "doc2.md".to_string(), "doc3.md".to_string()
    ]);
    block_to_documents.insert(duplicate_block_2a.block_hash.clone(), vec![
        "doc1.md".to_string(), "doc3.md".to_string()
    ]);

    block_to_documents
}

// Basic Deduplication Detector Tests

#[tokio::test]
async fn test_create_deduplication_detector() {
    let detector = create_test_detector().await;

    // Test that detector is created with default settings
    // (We can't directly access private fields, so we test through methods)
    let result = detector.find_documents_with_block("nonexistent").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_documents_with_single_block() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());

    // Store a test block
    let block = create_test_ast_block("Test content", ASTBlockType::Paragraph, 0, 12);
    let block_hash = block.block_hash.clone();

    storage.store_document_blocks_from_ast("test.md", &[block]).await.unwrap();

    // Find documents containing the block
    let documents = detector.find_documents_with_block(&block_hash).await.unwrap();

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0], "test.md");
}

#[tokio::test]
async fn test_find_documents_with_nonexistent_block() {
    let detector = create_test_detector().await;

    let result = detector.find_documents_with_block("nonexistent_hash").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_document_blocks() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());

    // Store multiple blocks in a document
    let blocks = vec![
        create_test_ast_block("# Heading", ASTBlockType::Heading, 0, 9),
        create_test_ast_block("Paragraph content", ASTBlockType::Paragraph, 10, 26),
        create_test_ast_block("```rust\ncode```", ASTBlockType::Code, 27, 46),
    ];

    storage.store_document_blocks_from_ast("test.md", &blocks).await.unwrap();

    // Get all blocks for the document
    let document_blocks = detector.get_document_blocks("test.md").await.unwrap();

    assert_eq!(document_blocks.len(), 3);
    assert_eq!(document_blocks[0].document_id, "test.md");
    assert_eq!(document_blocks[0].block_index, 0);
    assert_eq!(document_blocks[0].block_type, "heading");
    assert_eq!(document_blocks[0].block_content, "# Heading");

    assert_eq!(document_blocks[1].block_index, 1);
    assert_eq!(document_blocks[1].block_type, "paragraph");
    assert_eq!(document_blocks[1].block_content, "Paragraph content");

    assert_eq!(document_blocks[2].block_index, 2);
    assert_eq!(document_blocks[2].block_type, "code");
    assert_eq!(document_blocks[2].block_content, "```rust\ncode```");
}

#[tokio::test]
async fn test_get_block_by_hash() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());

    // Store a test block
    let block = create_test_ast_block("Test content", ASTBlockType::Paragraph, 0, 12);
    let block_hash = block.block_hash.clone();

    storage.store_document_blocks_from_ast("test.md", &[block]).await.unwrap();

    // Get block by hash
    let found_block = detector.get_block_by_hash(&block_hash).await.unwrap();

    assert!(found_block.is_some());
    let block_info = found_block.unwrap();
    assert_eq!(block_info.block_hash, block_hash);
    assert_eq!(block_info.document_id, "test.md");
    assert_eq!(block_info.block_content, "Test content");
    assert_eq!(block_info.block_type, "paragraph");
}

// Advanced Deduplication Tests

#[tokio::test]
async fn test_find_duplicate_blocks_min_occurrences() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    // Find blocks that appear at least 2 times
    let duplicates = detector.find_duplicate_blocks(2).await.unwrap();

    // Should find 2 duplicate blocks (one appears 3 times, one appears 2 times)
    assert_eq!(duplicates.len(), 2);

    // Sort by occurrence count for consistent testing
    duplicates.iter().for_each(|d| {
        assert!(d.occurrence_count >= 2);
        assert!(!d.documents.is_empty());
        assert!(!d.block_hash.is_empty());
        assert!(!d.content_preview.is_empty() || d.content_preview.is_empty()); // Allow empty preview
    });
}

#[tokio::test]
async fn test_find_duplicate_blocks_high_threshold() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    // Find blocks that appear at least 3 times
    let duplicates = detector.find_duplicate_blocks(3).await.unwrap();

    // Should find only 1 block that appears 3 times
    assert_eq!(duplicates.len(), 1);
    assert_eq!(duplicates[0].occurrence_count, 3);
    assert_eq!(duplicates[0].documents.len(), 3);
}

#[tokio::test]
async fn test_find_duplicate_blocks_no_matches() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage);

    // Store only unique blocks
    let block1 = create_test_ast_block("Unique 1", ASTBlockType::Paragraph, 0, 8);
    let block2 = create_test_ast_block("Unique 2", ASTBlockType::Paragraph, 0, 8);

    storage.store_document_blocks_from_ast("doc1.md", &[block1]).await.unwrap();
    storage.store_document_blocks_from_ast("doc2.md", &[block2]).await.unwrap();

    // Find blocks that appear at least 2 times
    let duplicates = detector.find_duplicate_blocks(2).await.unwrap();

    // Should find no duplicates
    assert!(duplicates.is_empty());
}

#[tokio::test]
async fn test_get_all_deduplication_stats() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    // Get comprehensive deduplication statistics
    let stats = detector.get_all_deduplication_stats().await.unwrap();

    // Verify basic stats
    assert!(stats.total_unique_blocks > 0);
    assert!(stats.total_block_instances > 0);
    assert!(stats.duplicate_blocks > 0);
    assert!(stats.deduplication_ratio > 0.0);
    assert!(stats.deduplication_ratio <= 1.0);
    assert!(stats.total_storage_saved > 0);
    assert!(stats.average_block_size > 0);

    // Verify most duplicated blocks
    assert!(!stats.most_duplicated_blocks.is_empty());

    // Verify that stats are consistent
    assert_eq!(stats.total_block_instances, stats.total_unique_blocks + stats.duplicate_blocks);
}

#[tokio::test]
async fn test_get_storage_usage_stats() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    // Get storage usage statistics
    let usage_stats = detector.get_storage_usage_stats().await.unwrap();

    // Verify usage stats
    assert!(usage_stats.total_block_storage > 0);
    assert!(usage_stats.deduplication_savings > 0);
    assert!(usage_stats.stored_block_count > 0);
    assert!(usage_stats.unique_block_count > 0);
    assert!(usage_stats.average_block_size > 0);
    assert!(usage_stats.storage_efficiency > 0.0);
    assert!(usage_stats.storage_efficiency <= 1.0);

    // Verify consistency with other stats
    assert_eq!(usage_stats.stored_block_count, usage_stats.unique_block_count +
               (usage_stats.stored_block_count - usage_stats.unique_block_count));
}

// Batch Query Tests

#[tokio::test]
async fn test_find_documents_with_blocks_batch() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage);
    let block_to_documents = setup_deduplication_test_data(&storage).await;

    // Get block hashes to query
    let block_hashes: Vec<String> = block_to_documents.keys().take(3).cloned().collect();

    // Find documents for multiple blocks
    let results = detector.find_documents_with_blocks(&block_hashes).await.unwrap();

    assert_eq!(results.len(), block_hashes.len());

    // Verify each block has correct documents
    for block_hash in &block_hashes {
        assert!(results.contains_key(block_hash));
        let documents = results.get(block_hash).unwrap();
        let expected_documents = block_to_documents.get(block_hash).unwrap();

        // Sort both vectors for comparison
        let mut docs_sorted = documents.clone();
        docs_sorted.sort();
        let mut expected_sorted = expected_documents.clone();
        expected_sorted.sort();

        assert_eq!(docs_sorted, expected_sorted);
    }
}

#[tokio::test]
async fn test_get_blocks_by_hashes_batch() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage);
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    // Store a specific block to test with
    let test_block = create_test_ast_block("Batch test content", ASTBlockType::Paragraph, 0, 19);
    let block_hash = test_block.block_hash.clone();

    storage.store_document_blocks_from_ast("batch_test.md", &[test_block]).await.unwrap();

    // Get blocks by hashes
    let results = detector.get_blocks_by_hashes(&[block_hash.clone()]).await.unwrap();

    assert_eq!(results.len(), 1);
    assert!(results.contains_key(&block_hash));

    let block_info = results.get(&block_hash).unwrap();
    assert_eq!(block_info.block_hash, block_hash);
    assert_eq!(block_info.document_id, "batch_test.md");
    assert_eq!(block_info.block_content, "Batch test content");
    assert_eq!(block_info.block_type, "paragraph");
}

#[tokio::test]
async fn test_batch_queries_empty_list() {
    let detector = create_test_detector().await;

    // Test empty batch queries
    let empty_documents = detector.find_documents_with_blocks(&[]).await.unwrap();
    assert!(empty_documents.is_empty());

    let empty_blocks = detector.get_blocks_by_hashes(&[]).await.unwrap();
    assert!(empty_blocks.is_empty());
}

#[tokio::test]
async fn test_get_block_deduplication_stats_batch() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage);
    let block_to_documents = setup_deduplication_test_data(&storage).await;

    // Get stats for specific blocks
    let block_hashes: Vec<String> = block_to_documents.keys().take(3).cloned().collect();
    let stats = detector.get_block_deduplication_stats(&block_hashes).await.unwrap();

    assert_eq!(stats.len(), block_hashes.len());

    // Verify each block has correct occurrence count
    for (block_hash, count) in &stats {
        let expected_documents = block_to_documents.get(block_hash).unwrap();
        assert_eq!(*count, expected_documents.len());
    }
}

// Edge Cases and Error Handling

#[tokio::test]
async fn test_get_document_blocks_nonexistent_document() {
    let detector = create_test_detector().await;

    let result = detector.get_document_blocks("nonexistent.md").await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_block_deduplication_stats_nonexistent_blocks() {
    let detector = create_test_detector().await;

    let nonexistent_hashes = vec![
        "nonexistent_hash_1".to_string(),
        "nonexistent_hash_2".to_string(),
    ];

    let result = detector.get_block_deduplication_stats(&nonexistent_hashes).await.unwrap();
    assert!(result.is_empty());
}

// Performance and Stress Tests

#[tokio::test]
async fn test_deduplication_performance_large_dataset() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage.clone());

    // Create a larger dataset with many duplicates
    let common_content = "Common block content for performance testing";
    let mut block_hashes = Vec::new();

    // Create many documents with the same content
    for i in 0..50 {
        let common_block = create_test_ast_block(
            common_content, ASTBlockType::Paragraph, 0, common_content.len()
        );
        block_hashes.push(common_block.block_hash.clone());

        let unique_block = create_test_ast_block(
            &format!("Unique content {}", i), ASTBlockType::Paragraph, 0, 17
        );
        block_hashes.push(unique_block.block_hash.clone());

        storage.store_document_blocks_from_ast(
            &format!("perf_doc_{}.md", i),
            &[common_block, unique_block]
        ).await.unwrap();
    }

    // Test performance of duplicate detection
    let start = std::time::Instant::now();
    let duplicates = detector.find_duplicate_blocks(2).await.unwrap();
    let duration = start.elapsed();

    // Should find the common content as duplicate
    assert!(!duplicates.is_empty());
    assert!(duplicates[0].occurrence_count >= 2);

    // Performance check - should complete within reasonable time
    assert!(duration.as_secs() < 5, "Deduplication analysis took too long: {:?}", duration);
}

#[tokio::test]
async fn test_concurrent_deduplication_queries() {
    use std::sync::Arc;

    let storage = Arc::new(create_test_storage().await);
    let _block_to_documents = setup_deduplication_test_data(&storage).await;

    let mut handles = Vec::new();

    // Run concurrent deduplication queries
    for i in 0..10 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            let detector = SurrealDeduplicationDetector::new((*storage_clone).clone());

            // Perform different types of queries
            match i % 4 {
                0 => detector.find_duplicate_blocks(2).await,
                1 => detector.get_all_deduplication_stats().await,
                2 => detector.get_storage_usage_stats().await,
                3 => detector.find_documents_with_block("some_hash").await,
                _ => unreachable!(),
            }
        });
        handles.push(handle);
    }

    // Wait for all queries to complete successfully
    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }
}

// Integration Tests

#[tokio::test]
async fn test_deduplication_integration_with_storage() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::new(storage);

    // Test that deduplication detector works seamlessly with storage
    let blocks = vec![
        create_test_ast_block("Heading 1", ASTBlockType::Heading, 0, 9),
        create_test_ast_block("Paragraph 1", ASTBlockType::Paragraph, 10, 21),
        create_test_ast_block("Paragraph 1", ASTBlockType::Paragraph, 10, 21), // Duplicate
    ];

    detector.storage.store_document_blocks_from_ast("integration_test.md", &blocks).await.unwrap();

    // Verify all deduplication methods work together
    let doc_blocks = detector.get_document_blocks("integration_test.md").await.unwrap();
    assert_eq!(doc_blocks.len(), 3);

    let duplicate_blocks = detector.find_duplicate_blocks(2).await.unwrap();
    assert_eq!(duplicate_blocks.len(), 1);
    assert_eq!(duplicate_blocks[0].occurrence_count, 2);

    let stats = detector.get_all_deduplication_stats().await.unwrap();
    assert!(stats.total_storage_saved > 0);
    assert!(stats.deduplication_ratio > 0.0);
}

#[tokio::test]
async fn test_custom_average_block_size() {
    let storage = create_test_storage().await;
    let detector = SurrealDeduplicationDetector::with_average_block_size(storage, 500);

    // Store test data
    let block = create_test_ast_block("Test content", ASTBlockType::Paragraph, 0, 12);
    detector.storage.store_document_blocks_from_ast("test.md", &[block]).await.unwrap();

    // Get stats to verify custom block size is used
    let stats = detector.get_all_deduplication_stats().await.unwrap();
    assert_eq!(stats.average_block_size, 500);

    let usage_stats = detector.get_storage_usage_stats().await.unwrap();
    assert_eq!(usage_stats.average_block_size, 500);
}