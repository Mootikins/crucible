# File Hash Lookup Implementation

This document describes the implementation of file hash lookup functionality for efficient change detection during file scanning in the Crucible knowledge management system.

## Overview

The hash lookup system enables the file scanner to:

1. **Query stored file hashes** from the database by relative path
2. **Perform batch queries** for multiple files to reduce database round trips
3. **Compare stored hashes with newly computed hashes** to detect changes
4. **Support session caching** for improved performance during scanning
5. **Provide detailed change detection statistics**

## Architecture

### Core Components

#### 1. Hash Lookup Module (`hash_lookup.rs`)

The main module provides several key functions:

- **`lookup_file_hash()`** - Query a single file hash
- **`lookup_file_hashes_batch()`** - Query multiple files in batches
- **`lookup_file_hashes_batch_cached()`** - Batch queries with session caching
- **`lookup_files_by_content_hashes()`** - Find files with identical content
- **`lookup_changed_files_since()`** - Find files modified since a timestamp
- **`check_file_needs_update()`** - Check if a specific file needs processing

#### 2. Scanner Integration (`kiln_scanner.rs`)

The `KilnScanner` has been enhanced with:

- **Hash cache** for session-level caching of lookup results
- **Enhanced scanning methods** with hash lookup integration
- **Change detection analysis** and reporting
- **File filtering** based on hash comparison results

#### 3. Database Schema

The `notes` table includes a `file_hash` field (64-character BLAKE3 hex string) with an index for efficient lookups.

## Key Features

### 1. Efficient Batch Queries

The system uses parameterized batch queries to minimize database round trips:

```rust
let paths = vec!["doc1.md".to_string(), "doc2.md".to_string(), "doc3.md".to_string()];
let result = lookup_file_hashes_batch(&client, &paths, None).await?;
```

### 2. Session Caching

A `HashLookupCache` provides in-memory caching during scanning sessions:

```rust
let mut cache = HashLookupCache::new();
let result = lookup_file_hashes_batch_cached(&client, &paths, None, &mut cache).await?;
```

### 3. Change Detection

The scanner can identify unchanged, changed, and new files:

```rust
let scan_result = scanner.scan_kiln_directory_with_hash_lookup(&kiln_path).await?;
let summary = scanner.get_change_detection_summary(&scan_result);
```

### 4. Performance Optimization

- **Configurable batch sizes** for optimal database performance
- **Parameterized queries** for security and performance
- **Minimal memory overhead** with streaming hash computation
- **Error resilience** with graceful degradation

## Usage Examples

### Basic Hash Lookup

```rust
use crucible_surrealdb::{lookup_file_hash, SurrealClient};

let client = SurrealClient::new(config).await?;
let stored_hash = lookup_file_hash(&client, "projects/README.md").await?;

match stored_hash {
    Some(hash) => {
        println!("Found stored hash: {}", hash.file_hash);
        println!("File size: {} bytes", hash.file_size);
    }
    None => {
        println!("File not found in database");
    }
}
```

### Batch Hash Lookup

```rust
use crucible_surrealdb::{lookup_file_hashes_batch, BatchLookupConfig};

let paths = vec![
    "doc1.md".to_string(),
    "doc2.md".to_string(),
    "doc3.md".to_string(),
];

let config = BatchLookupConfig {
    max_batch_size: 50,
    use_parameterized_queries: true,
    enable_session_cache: true,
};

let result = lookup_file_hashes_batch(&client, &paths, Some(config)).await?;

println!("Found {} files in database", result.found_files.len());
println!("{} files are new", result.missing_files.len());
```

### Enhanced Scanning with Change Detection

```rust
use crucible_surrealdb::{create_kiln_scanner_with_embeddings, KilnScannerConfig};

// Create scanner with hash lookup enabled
let mut config = KilnScannerConfig::default();
config.enable_incremental = true;
config.track_file_changes = true;

let mut scanner = create_kiln_scanner_with_embeddings(
    config,
    &client,
    &embedding_pool,
).await?;

// Perform scan with hash lookup
let scan_result = scanner.scan_kiln_directory_with_hash_lookup(&kiln_path).await?;

// Get change detection summary
if let Some(summary) = scanner.get_change_detection_summary(&scan_result) {
    println!("Total files: {}", summary.total_files);
    println!("Unchanged: {}", summary.unchanged_files);
    println!("Changed: {}", summary.changed_files);
    println!("New: {}", summary.new_files);
}

// Get only files that need processing
let files_needing_processing = scanner.get_files_needing_processing(&scan_result);
println!("{} files need processing", files_needing_processing.len());
```

### Check Individual File for Updates

```rust
use crucible_surrealdb::check_file_needs_update;

let new_hash = "abc123..."; // Computed BLAKE3 hash
let needs_update = check_file_needs_update(&client, "doc.md", new_hash).await?;

if needs_update {
    println!("File has changed and needs reprocessing");
} else {
    println!("File unchanged, can skip processing");
}
```

## Configuration Options

### BatchLookupConfig

```rust
pub struct BatchLookupConfig {
    /// Maximum number of files per database query (default: 100)
    pub max_batch_size: usize,
    /// Use parameterized queries for security (default: true)
    pub use_parameterized_queries: bool,
    /// Enable session-level caching (default: true)
    pub enable_session_cache: bool,
}
```

### Scanner Configuration

```rust
let config = KilnScannerConfig {
    enable_incremental: true,           // Enable change detection
    track_file_changes: true,          // Track file modifications
    change_detection_method: ChangeDetectionMethod::ContentHash,
    batch_size: 16,                    // Files per processing batch
    // ... other configuration options
    ..Default::default()
};
```

## Performance Characteristics

### Database Efficiency

- **Single round trip per batch**: Up to 100 files per query by default
- **Parameterized queries**: Prevents SQL injection and optimizes query plans
- **Indexed lookups**: Fast hash-based retrieval using database indexes

### Memory Usage

- **Streaming hash computation**: Minimal memory overhead for large files
- **Efficient caching**: LRU-style cache with configurable limits
- **Batch processing**: Reduces peak memory usage during scanning

### Scalability

- **Horizontal scaling**: Works with large kilns (10K+ files)
- **Configurable batching**: Tune batch sizes for your database
- **Graceful degradation**: Continues operation even if hash lookups fail

## Error Handling

The system implements comprehensive error handling:

```rust
// Database connection errors are handled gracefully
let result = lookup_file_hashes_batch(&client, &paths, Some(config)).await;

match result {
    Ok(hash_result) => {
        // Process hash lookup results
    }
    Err(e) => {
        // Log error but continue scanning without hash lookup
        warn!("Hash lookup failed: {}, continuing without change detection", e);
    }
}
```

## Monitoring and Debugging

### Cache Statistics

```rust
let cache_stats = scanner.get_hash_cache_stats();
println!("Cache hit rate: {:.1}%", cache_stats.hit_rate * 100.0);
println!("Cache entries: {}", cache_stats.entries);
```

### Logging

The system provides detailed logging at different levels:

```rust
// Debug level: Individual file hash lookups
debug!("Looking up hash for file: {}", relative_path);

// Info level: Batch operations and statistics
info!("Hash lookup complete: {}/{} files found", found, total);

// Warn level: Errors and fallback behavior
warn!("Hash lookup failed, continuing without change detection: {}", e);
```

## Database Schema

### Notes Table Structure

```sql
DEFINE TABLE notes SCHEMAFULL;

DEFINE FIELD path ON TABLE notes TYPE string ASSERT $value != NONE;
DEFINE FIELD file_hash ON TABLE notes TYPE string ASSERT $value != NONE AND string::len($value) == 64;
DEFINE FIELD file_size ON TABLE notes TYPE int;
DEFINE FIELD modified_at ON TABLE notes TYPE datetime DEFAULT time::now();

DEFINE INDEX unique_path ON TABLE notes COLUMNS path UNIQUE;
DEFINE INDEX file_hash_idx ON TABLE notes COLUMNS file_hash;
```

### Query Examples

```sql
-- Single file lookup
SELECT id, path, file_hash, file_size, modified_at FROM notes WHERE path = $path LIMIT 1;

-- Batch file lookup
SELECT id, path, file_hash, file_size, modified_at FROM notes WHERE path IN ($0, $1, $2);

-- Content hash lookup (find duplicates)
SELECT id, path, file_hash FROM notes WHERE file_hash IN ($hash1, $hash2);

-- Changed files since timestamp
SELECT id, path, file_hash, modified_at FROM notes WHERE modified_at > time::('2023-01-01T00:00:00Z');
```

## Integration with Processing Pipeline

The hash lookup system integrates seamlessly with the existing processing pipeline:

1. **File Discovery**: Scanner discovers all files in the kiln
2. **Hash Lookup**: Query database for existing file hashes
3. **Change Detection**: Compare stored vs. computed hashes
4. **Selective Processing**: Only process changed or new files
5. **Hash Storage**: Store new hashes after successful processing

### Example Integration

```rust
// 1. Scan with hash lookup
let scan_result = scanner.scan_kiln_directory_with_hash_lookup(&kiln_path).await?;

// 2. Filter files that need processing
let files_to_process = scanner.get_files_needing_processing(&scan_result);

// 3. Process only changed/new files
let process_result = scanner.process_kiln_files(&files_to_process).await?;

println!("Processed {} out of {} discovered files",
    files_to_process.len(),
    scan_result.discovered_files.len());
```

## Best Practices

### 1. Configuration Tuning

- **Batch size**: Use 50-100 files per batch for optimal performance
- **Cache enabled**: Always enable session caching for repeated scans
- **Parameterized queries**: Keep enabled for security and performance

### 2. Error Handling

- **Graceful degradation**: Continue scanning even if hash lookups fail
- **Logging**: Log hash lookup errors at WARN level
- **Retry logic**: Implement retry for transient database errors

### 3. Performance Monitoring

- **Monitor cache hit rates**: Aim for >80% hit rate in repeated scans
- **Track database round trips**: Minimize through batching
- **Profile hash computation**: Large files may need chunked hashing

### 4. Database Maintenance

- **Index maintenance**: Ensure `file_hash_idx` is properly maintained
- **Cleanup**: Remove stale records for deleted files
- **Backup**: Include hash data in regular backups

## Testing

The implementation includes comprehensive tests:

```bash
# Run hash lookup tests
cargo test -p crucible-surrealdb hash_lookup

# Run integration tests
cargo test -p crucible-surrealdb test_hash_lookup_integration

# Run performance tests
cargo test -p crucible-surrealdb test_hash_lookup_performance
```

### Example Test

```rust
#[tokio::test]
async fn test_batch_hash_lookup() {
    let client = create_test_client().await;
    let paths = vec!["test1.md".to_string(), "test2.md".to_string()];

    let result = lookup_file_hashes_batch(&client, &paths, None).await.unwrap();

    assert_eq!(result.total_queried, 2);
    assert!(result.database_round_trips <= 1);
}
```

## Future Enhancements

Potential improvements to consider:

1. **Background refresh**: Periodic cache updates for long-running scans
2. **Distributed caching**: Redis/memcached for multi-instance deployments
3. **Incremental indexing**: File system watching for real-time updates
4. **Content deduplication**: Advanced duplicate content detection
5. **Hash versioning**: Support for multiple hash algorithms

## Troubleshooting

### Common Issues

1. **Slow hash lookups**: Increase batch size or check database indexes
2. **High memory usage**: Reduce batch size or disable caching
3. **Cache misses**: Ensure cache is properly configured
4. **Database timeouts**: Increase query timeout or reduce batch size

### Debug Information

Enable debug logging to troubleshoot issues:

```rust
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter("crucible_surrealdb::hash_lookup=debug")
    .init();
```

This will show detailed information about hash lookup operations, cache hits/misses, and database queries.