//! Enhanced Parser Integration Examples
//!
//! This module provides comprehensive examples demonstrating how to use the enhanced
//! Parser Integration Bridge with content-addressed storage, Merkle trees, and change detection.
//!
//! ## Examples Covered
//!
//! - Basic storage-aware parsing
//! - Batch processing with coordinator
//! - Change detection and comparison
//! - Transaction support
//! - Performance optimization
//! - Error handling and recovery

use crate::hashing::blake3::Blake3Hasher;
use crate::parser::coordinator::factory as coordinator_factory;
use crate::parser::storage_bridge::factory as parser_factory;
use crate::parser::{
    CoordinatorConfig, OperationMetadata, OperationPriority, OperationType,
    ParserStorageCoordinator, ParsingOperation, StorageAwareMarkdownParser,
    StorageAwareParserConfig,
};
use crate::storage::{BlockSize, ContentAddressedStorage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Example 1: Basic Storage-Aware Parsing
///
/// Demonstrates how to parse a markdown note with automatic storage integration,
/// Merkle tree creation, and change detection.
pub async fn basic_storage_aware_parsing_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Basic Storage-Aware Parsing Example ===\n");

    // Create a storage-aware parser with default configuration
    let parser = parser_factory::create_storage_aware_parser();

    // Create a mock storage backend (in a real application, use a proper backend)
    let storage = create_mock_storage_backend();

    // Parse some content
    let content = r#"
# Advanced Rust Programming

## Introduction

This note covers advanced Rust programming concepts including:

- Memory management and ownership
- Concurrency with async/await
- Error handling patterns
- Performance optimization

## Memory Management

Rust's ownership system ensures memory safety without garbage collection:

```rust
fn process_data(data: Vec<String>) -> usize {
    data.len() // data is automatically freed here
}
```

## Conclusion

Master these concepts to become proficient in Rust development.
"#;

    let source_path = Path::new("advanced_rust.md");

    // Parse with storage integration
    let result = parser
        .parse_content_with_storage(content, source_path, Some(storage))
        .await?;

    println!("✅ Parsed note successfully!");
    println!("📄 Note path: {}", result.note.path.display());
    println!("🔤 Content hash: {}", result.content_hash);
    println!("🧱 Number of blocks: {}", result.blocks.len());
    println!("🌳 Merkle tree created: {}", result.merkle_tree.is_some());

    if let Some(tree) = &result.merkle_tree {
        println!("🌳 Tree root hash: {}", tree.root_hash);
        println!("🌳 Tree depth: {}", tree.depth);
        println!("🌳 Tree block count: {}", tree.block_count);
    }

    // Display parsing statistics
    println!("\n📊 Parsing Statistics:");
    println!("   ⏱️  Parse time: {}ms", result.statistics.parse_time_ms);
    println!(
        "   💾 Storage time: {}ms",
        result.statistics.storage_time_ms
    );
    println!(
        "   📦 Content size: {} bytes",
        result.statistics.content_size_bytes
    );
    println!(
        "   🔄 Deduplication ratio: {:.2}",
        result.statistics.deduplication_ratio
    );
    println!(
        "   🔀 Parallel processing: {}",
        result.statistics.parallel_processing_used
    );

    Ok(())
}

/// Example 2: Batch Processing with Coordinator
///
/// Demonstrates how to process multiple documents efficiently using the coordinator.
pub async fn batch_processing_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Batch Processing Example ===\n");

    // Create coordinator with custom configuration
    let coordinator_config = CoordinatorConfig {
        max_concurrent_operations: 4,
        enable_parallel_processing: true,
        operation_timeout_seconds: 120,
        enable_rollback: true,
        cache_size: 500,
        max_batch_size: 50,
        ..Default::default()
    };

    let coordinator =
        coordinator_factory::create_default_coordinator(Some(coordinator_config)).await?;

    // Create multiple parsing operations
    let documents = vec![
        ("document1.md", "# Note 1\n\nContent for note 1..."),
        ("document2.md", "# Note 2\n\nContent for note 2..."),
        ("document3.md", "# Note 3\n\nContent for note 3..."),
        ("document4.md", "# Note 4\n\nContent for note 4..."),
    ];

    let operations: Vec<ParsingOperation> = documents
        .into_iter()
        .enumerate()
        .map(|(i, (filename, content))| ParsingOperation {
            id: format!("batch_op_{}", i + 1),
            source_path: PathBuf::from(filename),
            content: Some(content.to_string()),
            operation_type: OperationType::FromContent,
            priority: OperationPriority::Normal,
            metadata: OperationMetadata {
                initiator: "batch_example".to_string(),
                tags: vec!["batch".to_string(), "example".to_string()],
                custom_fields: {
                    let mut fields = HashMap::new();
                    fields.insert("batch_id".to_string(), "example_batch_1".to_string());
                    fields
                },
                ..Default::default()
            },
        })
        .collect();

    println!("🚀 Processing {} documents in batch...", operations.len());

    // Process batch with transaction support
    let batch_result = coordinator.process_batch(operations, true).await?;

    println!("✅ Batch processing completed!");
    println!("📊 Batch ID: {}", batch_result.batch_id);
    println!("⏱️  Total duration: {}ms", batch_result.total_duration_ms);
    println!(
        "✅ Successful operations: {}",
        batch_result.successful_operations
    );
    println!("❌ Failed operations: {}", batch_result.failed_operations);
    println!("🎯 Overall success: {}", batch_result.success);

    // Display aggregate statistics
    let stats = batch_result.aggregate_statistics;
    println!("\n📈 Aggregate Statistics:");
    println!("   📄 Total documents: {}", stats.total_documents);
    println!(
        "   📦 Total content size: {} bytes",
        stats.total_content_size
    );
    println!("   🧱 Total blocks: {}", stats.total_blocks);
    println!("   🔀 Total unique blocks: {}", stats.total_unique_blocks);
    println!(
        "   🔄 Average deduplication ratio: {:.3}",
        stats.average_deduplication_ratio
    );
    println!(
        "   ⏱️  Average parse time: {:.1}ms",
        stats.average_parse_time_ms
    );
    println!(
        "   💾 Average storage time: {:.1}ms",
        stats.average_storage_time_ms
    );
    println!("   🔄 Total changes detected: {}", stats.total_changes);

    Ok(())
}

/// Example 3: Change Detection and Comparison
///
/// Demonstrates how to detect changes between note versions.
pub async fn change_detection_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Change Detection Example ===\n");

    let parser = parser_factory::create_storage_aware_parser();
    let storage = create_mock_storage_backend();

    // Original note
    let original_content = r#"
# Project Roadmap

## Phase 1: Foundation
- Set up development environment
- Create basic architecture
- Implement core features

## Phase 2: Enhancement
- Add advanced features
- Improve performance
- User testing

## Phase 3: Release
- Bug fixes
- Documentation
- Launch preparation
"#;

    let source_path = Path::new("roadmap.md");

    println!("📄 Parsing original note...");
    let original_result = parser
        .parse_content_with_storage(original_content, source_path, Some(Arc::clone(&storage)))
        .await?;

    println!("✅ Original note parsed");
    println!("🔤 Original hash: {}", original_result.content_hash);

    // Modified note
    let modified_content = r#"
# Project Roadmap

## Phase 1: Foundation
- Set up development environment
- Create basic architecture
- Implement core features
- Add unit tests

## Phase 2: Enhancement
- Add advanced features
- Improve performance
- User testing
- Performance optimization

## Phase 3: Release
- Bug fixes
- Documentation
- Launch preparation

## Phase 4: Maintenance
- Ongoing support
- Feature updates
- Community management
"#;

    println!("\n📄 Parsing modified note...");
    let modified_result = parser
        .parse_and_compare(
            modified_content,
            source_path,
            &original_result,
            Some(Arc::clone(&storage)),
        )
        .await?;

    println!("✅ Modified note parsed");
    println!("🔤 Modified hash: {}", modified_result.content_hash);

    // Analyze changes
    if let Some(changes) = &modified_result.changes {
        println!("\n🔍 Changes Detected:");
        println!("   📊 Total changes: {}", changes.len());

        for (i, change) in changes.iter().enumerate() {
            match change {
                crate::storage::EnhancedTreeChange::AddedBlock { index, hash, .. } => {
                    println!(
                        "   ➕ {}: Added block at index {} (hash: {})",
                        i + 1,
                        index,
                        &hash[..8]
                    );
                }
                crate::storage::EnhancedTreeChange::ModifiedBlock {
                    index,
                    old_hash,
                    new_hash,
                    similarity_score,
                    ..
                } => {
                    println!("   🔄 {}: Modified block at index {}", i + 1, index);
                    println!("       📝 Similarity: {:.1}%", similarity_score * 100.0);
                    println!("       🔤 Old hash: {}...", &old_hash[..8]);
                    println!("       🔤 New hash: {}...", &new_hash[..8]);
                }
                crate::storage::EnhancedTreeChange::DeletedBlock { index, hash, .. } => {
                    println!(
                        "   ➖ {}: Deleted block at index {} (hash: {})",
                        i + 1,
                        index,
                        &hash[..8]
                    );
                }
                crate::storage::EnhancedTreeChange::MovedBlock {
                    old_index,
                    new_index,
                    hash,
                    ..
                } => {
                    println!(
                        "   ↔️  {}: Moved block {} → {} (hash: {})",
                        i + 1,
                        old_index,
                        new_index,
                        &hash[..8]
                    );
                }
                _ => {
                    println!("   ❓ {}: Other change type", i + 1);
                }
            }
        }
    } else {
        println!("\n🔍 No changes detected");
    }

    // Compare statistics
    let original_stats = original_result.statistics;
    let modified_stats = modified_result.statistics;

    println!("\n📊 Statistics Comparison:");
    println!("                Original | Modified");
    println!(
        "   📦 Content size: {:8} | {:8}",
        original_stats.content_size_bytes, modified_stats.content_size_bytes
    );
    println!(
        "   🧱 Block count:   {:8} | {:8}",
        original_stats.block_count, modified_stats.block_count
    );
    println!(
        "   ⏱️  Parse time:    {:8}ms | {:8}ms",
        original_stats.parse_time_ms, modified_stats.parse_time_ms
    );

    Ok(())
}

/// Example 4: Performance Optimization
///
/// Demonstrates performance optimization techniques.
pub async fn performance_optimization_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Performance Optimization Example ===\n");

    // Create optimized parser configuration
    let parser_config = StorageAwareParserConfig {
        block_size: BlockSize::Adaptive {
            min: 1024,
            max: 16384,
        }, // Adaptive sizing
        enable_storage: true,
        enable_merkle_trees: true,
        enable_change_detection: true,
        enable_deduplication: true,
        store_metadata: true,
        enable_parallel_processing: true,
        parallel_threshold: 32 * 1024, // 32KB threshold for parallel processing
    };

    // Create parser with custom hasher
    let custom_hasher = Arc::new(Blake3Hasher::new());
    let base_parser = Box::new(crate::parser::bridge::ParserAdapter::new());
    let parser = Arc::new(
        crate::parser::storage_bridge::StorageAwareParser::with_config(
            base_parser,
            parser_config,
            custom_hasher,
        ),
    );

    // Generate large content for testing
    let large_content = generate_large_markdown_content(500); // 500 lines
    let storage = create_mock_storage_backend();
    let source_path = Path::new("large_document.md");

    println!(
        "📄 Generated large note ({} lines, {} bytes)",
        large_content.lines().count(),
        large_content.len()
    );

    // Parse with timeout to prevent hanging
    println!("⚡ Parsing with performance optimizations...");
    let parse_result = timeout(
        Duration::from_secs(30),
        parser.parse_content_with_storage(&large_content, source_path, Some(storage)),
    )
    .await??;

    println!("✅ Large note parsed successfully!");
    println!("🔤 Content hash: {}", parse_result.content_hash);
    println!("🧱 Blocks created: {}", parse_result.blocks.len());
    println!(
        "🌳 Merkle tree depth: {}",
        parse_result.merkle_tree.as_ref().map_or(0, |t| t.depth)
    );

    // Performance metrics
    let stats = parse_result.statistics;
    println!("\n⚡ Performance Metrics:");
    println!("   ⏱️  Parse time: {}ms", stats.parse_time_ms);
    println!("   💾 Storage time: {}ms", stats.storage_time_ms);
    println!("   📦 Content size: {} bytes", stats.content_size_bytes);
    println!("   🧱 Block count: {}", stats.block_count);
    println!("   🔀 Unique blocks: {}", stats.unique_blocks);
    println!(
        "   🔄 Deduplication ratio: {:.3}",
        stats.deduplication_ratio
    );
    println!(
        "   🚀 Parallel processing: {}",
        stats.parallel_processing_used
    );

    // Calculate processing throughput
    if stats.parse_time_ms > 0 {
        let throughput_mbps = (stats.content_size_bytes as f64) / (stats.parse_time_ms as f64)
            * 1000.0
            / (1024.0 * 1024.0);
        println!("   📈 Throughput: {:.2} MB/s", throughput_mbps);
    }

    Ok(())
}

/// Example 5: Error Handling and Recovery
///
/// Demonstrates robust error handling and recovery mechanisms.
pub async fn error_handling_example() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Error Handling and Recovery Example ===\n");

    let coordinator = coordinator_factory::create_default_coordinator(None).await?;

    // Create operations with potential issues
    let operations = vec![
        ParsingOperation {
            id: "valid_op".to_string(),
            source_path: PathBuf::from("valid.md"),
            content: Some("# Valid Note\n\nThis should parse successfully.".to_string()),
            operation_type: OperationType::FromContent,
            priority: OperationPriority::Normal,
            metadata: OperationMetadata::default(),
        },
        ParsingOperation {
            id: "empty_op".to_string(),
            source_path: PathBuf::from("empty.md"),
            content: Some("".to_string()), // Empty content
            operation_type: OperationType::FromContent,
            priority: OperationPriority::Normal,
            metadata: OperationMetadata::default(),
        },
        ParsingOperation {
            id: "missing_content_op".to_string(),
            source_path: PathBuf::from("missing.md"),
            content: None, // Missing content for FromContent operation
            operation_type: OperationType::FromContent,
            priority: OperationPriority::Normal,
            metadata: OperationMetadata::default(),
        },
    ];

    println!("🧪 Processing operations with potential issues...");

    // Process batch with error handling
    let batch_result = coordinator.process_batch(operations, true).await?;

    println!("✅ Batch processing completed with error handling");
    println!("📊 Batch Results:");
    println!("   ✅ Successful: {}", batch_result.successful_operations);
    println!("   ❌ Failed: {}", batch_result.failed_operations);

    // Analyze individual operation results
    println!("\n🔍 Individual Operation Results:");
    for result in &batch_result.operation_results {
        println!(
            "   {} {}: {}",
            if result.success { "✅" } else { "❌" },
            result.operation_id,
            if result.success {
                "Success".to_string()
            } else {
                result
                    .error
                    .as_ref()
                    .unwrap_or(&"Unknown error".to_string())
                    .clone()
            }
        );
    }

    // Demonstrate error recovery
    if !batch_result.success {
        println!("\n🔄 Demonstrating error recovery...");

        // Retry only the failed operations
        let failed_operations: Vec<ParsingOperation> = batch_result
            .operation_results
            .into_iter()
            .filter(|r| !r.success)
            .map(|r| ParsingOperation {
                id: format!("retry_{}", r.operation_id),
                source_path: PathBuf::from("retry.md"),
                content: Some("# Retry Note\n\nThis should work now.".to_string()),
                operation_type: OperationType::FromContent,
                priority: OperationPriority::High,
                metadata: OperationMetadata {
                    initiator: "error_recovery".to_string(),
                    ..Default::default()
                },
            })
            .collect();

        if !failed_operations.is_empty() {
            let retry_result = coordinator.process_batch(failed_operations, false).await?;
            println!(
                "🔄 Retry completed: {} successful, {} failed",
                retry_result.successful_operations, retry_result.failed_operations
            );
        }
    }

    Ok(())
}

/// Helper function to create a mock storage backend
fn create_mock_storage_backend() -> Arc<dyn ContentAddressedStorage> {
    Arc::new(crate::parser::coordinator::MockStorageBackend::new())
}

/// Helper function to generate large markdown content for testing
fn generate_large_markdown_content(lines: usize) -> String {
    let mut content = String::new();
    content.push_str("# Large Note\n\n");
    content.push_str("This is a large note generated for performance testing.\n\n");

    for i in 1..=lines {
        if i % 20 == 0 {
            content.push_str(&format!("## Section {}\n\n", i / 20));
        }

        content.push_str(&format!(
            "This is line {} of the note. It contains some sample text to test parsing performance. ",
            i
        ));

        if i % 3 == 0 {
            content.push_str("Some **bold text** and *italic text* for variety. ");
        }

        if i % 5 == 0 {
            content.push_str(&format!("- Item {} in a list\n", i / 5));
        }

        if i % 7 == 0 {
            content.push_str(&format!(
                "```rust\nfn example_{}() {{\n    println!(\"Example {}\");\n}}\n```\n\n",
                i, i
            ));
        } else {
            content.push_str("\n");
        }
    }

    content.push_str("\n## Conclusion\n\nEnd of large note.\n");
    content
}

/// Run all examples
pub async fn run_all_examples() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Enhanced Parser Integration Bridge Examples\n");
    println!(
        "This demo showcases the comprehensive integration between parsing and storage systems.\n"
    );

    // Run all examples
    basic_storage_aware_parsing_example().await?;
    batch_processing_example().await?;
    change_detection_example().await?;
    performance_optimization_example().await?;
    error_handling_example().await?;

    println!("\n🎉 All examples completed successfully!");
    println!("\n📚 Key Features Demonstrated:");
    println!("   ✅ Storage-aware parsing with automatic Merkle tree creation");
    println!("   ✅ Batch processing with parallel operations");
    println!("   ✅ Change detection and note comparison");
    println!("   ✅ Performance optimization techniques");
    println!("   ✅ Robust error handling and recovery");
    println!("   ✅ Transaction support for batch operations");
    println!("   ✅ Content deduplication and efficient storage");
    println!("   ✅ Comprehensive statistics and monitoring");

    Ok(())
}

// Tests removed: These were examples with assert wrappers, not real behavioral tests.
// The example functions remain available for documentation purposes.
// See SOLID refactoring decision: removed 7 low-value tests that only verified "no crash"
// without validating actual behavior.
