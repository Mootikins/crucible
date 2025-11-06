# Design Document: Optimize Data Flow

## Context

The current implementation reprocesses all files on every CLI invocation, regardless of whether content has changed. This creates unnecessary latency and resource usage, particularly for large vaults (1000+ documents).

This design covers incremental processing through file-level and block-level change detection using content hashing and Merkle tree structures.

## Goals / Non-Goals

**Goals:**
- Enable sub-second CLI startup for vaults with no changes
- Reduce embedding API calls by 90%+ for typical edit workflows
- Provide foundation for future sync/collaboration features
- Maintain data consistency and reliability

**Non-Goals:**
- Real-time file watching (future enhancement)
- Distributed consensus or conflict resolution (future)
- Backward compatibility with existing vault data (migration acceptable)

## Decisions

### Decision 1: BLAKE3 for Content Hashing

**Choice:** Use BLAKE3 instead of xxHash, SHA-256, or MD5

**Rationale:**

| Algorithm | Speed (50KB file) | Collision Resistance | Future-Proof |
|-----------|------------------|---------------------|--------------|
| MD5       | ~2ms            | ❌ Broken          | ❌          |
| SHA-256   | ~8ms            | ✅ Strong          | ✅          |
| xxHash128 | ~0.4ms          | ⚠️ Non-crypto      | ❌          |
| **BLAKE3**| **~1ms**        | ✅ **Strong**      | ✅          |

**Why not xxHash?** While 2-3x faster for local-only workloads, our roadmap includes:
- Content-addressed block storage (Section 3)
- Cross-document deduplication (Section 3.5)
- Future sync capabilities (mentioned in proposal)

These features require cryptographic collision resistance to prevent attacks when content crosses trust boundaries. Switching from xxHash to BLAKE3 later would require rehashing all files in all user vaults.

**Performance trade-off:** We accept ~0.6s overhead per 1000 files in exchange for avoiding a painful migration when implementing content-addressed features.

**Alternatives considered:**
- **SHA-256:** Industry standard but 8x slower than BLAKE3 with same security
- **xxHash128:** 3x faster but unsuitable for content addressing or future network sync
- **File metadata (mtime/size):** Unreliable, can't detect content-identical files

### Decision 2: Proper Module Separation Following SOLID Principles

**Choice:** Refactor Phase 1 implementation to follow proper architectural boundaries before continuing with Phase 2

**Rationale:**
The initial Phase 1 implementation placed file hashing and change detection in `crucible-surrealdb`, violating:
- Single Responsibility Principle (SurrealDB crate doing file I/O)
- Dependency Inversion (scanner directly coupled to concrete database)
- Separation of Concerns (file operations mixed with database code)

**Correct Architecture:**
```
crucible-cli (orchestration, dependency injection)
    ↓
    ├─→ crucible-watch (file scanning, change detection)
    │       ↓
    │   crucible-core (traits, pure functions)
    │       ↑
    └─→ crucible-surrealdb (database operations)
```

**Module Responsibilities:**
- `crucible-core`: Traits (`ContentHasher`, `HashLookupStorage`, `ChangeDetector`) and pure hashing functions
- `crucible-watch`: File discovery, scanning, watching, change detection logic
- `crucible-parser`: AST operations, block extraction, document structure
- `crucible-surrealdb`: Database schema, queries, persistence only
- `crucible-cli`: Wire components together with dependency injection

**Migration Strategy:**
1. Create traits in `crucible-core`
2. Move file hashing to `crucible-core/src/hashing/file_hasher.rs`
3. Move file scanning to `crucible-watch/src/file_scanner.rs`
4. Move change detection to `crucible-watch/src/change_detector.rs`
5. Keep `hash_lookup.rs` in `crucible-surrealdb`, implement trait
6. Update `crucible-cli` to inject dependencies

**Benefits:**
- Testability: Mock storage for testing change detection without database
- Reusability: File scanner works with any storage backend
- Maintainability: Clear boundaries, easy to understand
- Flexibility: Swap implementations without changing dependent code

### Decision 3: File-Level Then Block-Level (Phased Approach)

**Choice:** Implement file-level change detection first (Phase 1), then add block-level hashing with Merkle trees (Phase 2)

**Rationale:**
- File-level detection alone provides 90%+ of performance gains for typical workflows
- Simpler implementation reduces risk and time-to-value
- Block-level features can be validated incrementally without disrupting file-level system
- Proper architecture in Phase 1 makes Phase 2+ easier

**Phase 1 (Refactored Architecture):**
- Traits in `crucible-core` for extensibility
- File hashing in `crucible-core` (pure functions)
- Scanning/detection in `crucible-watch` (file operations)
- Hash lookup in `crucible-surrealdb` (database)
- Wiring in `crucible-cli` (orchestration)

**Phase 2 (Future):**
- Block extraction in `crucible-parser`
- Block hashing in `crucible-core`
- Merkle trees in `crucible-core`
- Block storage in `crucible-surrealdb`

### Decision 4: AST Nodes as Block Boundaries

**Choice:** Use parsed AST nodes (headings, paragraphs, lists, code blocks) as natural block boundaries instead of character-based chunking

**Rationale:**
- Aligns with user mental model (editing a paragraph = one block changed)
- Matches HTML rendering (one AST node = one HTML element)
- Preserves semantic coherence (complete thoughts, not arbitrary splits)
- Enables accurate block-level result highlighting in search

**Alternatives considered:**
- Fixed-size chunks (512/1024 bytes): Breaks semantic boundaries, poor UX
- Sentence-based splitting: Complex parsing, language-dependent, still arbitrary
- Paragraph-only: Misses other semantic units (headings, lists, code)

### Decision 5: Streaming Hash Computation

**Choice:** Hash files in 64KB chunks using async I/O instead of loading entire files into memory

**Rationale:**
- Handles files of any size without memory pressure
- 64KB is optimal for disk I/O performance (typical filesystem block size multiples)
- Buffered async I/O prevents blocking on large files
- BLAKE3 is optimized for streaming with minimal overhead

## Risks / Trade-offs

### Risk 1: Hash Storage Overhead
**Risk:** Storing 32-byte hash per file increases database size

**Mitigation:**
- 32 bytes per file is negligible (1000 files = 32KB storage)
- Indexed for fast lookups without full table scans
- Hex string representation (64 chars) is human-readable for debugging

### Risk 2: Migration Complexity
**Risk:** Existing vaults need migration to add file_hash field

**Mitigation:**
- Schema migration system with versioning implemented
- file_hash is optional (NONE allowed) for backward compatibility
- Scanner automatically populates on first scan
- No data loss, graceful degradation if migration fails

### Risk 3: Hash Computation Overhead
**Risk:** Computing BLAKE3 hash adds latency to file discovery

**Trade-off Analysis:**
- Hash computation: ~1ms per file
- Parsing markdown: ~50-200ms per file
- Generating embeddings: ~500-2000ms per file
- **Net gain:** Hashing is 50-2000x faster than what it helps skip

For 1000-file vault with 10 changes:
- Hash cost: 1000 files × 1ms = 1 second
- Parse/embed savings: 990 files × 500ms avg = **495 seconds saved**
- **Net savings: 494 seconds (99.8% reduction)**

### Risk 4: Future Algorithm Change
**Risk:** BLAKE3 could be superseded by better algorithm

**Mitigation:**
- Hash algorithm field can be added to schema (future)
- Content-addressed storage already supports algorithm agility
- BLAKE3 is modern (2020), unlikely to be deprecated soon
- If needed, can support multiple algorithms during transition

## Migration Plan

### Phase 1 (File-Level) - **IN PROGRESS - NEEDS REFACTORING**

**Current State:**
- ✅ BLAKE3 streaming hash implemented
- ✅ Database schema with file_hash column
- ✅ Hash lookup queries with batching and caching
- ✅ Change detection and skipping logic
- ❌ **PROBLEM**: Code in wrong modules (violates SOLID)

**Refactoring Steps:**
1. Create `ContentHasher`, `HashLookupStorage`, `ChangeDetector` traits in `crucible-core`
2. Move file hashing to `crucible-core/src/hashing/file_hasher.rs`
3. Move file scanning to `crucible-watch/src/file_scanner.rs`
4. Move change detection to `crucible-watch/src/change_detector.rs`
5. Keep `hash_lookup.rs` in `crucible-surrealdb`, implement trait
6. Update `crucible-cli` to wire components with dependency injection
7. Move tests to appropriate modules
8. Update documentation

**Rollback:** Revert to previous commit, system reverts to full processing

### Phase 2 (Block-Level) - **FUTURE**
1. Add document_blocks table for block → document mapping
2. Add block_embeddings table for content-addressed embeddings
3. Extend ParsedDocument to store block hashes
4. Build Merkle trees during parsing
5. Implement tree diffing for changed block detection

**Rollback:** Continue using file-level detection, ignore block tables

## Open Questions

1. **Block size tuning:** Should we combine very small AST nodes (e.g., single-sentence paragraphs) into larger blocks for embedding efficiency?

2. **Cache eviction policy:** When does the session-level hash cache get cleared? Per-command? Time-based?

3. **Database vacuum:** How often should we clean up orphaned block records when files are deleted?

4. **Embedding migration:** How do we handle transition from document-level to block-level embeddings without losing user data?

## References

- BLAKE3 specification: https://github.com/BLAKE3-team/BLAKE3-specs
- Content-addressed storage implementation: `crates/crucible-surrealdb/src/content_addressed_storage.rs`
- File-level implementation: `crates/crucible-surrealdb/src/hash_lookup.rs`
- Merkle tree library: `crucible-core` storage module
