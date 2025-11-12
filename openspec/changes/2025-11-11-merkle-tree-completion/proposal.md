# Merkle Tree Completion for Incremental Updates

**Change ID**: `2025-11-11-merkle-tree-completion`
**Status**: Ready for Implementation
**Created**: 2025-11-11
**Author**: Matthew Krohn

## Why

Based on expert analysis comparing with Oxen AI's production Merkle tree implementation, Crucible's hybrid tree has solid foundations (75% complete) but needs key production patterns for scalability and reliability. The current implementation has performance inefficiencies and missing persistence features that prevent reliable incremental updates.

## What Changes

- **Efficient Hash Infrastructure**: Replace String-based hashes with proper `[u8; 16]` NodeHash type (inspired by Oxen)
- **LRU Caching System**: Implement bounded caching with `lru` crate for memory management
- **Virtual Sections**: Add virtualization for documents with >100 sections (prevents memory issues)
- **Efficient Storage Format**: Binary storage with path sharding for large-scale deployments
- **Thread Safety**: Add `Arc<RwLock<T>>` wrapper for concurrent access
- **Persistence Layer**: Complete SurrealDB storage for tree metadata and section data
- **Integration Points**: Clean connections to `DocumentIngestor` and file watching systems

## Impact

- **Affected specs**: `storage`, `merkle`
- **Affected code**:
  - `crates/crucible-core/src/merkle/` (hash infrastructure, virtualization, caching)
  - `crates/crucible-surrealdb/src/merkle_persistence.rs` (new implementation)
  - `crates/crucible-core/src/hybrid.rs` (performance and thread safety improvements)

## Success Criteria

1. **Efficient Hash Operations**: NodeHash type with optimized hash combination algorithms
2. **Memory Management**: Bounded LRU caching with configurable limits (default 1000 nodes)
3. **Large Document Support**: Virtual sections for documents with >100 sections
4. **Thread Safety**: Concurrent access with `Arc<RwLock<T>>` wrapper
5. **Production Storage**: Binary storage with path sharding for scale
6. **Performance**: Tree operations <50ms for documents up to 10,000 blocks
7. **Integration Ready**: Clean API for file watching and parser systems

## Oxen AI Patterns Adopted

Based on production battle-testing at Oxen AI:
- **NodeHash type** for efficient hash operations (vs current String-based)
- **LRU caching** for memory management (vs current unbounded caching)
- **Virtual sections** for large document handling (vs current flat structure)
- **Binary storage format** with path sharding (vs current JSON storage)
- **Thread-safe wrapper** pattern for concurrent access

## Crucible-Specific Enhancements

Preserving Crucible's innovations while adding production reliability:
- **Hybrid semantic/block structure** maintained
- **Section-based organization** by markdown headings preserved
- **Content-addressed storage** integration maintained
- **Parser integration points** kept consistent