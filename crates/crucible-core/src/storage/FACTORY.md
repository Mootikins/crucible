# Storage Factory Pattern

## Overview

The Storage Factory provides a centralized, configuration-driven approach to creating storage backend instances in Crucible. It implements the Factory design pattern to encapsulate the complexity of backend creation and enable easy switching between different storage implementations.

## Key Features

- **Configuration-Driven**: Select and configure backends via configuration files or environment variables
- **Type-Safe**: Leverages Rust's type system for compile-time guarantees
- **Extensible**: Easy to add new backends without modifying existing code
- **Testable**: Simple in-memory backend for unit and integration tests
- **Production-Ready**: Comprehensive error handling and validation

## Architecture

```
┌─────────────────┐
│  StorageConfig  │ ← Configuration (from files, env, code)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ StorageFactory  │ ← Factory (creates instances)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   Storage       │ ← Trait-based abstraction
│   Backend       │   (InMemory, FileBased, SurrealDB)
└─────────────────┘
```

## Usage

### Basic Usage

```rust
use crucible_core::storage::factory::{StorageFactory, StorageConfig};

// Create in-memory storage with default settings
let config = StorageConfig::in_memory(Some(1024 * 1024 * 1024)); // 1GB
let storage = StorageFactory::create(config).await?;

// Use the storage
storage.store_block("hash123", b"data").await?;
let data = storage.get_block("hash123").await?;
```

### Configuration-Based Creation

```rust
use crucible_core::storage::factory::{StorageConfig, BackendConfig, HashAlgorithm};

// Create configuration
let config = StorageConfig {
    backend: BackendConfig::InMemory {
        memory_limit: Some(512 * 1024 * 1024),  // 512MB
        enable_lru_eviction: true,
        enable_stats_tracking: true,
    },
    hash_algorithm: HashAlgorithm::Blake3,
    enable_deduplication: true,
    enable_maintenance: true,
    validate_config: true,
};

// Create storage from configuration
let storage = StorageFactory::create(config).await?;
```

### JSON Configuration

```json
{
  "backend": {
    "type": "in_memory",
    "memory_limit": 536870912,
    "enable_lru_eviction": true,
    "enable_stats_tracking": true
  },
  "hash_algorithm": "blake3",
  "enable_deduplication": true,
  "enable_maintenance": true,
  "validate_config": true
}
```

```rust
// Load from JSON
let json = std::fs::read_to_string("storage_config.json")?;
let config: StorageConfig = serde_json::from_str(&json)?;
let storage = StorageFactory::create(config).await?;
```

### Custom Backend Injection

```rust
use crucible_core::storage::memory::MemoryStorage;

// Create custom backend
let custom = Arc::new(MemoryStorage::new());

// Inject via configuration
let config = StorageConfig::custom(custom as Arc<dyn ContentAddressedStorage>);
let storage = StorageFactory::create(config).await?;
```

## Backend Types

### InMemory

Fast, RAM-based storage ideal for testing and temporary data.

**Configuration:**
```rust
BackendConfig::InMemory {
    memory_limit: Some(512 * 1024 * 1024),  // 512MB
    enable_lru_eviction: true,
    enable_stats_tracking: true,
}
```

**Pros:**
- Extremely fast
- No I/O overhead
- Perfect for testing

**Cons:**
- Data not persistent
- Limited by available RAM
- Lost on process restart

### FileBased (Not Yet Implemented)

Local filesystem persistence.

**Configuration:**
```rust
BackendConfig::FileBased {
    directory: PathBuf::from("/var/lib/crucible/storage"),
    create_if_missing: true,
    enable_compression: true,
    size_limit: Some(10 * 1024 * 1024 * 1024),  // 10GB
}
```

### SurrealDB (Requires Dependency Injection)

Production database backend with ACID compliance.

**Configuration:**
```rust
BackendConfig::SurrealDB {
    connection_string: "ws://localhost:8000".to_string(),
    namespace: "crucible".to_string(),
    database: "storage".to_string(),
    connection_timeout_secs: 30,
    max_connections: 10,
}
```

**Note:** Due to circular dependency constraints, SurrealDB backends must be created externally and injected via `BackendConfig::Custom`.

### Custom

Dependency injection for custom implementations.

```rust
BackendConfig::Custom(your_storage_instance)
```

## Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `STORAGE_BACKEND` | Backend type | `in_memory`, `file_based`, `surrealdb` |
| `STORAGE_MEMORY_LIMIT` | Memory limit (bytes) | `1073741824` |
| `STORAGE_DIRECTORY` | Directory path (file-based) | `/var/lib/crucible` |
| `STORAGE_CONNECTION_STRING` | DB connection (SurrealDB) | `ws://localhost:8000` |
| `STORAGE_NAMESPACE` | DB namespace (SurrealDB) | `crucible` |
| `STORAGE_DATABASE` | DB name (SurrealDB) | `storage` |

## Configuration Validation

The factory performs comprehensive validation before creating backends:

```rust
let config = StorageConfig {
    backend: BackendConfig::InMemory {
        memory_limit: Some(0),  // Invalid!
        enable_lru_eviction: true,
        enable_stats_tracking: true,
    },
    ..Default::default()
};

// This will fail with a clear error message
match StorageFactory::create(config).await {
    Err(StorageError::Configuration(msg)) => {
        println!("Validation failed: {}", msg);
        // Output: "In-memory storage memory_limit must be greater than 0"
    }
    _ => {}
}
```

Validation checks include:
- Memory limits > 0
- Non-empty directory paths
- Valid connection strings
- Non-empty namespace/database names
- Connection timeouts > 0
- Max connections > 0

## Error Handling

The factory provides clear, actionable error messages:

```rust
use crucible_core::storage::StorageError;

match StorageFactory::create(config).await {
    Ok(storage) => { /* use storage */ }
    Err(StorageError::Configuration(msg)) => {
        // Configuration validation error
        eprintln!("Config error: {}", msg);
    }
    Err(StorageError::Backend(msg)) => {
        // Backend creation error
        eprintln!("Backend error: {}", msg);
    }
    Err(e) => {
        // Other storage errors
        eprintln!("Error: {}", e);
    }
}
```

## Testing

The factory pattern makes testing straightforward:

```rust
#[tokio::test]
async fn test_my_feature() {
    // Create test storage
    let config = StorageConfig::in_memory(Some(10_000_000));
    let storage = StorageFactory::create(config).await.unwrap();

    // Test your feature
    my_feature(&storage).await;

    // Verify results
    assert!(storage.block_exists("expected_hash").await.unwrap());
}
```

## Performance Considerations

### InMemory Backend
- **Fastest option** for development and testing
- O(1) lookups with HashMap
- Memory usage scales linearly with data
- LRU eviction prevents OOM when limit is set

### SurrealDB Backend
- Persistent, ACID-compliant storage
- Indexed hash lookups for fast retrieval
- Connection pooling for concurrent access
- Transaction support for consistency

## Best Practices

1. **Use InMemory for Tests**: Fast, isolated, no cleanup required
2. **Configure via Environment**: 12-factor app compliance
3. **Validate Early**: Enable validation in production
4. **Set Memory Limits**: Prevent OOM with in-memory backend
5. **Handle Errors**: Always match on `StorageError` variants
6. **Use Custom for Integration**: Inject real implementations via DI
7. **Document Configuration**: Keep config files versioned and documented

## Examples

See `/home/moot/crucible/crates/crucible-core/examples/storage_factory_demo.rs` for comprehensive examples demonstrating:

- In-memory storage creation
- Custom configuration
- Environment-based creation
- Configuration serialization/deserialization
- Error handling
- Custom backend injection
- Different hash algorithms

Run the example:
```bash
cargo run --package crucible-core --example storage_factory_demo
```

## Future Extensions

Potential future backends:

- **Redis**: For distributed caching
- **S3**: For cloud object storage
- **SQLite**: For lightweight persistence
- **PostgreSQL**: For advanced querying
- **RocksDB**: For high-performance local storage

Adding a new backend requires:
1. Implement `ContentAddressedStorage` trait
2. Add variant to `BackendConfig` enum
3. Add creation logic in `StorageFactory::create_backend`
4. Add validation logic in `StorageConfig::validate`
5. Add tests

## API Reference

### `StorageFactory`

Static factory for creating storage backends.

**Methods:**
- `create(config: StorageConfig) -> Result<Arc<dyn ContentAddressedStorage>>`

### `StorageConfig`

Complete storage configuration.

**Constructors:**
- `new() -> Self` - Default configuration
- `in_memory(limit: Option<u64>) -> Self`
- `file_based(directory: impl Into<PathBuf>) -> Self`
- `surrealdb(connection_string, namespace, database) -> Self`
- `custom(backend: Arc<dyn ContentAddressedStorage>) -> Self`

**Methods:**
- `validate(&self) -> StorageResult<()>` - Validate configuration

### `BackendConfig`

Backend-specific configuration enum.

**Variants:**
- `InMemory { memory_limit, enable_lru_eviction, enable_stats_tracking }`
- `FileBased { directory, create_if_missing, enable_compression, size_limit }`
- `SurrealDB { connection_string, namespace, database, connection_timeout_secs, max_connections }`
- `Custom(Arc<dyn ContentAddressedStorage>)`

### `HashAlgorithm`

Hashing algorithm selection.

**Variants:**
- `Blake3` - Recommended for production (fast, secure)
- `Sha256` - Legacy support (slower, widely compatible)

## License

This module is part of the Crucible project and follows the project's license terms.
