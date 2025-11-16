# Vector Embedding Integration Implementation Tasks

**Change ID**: `2025-11-11-vector-embedding-integration`
**Status**: In Progress - Phase 0 & Phase 1 Complete, Starting Phase 2
**Updated**: 2025-11-16
**Timeline**: 4 weeks (44 developer days)
**Scope**: Major refactoring - 11,846 lines to move, 3,800 lines new code, 5 files deleted

## Recent Completions

### ✅ Moved EnrichmentConfig to Core (2025-11-16)
- Created type-safe EnrichmentConfig with provider-specific enums in crucible-core
- Each provider (OpenAI, Ollama, FastEmbed, Cohere, VertexAI, Custom, Mock) has dedicated struct
- Added PipelineConfig for enrichment pipeline settings (workers, batching, circuit breaker)
- Conversion utilities in crucible-llm for old/new config interop
- Deprecated old EmbeddingProviderConfig with migration guide
- **Commits**: 7ab2b23

### ✅ Created crucible-enrichment Crate (2025-11-16)
- Extracted enrichment business logic from crucible-core into separate crate
- Moved EnrichmentService, EnrichedNote, DocumentProcessor to crucible-enrichment
- Kept only EmbeddingProvider trait in crucible-core (clean architecture)
- Eliminated circular dependencies between core and enrichment
- All tests passing (730 tests across refactored crates)
- **Commits**: 02af23d, d248d71, 232c667

## Overview

This implementation corrects severe architectural violations and establishes the enrichment layer as documented in ARCHITECTURE.md. External review confirms this is HIGH EFFORT refactoring with significant layer violations requiring careful sequential execution.

**Scale of Change**:
- EmbeddingPipeline (7,751 lines) - Move from infrastructure to core
- EmbeddingThreadPool (1,477 lines) - Move to provider implementation
- Configuration (850 lines) - Migrate to core
- Event system (1,036 lines) - Refactor for new architecture
- New enrichment orchestration (estimated 3,800 lines)

**Risk Level**: HIGH - Breaking changes, circular dependencies, data migration risks
**Migration Strategy**: Clean refactor - no feature flags, direct replacement (solo developer, breaking changes acceptable)

## TDD Methodology

**Every task follows RED-GREEN-REFACTOR cycle:**
1. **RED**: Write failing test first
2. **GREEN**: Write minimal code to pass
3. **REFACTOR**: Clean up while keeping tests green
4. **VERIFY**: Run full test suite

---

## Phase 0: Refactoring (Clean Up Architecture Violations) ✅ COMPLETE

**Goal**: Move embedding business logic out of infrastructure layer, prepare for clean architecture.

**Summary**: All tasks complete. Created crucible-enrichment crate, split metadata between parser and enrichment, moved configuration to core with type-safe provider enums.

### Task 0.1: Audit and Document Current State

**Files to Analyze:**
- `crates/crucible-surrealdb/src/embedding_pipeline.rs`
- `crates/crucible-surrealdb/src/embedding_pool.rs`
- `crates/crucible-surrealdb/src/embedding_config.rs`
- `crates/crucible-surrealdb/src/embedding.rs`
- `crates/crucible-watch/src/embedding_events.rs`

**TDD Steps:**
1. **Document**: Map out what business logic exists in storage layer
2. **Identify**: Which components move to core, which to LLM crate, which stay
3. **Plan**: Migration strategy with minimal breaking changes
4. **Verify**: Document current test coverage and ensure it's maintained

**Acceptance Criteria:**
- [x] Complete audit document listing all embedding-related code
- [x] Migration plan identifies destination for each component
- [x] Test coverage baseline established
- [x] No changes made yet (documentation only)

**Status**: ✅ COMPLETE - See PHASE0_AUDIT.md

### Task 0.2: Create crucible-enrichment Crate

**Files Created:**
- `crates/crucible-enrichment/Cargo.toml`
- `crates/crucible-enrichment/src/lib.rs`
- `crates/crucible-enrichment/src/service.rs` (moved from core)
- `crates/crucible-enrichment/src/types.rs` (moved from core)
- `crates/crucible-enrichment/src/config.rs` (moved from core)
- `crates/crucible-enrichment/src/document_processor.rs` (moved from core)

**Files Modified:**
- `Cargo.toml` (workspace members)
- `crates/crucible-core/src/enrichment/mod.rs` (only exports trait)
- `crates/crucible-core/src/lib.rs` (updated exports)
- `crates/crucible-core/Cargo.toml` (removed circular dependency)
- `crates/crucible-surrealdb/src/embedding*.rs` (updated imports)

**TDD Steps:**
1. ✅ Created new crate structure
2. ✅ Moved enrichment business logic from core
3. ✅ Updated all imports across codebase
4. ✅ Verified all tests pass (730 tests)

**Acceptance Criteria:**
- [x] crucible-enrichment crate created with proper structure
- [x] EnrichmentService, EnrichedNote, DocumentProcessor moved
- [x] Only EmbeddingProvider trait remains in crucible-core
- [x] No circular dependencies
- [x] All tests passing
- [x] crucible-surrealdb imports updated

**Status**: ✅ COMPLETE

---

### Task 0.3: Split Metadata Between Parser and Enrichment

**Research Basis**: Industry standard pattern from Unified/Remark, Pandoc, Elasticsearch, Apache Tika
- **Parse phase**: Structural metadata (word count, char count, tags, links, headings)
- **Enrich phase**: Computed metadata (complexity score, reading time, semantic analysis)

**Files to Create:**
- `crates/crucible-parser/src/metadata.rs` (new)

**Files to Modify:**
- `crates/crucible-parser/src/types.rs` (add ParsedNoteMetadata)
- `crates/crucible-enrichment/src/types.rs` (update NoteMetadata)
- `crates/crucible-enrichment/src/service.rs` (use parser metadata)

**TDD Steps:**
1. **RED**: Write tests for parser extracting structural metadata
2. **GREEN**: Implement metadata extraction during parse
3. **REFACTOR**: Update enrichment to use parser metadata + add computed fields
4. **VERIFY**: All tests pass, metadata split is clean

**Acceptance Criteria:**
- [ ] `ParsedNote` includes structural metadata:
  - `word_count: usize`
  - `char_count: usize`
  - `heading_count: usize`
  - `code_block_count: usize`
  - `list_count: usize`
  - Available immediately after parsing, no enrichment needed
- [ ] `EnrichedNote` metadata includes computed fields:
  - `complexity_score: f64` (computed from AST structure)
  - `reading_time: Duration` (computed from word count)
  - `readability_score: Option<f64>` (future: Flesch-Kincaid)
- [ ] Clear separation: parser = structure, enrichment = semantics
- [ ] Performance: metadata extraction adds <5% to parse time
- [ ] All existing tests pass with updated metadata structure

**Future Work**:
- **Phase 2**: Transform pattern (`ASTTransform` trait) for extensible pipeline
  - Trait definition in crucible-core
  - EnrichmentService becomes one transform in a pipeline
  - Enables plugins/extensions to add custom transforms
  - Mirrors industry patterns: Pandoc filters, Unified/Remark plugins
  - Design after validating metadata split

**Status**: ✅ COMPLETE - ParsedNoteMetadata exists with all structural metadata fields

---

### Task 0.4: Extract Configuration to Core

**Files to Create/Modify:**
- `crates/crucible-core/src/config/embedding.rs` (new)
- `crates/crucible-surrealdb/src/embedding_config.rs` (refactor/remove)

**TDD Steps:**
1. **RED**: Write tests for embedding configuration in core
2. **GREEN**: Move configuration types to core
3. **REFACTOR**: Update imports across codebase
4. **VERIFY**: All existing tests pass with new config location

**Acceptance Criteria:**
- [x] `EnrichmentConfig` moved to crucible-core/src/enrichment/config.rs
- [x] Configuration is provider-specific with type-safe enums (OpenAI, Ollama, FastEmbed, etc.)
- [x] Conversion utilities added in crucible-llm for migration
- [x] Old config deprecated with migration guide

**Status**: ✅ COMPLETE - EnrichmentConfig with provider-specific enums created in crucible-core
**Commits**: 7ab2b23

---

## Phase 1: Core Enrichment Layer (Week 1) ✅ COMPLETE

**Goal**: Establish clean architecture with trait definitions and orchestration in crucible-core.

**Summary**: EmbeddingProvider trait defined in crucible-core, EnrichmentService and EnrichedNote implemented in crucible-enrichment crate.

### Task 1.1: Define Embedding Provider Trait ✅ COMPLETE

**Files Created:**
- ✅ `crates/crucible-core/src/enrichment/mod.rs`
- ✅ `crates/crucible-core/src/enrichment/embedding.rs`
- ✅ `crates/crucible-core/src/enrichment/types.rs`

**TDD Steps:**
1. ✅ **RED**: Write tests for EmbeddingProvider trait with mock implementation
2. ✅ **GREEN**: Define trait with async methods (embed_text, embed_batch)
3. ✅ **REFACTOR**: Add documentation and usage examples
4. ✅ **VERIFY**: Mock implementation passes all tests

**Acceptance Criteria:**
- [x] `EmbeddingProvider` trait defined with:
  - `async fn embed(&self, text: &str) -> Result<Vec<f32>>`
  - `async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>>`
  - `fn model_name(&self) -> &str`
  - `fn dimensions(&self) -> usize`
- [x] Trait is Send + Sync for async/thread safety
- [x] Mock provider for testing exists (crucible-llm)
- [x] Comprehensive documentation with examples

**Status**: ✅ COMPLETE - Trait defined in crucible-core/src/enrichment/embedding.rs

### Task 1.2: Implement EnrichmentService Orchestrator ✅ COMPLETE

**Files Created:**
- ✅ `crates/crucible-core/src/enrichment/service.rs` (Trait definition)
- ✅ `crates/crucible-enrichment/src/service.rs` (DefaultEnrichmentService implementation)
- ⚠️ `crates/crucible-enrichment/src/metadata.rs` (May need creation)
- ⚠️ `crates/crucible-enrichment/src/relations.rs` (May need creation)

**TDD Steps:**
1. ✅ **RED**: Write tests for EnrichmentService with mock dependencies
2. ✅ **GREEN**: Implement service orchestrating parallel enrichment operations
3. ⚠️ **REFACTOR**: Extract metadata and relation extraction to separate modules (verify)
4. ⚠️ **VERIFY**: End-to-end enrichment pipeline works with mocks (needs testing)

**Acceptance Criteria:**
- [x] `EnrichmentService` trait coordinates all enrichment operations
- [x] Accepts dependencies via constructor (DI pattern):
  - `embedding_provider: Arc<dyn EmbeddingProvider>`
- [~] `enrich()` method runs operations (parallel execution needs verification)
- [x] Returns `EnrichedNote` with all enrichment data
- [~] Handles partial failures gracefully (needs testing verification)

**Status**: ✅ TRAIT COMPLETE, ⚠️ IMPLEMENTATION NEEDS VERIFICATION
- Trait defined in crucible-core/src/enrichment/service.rs
- DefaultEnrichmentService exists in crucible-enrichment/src/service.rs
- **Next**: Verify implementation completeness and test coverage

### Task 1.3: Define EnrichedNote Type ✅ COMPLETE

**Files Modified:**
- ✅ `crates/crucible-core/src/enrichment/types.rs`
- ✅ `crates/crucible-enrichment/src/types.rs`

**TDD Steps:**
1. ✅ **RED**: Write tests for EnrichedNote construction and validation
2. ✅ **GREEN**: Implement type with all enrichment data fields
3. ⚠️ **REFACTOR**: Add builder pattern for ergonomic construction (needs verification)
4. ⚠️ **VERIFY**: Serialization/deserialization works correctly (needs testing)

**Acceptance Criteria:**
- [x] `EnrichedNote` contains:
  - `parsed: ParsedNote` (original AST)
  - `merkle_tree: HybridMerkleTree` (computed tree)
  - `embeddings: Vec<BlockEmbedding>` (vectors for changed blocks)
  - `metadata: NoteMetadata` (word count, language, etc.)
  - `relations: Vec<InferredRelation>` (extracted relationships)
- [x] Type is cloneable and serializable
- [~] Builder pattern available for construction (needs verification)
- [~] Validation ensures data consistency (needs testing)

**Status**: ✅ TYPE DEFINED, ⚠️ BUILDER PATTERN NEEDS VERIFICATION
- Types defined in crucible-core/src/enrichment/types.rs
- **Next**: Verify builder pattern and validation logic

---

## Phase 2: Provider Implementations (Week 1-2)

**Goal**: Implement concrete embedding providers in crucible-llm crate.

### Task 2.1: Set Up Provider Module in crucible-llm

**Files to Create:**
- `crates/crucible-llm/src/providers/mod.rs`
- `crates/crucible-llm/Cargo.toml` (add fastembed dependency)

**TDD Steps:**
1. **RED**: Write tests for provider module structure
2. **GREEN**: Create module with re-exports
3. **REFACTOR**: Set up feature flags for optional providers
4. **VERIFY**: Module compiles and exports are accessible

**Acceptance Criteria:**
- [ ] Provider module created with clean exports
- [ ] Feature flags: `fastembed` (default), `openai` (optional)
- [ ] Dependencies added conditionally based on features
- [ ] Module documentation explains provider system

### Task 2.2: Implement FastembedProvider

**Files to Create:**
- `crates/crucible-llm/src/providers/fastembed.rs`

**TDD Steps:**
1. **RED**: Write tests for FastembedProvider (requires model loading)
2. **GREEN**: Implement EmbeddingProvider trait using fastembed-rs
3. **REFACTOR**: Add thread-safe model caching and batch optimization
4. **VERIFY**: Performance tests show <30s for 1000 blocks

**Acceptance Criteria:**
- [ ] `FastembedProvider` implements `EmbeddingProvider` trait
- [ ] Uses `fastembed::TextEmbedding` with local ONNX models
- [ ] Model loaded once and reused (Arc for shared ownership)
- [ ] `embed_batch` uses tokio::task::spawn_blocking for CPU work
- [ ] Supports multiple model types (via config):
  - `LocalMini` (fast, smaller dimensions)
  - `LocalStandard` (balanced)
  - `LocalLarge` (higher quality)
- [ ] Error handling for model loading failures
- [ ] Performance: <30s for 1000 blocks on typical hardware

### Task 2.3: Create OpenAI Provider Stub

**Files to Create:**
- `crates/crucible-llm/src/providers/openai.rs`

**TDD Steps:**
1. **RED**: Write tests for OpenAI provider (mocked API)
2. **GREEN**: Implement stub with feature flag
3. **REFACTOR**: Add rate limiting and retry logic
4. **VERIFY**: Mock tests pass, ready for future implementation

**Acceptance Criteria:**
- [ ] `OpenAIProvider` stub implements `EmbeddingProvider` trait
- [ ] Feature-gated with `openai` feature flag
- [ ] API client setup (using reqwest or similar)
- [ ] Rate limiting and retry logic in place
- [ ] Credential management (environment variables)
- [ ] Marked as "future implementation" in docs

---

## Phase 3: Pipeline Integration (Week 2)

**Goal**: Integrate enrichment layer into document processing pipeline with five-phase architecture.

### Task 3.1: Implement Five-Phase Document Processor

**Files to Create:**
- `crates/crucible-core/src/processing/document_processor.rs`

**Files to Modify:**
- `crates/crucible-core/src/processing/mod.rs`

**TDD Steps:**
1. **RED**: Write integration tests for five-phase pipeline
2. **GREEN**: Implement DocumentProcessor with all phases
3. **REFACTOR**: Extract each phase into separate method
4. **VERIFY**: End-to-end test with real files and mock provider

**Acceptance Criteria:**
- [ ] `DocumentProcessor` implements five phases:
  - **Phase 1**: File date + BLAKE3 hash check (skip if unchanged)
  - **Phase 2**: Full file parse to AST
  - **Phase 3**: Build Merkle tree, diff, identify changed blocks
  - **Phase 4**: Call EnrichmentService with changed block list
  - **Phase 5**: Store EnrichedNote in single transaction
- [ ] Each phase has separate method with clear inputs/outputs
- [ ] Error handling per phase with appropriate logging
- [ ] Metrics tracked for each phase (duration, blocks processed, etc.)

### Task 3.2: Integrate with NoteIngestor

**Files to Modify:**
- `crates/crucible-surrealdb/src/eav_graph/ingest.rs`

**TDD Steps:**
1. **RED**: Write tests for NoteIngestor calling DocumentProcessor
2. **GREEN**: Refactor NoteIngestor to delegate to enrichment layer
3. **REFACTOR**: Remove embedding logic from NoteIngestor
4. **VERIFY**: Existing ingestion tests pass with new architecture

**Acceptance Criteria:**
- [ ] `NoteIngestor::ingest()` calls `DocumentProcessor` instead of doing enrichment
- [ ] All embedding-related code removed from NoteIngestor
- [ ] Merkle tree integration maintained (already exists)
- [ ] Backward compatibility maintained where possible
- [ ] Integration tests verify enriched data is stored correctly

### Task 3.3: Update File Watcher Integration

**Files to Modify:**
- `crates/crucible-watch/src/lib.rs`
- `crates/crucible-watch/src/embedding_events.rs` (refactor or remove)

**TDD Steps:**
1. **RED**: Write tests for file watcher triggering DocumentProcessor
2. **GREEN**: Update watcher to use new five-phase pipeline
3. **REFACTOR**: Remove or adapt embedding_events if redundant
4. **VERIFY**: File change events trigger correct pipeline execution

**Acceptance Criteria:**
- [ ] File watcher detects changes and triggers DocumentProcessor
- [ ] Event-driven system uses five-phase pipeline
- [ ] `embedding_events.rs` refactored to fit new architecture or removed if redundant
- [ ] Real-time processing works for file modifications
- [ ] Debouncing prevents redundant processing on rapid changes

---

## Phase 4: Storage Layer Refactoring (Week 2-3)

**Goal**: Reduce storage layer to pure I/O, remove all business logic.

### Task 4.1: Refactor Embedding Storage to I/O Only

**Files to Modify:**
- `crates/crucible-surrealdb/src/embedding.rs`

**Files to Delete:**
- `crates/crucible-surrealdb/src/embedding_pipeline.rs`
- `crates/crucible-surrealdb/src/embedding_pool.rs`

**TDD Steps:**
1. **RED**: Write tests for pure storage operations (store, retrieve, delete, search)
2. **GREEN**: Refactor to remove all embedding generation logic
3. **REFACTOR**: Simplify to only SurrealDB operations
4. **VERIFY**: All business logic now lives in core/LLM layers

**Acceptance Criteria:**
- [ ] `embedding.rs` contains ONLY:
  - `store_embedding()` - persist vector with metadata
  - `get_embeddings()` - retrieve by block ID
  - `delete_embeddings()` - remove by block ID(s)
  - `search_similar()` - vector similarity search
- [ ] NO embedding generation code in storage layer
- [ ] NO provider management or pool logic
- [ ] Clean trait implementation for `EmbeddingStore`
- [ ] embedding_pipeline.rs and embedding_pool.rs deleted

### Task 4.2: Implement Transactional Storage for EnrichedNote

**Files to Modify:**
- `crates/crucible-surrealdb/src/eav_graph/store.rs`

**TDD Steps:**
1. **RED**: Write tests for atomic EnrichedNote storage
2. **GREEN**: Implement single transaction for all enrichment data
3. **REFACTOR**: Ensure rollback on any failure
4. **VERIFY**: Transaction tests verify atomicity

**Acceptance Criteria:**
- [ ] `store_enriched_note()` method handles:
  - Entity/block upserts
  - Embedding deletion (changed blocks)
  - Embedding insertion (new vectors)
  - Merkle tree storage
  - Metadata storage
  - Relation storage
- [ ] All operations in single SurrealDB transaction
- [ ] Rollback on any failure maintains consistency
- [ ] Performance acceptable for typical note sizes

---

## Phase 5: Testing & Performance (Week 3)

**Goal**: Comprehensive testing, performance validation, and documentation.

### Task 5.1: Integration Testing

**Files to Create:**
- `tests/integration/enrichment_pipeline_tests.rs`
- `tests/integration/embedding_incremental_tests.rs`

**TDD Steps:**
1. **RED**: Write comprehensive integration test scenarios
2. **GREEN**: Ensure all scenarios pass
3. **REFACTOR**: Extract test helpers and fixtures
4. **VERIFY**: Full test suite passes in CI

**Acceptance Criteria:**
- [ ] End-to-end pipeline test with real files
- [ ] Incremental processing test (modify file, only changed blocks re-embedded)
- [ ] Provider switching test (Fastembed ↔ mock)
- [ ] Error handling tests (provider failure, partial enrichment)
- [ ] Merkle diff accuracy tests
- [ ] All tests run in CI with appropriate fixtures

### Task 5.2: Performance Benchmarking

**Files to Create:**
- `benches/enrichment_pipeline.rs`

**TDD Steps:**
1. **Baseline**: Measure current performance (if any existing implementation)
2. **Benchmark**: Run criterion benchmarks for each phase
3. **Optimize**: Address bottlenecks revealed by benchmarks
4. **Verify**: Meet success criteria (<30s for 1000 blocks)

**Acceptance Criteria:**
- [ ] Benchmark suite for:
  - Phase 1 quick filter (should be <10ms per file)
  - Phase 2 parsing (existing benchmarks)
  - Phase 3 Merkle diff (existing benchmarks)
  - Phase 4 embedding generation (<30s for 1000 blocks)
  - Phase 5 storage (transactional overhead measured)
- [ ] Performance targets met:
  - 1000 blocks embedded in <30 seconds (Fastembed)
  - Merkle diff identifies changes in <100ms
  - Full pipeline <1s for small changes (1-5 blocks)
- [ ] Benchmark results documented

### Task 5.3: Documentation and Examples

**Files to Create:**
- `crates/crucible-core/src/enrichment/README.md`
- `examples/embedding_pipeline_example.rs`

**TDD Steps:**
1. **Document**: Write comprehensive module documentation
2. **Examples**: Create runnable examples showing usage
3. **Update**: Update ARCHITECTURE.md with completion status
4. **Verify**: Examples compile and run successfully

**Acceptance Criteria:**
- [ ] Enrichment module has comprehensive documentation:
  - Architecture overview
  - Data flow diagrams
  - Usage examples with dependency injection
  - Provider implementation guide
- [ ] Runnable example demonstrating:
  - Setting up EnrichmentService
  - Processing a document through five phases
  - Switching embedding providers
- [ ] ARCHITECTURE.md updated to reflect completed enrichment layer
- [ ] Migration guide for existing code using old architecture

---

## Success Metrics

### Phase 0 (Refactoring):
- [ ] Zero regression in existing functionality
- [ ] Test coverage maintained or improved
- [ ] Migration plan documented and approved

### Phase 1 (Core Layer):
- [ ] EnrichmentService in crucible-core with trait abstractions
- [ ] Mock provider allows testing without real embeddings
- [ ] Clean separation: core defines contracts, infrastructure implements

### Phase 2 (Providers):
- [ ] Fastembed provider functional with <30s for 1000 blocks
- [ ] Feature flags allow conditional compilation
- [ ] OpenAI stub ready for future implementation

### Phase 3 (Pipeline):
- [ ] Five-phase architecture implemented and tested
- [ ] Incremental processing verified (only changed blocks re-embedded)
- [ ] File watcher integration works end-to-end

### Phase 4 (Storage):
- [ ] Storage layer is pure I/O with zero business logic
- [ ] Transactional storage ensures atomicity
- [ ] Old embedding files deleted (pipeline, pool)

### Phase 5 (Testing):
- [ ] Integration tests cover all major scenarios
- [ ] Performance benchmarks meet targets
- [ ] Documentation complete and examples runnable

### Overall Success:
- [ ] Architecture matches ARCHITECTURE.md enrichment pipeline
- [ ] SOLID principles demonstrated (Dependency Inversion with traits)
- [ ] Clean data flow: Filter → Parse → Diff → Enrich → Store
- [ ] Zero architectural violations (no business logic in infrastructure)

---

## Dependencies

**Existing Infrastructure:**
- ✅ Parser with block-level AST (crucible-core)
- ✅ Merkle tree implementation with diff capability (crucible-core)
- ✅ SurrealDB with vector search support (crucible-surrealdb)
- ✅ File watcher with event system (crucible-watch)

**New Dependencies:**
- `fastembed` crate for local embeddings (crucible-llm)
- `tokio` features for parallel enrichment (likely already present)

**Test Dependencies:**
- Test fixtures with sample markdown files
- Mock embedding provider for unit tests

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| **Breaking changes during refactoring** | Direct replacement - tests ensure correctness; no backward compatibility needed |
| **Circular dependencies** | Careful dependency ordering; core defines traits, infrastructure implements |
| **Performance degradation** | Benchmark each phase; optimize bottlenecks; use parallel processing |
| **Fastembed model loading overhead** | Load model once, cache in Arc, reuse across batches |
| **Storage transaction size** | Batch large operations; provide streaming API for huge notes |
| **Provider failures** | Graceful degradation; partial enrichment stored; retry mechanism |
| **Merkle diff inaccuracy** | Comprehensive test suite; verify against known change scenarios |
| **Data migration** | Existing embeddings can be regenerated; no complex migration needed |

---

## Implementation Notes

1. **Start with Phase 0**: Refactoring is critical to avoid building on unstable foundation
2. **TDD Throughout**: Every task uses RED-GREEN-REFACTOR; no exceptions
3. **Parallel Work Possible**: Phase 1 and Phase 2 can overlap once traits are defined
4. **Integration Early**: Don't wait until end to test phase integration
5. **Performance Monitoring**: Track metrics from start, not just at end

---

**Timeline Summary:**
- **Week 1**: Phase 0 (refactor) + Phase 1 (core) + start Phase 2 (providers)
- **Week 2**: Complete Phase 2 + Phase 3 (pipeline integration) + start Phase 4 (storage)
- **Week 3**: Complete Phase 4 + Phase 5 (testing/docs) + final verification

**Estimated Effort**: 2-3 weeks depending on refactoring complexity and test coverage requirements.
