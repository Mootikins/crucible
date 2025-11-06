//! Content-Addressed Block Storage Tests
//!
//! Comprehensive tests for Phase 3 content-addressed block storage functionality
//! including document_blocks table operations, batch queries, and deduplication.

use crucible_surrealdb::{
    ContentAddressedStorageSurrealDB, SurrealDbConfig,
    content_addressed_storage::DocumentBlockRecord,
};
use crucible_parser::types::{ASTBlock, ASTBlockMetadata, ASTBlockType};
use crucible_core::hashing::blake3::Blake3Hasher;
use crucible_core::storage::ContentHasher;
use crucible_core::storage::traits::BlockOperations;
use std::collections::HashMap;

/// Test helper to create an AST block for testing
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

/// Create a test storage instance
async fn create_test_storage() -> ContentAddressedStorageSurrealDB {
    let config = SurrealDbConfig::memory();
    let storage = ContentAddressedStorageSurrealDB::new(config).await.unwrap();
    // Schema is initialized automatically in new()
    storage
}

#[tokio::test]
async fn test_store_document_blocks_from_ast() {
    let storage = create_test_storage().await;

    let document_id = "test/document.md";
    let blocks = vec![
        create_test_ast_block("# Test Heading", ASTBlockType::Heading, 0, 13),
        create_test_ast_block("Test paragraph content.", ASTBlockType::Paragraph, 14, 38),
        create_test_ast_block("```rust\nlet x = 42;\n```", ASTBlockType::Code, 39, 65),
    ];

    // Store blocks from AST
    storage.store_document_blocks_from_ast(document_id, &blocks).await.unwrap();

    // Retrieve and verify
    let retrieved_blocks = storage.get_document_blocks(document_id).await.unwrap();
    assert_eq!(retrieved_blocks.len(), 3);

    // Verify block order and content
    assert_eq!(retrieved_blocks[0].block_index, 0);
    assert_eq!(retrieved_blocks[0].block_content, "# Test Heading");
    assert_eq!(retrieved_blocks[0].block_type, "heading");

    assert_eq!(retrieved_blocks[1].block_index, 1);
    assert_eq!(retrieved_blocks[1].block_content, "Test paragraph content.");
    assert_eq!(retrieved_blocks[1].block_type, "paragraph");

    assert_eq!(retrieved_blocks[2].block_index, 2);
    assert_eq!(retrieved_blocks[2].block_content, "```rust\nlet x = 42;\n```");
    assert_eq!(retrieved_blocks[2].block_type, "code");
}

#[tokio::test]
async fn test_find_documents_with_block_single_document() {
    let storage = create_test_storage().await;

    let document_id = "test/doc.md";
    let blocks = vec![
        create_test_ast_block("Unique content", ASTBlockType::Paragraph, 0, 14),
    ];

    storage.store_document_blocks_from_ast(document_id, &blocks).await.unwrap();

    // Find documents containing the block
    let block_hash = &blocks[0].block_hash;
    let documents = storage.find_documents_with_block(block_hash).await.unwrap();

    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0], document_id);
}

#[tokio::test]
async fn test_find_documents_with_block_multiple_documents() {
    let storage = create_test_storage().await;

    let common_content = "Common block content";
    let common_hash = {
        let hasher = Blake3Hasher::new();
        hasher.hash_block(common_content.as_bytes()).to_string()
    };

    let block1 = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());
    let block2 = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());

    // Store same block in multiple documents
    storage.store_document_blocks_from_ast("doc1.md", &[block1]).await.unwrap();
    storage.store_document_blocks_from_ast("doc2.md", &[block2]).await.unwrap();

    // Find documents containing the block
    let documents = storage.find_documents_with_block(&common_hash).await.unwrap();

    assert_eq!(documents.len(), 2);
    assert!(documents.contains(&"doc1.md".to_string()));
    assert!(documents.contains(&"doc2.md".to_string()));
}

#[tokio::test]
async fn test_get_block_by_hash_exists() {
    let storage = create_test_storage().await;

    let document_id = "test.md";
    let blocks = vec![
        create_test_ast_block("Test content", ASTBlockType::Paragraph, 0, 12),
    ];

    storage.store_document_blocks_from_ast(document_id, &blocks).await.unwrap();

    // Get block by hash
    let block_hash = &blocks[0].block_hash;
    let retrieved_block = storage.get_block_by_hash(block_hash).await.unwrap();

    assert!(retrieved_block.is_some());

    let block = retrieved_block.unwrap();
    assert_eq!(block.block_content, "Test content");
    assert_eq!(block.block_type, "paragraph");
    assert_eq!(block.document_id, document_id);
    assert_eq!(block.block_index, 0);
}

#[tokio::test]
async fn test_get_block_by_hash_not_exists() {
    let storage = create_test_storage().await;

    let retrieved_block = storage.get_block_by_hash("nonexistent_hash").await.unwrap();
    assert!(retrieved_block.is_none());
}

#[tokio::test]
async fn test_delete_document_blocks() {
    let storage = create_test_storage().await;

    let document_id = "test.md";
    let blocks = vec![
        create_test_ast_block("Block 1", ASTBlockType::Paragraph, 0, 6),
        create_test_ast_block("Block 2", ASTBlockType::Paragraph, 7, 13),
        create_test_ast_block("Block 3", ASTBlockType::Code, 14, 25),
    ];

    storage.store_document_blocks_from_ast(document_id, &blocks).await.unwrap();

    // Verify blocks exist
    let retrieved = storage.get_document_blocks(document_id).await.unwrap();
    assert_eq!(retrieved.len(), 3);

    // Delete blocks
    let deleted_count = storage.delete_document_blocks(document_id).await.unwrap();
    assert_eq!(deleted_count, 3);

    // Verify blocks are deleted
    let retrieved = storage.get_document_blocks(document_id).await.unwrap();
    assert_eq!(retrieved.len(), 0);
}

#[tokio::test]
async fn test_delete_document_blocks_empty_document() {
    let storage = create_test_storage().await;

    let document_id = "empty.md";
    let deleted_count = storage.delete_document_blocks(document_id).await.unwrap();
    assert_eq!(deleted_count, 0);
}

// Batch Query Tests

#[tokio::test]
async fn test_find_documents_with_blocks_empty_list() {
    let storage = create_test_storage().await;

    let result = storage.find_documents_with_blocks(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_find_documents_with_blocks_multiple_hashes() {
    let storage = create_test_storage().await;

    let hash1 = {
        let hasher = Blake3Hasher::new();
        hasher.hash_block(b"Content 1").to_string()
    };
    let hash2 = {
        let hasher = Blake3Hasher::new();
        hasher.hash_block(b"Content 2").to_string()
    };

    let block1 = create_test_ast_block("Content 1", ASTBlockType::Paragraph, 0, 9);
    let block2 = create_test_ast_block("Content 2", ASTBlockType::Paragraph, 0, 9);

    // Store blocks in different documents
    storage.store_document_blocks_from_ast("doc1.md", &[block1]).await.unwrap();
    storage.store_document_blocks_from_ast("doc2.md", &[block2]).await.unwrap();

    // Query multiple hashes
    let block_hashes = vec![hash1.clone(), hash2.clone()];
    let result = storage.find_documents_with_blocks(&block_hashes).await.unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result.get(&hash1).unwrap(), &["doc1.md"]);
    assert_eq!(result.get(&hash2).unwrap(), &["doc2.md"]);
}

#[tokio::test]
async fn test_get_blocks_by_hashes_empty_list() {
    let storage = create_test_storage().await;

    let result = storage.get_blocks_by_hashes(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_blocks_by_hashes_multiple_hashes() {
    let storage = create_test_storage().await;

    let blocks = vec![
        create_test_ast_block("Content A", ASTBlockType::Paragraph, 0, 9),
        create_test_ast_block("Content B", ASTBlockType::Code, 10, 20),
        create_test_ast_block("Content C", ASTBlockType::Heading, 21, 30),
    ];

    let block_hashes: Vec<String> = blocks.iter().map(|b| b.block_hash.clone()).collect();

    // Store blocks
    storage.store_document_blocks_from_ast("test.md", &blocks).await.unwrap();

    // Get blocks by hashes
    let result = storage.get_blocks_by_hashes(&block_hashes).await.unwrap();

    assert_eq!(result.len(), 3);

    for (i, block) in blocks.iter().enumerate() {
        let retrieved = result.get(&block.block_hash).unwrap();
        assert_eq!(retrieved.block_content, block.content);
        assert_eq!(retrieved.block_index, i);
        assert_eq!(retrieved.document_id, "test.md");
    }
}

#[tokio::test]
async fn test_get_blocks_by_hashes_mixed_existence() {
    let storage = create_test_storage().await;

    let existing_block = create_test_ast_block("Existing content", ASTBlockType::Paragraph, 0, 16);
    let existing_hash = existing_block.block_hash.clone();

    let nonexistent_hash = "nonexistent_hash_value";

    // Store only one block
    storage.store_document_blocks_from_ast("test.md", &[existing_block]).await.unwrap();

    // Query both existing and non-existing hashes
    let block_hashes = vec![existing_hash.clone(), nonexistent_hash.to_string()];
    let result = storage.get_blocks_by_hashes(&block_hashes).await.unwrap();

    assert_eq!(result.len(), 1);
    assert!(result.contains_key(&existing_hash));
    assert!(!result.contains_key(nonexistent_hash));
}

// Deduplication Statistics Tests

#[tokio::test]
async fn test_get_block_deduplication_stats_empty() {
    let storage = create_test_storage().await;

    let result = storage.get_block_deduplication_stats(&[]).await.unwrap();
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_get_block_deduplication_stats_unique_blocks() {
    let storage = create_test_storage().await;

    let blocks = vec![
        create_test_ast_block("Unique content 1", ASTBlockType::Paragraph, 0, 16),
        create_test_ast_block("Unique content 2", ASTBlockType::Code, 17, 30),
        create_test_ast_block("Unique content 3", ASTBlockType::Heading, 31, 45),
    ];

    let block_hashes: Vec<String> = blocks.iter().map(|b| b.block_hash.clone()).collect();

    // Store unique blocks
    for block in &blocks {
        storage.store_document_blocks_from_ast(&format!("doc_{}.md", block_hashes.iter().position(|h| h == &block.block_hash).unwrap()), &[block.clone()]).await.unwrap();
    }

    // Get deduplication stats
    let result = storage.get_block_deduplication_stats(&block_hashes).await.unwrap();

    assert_eq!(result.len(), 3);
    for (_, count) in result {
        assert_eq!(count, 1); // All should have count of 1
    }
}

#[tokio::test]
async fn test_get_block_deduplication_stats_with_duplicates() {
    let storage = create_test_storage().await;

    let common_content = "Shared content";
    let unique_content = "Unique content";

    let common_block = create_test_ast_block(common_content, ASTBlockType::Paragraph, 0, common_content.len());
    let unique_block = create_test_ast_block(unique_content, ASTBlockType::Code, 0, unique_content.len());

    // Clone the hashes before moving the blocks
    let common_hash = common_block.block_hash.clone();
    let unique_hash = unique_block.block_hash.clone();

    // Store common block in multiple documents
    storage.store_document_blocks_from_ast("doc1.md", &[common_block.clone()]).await.unwrap();
    storage.store_document_blocks_from_ast("doc2.md", &[common_block.clone()]).await.unwrap();
    storage.store_document_blocks_from_ast("doc3.md", &[common_block.clone()]).await.unwrap();

    // Store unique block
    storage.store_document_blocks_from_ast("unique.md", &[unique_block]).await.unwrap();

    // Get deduplication stats
    let block_hashes = vec![common_hash.clone(), unique_hash.clone()];
    let result = storage.get_block_deduplication_stats(&block_hashes).await.unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(*result.get(&common_hash).unwrap(), 3); // Appears in 3 documents
    assert_eq!(*result.get(&unique_hash).unwrap(), 1); // Appears in 1 document
}

#[tokio::test]
async fn test_get_all_block_deduplication_stats_comprehensive() {
    let storage = create_test_storage().await;

    // Create a mix of unique and duplicate blocks
    let blocks = vec![
        create_test_ast_block("Heading 1", ASTBlockType::Heading, 0, 9),
        create_test_ast_block("Paragraph 1", ASTBlockType::Paragraph, 10, 22),
        create_test_ast_block("Code 1", ASTBlockType::Code, 23, 35),
        create_test_ast_block("Paragraph 1", ASTBlockType::Paragraph, 0, 12), // Duplicate
        create_test_ast_block("Code 1", ASTBlockType::Code, 23, 35),    // Duplicate
        create_test_ast_block("Unique content", ASTBlockType::Paragraph, 36, 48),
    ];

    // Store blocks across multiple documents
    for (i, block) in blocks.iter().enumerate() {
        storage.store_document_blocks_from_ast(&format!("doc_{}.md", i), &[block.clone()]).await.unwrap();
    }

    // Get comprehensive deduplication stats
    let stats = storage.get_all_block_deduplication_stats().await.unwrap();

    // The stats HashMap contains block_hash -> count mappings for duplicates
    // We need to verify that duplicates were found
    assert!(!stats.is_empty(), "Should have found duplicate blocks");

    // Verify that we have the expected duplicate blocks
    // Based on the test data, we should have 2 blocks that appear multiple times
    assert_eq!(stats.len(), 2, "Should have 2 different duplicate block hashes");
}

// Performance Tests

#[tokio::test]
async fn test_batch_query_performance_large_dataset() {
    let storage = create_test_storage().await;

    // Create a large dataset
    let mut block_hashes = Vec::new();
    let num_blocks = 100;

    for i in 0..num_blocks {
        let content = format!("Block content {}", i);
        let block = create_test_ast_block(&content, ASTBlockType::Paragraph, 0, content.len());
        block_hashes.push(block.block_hash.clone());

        // Store in document groups
        let doc_id = format!("doc_{}.md", i % 10); // 10 documents
        storage.store_document_blocks_from_ast(&doc_id, &[block]).await.unwrap();
    }

    // Test batch query performance
    let start = std::time::Instant::now();
    let result = storage.find_documents_with_blocks(&block_hashes).await.unwrap();
    let duration = start.elapsed();

    assert_eq!(result.len(), num_blocks);
    assert!(duration.as_millis() < 1000, "Batch query should complete within 1 second, took {:?}", duration);
}

#[tokio::test]
async fn test_concurrent_block_operations() {
    let storage = std::sync::Arc::new(create_test_storage().await);
    let mut handles = Vec::new();

    // Perform concurrent store operations
    for i in 0..10 {
        let storage_clone = storage.clone();
        let handle = tokio::spawn(async move {
            let blocks = vec![
                create_test_ast_block(&format!("Concurrent block {}", i), ASTBlockType::Paragraph, 0, 18),
            ];
            let doc_id = format!("concurrent_doc_{}.md", i);
            storage_clone.store_document_blocks_from_ast(&doc_id, &blocks).await
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // Verify all blocks were stored correctly
    let mut total_blocks = 0;
    for i in 0..10 {
        let doc_id = format!("concurrent_doc_{}.md", i);
        let blocks = storage.get_document_blocks(&doc_id).await.unwrap();
        total_blocks += blocks.len();
    }

    assert_eq!(total_blocks, 10);
}

// Error Handling Tests

#[tokio::test]
async fn test_error_handling_empty_document_id() {
    let storage = create_test_storage().await;

    let blocks = vec![create_test_ast_block("Test", ASTBlockType::Paragraph, 0, 4)];

    // Should fail with empty document ID
    let result = storage.store_document_blocks_from_ast("", &blocks).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_handling_empty_block_list() {
    let storage = create_test_storage().await;

    // Empty block list should not fail (just no-op)
    let result = storage.store_document_blocks_from_ast("test.md", &[]).await;
    assert!(result.is_ok());

    // Verify no blocks were stored
    let retrieved = storage.get_document_blocks("test.md").await.unwrap();
    assert_eq!(retrieved.len(), 0);
}

#[tokio::test]
async fn test_error_handling_invalid_block_indices() {
    let storage = create_test_storage().await;

    // This test would require manual database manipulation to create invalid block indices
    // For now, we test normal operation with valid indices
    let blocks = vec![
        create_test_ast_block("Valid block", ASTBlockType::Paragraph, 0, 11),
    ];

    storage.store_document_blocks_from_ast("test.md", &blocks).await.unwrap();

    let retrieved = storage.get_document_blocks("test.md").await.unwrap();
    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].block_index, 0);
}

// Integration with Existing Tests

#[tokio::test]
async fn test_integration_with_existing_content_storage() {
    let storage = create_test_storage().await;

    // Test that both the old and new block storage methods work together
    let data = "Test block data";
    let hash = "test_hash_integration";

    // Store using the old method
    storage.store_block(hash, data.as_bytes()).await.unwrap();

    // Store related document blocks
    let blocks = vec![create_test_ast_block(data, ASTBlockType::Paragraph, 0, data.len())];
    storage.store_document_blocks_from_ast("integration_test.md", &blocks).await.unwrap();

    // Verify both storage methods coexist
    let block_data = storage.get_block(hash).await.unwrap();
    assert_eq!(block_data, Some(data.as_bytes().to_vec()));

    let document_blocks = storage.get_document_blocks("integration_test.md").await.unwrap();
    assert_eq!(document_blocks.len(), 1);
    assert_eq!(document_blocks[0].block_content, data);
}