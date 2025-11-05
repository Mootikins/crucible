## Context

The current Crucible system uses simple file-level SHA256 hashing for content integrity, which creates performance bottlenecks and limits scalability. As identified in the architecture analysis, this approach requires full reprocessing for any change and cannot efficiently handle partial modifications. The system also lacks proper dependency inversion patterns, making testing and modularization difficult.

This change implements Phase 2 of the roadmap: "Merkle Tree and Database Layer" with proper architectural patterns for long-term maintainability.

## Goals / Non-Goals

**Goals:**
- Implement efficient content-addressed storage with block-level Merkle trees
- Create dependency inversion patterns for storage and hashing components
- Enable incremental change detection and updates
- Provide clean abstraction layers for testing and extensibility
- Achieve sub-100ms change detection for typical document modifications
- Support cryptographically verifiable audit trails

**Non-Goals:**
- Full synchronization/replication system (that's Phase 6+)
- Agent orchestration integration
- UI layer changes
- Database migration for existing content (handled separately)
- Compression or optimization of storage format

## Decisions

### Decision 1: Block-based Merkle Tree Structure
**What**: Implement content-addressed storage using 4KB blocks organized in a binary Merkle tree structure.

**Why**:
- Efficient change detection: only modified blocks need rehashing
- Supports partial updates without full document reprocessing
- Provides cryptographic integrity verification
- Standard approach used by Git, IPFS, and similar systems

**Alternatives considered**:
- *File-level hashing*: Current approach, inefficient for partial changes
- *Fixed-size chunks*: Less flexible, fragmentation issues
- *Content-defined chunking*: More complex to implement and debug

### Decision 2: Trait-based Dependency Inversion
**What**: Create trait abstractions for storage, hashing, and change detection that can be injected via constructors.

**Why**:
- Enables comprehensive unit testing with mock implementations
- Supports multiple storage backends (SurrealDB, in-memory, file-based)
- Facilitates future extension and customization
- Follows SOLID principles for clean architecture

**Alternatives considered**:
- *Direct dependencies*: Tighter coupling, harder to test
- *Service locator pattern**: Runtime resolution complexity, hidden dependencies

### Decision 3: Builder Pattern for Configuration
**What**: Use builder pattern for configuring storage and hashing components with sensible defaults.

**Why**:
- Provides clear configuration interface
- Enables step-by-step construction with validation
- Supports multiple usage patterns (development, production, testing)
- Makes dependency injection explicit and type-safe

**Alternatives considered**:
- *Constructor parameters*: Can become unwieldy with many options
- *Configuration structs*: Less flexible for different use cases

### Decision 4: Incremental Update Strategy
**What**: Implement change detection by comparing Merkle tree hashes and updating only modified branches.

**Why**:
- Minimizes I/O operations for small changes
- Provides audit trail of what changed
- Supports efficient synchronization for future features
- Maintains performance with large document sets

**Alternatives considered**:
- *Full reprocessing*: Simple but inefficient
- *Timestamp-based comparison*: Less reliable, no integrity verification

## Risks / Trade-offs

### Risk: Implementation Complexity
**Risk**: Merkle tree implementation can be complex and error-prone
**Mitigation**:
- Use well-tested cryptographic libraries
- Implement comprehensive test suite with known-good reference
- Start with simple binary tree structure, evolve if needed
- Add extensive documentation and examples

### Risk: Performance Overhead
**Risk**: Block-based processing may have overhead for very small files
**Mitigation**:
- Adaptive block sizing based on file size
- In-memory caching for frequently accessed blocks
- Performance benchmarking and optimization
- Fallback to simple hashing for very small files (<1KB)

### Risk: Storage Space Increase
**Risk**: Merkle tree metadata increases storage requirements
**Mitigation**:
- Compact storage of intermediate hashes
- Hash deduplication for identical blocks
- Optional tree pruning for historical data
- Storage cost analysis and monitoring

### Trade-off: Complexity vs Performance
**Trade-off**: Increased code complexity for significant performance gains
**Decision**: Accept complexity in exchange for:
- 90%+ performance improvement for incremental changes
- Cryptographic integrity guarantees
- Foundation for future synchronization features
- Better testability and maintainability

## Migration Plan

### Phase 1: Foundation Implementation (Week 1-2)
1. Create trait abstractions for storage and hashing
2. Implement basic Merkle tree data structure
3. Add block-based content processing
4. Create builder patterns for dependency injection

### Phase 2: Integration (Week 3-4)
1. Integrate with existing SurrealDB storage
2. Add change detection pipeline
3. Implement incremental update logic
4. Update CLI commands to use new system

### Phase 3: Testing and Validation (Week 5-6)
1. Comprehensive unit and integration tests
2. Performance benchmarking
3. Migration validation with existing data
4. Documentation and examples

### Rollback Strategy
- Keep existing SHA256 implementation as fallback
- Feature flags to switch between old and new systems
- Database migration scripts to preserve existing data
- Monitoring and alerting for performance regressions

## Open Questions

1. **Block Size Optimization**: What is the optimal block size for typical markdown documents?
   - *Action*: Benchmark with 1KB, 4KB, 8KB block sizes on representative dataset

2. **Tree Structure**: Should we use binary tree or N-ary tree structure?
   - *Action*: Prototype both approaches and measure performance

3. **Hash Algorithm**: SHA256 vs SHA3 vs BLAKE3 for block hashing?
   - *Action*: Evaluate performance characteristics and cryptographic requirements

4. **Storage Schema**: How to best store Merkle tree metadata in SurrealDB?
   - *Action*: Design schema that balances query performance and storage efficiency

5. **Backward Compatibility**: How to handle existing content with simple hashes?
   - *Action*: Implement migration path and compatibility layer

## Technical Architecture

### Core Components

```rust
// Storage abstraction
pub trait ContentAddressedStorage: Send + Sync {
    async fn store_block(&self, hash: &str, data: &[u8]) -> Result<(), StorageError>;
    async fn get_block(&self, hash: &str) -> Result<Option<Vec<u8>>, StorageError>;
    async fn store_tree(&self, root_hash: &str, tree: &MerkleTree) -> Result<(), StorageError>;
    async fn get_tree(&self, root_hash: &str) -> Result<Option<MerkleTree>, StorageError>;
}

// Hashing abstraction
pub trait ContentHasher: Send + Sync {
    fn hash_block(&self, data: &[u8]) -> String;
    fn hash_nodes(&self, left: &str, right: &str) -> String;
}

// Merkle tree structure
pub struct MerkleTree {
    pub root_hash: String,
    pub blocks: Vec<HashedBlock>,
    pub depth: usize,
}

// Builder pattern
pub struct ContentAddressedStorageBuilder {
    block_size: usize,
    hasher: Box<dyn ContentHasher>,
    backend: Box<dyn ContentAddressedStorage>,
}
```

### Data Flow

1. **Document Processing**:
   ```
   Document → Block Splitter → Block Hasher → Merkle Tree Builder → Storage
   ```

2. **Change Detection**:
   ```
   Old Tree + New Document → Block Comparison → Changed Blocks → Incremental Update
   ```

3. **Storage Pattern**:
   ```
   Block Hash → Block Content
   Root Hash → Merkle Tree Metadata
   Document ID → Root Hash + Metadata
   ```

### Integration Points

- **CLI Commands**: Update search, note, and stats commands to use new storage
- **SurrealDB**: Implement ContentAddressedStorage trait with optimized schema
- **Parser Integration**: Connect existing markdown parser to block-based processing
- **Testing**: Provide in-memory mock implementations for comprehensive testing