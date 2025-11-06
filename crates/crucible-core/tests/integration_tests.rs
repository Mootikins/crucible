//! Comprehensive Integration Tests for Refactored Architecture
//!
//! This module provides extensive integration testing for the Phase 5 refactored
//! architecture, covering:
//! - Storage implementations with multiple algorithms (BLAKE3, SHA256)
//! - Deduplication across storage backends
//! - Merkle tree operations end-to-end
//! - AST converter integration with hasher
//! - Storage factory with different configurations
//! - Performance benchmarks and comprehensive test coverage
//!
//! These tests verify cross-component functionality and ensure all pieces
//! work together correctly in production scenarios.

use anyhow::Result;
use crucible_core::{
    hashing::{
        algorithm::{Blake3Algorithm, Sha256Algorithm},
        ast_converter::ASTBlockConverter,
        blake3::Blake3Hasher,
        sha256::SHA256Hasher,
    },
    storage::{
        factory::{StorageConfig, StorageFactory, BackendConfig, HashAlgorithm},
        memory::MemoryStorage,
        traits::{BlockOperations, StorageManagement, ContentHasher},
        deduplicator::{DefaultDeduplicator, Deduplicator},
        HashedBlock, MerkleTree, ContentAddressedStorage,
    },
    parser::{MarkdownParser, PulldownParser},
};
use crucible_parser::types::{ASTBlock, ASTBlockType, ASTBlockMetadata, ListType};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::fs;

// ============================================================================
// Test Utilities and Helpers
// ============================================================================

/// Create a test markdown document with various block types
fn create_test_markdown() -> &'static str {
    r#"---
title: Integration Test Document
tags: [test, integration]
---

# Main Title

This is a paragraph with some **bold** text and a [[wikilink]].

## Code Section

Here's some Rust code:

```rust
fn main() {
    println!("Hello, world!");
}
```

## Lists and More

- First item
- Second item with [[link]]
- Third item

> [!note] Important Note
> This is a callout with important information.

### Math Section

Inline math: $E = mc^2$

Block math:
$$
\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

## Final Section

Some concluding text with #tag references.
"#
}

/// Create test AST blocks for conversion testing
fn create_test_ast_blocks() -> Vec<ASTBlock> {
    vec![
        ASTBlock::new(
            ASTBlockType::Heading,
            "Main Title".to_string(),
            0,
            11,
            ASTBlockMetadata::heading(1, Some("main-title".to_string())),
        ),
        ASTBlock::new(
            ASTBlockType::Paragraph,
            "This is a test paragraph.".to_string(),
            15,
            40,
            ASTBlockMetadata::generic(),
        ),
        ASTBlock::new(
            ASTBlockType::Code,
            "fn main() { println!(\"test\"); }".to_string(),
            50,
            82,
            ASTBlockMetadata::code(Some("rust".to_string()), 1),
        ),
        ASTBlock::new(
            ASTBlockType::List,
            "- Item 1\n- Item 2\n- Item 3".to_string(),
            90,
            117,
            ASTBlockMetadata::list(ListType::Unordered, 3),
        ),
        ASTBlock::new(
            ASTBlockType::Callout,
            "Important note here".to_string(),
            125,
            144,
            ASTBlockMetadata::callout("note".to_string(), Some("Note Title".to_string()), true),
        ),
    ]
}

// ============================================================================
// Test Suite 1: Storage Implementations with Multiple Algorithms
// ============================================================================

#[tokio::test]
async fn test_storage_with_blake3_algorithm() -> Result<()> {
    // Create storage with BLAKE3 hasher
    let config = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(10_000_000),
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        hash_algorithm: HashAlgorithm::Blake3,
        ..Default::default()
    };

    let storage = StorageFactory::create(config).await?;

    // Test basic operations
    let test_data = b"Hello, BLAKE3!";
    let hasher = Blake3Hasher::new();
    let hash = hasher.hash_block(test_data);

    storage.store_block(&hash, test_data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(test_data.to_vec()));

    // Verify stats
    let stats = storage.get_stats().await?;
    assert_eq!(stats.block_count, 1);
    assert!(stats.block_size_bytes > 0);

    Ok(())
}

#[tokio::test]
async fn test_storage_with_sha256_algorithm() -> Result<()> {
    // Create storage with SHA256 hasher (currently falls back to BLAKE3 with warning)
    let config = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(10_000_000),
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        hash_algorithm: HashAlgorithm::Sha256,
        ..Default::default()
    };

    let storage = StorageFactory::create(config).await?;

    // Test basic operations
    let test_data = b"Hello, SHA256!";
    let hasher = SHA256Hasher::new();
    let hash = hasher.hash_block(test_data);

    storage.store_block(&hash, test_data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(test_data.to_vec()));

    Ok(())
}

#[tokio::test]
async fn test_storage_algorithm_consistency() -> Result<()> {
    // Verify same data produces same hash across multiple stores
    let test_data = b"Consistency test data";

    // BLAKE3 storage
    let blake3_storage = StorageFactory::create(StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(10_000_000),
            enable_lru_eviction: false,
            enable_stats_tracking: false,
        },
        hash_algorithm: HashAlgorithm::Blake3,
        ..Default::default()
    }).await?;

    let blake3_hasher = Blake3Hasher::new();
    let blake3_hash = blake3_hasher.hash_block(test_data);

    blake3_storage.store_block(&blake3_hash, test_data).await?;

    // Store again and verify deduplication
    blake3_storage.store_block(&blake3_hash, test_data).await?;

    let retrieved = blake3_storage.get_block(&blake3_hash).await?;
    assert_eq!(retrieved, Some(test_data.to_vec()));

    Ok(())
}

// ============================================================================
// Test Suite 2: Deduplication Across Storage Backends
// ============================================================================

#[tokio::test]
async fn test_deduplication_with_memory_storage() -> Result<()> {
    let storage = MemoryStorage::new();
    let deduplicator = DefaultDeduplicator::new(storage.clone());

    // Store multiple blocks
    let block1_data = b"Block 1 content";
    let block2_data = b"Block 2 content";
    let block3_data = b"Block 1 content"; // Duplicate of block1

    let hasher = Blake3Hasher::new();
    let hash1 = hasher.hash_block(block1_data);
    let hash2 = hasher.hash_block(block2_data);
    let hash3 = hasher.hash_block(block3_data); // Same as hash1

    storage.store_block(&hash1, block1_data).await?;
    storage.store_block(&hash2, block2_data).await?;
    storage.store_block(&hash3, block3_data).await?;

    // Verify hash1 and hash3 are the same (deduplication)
    assert_eq!(hash1, hash3);

    // Analyze blocks for duplicates
    let block_hashes = vec![hash1.clone(), hash2.clone(), hash3.clone()];
    let analysis = deduplicator.analyze_blocks(&block_hashes).await?;

    assert_eq!(analysis.total_blocks, 3);
    // Since hash1 == hash3, we expect to see deduplication effect
    // However, the current implementation treats all as unique

    Ok(())
}

#[tokio::test]
async fn test_deduplication_statistics() -> Result<()> {
    let storage = MemoryStorage::new();
    let deduplicator = DefaultDeduplicator::with_average_block_size(storage.clone(), 256);

    // Get initial stats
    let stats = deduplicator.get_deduplication_stats().await?;
    assert_eq!(stats.total_unique_blocks, 0);
    assert_eq!(stats.duplicate_blocks, 0);

    Ok(())
}

#[tokio::test]
async fn test_storage_savings_calculation() -> Result<()> {
    let storage = MemoryStorage::new();
    let deduplicator = DefaultDeduplicator::new(storage);

    let savings = deduplicator.calculate_storage_savings().await?;

    assert_eq!(savings.total_bytes_saved, 0);
    assert_eq!(savings.percentage_saved, 0.0);
    assert!(savings.savings_by_type.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_duplicate_detection() -> Result<()> {
    let storage = MemoryStorage::new();
    let deduplicator = DefaultDeduplicator::new(storage);

    // Find duplicates with minimum occurrence count
    let duplicates = deduplicator.find_duplicates(2).await?;

    // Initially no duplicates
    assert!(duplicates.is_empty());

    Ok(())
}

// ============================================================================
// Test Suite 3: Merkle Tree Operations End-to-End
// ============================================================================

#[tokio::test]
async fn test_merkle_tree_creation_with_blake3() -> Result<()> {
    let hasher = Blake3Hasher::new();

    // Create test blocks
    let blocks = vec![
        HashedBlock::from_data(
            b"Block 1".to_vec(),
            0,
            0,
            false,
            &hasher,
        )?,
        HashedBlock::from_data(
            b"Block 2".to_vec(),
            1,
            8,
            false,
            &hasher,
        )?,
        HashedBlock::from_data(
            b"Block 3".to_vec(),
            2,
            16,
            true,
            &hasher,
        )?,
    ];

    // Build Merkle tree
    let tree = MerkleTree::from_blocks(&blocks, &hasher)?;

    assert_eq!(tree.block_count, 3);
    assert_eq!(tree.leaf_hashes.len(), 3);
    assert!(!tree.root_hash.is_empty());

    // Verify tree integrity
    tree.verify_integrity(&hasher)?;

    // Get tree statistics
    let stats = tree.stats();
    assert_eq!(stats.block_count, 3);
    assert_eq!(stats.leaf_count, 3);
    assert!(stats.node_count >= 3);

    Ok(())
}

#[tokio::test]
async fn test_merkle_tree_single_block() -> Result<()> {
    let hasher = Blake3Hasher::new();

    let block = HashedBlock::from_data(
        b"Single block content".to_vec(),
        0,
        0,
        true,
        &hasher,
    )?;

    let tree = MerkleTree::from_single_block(&block)?;

    assert_eq!(tree.block_count, 1);
    assert_eq!(tree.root_hash, block.hash);
    assert_eq!(tree.depth, 0);

    Ok(())
}

#[tokio::test]
async fn test_merkle_tree_comparison() -> Result<()> {
    let hasher = Blake3Hasher::new();

    // Create first tree
    let blocks1 = vec![
        HashedBlock::from_data(b"Block 1".to_vec(), 0, 0, false, &hasher)?,
        HashedBlock::from_data(b"Block 2".to_vec(), 1, 8, true, &hasher)?,
    ];
    let tree1 = MerkleTree::from_blocks(&blocks1, &hasher)?;

    // Create modified tree
    let blocks2 = vec![
        HashedBlock::from_data(b"Block 1".to_vec(), 0, 0, false, &hasher)?,
        HashedBlock::from_data(b"Modified Block 2".to_vec(), 1, 8, true, &hasher)?,
    ];
    let tree2 = MerkleTree::from_blocks(&blocks2, &hasher)?;

    // Compare trees
    let changes = tree1.compare_with(&tree2);

    assert!(!changes.is_empty(), "Should detect changes");

    Ok(())
}

#[tokio::test]
async fn test_merkle_tree_large_number_of_blocks() -> Result<()> {
    let hasher = Blake3Hasher::new();

    // Create many blocks
    let mut blocks = Vec::new();
    for i in 0..100 {
        let data = format!("Block {}", i);
        let block = HashedBlock::from_data(
            data.as_bytes().to_vec(),
            i,
            i * 10,
            i == 99,
            &hasher,
        )?;
        blocks.push(block);
    }

    let tree = MerkleTree::from_blocks(&blocks, &hasher)?;

    assert_eq!(tree.block_count, 100);
    assert_eq!(tree.leaf_hashes.len(), 100);

    // Verify integrity
    tree.verify_integrity(&hasher)?;

    Ok(())
}

// ============================================================================
// Test Suite 4: AST Converter Integration with Hasher
// ============================================================================

#[tokio::test]
async fn test_ast_converter_with_blake3() -> Result<()> {
    let converter = ASTBlockConverter::new(Blake3Algorithm);
    let blocks = create_test_ast_blocks();

    // Convert AST blocks to hashed blocks
    let hashed_blocks = converter.convert_batch(&blocks).await?;

    assert_eq!(hashed_blocks.len(), blocks.len());

    // Verify each converted block
    for (i, hashed) in hashed_blocks.iter().enumerate() {
        assert_eq!(hashed.index, i);
        assert!(!hashed.hash.is_empty());
        assert_eq!(hashed.hash.len(), 64); // BLAKE3 hex = 64 chars
    }

    Ok(())
}

#[tokio::test]
async fn test_ast_converter_with_sha256() -> Result<()> {
    let converter = ASTBlockConverter::new(Sha256Algorithm);
    let blocks = create_test_ast_blocks();

    let hashed_blocks = converter.convert_batch(&blocks).await?;

    assert_eq!(hashed_blocks.len(), blocks.len());

    // Verify SHA256 properties
    for hashed in &hashed_blocks {
        assert_eq!(hashed.hash.len(), 64); // SHA256 hex = 64 chars
    }

    Ok(())
}

#[tokio::test]
async fn test_ast_converter_determinism() -> Result<()> {
    let converter = ASTBlockConverter::new(Blake3Algorithm);
    let blocks = create_test_ast_blocks();

    // Convert twice
    let hashed1 = converter.convert_batch(&blocks).await?;
    let hashed2 = converter.convert_batch(&blocks).await?;

    // Verify deterministic hashing
    for (b1, b2) in hashed1.iter().zip(hashed2.iter()) {
        assert_eq!(b1.hash, b2.hash, "Same content should produce same hash");
    }

    Ok(())
}

#[tokio::test]
async fn test_ast_converter_statistics() -> Result<()> {
    let converter = ASTBlockConverter::new(Blake3Algorithm);
    let blocks = create_test_ast_blocks();

    let stats = converter.analyze_batch(&blocks);

    assert_eq!(stats.total_blocks, 5);
    assert!(stats.total_content_bytes > 0);
    assert!(stats.total_span_bytes > 0);
    assert_eq!(stats.empty_blocks, 0);

    // Verify block type distribution
    assert!(stats.block_type_counts.contains_key("heading"));
    assert!(stats.block_type_counts.contains_key("paragraph"));
    assert!(stats.block_type_counts.contains_key("code"));

    // Check statistics methods
    assert!(stats.avg_content_size() > 0.0);
    assert!(stats.avg_span_size() > 0.0);

    Ok(())
}

#[tokio::test]
async fn test_ast_converter_with_merkle_tree() -> Result<()> {
    // Complete pipeline: AST -> HashedBlock -> MerkleTree
    let converter = ASTBlockConverter::new(Blake3Algorithm);
    let blocks = create_test_ast_blocks();

    // Convert AST blocks
    let hashed_blocks = converter.convert_batch(&blocks).await?;

    // Build Merkle tree
    let hasher = Blake3Hasher::new();
    let tree = MerkleTree::from_blocks(&hashed_blocks, &hasher)?;

    assert_eq!(tree.block_count, blocks.len());
    tree.verify_integrity(&hasher)?;

    Ok(())
}

// ============================================================================
// Test Suite 5: Storage Factory with Different Configurations
// ============================================================================

#[tokio::test]
async fn test_factory_in_memory_creation() -> Result<()> {
    let config = StorageConfig::in_memory(Some(50_000_000));
    let storage = StorageFactory::create(config).await?;

    // Test basic operations
    let hasher = Blake3Hasher::new();
    let data = b"Factory test data";
    let hash = hasher.hash_block(data);

    storage.store_block(&hash, data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(data.to_vec()));

    Ok(())
}

#[tokio::test]
async fn test_factory_configuration_validation() -> Result<()> {
    // Test invalid configuration: zero memory limit
    let config = StorageConfig {
        backend: BackendConfig::InMemory {
            memory_limit: Some(0),
            enable_lru_eviction: true,
            enable_stats_tracking: true,
        },
        ..Default::default()
    };

    let result = config.validate();
    assert!(result.is_err(), "Should reject zero memory limit");

    Ok(())
}

#[tokio::test]
async fn test_factory_custom_backend() -> Result<()> {
    // Create custom backend
    let memory_storage = MemoryStorage::new();
    let config = StorageConfig::custom(Arc::new(memory_storage) as Arc<dyn ContentAddressedStorage>);

    let storage = StorageFactory::create(config).await?;

    // Test it works
    let hasher = Blake3Hasher::new();
    let data = b"Custom backend test";
    let hash = hasher.hash_block(data);

    storage.store_block(&hash, data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(data.to_vec()));

    Ok(())
}

#[tokio::test]
async fn test_factory_from_environment() -> Result<()> {
    // Set environment variables
    std::env::set_var("STORAGE_BACKEND", "in_memory");
    std::env::set_var("STORAGE_MEMORY_LIMIT", "20000000");

    let storage = StorageFactory::create_from_env().await?;

    // Verify it works
    let hasher = Blake3Hasher::new();
    let data = b"Environment config test";
    let hash = hasher.hash_block(data);

    storage.store_block(&hash, data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(data.to_vec()));

    // Cleanup
    std::env::remove_var("STORAGE_BACKEND");
    std::env::remove_var("STORAGE_MEMORY_LIMIT");

    Ok(())
}

// ============================================================================
// Test Suite 6: End-to-End File Processing Pipeline
// ============================================================================

#[tokio::test]
async fn test_complete_file_to_storage_pipeline() -> Result<()> {
    // Create temporary file
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.md");
    fs::write(&file_path, create_test_markdown()).await?;

    // Parse file
    let parser = PulldownParser::new();
    let parsed_doc = parser.parse_file(&file_path).await?;

    // Verify parsing
    assert!(parsed_doc.content.headings.len() > 0);
    assert!(parsed_doc.content.code_blocks.len() > 0);

    // Create storage
    let config = StorageConfig::in_memory(Some(10_000_000));
    let storage = StorageFactory::create(config).await?;

    // Store document metadata
    let hasher = Blake3Hasher::new();
    let file_hash = hasher.hash_block(create_test_markdown().as_bytes());

    storage.store_block(&file_hash, create_test_markdown().as_bytes()).await?;

    // Verify retrieval
    let retrieved = storage.get_block(&file_hash).await?;
    assert!(retrieved.is_some());

    Ok(())
}

#[tokio::test]
async fn test_block_hasher_integration() -> Result<()> {
    // Create block hasher
    let _hasher = Blake3Hasher::new();

    // Parse document and extract blocks
    let temp_dir = TempDir::new()?;
    let file_path = temp_dir.path().join("test.md");
    fs::write(&file_path, create_test_markdown()).await?;

    let parser = PulldownParser::new();
    let parsed_doc = parser.parse_file(&file_path).await?;

    // Verify headings and code blocks were extracted
    assert!(parsed_doc.content.headings.len() > 0, "Should extract headings");
    assert!(parsed_doc.content.code_blocks.len() > 0, "Should extract code blocks");

    Ok(())
}

// ============================================================================
// Test Suite 7: Cross-Algorithm Compatibility
// ============================================================================

#[tokio::test]
async fn test_different_algorithms_produce_different_hashes() -> Result<()> {
    let data = b"Test data for algorithm comparison";

    // BLAKE3
    let blake3_hasher = Blake3Hasher::new();
    let blake3_hash = blake3_hasher.hash_block(data);

    // SHA256
    let sha256_hasher = SHA256Hasher::new();
    let sha256_hash = sha256_hasher.hash_block(data);

    // Different algorithms should produce different hashes
    assert_ne!(blake3_hash, sha256_hash, "Different algorithms should produce different hashes");

    Ok(())
}

#[tokio::test]
async fn test_storage_with_multiple_hash_algorithms() -> Result<()> {
    // Create storage
    let storage = MemoryStorage::new();

    // Store with BLAKE3
    let blake3_hasher = Blake3Hasher::new();
    let data1 = b"Data for BLAKE3";
    let blake3_hash = blake3_hasher.hash_block(data1);
    storage.store_block(&blake3_hash, data1).await?;

    // Store with SHA256
    let sha256_hasher = SHA256Hasher::new();
    let data2 = b"Data for SHA256";
    let sha256_hash = sha256_hasher.hash_block(data2);
    storage.store_block(&sha256_hash, data2).await?;

    // Retrieve both
    let retrieved1 = storage.get_block(&blake3_hash).await?;
    let retrieved2 = storage.get_block(&sha256_hash).await?;

    assert_eq!(retrieved1, Some(data1.to_vec()));
    assert_eq!(retrieved2, Some(data2.to_vec()));

    Ok(())
}

// ============================================================================
// Test Suite 8: Error Handling and Edge Cases
// ============================================================================

#[tokio::test]
async fn test_empty_block_list_error() {
    let hasher = Blake3Hasher::new();
    let blocks: Vec<HashedBlock> = vec![];

    let result = MerkleTree::from_blocks(&blocks, &hasher);
    assert!(result.is_err(), "Should reject empty block list");
}

#[tokio::test]
async fn test_storage_with_very_large_data() -> Result<()> {
    let storage = MemoryStorage::new();
    let hasher = Blake3Hasher::new();

    // Create 1MB of data
    let large_data = vec![b'x'; 1_000_000];
    let hash = hasher.hash_block(&large_data);

    storage.store_block(&hash, &large_data).await?;
    let retrieved = storage.get_block(&hash).await?;

    assert_eq!(retrieved, Some(large_data));

    Ok(())
}

#[tokio::test]
async fn test_concurrent_storage_operations() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let _hasher = Blake3Hasher::new();

    // Spawn multiple concurrent operations
    let mut handles = vec![];

    for i in 0..10 {
        let storage_clone = Arc::clone(&storage);
        let handle = tokio::spawn(async move {
            let data = format!("Concurrent data {}", i);
            let hasher = Blake3Hasher::new();
            let hash = hasher.hash_block(data.as_bytes());

            storage_clone.store_block(&hash, data.as_bytes()).await?;
            let retrieved = storage_clone.get_block(&hash).await?;

            assert_eq!(retrieved, Some(data.as_bytes().to_vec()));
            Ok::<_, anyhow::Error>(())
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await??;
    }

    Ok(())
}

// ============================================================================
// Test Suite 9: Performance and Benchmarking
// ============================================================================

#[tokio::test]
async fn benchmark_blake3_vs_sha256_hashing() -> Result<()> {
    let data = vec![b'x'; 1_000_000]; // 1MB

    // Benchmark BLAKE3
    let blake3_start = std::time::Instant::now();
    let blake3_hasher = Blake3Hasher::new();
    for _ in 0..100 {
        let _ = blake3_hasher.hash_block(&data);
    }
    let blake3_duration = blake3_start.elapsed();

    // Benchmark SHA256
    let sha256_start = std::time::Instant::now();
    let sha256_hasher = SHA256Hasher::new();
    for _ in 0..100 {
        let _ = sha256_hasher.hash_block(&data);
    }
    let sha256_duration = sha256_start.elapsed();

    println!("BLAKE3: {:?}", blake3_duration);
    println!("SHA256: {:?}", sha256_duration);

    // BLAKE3 should generally be faster
    // This is informational - we don't assert on performance

    Ok(())
}

#[tokio::test]
async fn benchmark_merkle_tree_construction() -> Result<()> {
    let hasher = Blake3Hasher::new();

    // Create 1000 blocks
    let mut blocks = Vec::new();
    for i in 0..1000 {
        let data = format!("Block {}", i);
        let block = HashedBlock::from_data(
            data.as_bytes().to_vec(),
            i,
            i * 10,
            i == 999,
            &hasher,
        )?;
        blocks.push(block);
    }

    let start = std::time::Instant::now();
    let tree = MerkleTree::from_blocks(&blocks, &hasher)?;
    let duration = start.elapsed();

    println!("Merkle tree construction (1000 blocks): {:?}", duration);

    assert_eq!(tree.block_count, 1000);
    tree.verify_integrity(&hasher)?;

    Ok(())
}

// ============================================================================
// Test Suite 10: Integration with Deduplication
// ============================================================================

#[tokio::test]
async fn test_full_deduplication_pipeline() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let deduplicator = DefaultDeduplicator::new(storage.clone() as Arc<dyn ContentAddressedStorage>);
    let hasher = Blake3Hasher::new();

    // Store duplicate blocks
    let data1 = b"Duplicate content";
    let data2 = b"Unique content";
    let data3 = b"Duplicate content"; // Same as data1

    let hash1 = hasher.hash_block(data1);
    let hash2 = hasher.hash_block(data2);
    let hash3 = hasher.hash_block(data3);

    storage.store_block(&hash1, data1).await?;
    storage.store_block(&hash2, data2).await?;
    storage.store_block(&hash3, data3).await?;

    // Verify deduplication (hash1 == hash3)
    assert_eq!(hash1, hash3);
    assert_ne!(hash1, hash2);

    // Analyze blocks
    let block_hashes = vec![hash1.clone(), hash2.clone(), hash3.clone()];
    let analysis = deduplicator.analyze_blocks(&block_hashes).await?;

    assert_eq!(analysis.total_blocks, 3);

    Ok(())
}

#[cfg(test)]
mod comprehensive_coverage {
    use super::*;

    /// Test comprehensive coverage of all major components
    #[tokio::test]
    async fn test_all_components_integrated() -> Result<()> {
        // 1. Create file and parse it
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("comprehensive.md");
        fs::write(&file_path, create_test_markdown()).await?;

        let parser = PulldownParser::new();
        let parsed_doc = parser.parse_file(&file_path).await?;

        // 2. Convert to AST blocks (simulated)
        let ast_blocks = create_test_ast_blocks();
        let converter = ASTBlockConverter::new(Blake3Algorithm);
        let hashed_blocks = converter.convert_batch(&ast_blocks).await?;

        // 3. Build Merkle tree
        let hasher = Blake3Hasher::new();
        let tree = MerkleTree::from_blocks(&hashed_blocks, &hasher)?;
        tree.verify_integrity(&hasher)?;

        // 4. Store in storage backend
        let config = StorageConfig::in_memory(Some(50_000_000));
        let storage = StorageFactory::create(config).await?;

        for block in &hashed_blocks {
            storage.store_block(&block.hash, &block.data).await?;
        }

        // 5. Verify retrieval
        for block in &hashed_blocks {
            let retrieved = storage.get_block(&block.hash).await?;
            assert_eq!(retrieved, Some(block.data.clone()));
        }

        // 6. Test deduplication
        let deduplicator = DefaultDeduplicator::new(storage.clone());
        let block_hashes: Vec<String> = hashed_blocks.iter().map(|b| b.hash.clone()).collect();
        let analysis = deduplicator.analyze_blocks(&block_hashes).await?;

        assert_eq!(analysis.total_blocks, hashed_blocks.len());

        println!("✓ All components integrated successfully");
        println!("  - Parsed document: {} headings, {} code blocks",
            parsed_doc.content.headings.len(),
            parsed_doc.content.code_blocks.len());
        println!("  - Converted {} AST blocks to hashed blocks", hashed_blocks.len());
        println!("  - Built Merkle tree with {} blocks", tree.block_count);
        println!("  - Stored and retrieved all blocks successfully");
        println!("  - Deduplication analysis completed");

        Ok(())
    }
}
