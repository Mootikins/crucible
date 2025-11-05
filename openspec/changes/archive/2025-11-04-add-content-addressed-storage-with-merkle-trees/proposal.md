## Why

The current system uses simple file-level SHA256 hashing which cannot efficiently detect partial content changes, support incremental updates, or provide verifiable audit trails. This prevents efficient synchronization, makes full reprocessing necessary for any change, and limits scalability. Additionally, the current architecture lacks proper dependency inversion patterns, making testing and modularization difficult.

## What Changes

- **Content-Addressed Storage**: Replace simple file hashing with block-level Merkle tree implementation for efficient change detection and incremental updates
- **Dependency Inversion**: Implement proper abstraction layers with trait-based interfaces for storage, hashing, and change detection
- **Change Detection Pipeline**: Add hash-based diff calculation system for efficient synchronization
- **Storage Abstraction**: Create clean interfaces that separate storage implementation from business logic
- **Performance Optimization**: Enable sub-100ms change detection and incremental processing

## Impact

- **Affected specs**:
  - `content-addressed-storage` (new capability)
  - `dependency-injection` (new capability)
- **Affected code**:
  - `crates/crucible-core/src/storage/` - new storage abstractions
  - `crates/crucible-core/src/hashing/` - new Merkle tree implementation
  - `crates/crucible-surrealdb/src/` - database schema updates
  - `crates/crucible-cli/src/` - CLI integration for change detection
- **Performance impact**:
  - Reduces change detection from O(n) full reprocessing to O(log n) Merkle tree traversal
  - Enables incremental updates for large files
  - Improves synchronization efficiency by >90% for small changes
- **Testing impact**:
  - Enables dependency injection for mockable storage in tests
  - Improves test isolation and speed
  - Supports in-memory storage for unit tests