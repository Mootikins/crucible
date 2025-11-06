# Mock Implementations for Testing

Comprehensive mock implementations of all core traits for fast, deterministic testing.

## Overview

This module provides production-quality mock implementations that enable:

- **Fast Testing**: In-memory operations with zero I/O overhead
- **Deterministic Behavior**: Same inputs always produce same outputs
- **Complete Observability**: Track all operations and verify test expectations
- **Error Injection**: Simulate failures to test error handling paths
- **Isolation**: No external dependencies or side effects

## Available Mocks

### 1. MockHashingAlgorithm

Simple, deterministic hashing algorithm for testing.

**Features:**
- Deterministic hash computation
- 32-byte output (compatible with BLAKE3/SHA256)
- Fast arithmetic-based algorithm
- Not cryptographic (test-only)

**Usage:**

```rust
use crucible_core::test_support::mocks::MockHashingAlgorithm;
use crucible_core::hashing::algorithm::HashingAlgorithm;

let hasher = MockHashingAlgorithm::new();

// Hash data
let hash = hasher.hash(b"test data");
assert_eq!(hash.len(), 32);

// Verify determinism
let hash2 = hasher.hash(b"test data");
assert_eq!(hash, hash2);

// Hex conversion
let hex = hasher.to_hex(&hash);
assert_eq!(hex.len(), 64);
```

**Hash Structure:**
- First 8 bytes: Sum of all input bytes
- Next 8 bytes: XOR of all input bytes
- Next 8 bytes: Input length
- Last 8 bytes: Constant pattern (0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11)

### 2. MockStorage

In-memory storage with comprehensive operation tracking.

**Features:**
- Complete `BlockOperations`, `TreeOperations`, `StorageManagement` trait implementations
- Operation statistics tracking
- Error injection support
- Thread-safe with Arc<Mutex<>>

**Usage:**

```rust
use crucible_core::test_support::mocks::MockStorage;
use crucible_core::storage::traits::BlockOperations;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let storage = MockStorage::new();

// Store blocks
storage.store_block("hash1", b"data1").await?;
storage.store_block("hash2", b"data2").await?;

// Retrieve blocks
let data = storage.get_block("hash1").await?;
assert_eq!(data, Some(b"data1".to_vec()));

// Check statistics
let stats = storage.stats();
assert_eq!(stats.store_count, 2);
assert_eq!(stats.get_count, 1);

// Error injection
storage.set_simulate_errors(true, "Storage full");
let result = storage.store_block("hash3", b"data3").await;
assert!(result.is_err());

// Reset for next test
storage.set_simulate_errors(false, "");
storage.reset();
# Ok(())
# }
```

**Statistics Tracked:**
- `store_count`: Number of block store operations
- `get_count`: Number of block retrieval operations
- `exists_count`: Number of existence checks
- `delete_count`: Number of delete operations
- `store_tree_count`: Number of tree store operations
- `get_tree_count`: Number of tree retrieval operations
- `total_bytes_stored`: Total bytes stored
- `total_bytes_retrieved`: Total bytes retrieved

### 3. MockContentHasher

Configurable content hasher with operation tracking.

**Features:**
- Configure specific hashes for paths/content
- Deterministic fallback for unconfigured inputs
- Operation counting
- Error injection

**Usage:**

```rust
use crucible_core::test_support::mocks::MockContentHasher;
use crucible_core::traits::change_detection::ContentHasher;
use std::path::Path;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let hasher = MockContentHasher::new();

// Configure specific hash for a path
let custom_hash = vec![1u8; 32];
hasher.set_file_hash("test.md", custom_hash.clone());

// Hash file - returns configured hash
let hash = hasher.hash_file(Path::new("test.md")).await?;
assert_eq!(hash.as_bytes(), &custom_hash[..]);

// Unconfigured path uses deterministic fallback
let other_hash = hasher.hash_file(Path::new("other.md")).await?;
assert_eq!(other_hash.as_bytes().len(), 32);

// Configure block hashes
hasher.set_block_hash("content", vec![2u8; 32]);
let block_hash = hasher.hash_block("content").await?;
assert_eq!(block_hash.as_bytes(), &vec![2u8; 32][..]);

// Check operation counts
let (file_count, block_count) = hasher.operation_counts();
assert_eq!(file_count, 2);
assert_eq!(block_count, 1);
# Ok(())
# }
```

**Configuration Methods:**
- `set_file_hash(path, hash)`: Configure hash for specific file path
- `set_block_hash(content, hash)`: Configure hash for specific content
- `set_simulate_errors(enabled, message)`: Enable error injection
- `reset()`: Clear all configuration and statistics

### 4. MockHashLookupStorage

In-memory hash storage with batch operations.

**Features:**
- Complete `HashLookupStorage` trait implementation
- Batch lookup support
- Operation tracking
- Error injection

**Usage:**

```rust
use crucible_core::test_support::mocks::MockHashLookupStorage;
use crucible_core::traits::change_detection::HashLookupStorage;
use crucible_core::types::hashing::{FileHash, FileHashInfo, HashAlgorithm};
use std::time::SystemTime;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let storage = MockHashLookupStorage::new();

// Store file hashes
let file_info = FileHashInfo::new(
    FileHash::new([1u8; 32]),
    1024,
    SystemTime::now(),
    HashAlgorithm::Blake3,
    "test.md".to_string(),
);
storage.store_hashes(&[file_info]).await?;

// Lookup single file
let result = storage.lookup_file_hash("test.md").await?;
assert!(result.is_some());

// Batch lookup
let paths = vec!["test.md".to_string(), "missing.md".to_string()];
let batch_result = storage.lookup_file_hashes_batch(&paths, None).await?;
assert_eq!(batch_result.found_files.len(), 1);
assert_eq!(batch_result.missing_files.len(), 1);

// Check operation counts
let (lookups, batch_lookups, stores) = storage.operation_counts();
assert_eq!(lookups, 1);
assert_eq!(batch_lookups, 1);
assert_eq!(stores, 1);
# Ok(())
# }
```

**Methods:**
- All `HashLookupStorage` trait methods
- `add_stored_hash(path, stored)`: Add hash directly for testing
- `set_simulate_errors(enabled, message)`: Enable error injection
- `operation_counts()`: Get (lookups, batch_lookups, stores) counts
- `reset()`: Clear all data and statistics

### 5. MockChangeDetector

Complete change detection with performance metrics.

**Features:**
- Full `ChangeDetector` trait implementation
- Uses `MockHashLookupStorage` internally
- Performance metrics tracking
- Configurable storage

**Usage:**

```rust
use crucible_core::test_support::mocks::MockChangeDetector;
use crucible_core::traits::change_detection::ChangeDetector;
use crucible_core::types::hashing::{FileHash, FileHashInfo, HashAlgorithm};
use std::time::SystemTime;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let detector = MockChangeDetector::new();

// Store some existing files
let stored_file = FileHashInfo::new(
    FileHash::new([1u8; 32]),
    1024,
    SystemTime::now(),
    HashAlgorithm::Blake3,
    "existing.md".to_string(),
);
detector.storage().store_hashes(&[stored_file]).await?;

// Create current files
let current_files = vec![
    FileHashInfo::new(
        FileHash::new([2u8; 32]),  // Changed hash
        1024,
        SystemTime::now(),
        HashAlgorithm::Blake3,
        "existing.md".to_string(),
    ),
    FileHashInfo::new(
        FileHash::new([3u8; 32]),  // New file
        2048,
        SystemTime::now(),
        HashAlgorithm::Blake3,
        "new.md".to_string(),
    ),
];

// Detect changes with metrics
let result = detector.detect_changes_with_metrics(&current_files).await?;

assert_eq!(result.changes.changed.len(), 1);
assert_eq!(result.changes.new.len(), 1);
assert!(result.has_changes());

// Check performance metrics
println!("Files/second: {}", result.metrics.files_per_second);
println!("Cache hit rate: {}%", result.metrics.cache_hit_rate * 100.0);
# Ok(())
# }
```

**Access Storage:**
```rust
let detector = MockChangeDetector::new();
let storage = detector.storage(); // Access underlying MockHashLookupStorage
```

## Testing Patterns

### Pattern 1: Basic Unit Test

```rust
#[tokio::test]
async fn test_storage_operations() {
    let storage = MockStorage::new();

    // Test operation
    storage.store_block("hash1", b"data").await.unwrap();

    // Verify
    let data = storage.get_block("hash1").await.unwrap();
    assert_eq!(data, Some(b"data".to_vec()));

    // Check statistics
    let stats = storage.stats();
    assert_eq!(stats.store_count, 1);
    assert_eq!(stats.get_count, 1);
}
```

### Pattern 2: Error Path Testing

```rust
#[tokio::test]
async fn test_storage_error_handling() {
    let storage = MockStorage::new();

    // Enable error simulation
    storage.set_simulate_errors(true, "Simulated failure");

    // Verify error is returned
    let result = storage.store_block("hash1", b"data").await;
    assert!(result.is_err());

    // Disable errors and verify recovery
    storage.set_simulate_errors(false, "");
    storage.store_block("hash1", b"data").await.unwrap();
}
```

### Pattern 3: Configured Behavior

```rust
#[tokio::test]
async fn test_with_configured_hashes() {
    let hasher = MockContentHasher::new();

    // Configure expected hash
    hasher.set_file_hash("test.md", vec![0x42; 32]);

    // Test code that uses hasher
    let hash = hasher.hash_file(Path::new("test.md")).await.unwrap();
    assert_eq!(hash.as_bytes(), &vec![0x42; 32][..]);
}
```

### Pattern 4: Change Detection Testing

```rust
#[tokio::test]
async fn test_change_detection() {
    let detector = MockChangeDetector::new();

    // Setup: add existing files
    let existing = FileHashInfo::new(/* ... */);
    detector.storage().store_hashes(&[existing]).await.unwrap();

    // Act: detect changes
    let current_files = vec![/* ... */];
    let changes = detector.detect_changes(&current_files).await.unwrap();

    // Assert: verify correct categorization
    assert_eq!(changes.unchanged.len(), 1);
    assert_eq!(changes.changed.len(), 0);
}
```

## Performance Characteristics

All mocks are designed for fast testing:

| Operation | Time Complexity | Space Complexity |
|-----------|----------------|------------------|
| Hash computation | O(n) | O(1) |
| Storage store/get | O(1) expected | O(n) storage |
| Lookup single | O(1) expected | O(n) storage |
| Lookup batch | O(k) where k = batch size | O(k) result |
| Change detection | O(n) where n = current files | O(n) changes |

Typical operation times:
- Hash: < 1 μs
- Storage operation: < 1 μs
- Lookup: < 1 μs
- Change detection (100 files): < 100 μs

## Best Practices

### 1. Reset Between Tests

```rust
#[tokio::test]
async fn test_something() {
    let storage = MockStorage::new();

    // ... test code ...

    // Reset for next use if sharing
    storage.reset();
}
```

### 2. Verify Operation Counts

```rust
#[tokio::test]
async fn test_with_statistics() {
    let storage = MockStorage::new();

    // Do operations
    storage.store_block("hash1", b"data").await.unwrap();
    storage.get_block("hash1").await.unwrap();

    // Verify expected number of calls
    let stats = storage.stats();
    assert_eq!(stats.store_count, 1, "Should store once");
    assert_eq!(stats.get_count, 1, "Should get once");
}
```

### 3. Test Error Handling

```rust
#[tokio::test]
async fn test_handles_storage_errors() {
    let storage = MockStorage::new();
    storage.set_simulate_errors(true, "Storage full");

    let result = my_function(&storage).await;

    // Verify function handles error gracefully
    assert!(result.is_ok() || result_has_fallback_behavior());
}
```

### 4. Use Configured Hashes for Specific Scenarios

```rust
#[tokio::test]
async fn test_duplicate_detection() {
    let hasher = MockContentHasher::new();

    // Configure same hash for different paths (duplicates)
    let same_hash = vec![1u8; 32];
    hasher.set_file_hash("file1.md", same_hash.clone());
    hasher.set_file_hash("file2.md", same_hash.clone());

    // Test duplicate detection logic
    // ...
}
```

## Examples

See the following files for complete examples:

- **`examples/test_mocks_demo.rs`**: Comprehensive demonstration of all mocks
- **`tests/mocks_unit_tests.rs`**: Unit tests showing various testing patterns
- **Module tests**: Inline tests in `mocks.rs` showing basic usage

Run the demo:
```sh
cargo run -p crucible-core --example test_mocks_demo
```

Run the tests:
```sh
cargo test -p crucible-core test_support::mocks
```

## Architecture

### Trait Implementations

The mocks implement all core traits used throughout the system:

```
MockHashingAlgorithm
  └─ HashingAlgorithm (hashing::algorithm)

MockStorage
  ├─ BlockOperations (storage::traits)
  ├─ TreeOperations (storage::traits)
  └─ StorageManagement (storage::traits)

MockContentHasher
  └─ ContentHasher (traits::change_detection)

MockHashLookupStorage
  └─ HashLookupStorage (traits::change_detection)

MockChangeDetector
  └─ ChangeDetector (traits::change_detection)
```

### Thread Safety

All mocks are thread-safe:
- Use `Arc<Mutex<State>>` for interior mutability
- Implement `Send + Sync` for use across threads
- Support concurrent access patterns

### Error Injection

All mocks support error injection via `set_simulate_errors()`:
```rust
mock.set_simulate_errors(true, "Custom error message");
// All operations will fail with the custom message

mock.set_simulate_errors(false, "");
// Operations return to normal
```

## Comparison with Production Implementations

| Feature | Mock | Production |
|---------|------|-----------|
| Speed | ~1 μs | ~100 μs - 1 ms |
| I/O | None | Disk/Network |
| Dependencies | None | Database, FS |
| Determinism | 100% | Variable |
| Observability | Full | Limited |
| Error Control | Full | Limited |

## Limitations

These are test-only implementations with intentional limitations:

1. **Not Cryptographic**: MockHashingAlgorithm is NOT secure
2. **Memory Only**: All data lost on process exit
3. **No Persistence**: Reset clears everything
4. **Simple Implementation**: No optimization for production workloads
5. **No Concurrency Limits**: May use unlimited memory

**Do NOT use in production!**

## Contributing

When adding new traits, please provide corresponding mock implementations following these guidelines:

1. **Deterministic**: Same inputs → same outputs
2. **Observable**: Track all operations
3. **Configurable**: Support error injection
4. **Documented**: Comprehensive doc comments with examples
5. **Tested**: Inline tests demonstrating usage
6. **Thread-Safe**: Implement Send + Sync

## License

Same as the main Crucible project.
