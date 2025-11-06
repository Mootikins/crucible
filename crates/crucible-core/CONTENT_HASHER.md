# ContentHasher Trait Implementation

This document describes the ContentHasher trait implementation as part of the architectural refactoring for file system operations in Crucible.

## Overview

The ContentHasher trait provides a foundation for content hashing and change detection throughout the Crucible system. It enables dependency inversion by defining abstractions for hashing operations that can be implemented by different modules.

## Architecture

### Core Components

1. **ContentHasher Trait** (`crucible-core/src/traits/change_detection.rs`)
   - Abstraction for file and content block hashing
   - Async operations with proper error handling
   - Support for multiple hash algorithms (BLAKE3, SHA256)

2. **Hash Types** (`crucible-core/src/types/hashing.rs`)
   - `FileHash`: 32-byte BLAKE3 hash for files
   - `BlockHash`: 32-byte hash for content blocks
   - `HashAlgorithm`: Enum for supported algorithms
   - `FileHashInfo` / `BlockHashInfo`: Hashes with metadata

3. **FileHasher Implementation** (`crucible-core/src/hashing/file_hasher.rs`)
   - Concrete implementation of ContentHasher
   - Streaming I/O for large files
   - Batch processing support
   - Memory-efficient operations

4. **Storage Traits**
   - `HashLookupStorage`: For storing/retrieving hash information
   - `ChangeDetector`: For detecting file changes

## Key Features

### ✅ Async Support
All operations are async and non-blocking, suitable for high-throughput scenarios.

### ✅ Streaming I/O
Large files are processed with constant memory usage through streaming operations.

### ✅ Algorithm Agnostic
Supports both BLAKE3 (fast, modern) and SHA256 (widely compatible) algorithms.

### ✅ Batch Processing
Efficient handling of multiple files/blocks through concurrent operations.

### ✅ Comprehensive Error Handling
Specific error types for different failure scenarios.

### ✅ Thread Safety
All implementations are `Send + Sync` and can be shared across threads.

## Usage Examples

### Basic File Hashing

```rust
use crucible_core::{
    traits::change_detection::ContentHasher,
    hashing::FileHasher,
    types::hashing::HashAlgorithm,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let hasher = FileHasher::new(HashAlgorithm::Blake3);
    let path = Path::new("document.md");

    let hash = hasher.hash_file(path).await?;
    println!("File hash: {}", hash);

    Ok(())
}
```

### Batch Processing

```rust
let hasher = FileHasher::new(HashAlgorithm::Blake3);
let paths = vec![
    PathBuf::from("doc1.md"),
    PathBuf::from("doc2.md"),
    PathBuf::from("doc3.md"),
];

let hashes = hasher.hash_files_batch(&paths).await?;
for (path, hash) in paths.iter().zip(hashes.iter()) {
    println!("{}: {}", path.display(), hash);
}
```

### Content Block Hashing

```rust
let hasher = FileHasher::new(HashAlgorithm::Blake3);

// Hash individual content blocks
let heading = hasher.hash_block("# Introduction").await?;
let paragraph = hasher.hash_block("This is the introduction.").await?;

// Create detailed block info
let block_info = hasher.hash_block_info(
    "# Introduction",
    "heading".to_string(),
    0,
    13,
).await?;
```

### Change Detection Workflow

```rust
use crucible_core::{
    traits::change_detection::{HashLookupStorage, ChangeDetector},
    types::hashing::FileHashInfo,
};

// Store current file hashes
let storage: Arc<dyn HashLookupStorage> = /* implementation */;
let files = vec![file_info1, file_info2];
storage.store_hashes(&files).await?;

// Later, detect changes
let change_detector = /* implementation */;
let current_files = /* scan current directory */;
let changes = change_detector.detect_changes(&current_files).await?;

println!("Files changed: {}", changes.changed.len());
println!("New files: {}", changes.new.len());
```

## Performance Characteristics

### BLAKE3 Algorithm
- **Speed**: ~5-10 GB/s on modern CPUs
- **Security**: Modern cryptographic hash function
- **Features**: SIMD optimization, streaming mode

### SHA256 Algorithm
- **Speed**: ~2-3 GB/s on modern CPUs
- **Security**: Well-established, widely trusted
- **Features**: Hardware acceleration available

### Memory Usage
- **File Hashing**: O(1) - constant buffer size
- **Batch Operations**: O(n) - proportional to number of files
- **Block Hashing**: O(m) - proportional to block content size

## Integration Points

### 1. File System Scanning
The ContentHasher trait integrates with file system scanning operations to provide hash-based change detection.

### 2. Content Processing
Content blocks (headings, paragraphs, code blocks) are hashed for fine-grained change detection.

### 3. Database Storage
Hash information is stored in databases for persistence and comparison across sessions.

### 4. API Integration
The trait-based design allows easy integration with different storage backends and processing pipelines.

## Dependencies

- `async-trait`: For async trait methods
- `blake3`: BLAKE3 hashing implementation
- `sha2`: SHA256 hashing implementation
- `tokio`: Async runtime and I/O operations
- `hex`: Hex encoding/decoding
- `serde`: Serialization support
- `thiserror`: Error handling
- `futures`: Concurrent operations

## Testing

Comprehensive tests are included:

```bash
# Run all tests
cargo test -p crucible-core

# Run the demo example
cargo run --example content_hasher_demo --package crucible-core

# Run specific hashing tests
cargo test -p crucible-core hashing
```

## Migration Guide

### From Direct Hashing

**Before:**
```rust
let hash = blake3::hash(file_content);
```

**After:**
```rust
let hasher = FileHasher::new(HashAlgorithm::Blake3);
let hash = hasher.hash_file(path).await?;
```

### Benefits of Migration

1. **Async Operations**: Non-blocking I/O throughout
2. **Error Handling**: Comprehensive error types and recovery
3. **Flexibility**: Switch between algorithms without changing code
4. **Performance**: Optimized streaming and batch processing
5. **Testing**: Mock implementations for unit testing
6. **Architecture**: Clean separation of concerns

## Future Enhancements

### Planned Features

1. **Additional Algorithms**: Support for xxHash, MD5 (for compatibility)
2. **Parallel Processing**: Enhanced multi-core utilization
3. **Caching**: In-memory caching for frequently accessed hashes
4. **Compression**: Hash compression for storage efficiency
5. **Incremental Hashing**: Support for partial file updates

### Extension Points

The trait-based architecture allows for:
- Custom hash algorithms
- Specialized storage backends
- Enhanced change detection strategies
- Performance-optimized implementations

## Architectural Benefits

### SOLID Principles

- **Single Responsibility**: Each trait has a focused purpose
- **Open/Closed**: Extensible through new implementations
- **Liskov Substitution**: Any implementation works with the trait
- **Interface Segregation**: Small, focused traits
- **Dependency Inversion**: High-level modules depend on abstractions

### Testability

- Mock implementations for unit testing
- Dependency injection for integration testing
- Isolated components for focused testing

### Maintainability

- Clear module boundaries
- Comprehensive documentation
- Consistent error handling
- Well-defined interfaces

---

This ContentHasher implementation provides a solid foundation for the file system operations architectural refactoring, enabling efficient change detection and content addressing throughout the Crucible system.