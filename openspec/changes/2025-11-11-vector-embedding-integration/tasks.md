# Vector Embedding Integration Implementation Tasks

**Change ID**: `2025-11-11-vector-embedding-integration`
**Status**: Ready for Implementation
**Timeline**: 2 weeks

## TDD Methodology

**Every task follows RED-GREEN-REFACTOR cycle:**
1. **RED**: Write failing test first
2. **GREEN**: Write minimal code to pass
3. **REFACTOR**: Clean up while keeping tests green
4. **VERIFY**: Run full test suite

---

## Phase 1: Embedding Abstraction Layer (Week 1)

### Task 1.1: Define Embedding Provider Traits

**Files to Create:**
- `crates/crucible-core/src/embedding/mod.rs`
- `crates/crucible-core/src/embedding/provider.rs`

**TDD Steps:**
1. **RED**: Write tests for EmbeddingProvider trait
2. **GREEN**: Implement trait with Fastembed provider
3. **REFACTOR**: Optimize for batch processing
4. **VERIFY**: All tests pass

**Acceptance Criteria:**
- [ ] EmbeddingProvider trait defined with async embed_text/embed_batch methods
- [ ] FastembedProvider implementation using local models
- [ ] OpenAIProvider implementation for cloud embeddings
- [ ] Error handling for model loading failures
- [ ] Configuration system for model selection and parameters

### Task 1.2: Implement Embedding Storage Trait

**Files to Create:**
- `crates/crucible-core/src/embedding/storage.rs`
- `crates/crucible-surrealdb/src/embedding_store.rs`

**TDD Steps:**
1. **RED**: Write tests for EmbeddingStorage trait
2. **GREEN**: Implement SurrealDB vector storage
3. **REFACTOR**: Add indexing and query optimization
4. **VERIFY**: Vector search operations work correctly

**Acceptance Criteria:**
- [ ] EmbeddingStorage trait with store/search/delete operations
- [ ] SurrealDB implementation with vector similarity search
- [ ] Batch storage operations for performance
- [ ] Metadata preservation (model, dimensions, timestamp)
- [ ] Integration with existing EAV graph schema

### Task 1.3: Create Embedding Service

**Files to Create:**
- `crates/crucible-core/src/embedding/service.rs`

**TDD Steps:**
1. **RED**: Write tests for EmbeddingService coordination
2. **GREEN**: Implement service orchestrating provider + storage
3. **REFACTOR**: Add caching and deduplication
4. **VERIFY**: End-to-end embedding pipeline works

**Acceptance Criteria:**
- [ ] EmbeddingService coordinates provider and storage
- [ ] Automatic embedding generation for new content blocks
- [ ] Incremental update detection (changed blocks only)
- [ ] Batch processing for large documents
- [ ] Progress reporting and error recovery

---

## Phase 2: CLI Integration and Semantic Search (Week 2)

### Task 2.1: Add CLI Embedding Commands

**Files to Create:**
- `crates/crucible-cli/src/commands/embed.rs`
- `crates/crucible-cli/src/commands/semantic_search.rs`

**TDD Steps:**
1. **RED**: Write CLI integration tests
2. **GREEN**: Implement embed and semantic-search commands
3. **REFACTOR**: Add progress bars and error handling
4. **VERIFY**: Commands work with real data

**Acceptance Criteria:**
- [ ] `cru embed` command generates embeddings for kiln
- [ ] `cru semantic-search` command performs vector search
- [ ] Progress reporting for long-running operations
- [ ] Configuration options for model and parameters
- [ ] Error handling for embedding service failures

### Task 2.2: Implement Hybrid Search

**Files to Modify:**
- `crates/crucible-surrealdb/src/search.rs` (extend existing)

**TDD Steps:**
1. **RED**: Write tests for hybrid search combining semantic + graph
2. **GREEN**: Implement search orchestration
3. **REFACTOR**: Optimize query performance
4. **VERIFY**: Search results are relevant and fast

**Acceptance Criteria:**
- [ ] Hybrid search combining semantic similarity, graph relationships, and text matching
- [ ] Relevance scoring with configurable weights
- [ ] Result ranking and filtering options
- [ ] Performance: <500ms for typical queries
- [ ] Integration with existing search infrastructure

### Task 2.3: Integration Testing and Performance

**Files to Create:**
- `tests/integration/embedding_integration_tests.rs`
- `tests/integration/semantic_search_tests.rs`

**TDD Steps:**
1. **RED**: Write comprehensive integration tests
2. **GREEN**: Fix any integration issues discovered
3. **REFACTOR**: Optimize performance bottlenecks
4. **VERIFY**: All scenarios work correctly

**Acceptance Criteria:**
- [ ] End-to-end embedding pipeline with test-kiln data
- [ ] Semantic search accuracy with test queries
- [ ] Performance benchmarks meet targets
- [ ] Error scenarios handled gracefully
- [ ] Memory usage stays within limits

---

## Success Metrics

**Phase 1 (Embedding Layer):**
- [ ] 100% block embedding coverage (>5 words)
- [ ] <30s to embed 1000 blocks (local model)
- [ ] Support for multiple embedding providers
- [ ] Zero data loss through embedding pipeline

**Phase 2 (CLI & Search):**
- [ ] CLI commands provide helpful output and progress
- [ ] Semantic search returns relevant results
- [ ] Hybrid search performance <500ms
- [ ] Error messages guide users to resolution

**Overall:**
- [ ] Integration tests cover all major scenarios
- [ ] Memory usage optimized for large knowledge bases
- [ ] Configuration system supports different deployment scenarios
- [ ] Documentation explains setup and usage

## Dependencies

- Completed parser with block-level entities (✅ available)
- SurrealDB with vector extensions (✅ available)
- Fastembed Rust library (new dependency)
- Test-kiln data for validation (✅ available)

## Risk Mitigation

**Model Loading Failures**: Graceful fallback to text-only search
**Performance Issues**: Batch processing and caching strategies
**Storage Bloat**: Deduplication and compression algorithms
**Provider Failures**: Multiple provider support with failover