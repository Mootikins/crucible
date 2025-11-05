# Enhanced Change Detection System

This document describes the enhanced change detection system for the Crucible content-addressed storage system.

## Overview

The enhanced change detection system provides advanced capabilities for detecting and managing changes in Merkle trees, including:

- **Granular Change Detection**: Advanced algorithms for detecting moved blocks, reordered content, and similar content
- **Similarity Scoring**: Fuzzy matching for partially modified content with confidence scores
- **Performance Optimizations**: Parallel processing, caching, and efficient algorithms for large documents
- **Change Application System**: Atomic change application with rollback capabilities
- **Comprehensive Metadata**: Timestamps, change sources, and contextual information

## Architecture

### Core Components

1. **EnhancedChangeDetector** (`storage/diff.rs`)
   - Advanced change detection algorithms
   - Similarity analysis and fuzzy matching
   - Performance optimizations (caching, parallel processing)

2. **ChangeApplicationSystem** (`storage/change_application.rs`)
   - Atomic change application with rollback
   - Change validation and conflict resolution
   - Batch operations and optimization

3. **Enhanced Data Types**
   - `EnhancedTreeChange`: Extended change types with metadata
   - `ChangeMetadata`: Comprehensive change information
   - `DiffConfig` & `ApplicationConfig`: Configuration options

### Integration Points

- **MerkleTree Integration**: Enhanced methods on existing `MerkleTree` type
- **Storage Builder Pattern**: Configuration options in `ContentAddressedStorageBuilder`
- **Backward Compatibility**: Original `TreeChange` enum still supported

## Usage Examples

### Basic Enhanced Change Detection

```rust
use crucible_core::storage::{MerkleTree, HashedBlock};
use crucible_core::storage::diff::{EnhancedChangeDetector, ChangeSource};
use crucible_core::hashing::blake3::Blake3Hasher;

// Create hasher and detector
let hasher = Blake3Hasher::new();
let detector = EnhancedChangeDetector::new();

// Create two Merkle trees
let tree1 = MerkleTree::from_blocks(&blocks1, &hasher)?;
let tree2 = MerkleTree::from_blocks(&blocks2, &hasher)?;

// Detect changes with enhanced analysis
let changes = detector.compare_trees(&tree1, &tree2, &hasher, ChangeSource::UserEdit)?;

// Process detected changes
for change in changes {
    match change {
        EnhancedTreeChange::AddedBlock { index, hash, .. } => {
            println!("Added block at index {}: {}", index, hash);
        }
        EnhancedTreeChange::MovedBlock { old_index, new_index, .. } => {
            println!("Block moved from {} to {}", old_index, new_index);
        }
        EnhancedTreeChange::ModifiedBlock { index, similarity_score, .. } => {
            println!("Modified block at {} with similarity: {:.2}", index, similarity_score);
        }
        _ => {}
    }
}
```

### Using MerkleTree Integration Methods

```rust
use crucible_core::storage::{MerkleTree, diff::ChangeSource};

// Direct integration with MerkleTree
let changes = tree1.compare_enhanced(&tree2, &hasher, ChangeSource::Sync)?;

// Apply changes with rollback support
let result = tree1.apply_changes(&changes, &hasher)?;

// Access rollback information
if let Some(rollback_info) = result.rollback_info.rollback_supported {
    let restored_tree = application_system.rollback_changes(&result.rollback_info, &hasher)?;
}
```

### Change Application with Rollback

```rust
use crucible_core::storage::{MerkleTree, diff::EnhancedTreeChange, change_application::ChangeApplicationSystem};

let application_system = ChangeApplicationSystem::new();

// Apply multiple changes atomically
let changes = vec![
    EnhancedTreeChange::AddedBlock { /* ... */ },
    EnhancedTreeChange::ModifiedBlock { /* ... */ },
];

let result = application_system.apply_changes(&tree, &changes, &hasher)?;

// Check results
println!("Applied {} changes", result.applied_changes.len());
println!("Failed {} changes", result.failed_changes.len());

// Rollback if needed
if !result.failed_changes.is_empty() {
    let restored_tree = application_system.rollback_changes(&result.rollback_info, &hasher)?;
}
```

### Configuration Options

```rust
use crucible_core::storage::{
    diff::{EnhancedChangeDetector, DiffConfig},
    change_application::{ChangeApplicationSystem, ApplicationConfig},
};

// Configure change detection
let diff_config = DiffConfig::default()
    .with_similarity_threshold(0.8)
    .with_parallel_processing(100)
    .with_caching(2000);

let detector = EnhancedChangeDetector::with_config(diff_config);

// Configure change application
let app_config = ApplicationConfig::default()
    .with_rollback_support(true)
    .with_strict_validation(true)
    .with_max_batch_size(500);

let application_system = ChangeApplicationSystem::with_config(app_config);
```

### Storage Builder Integration

```rust
use crucible_core::storage::builder::ContentAddressedStorageBuilder;
use crucible_core::storage::BlockSize;

let storage = ContentAddressedStorageBuilder::new()
    .with_backend(StorageBackendType::InMemory)
    .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
    .with_similarity_detection(0.85)
    .with_parallel_processing(200)
    .with_rollback_support(true)
    .with_strict_validation(true)
    .build()?;
```

## Advanced Features

### Similarity Detection

The enhanced system can detect when content has been partially modified rather than completely replaced:

```rust
// Configure similarity detection
let config = DiffConfig::default()
    .with_similarity_threshold(0.7); // 70% similarity threshold

let detector = EnhancedChangeDetector::with_config(config);
let changes = detector.compare_trees(&tree1, &tree2, &hasher, ChangeSource::UserEdit)?;

// Check similarity scores
for change in changes {
    if let EnhancedTreeChange::ModifiedBlock { similarity_score, .. } = change {
        if similarity_score > 0.8 {
            println!("High similarity modification detected");
        }
    }
}
```

### Parallel Processing

For large trees, enable parallel processing for improved performance:

```rust
let config = DiffConfig::default()
    .with_parallel_processing(1000); // Use parallel processing for >1000 blocks

let detector = EnhancedChangeDetector::with_config(config);

// The detector will automatically use parallel processing for large trees
let changes = detector.compare_trees(&large_tree1, &large_tree2, &hasher, source)?;
```

### Change Optimization

The system can optimize changes before application:

```rust
let config = ApplicationConfig::default()
    .with_change_optimization(true)  // Enable optimization
    .with_auto_conflict_resolution(true); // Enable conflict resolution

let application_system = ChangeApplicationSystem::with_config(config);

// Redundant operations (like add+delete) will be automatically optimized
let result = application_system.apply_changes(&tree, &changes, &hasher)?;
```

### Change Sources and Metadata

Track the origin and context of changes:

```rust
use crucible_core::storage::diff::{ChangeMetadata, ChangeSource};

let mut metadata = ChangeMetadata::default();
metadata.source = ChangeSource::Import;
metadata.confidence = 0.95;
metadata.category = Some("bulk_import".to_string());
metadata.context.insert("source_file".to_string(), "document.txt".to_string());

let change = EnhancedTreeChange::AddedBlock {
    index: 5,
    hash: "block_hash".to_string(),
    metadata,
};
```

## Performance Considerations

### Caching

Enable result caching for repeated comparisons:

```rust
let config = DiffConfig::default()
    .with_caching(5000); // Cache up to 5000 comparison results

let detector = EnhancedChangeDetector::with_config(config);

// First comparison will be cached
let changes1 = detector.compare_trees(&tree1, &tree2, &hasher, source)?;

// Subsequent comparisons will use cache
let changes2 = detector.compare_trees(&tree1, &tree2, &hasher, source)?;
```

### Memory Usage

Monitor and optimize memory usage:

```rust
// Check cache statistics
let stats = detector.cache_stats();
println!("Cache entries: {}", stats.entries);
println!("Max entries: {}", stats.max_entries);

// Clear cache if needed
detector.clear_cache();
```

### Batch Operations

Process multiple changes efficiently:

```rust
let config = ApplicationConfig::default()
    .with_max_batch_size(1000)  // Limit batch size
    .with_verify_after_each_change(false); // Skip verification for speed

let application_system = ChangeApplicationSystem::with_config(config);
```

## Error Handling and Validation

### Change Validation

The system provides comprehensive validation:

```rust
let config = ApplicationConfig::default()
    .with_strict_validation(true)  // Enable strict validation
    .with_stop_on_first_error(false); // Continue processing other changes

let application_system = ChangeApplicationSystem::with_config(config);
let result = application_system.apply_changes(&tree, &changes, &hasher)?;

// Check for validation failures
for failed_change in result.failed_changes {
    println!("Change failed: {:?}", failed_change.error);
    if failed_change.recoverable {
        println!("This change can be recovered");
    }
}
```

### Tree Integrity

Verify tree integrity after changes:

```rust
let config = ApplicationConfig::default()
    .with_verify_after_each_change(true); // Verify after each change

let application_system = ChangeApplicationSystem::with_config(config);
let result = application_system.apply_changes(&tree, &changes, &hasher)?;

// All changes have been verified for tree integrity
```

## Best Practices

1. **Configure Similarity Thresholds**: Set appropriate similarity thresholds based on your content type
2. **Enable Caching**: Use caching for repeated comparisons
3. **Monitor Performance**: Use cache statistics and performance metrics
4. **Handle Rollbacks**: Always implement rollback handling for critical operations
5. **Validate Changes**: Use strict validation for production environments
6. **Batch Operations**: Group related changes for better performance
7. **Parallel Processing**: Enable for large trees (>1000 blocks)
8. **Change Sources**: Track change sources for audit trails

## Migration from Basic Change Detection

To migrate from the basic `compare_with` method:

1. **Replace Basic Detection**:
   ```rust
   // Old way
   let basic_changes = tree1.compare_with(&tree2);

   // New way
   let enhanced_changes = tree1.compare_enhanced(&tree2, &hasher, ChangeSource::UserEdit)?;
   ```

2. **Handle Additional Change Types**:
   ```rust
   for change in enhanced_changes {
       match change {
           EnhancedTreeChange::AddedBlock { .. } => { /* handle */ }
           EnhancedTreeChange::ModifiedBlock { similarity_score, .. } => {
               if similarity_score > 0.7 {
                   // Similar content modified
               }
           }
           EnhancedTreeChange::MovedBlock { old_index, new_index, .. } => {
               // Content was moved
           }
           // ... other change types
       }
   }
   ```

3. **Add Rollback Support**:
   ```rust
   let result = tree1.apply_changes(&changes, &hasher)?;

   // Store rollback information for potential undo
   let rollback_info = result.rollback_info;
   ```

## Feature Flags

The enhanced change detection system supports optional features:

- `parallel-processing`: Enable parallel processing using Rayon
- Default: Enabled for performance

```toml
[dependencies]
crucible-core = { version = "0.1", features = ["parallel-processing"] }
```

## Troubleshooting

### Common Issues

1. **High Memory Usage**:
   - Reduce cache size in `DiffConfig`
   - Clear cache periodically with `detector.clear_cache()`

2. **Slow Performance**:
   - Enable parallel processing for large trees
   - Optimize batch size in `ApplicationConfig`
   - Disable strict validation if not needed

3. **Change Detection Accuracy**:
   - Adjust similarity thresholds
   - Enable content analysis for better split/merge detection

4. **Rollback Failures**:
   - Ensure rollback support is enabled
   - Store rollback information properly
   - Verify tree integrity after operations

### Debug Information

Enable debug logging for detailed operation information:

```rust
use tracing::{info, debug, warn};

// Monitor cache performance
let stats = detector.cache_stats();
info!("Cache utilization: {}/{}", stats.entries, stats.max_entries);

// Monitor application performance
let result = application_system.apply_changes(&tree, &changes, &hasher)?;
info!("Applied {} changes in {}ms", result.stats.successful_changes, result.stats.total_time_ms);
```

## API Reference

See the Rust documentation for detailed API information:

- `EnhancedChangeDetector`: Advanced change detection
- `ChangeApplicationSystem`: Change application and rollback
- `EnhancedTreeChange`: Extended change types
- `DiffConfig`: Configuration options for change detection
- `ApplicationConfig`: Configuration options for change application