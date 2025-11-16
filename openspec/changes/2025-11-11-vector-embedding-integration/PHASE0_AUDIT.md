# Phase 0: Embedding Architecture Audit

**Date**: 2025-11-16
**Status**: Complete - Ready for Refactoring
**Scope**: Comprehensive analysis of embedding-related code for migration planning

---

## Executive Summary

**Total Code to Migrate**: 3,428 lines across 5 files
**Architecture Violations**: SEVERE - Business logic in infrastructure layer
**Existing Infrastructure**: EmbeddingProvider trait already exists in crucible-llm (GOOD!)
**Migration Complexity**: HIGH - Circular dependency risks, tight coupling to SurrealDB

---

## File-by-File Analysis

### 1. `crucible-surrealdb/src/embedding_config.rs` (850 lines)

**Current Location**: Infrastructure layer (crucible-surrealdb)
**Should Be**: Domain layer (crucible-core)

**Contents**:
- `EmbeddingConfig` - Thread pool configuration (**MOVE TO CORE**)
- `EmbeddingModel` enum (LocalMini, LocalStandard, LocalLarge) (**MOVE TO CORE**)
- `PrivacyMode` enum (StrictLocal, AllowExternalFallback, HybridMode) (**MOVE TO CORE**)
- `ThreadPoolMetrics` - Performance monitoring (**MOVE TO CORE**)
- `EmbeddingError` types and error handling (**MOVE TO CORE**)
- `EmbeddingProcessingResult` - Result types (**MOVE TO CORE**)
- `DocumentEmbedding` - Embedding representation (**MOVE TO CORE**)
- Optimization presets (throughput, latency, resources) (**KEEP IN CORE**)

**Migration Strategy**:
- Create `crucible-core/src/enrichment/config.rs`
- Move all types EXCEPT provider-specific configuration
- Make configuration provider-agnostic (no Fastembed specifics)
- Update imports across codebase

**Dependencies**:
```rust
// Current (WRONG)
use crate::embedding_config::*;  // in surrealdb

// Target (CORRECT)
use crucible_core::enrichment::EmbeddingConfig;  // in any crate
```

**Test Coverage**: 13 unit tests (100% coverage) - MUST maintain

---

### 2. `crucible-surrealdb/src/embedding_pool.rs` (1,476 lines)

**Current Location**: Infrastructure layer (crucible-surrealdb)
**Should Be**: Provider implementation (crucible-llm) OR orchestration (crucible-core)

**Contents**:
- `EmbeddingThreadPool` - Worker thread management (**DELETE/REFACTOR**)
  - Uses `crucible_llm::embeddings::EmbeddingProvider` (ALREADY EXISTS!)
  - Circuit breaker pattern
  - Task queue with semaphore
  - Metrics tracking
  - Mock/real provider switching

**Current Architecture Issues**:
```rust
// Thread pool is in INFRASTRUCTURE but uses DOMAIN logic
pub struct EmbeddingThreadPool {
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,  // ✅ Uses trait
    config: Arc<EmbeddingConfig>,  // ❌ In wrong layer
    // ... worker management (❌ Should be in provider impl)
}
```

**Migration Strategy**:
This is the MOST COMPLEX file. Two options:

**Option A** (Recommended): DELETE and simplify
- Thread pool management belongs in provider implementation
- FastembedProvider should handle its own threading
- EnrichmentService doesn't need thread pool abstraction
- Just use tokio::spawn for parallel enrichment

**Option B**: Move to crucible-llm
- Create `crucible-llm/src/providers/pool.rs`
- Make it provider-specific (FastembedPool, OllamaPool)
- Each provider manages its own concurrency

**Rationale for Option A**:
- Over-engineered for current needs
- Adds complexity without clear benefit
- Provider implementations already handle concurrency
- EnrichmentService can use simpler tokio::join! for parallelism

**Dependencies**:
- Uses `crucible_llm::embeddings::create_provider` (**EXISTS - GOOD!**)
- Tightly coupled to `SurrealClient` (**BAD - must decouple**)
- Direct database operations (**VIOLATION - move to storage layer**)

**Test Coverage**: Minimal integration tests - NEED MORE

---

### 3. `crucible-surrealdb/src/embedding_pipeline.rs` (750 lines)

**Current Location**: Infrastructure layer (crucible-surrealdb)
**Should Be**: Domain orchestration (crucible-core/src/enrichment/service.rs)

**Contents**:
- `EmbeddingPipeline` - Document processing orchestration (**MOVE TO CORE**)
  - Bulk processing logic
  - Chunking strategies (DEFAULT_CHUNK_SIZE, etc.)
  - Incremental update detection
  - Batch processing
  - Database retrieval and storage (**STORAGE LAYER VIOLATION**)

**Current Architecture Issues**:
```rust
pub struct EmbeddingPipeline {
    thread_pool: EmbeddingThreadPool,  // ❌ Infrastructure dependency
    chunk_size: usize,
    chunk_overlap: usize,
}

impl EmbeddingPipeline {
    pub async fn process_documents_with_embeddings(
        &self,
        client: &SurrealClient,  // ❌ DIRECT DB DEPENDENCY
        document_ids: &[String],
    ) -> Result<EmbeddingProcessingResult> {
        // ❌ Mixing orchestration with storage
        let documents = self.retrieve_documents(client, document_ids).await?;
        // ... business logic ...
    }
}
```

**Migration Strategy**:
1. **Extract orchestration logic** → `EnrichmentService` in core
2. **Remove direct DB access** → Use repository pattern
3. **Simplify chunking** → Move to metadata extraction phase
4. **Remove incremental logic** → Merkle diff handles this

**Becomes**:
```rust
// crucible-core/src/enrichment/service.rs
pub struct EnrichmentService {
    embedding_provider: Arc<dyn EmbeddingProvider>,  // ✅ Trait from core
    // NO database dependency!
}

impl EnrichmentService {
    pub async fn enrich(
        &self,
        parsed: ParsedNote,  // ✅ From parser
        changed_blocks: Vec<BlockId>,  // ✅ From Merkle diff
    ) -> Result<EnrichedNote> {
        // Pure business logic - NO storage
    }
}
```

**Dependencies to Break**:
- Direct SurrealClient usage → Repository trait
- Thread pool dependency → Use provider directly or simple tokio
- Document retrieval → Handled by pipeline coordinator, not enrichment

**Test Coverage**: Some integration tests - NEED UNIT TESTS

---

### 4. `crucible-surrealdb/src/embedding.rs` (20 lines)

**Current Location**: Infrastructure layer (crucible-surrealdb)
**Status**: **MOSTLY CORRECT** - Just re-exports

**Contents**:
```rust
// Re-exports from other modules
pub use crate::embedding_config::*;
pub use crate::embedding_pipeline::EmbeddingPipeline;
pub use crate::embedding_pool::EmbeddingThreadPool;
pub use crate::kiln_integration::{
    clear_document_embeddings,
    get_database_stats,
    get_document_embeddings,
    semantic_search,
    store_document_embedding,
};
```

**Migration Strategy**:
- **REFACTOR to storage-only functions**
- Remove re-exports of business logic (config, pipeline, pool)
- Keep ONLY storage operations:
  - `store_embedding()`
  - `get_embeddings()`
  - `delete_embeddings()`
  - `search_similar()`

**After Migration**:
```rust
// crucible-surrealdb/src/embedding.rs (storage only)
pub use crate::kiln_integration::{
    store_embedding,
    get_embeddings,
    delete_embeddings,
    semantic_search,
};
```

**Test Coverage**: Relies on kiln_integration tests

---

### 5. `crucible-watch/src/embedding_events.rs` (332 lines)

**Current Location**: Event system (crucible-watch)
**Status**: **REFACTOR NEEDED** - Event types good, but workflow unclear

**Contents**:
- `EmbeddingEvent` - Event structure (**KEEP BUT SIMPLIFY**)
- `EmbeddingEventMetadata` - Metadata (**KEEP**)
- `EmbeddingEventPriority` - Priority levels (**MAYBE REMOVE**)
- `EmbeddingEventResult` - Result tracking (**KEEP**)
- `EventDrivenEmbeddingConfig` - Configuration (**REMOVE - use core config**)
- Utility functions (generate_document_id, determine_content_type) (**MOVE TO CORE**)

**Migration Strategy**:
- **Simplify to file change notification only**
- Remove embedding-specific logic (priority, batching) → Handled by enrichment layer
- Events should just trigger the five-phase pipeline
- Use core configuration types, not event-specific config

**Becomes**:
```rust
// crucible-watch/src/file_events.rs
pub struct FileChangeEvent {
    pub file_path: PathBuf,
    pub event_kind: FileEventKind,
    pub timestamp: DateTime<Utc>,
}

// File watcher triggers DocumentProcessor (in core), not embedding-specific logic
```

**Dependencies to Update**:
- Use `crucible_core::enrichment::EnrichedNote` instead of custom types
- Trigger `DocumentProcessor` instead of embedding-specific pipeline

**Test Coverage**: Unit tests exist - UPDATE for new workflow

---

## Existing Infrastructure Analysis

### ✅ GOOD: EmbeddingProvider Trait Already Exists!

**Location**: `crucible-llm/src/embeddings/provider.rs`
**Size**: 800+ lines with comprehensive trait definition

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse>;
    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>>;
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> usize;
}
```

**Existing Implementations**:
- ✅ `FastembedProvider` (fastembed.rs) - 18,727 lines
- ✅ `MockProvider` (mock.rs) - 19,289 lines
- ✅ `OllamaProvider` (ollama.rs) - 18,877 lines
- ✅ `OpenAIProvider` (openai.rs) - 13,659 lines

**Implication**: We do NOT need to create the trait - it exists! Just need to:
1. Use it consistently in enrichment layer
2. Ensure configuration types moved to core are compatible
3. Remove redundant provider management from surrealdb layer

---

## Migration Dependency Graph

```
Phase 0.1 (Audit) ← YOU ARE HERE
    ↓
Phase 0.2 (Config Migration)
    ├─ Create crucible-core/src/enrichment/config.rs
    ├─ Move EmbeddingConfig types
    ├─ Update imports in crucible-surrealdb
    └─ Update imports in crucible-llm
    ↓
Phase 1 (Core Enrichment Layer)
    ├─ Create EnrichmentService (uses existing EmbeddingProvider trait)
    ├─ Define EnrichedNote type
    ├─ Implement enrichment orchestration
    └─ NO database dependencies
    ↓
Phase 2 (Provider Updates)
    ├─ Update FastembedProvider to use core config
    ├─ Remove thread pool complexity (providers manage own concurrency)
    └─ Ensure all providers implement consistent trait
    ↓
Phase 3 (Pipeline Integration)
    ├─ Create DocumentProcessor orchestrator
    ├─ Integrate five-phase pipeline
    ├─ Connect to EnrichmentService
    └─ Update file watcher to trigger pipeline
    ↓
Phase 4 (Storage Layer Cleanup)
    ├─ Refactor embedding.rs to storage-only
    ├─ Remove embedding_pipeline.rs (logic moved to core)
    ├─ Remove embedding_pool.rs (simplified concurrency)
    └─ Update kiln_integration for new architecture
```

---

## Circular Dependency Risks

### HIGH RISK: Configuration Location

**Problem**:
```
crucible-surrealdb (infrastructure)
    ↓ depends on
crucible-core (domain)
    ↓ needs config from
crucible-surrealdb (❌ CIRCULAR!)
```

**Solution**:
Move ALL configuration to crucible-core first (Phase 0.2)

### MEDIUM RISK: Provider Factory

**Current**:
```rust
// In crucible-llm
pub fn create_provider(config: EmbeddingConfig) -> Arc<dyn EmbeddingProvider>
```

**Issue**: Config type must be in core or llm, not surrealdb

**Solution**: Use config from core after Phase 0.2

---

## Test Coverage Baseline

### Existing Tests

**embedding_config.rs**: ✅ Excellent
- 13 unit tests covering all config paths
- Validation tests
- Optimization preset tests
- Must maintain 100% pass rate

**embedding_pool.rs**: ⚠️ Minimal
- Few integration tests
- Need more unit tests for refactored code

**embedding_pipeline.rs**: ⚠️ Some coverage
- Integration tests exist
- Need unit tests for orchestration logic

**embedding_events.rs**: ✅ Good
- Unit tests for event creation
- Need updates for simplified workflow

### Test Strategy During Migration

1. **Phase 0.2**: Run all existing tests after config move
2. **Phase 1**: Write NEW tests for EnrichmentService (TDD)
3. **Phase 2**: Update provider tests for new config
4. **Phase 3**: Integration tests for five-phase pipeline
5. **Phase 4**: Storage-only tests for refactored embedding.rs

**SUCCESS CRITERIA**: Zero test regressions, all phases tested

---

## Code Reuse Opportunities

### ✅ Can Reuse:
- EmbeddingProvider trait (crucible-llm) - **already exists!**
- Provider implementations (Fastembed, Ollama, OpenAI) - **already exists!**
- Configuration types (after moving to core) - **well-designed**
- Error types and handling - **comprehensive**
- Result types - **good structure**

### ❌ Should Delete/Simplify:
- `EmbeddingThreadPool` - **over-engineered, providers handle concurrency**
- `EmbeddingPipeline` - **too coupled to storage, replace with EnrichmentService**
- Event-specific configuration - **use core config instead**
- Custom batching logic - **providers handle this**

### 🔄 Should Refactor:
- Chunking logic - **move to metadata extraction**
- Incremental detection - **Merkle diff handles this**
- Database operations - **move to repository pattern**

---

## Migration Plan Summary

### Files to DELETE (completely):
1. `crucible-surrealdb/src/embedding_pool.rs` (1,476 lines)
   - Thread management belongs in providers
   - Over-engineered for current needs
   - Simplified concurrency with tokio

2. `crucible-surrealdb/src/embedding_pipeline.rs` (750 lines)
   - Replaced by EnrichmentService in core
   - Storage logic moves to repository
   - Orchestration done by DocumentProcessor

### Files to MOVE (to crucible-core):
1. `crucible-surrealdb/src/embedding_config.rs` (850 lines)
   → `crucible-core/src/enrichment/config.rs`
   - Remove provider-specific details
   - Make configuration provider-agnostic
   - Keep optimization presets

### Files to REFACTOR (keep but simplify):
1. `crucible-surrealdb/src/embedding.rs` (20 lines)
   - Reduce to pure storage operations
   - Remove business logic re-exports
   - Keep only: store, get, delete, search

2. `crucible-watch/src/embedding_events.rs` (332 lines)
   - Simplify to file change events
   - Remove embedding-specific logic
   - Use core types instead of custom

### New Files to CREATE:
1. `crucible-core/src/enrichment/mod.rs` - Module entry point
2. `crucible-core/src/enrichment/service.rs` - EnrichmentService orchestrator
3. `crucible-core/src/enrichment/embedding.rs` - Re-export EmbeddingProvider trait
4. `crucible-core/src/enrichment/metadata.rs` - MetadataExtractor
5. `crucible-core/src/enrichment/relations.rs` - RelationInferrer
6. `crucible-core/src/enrichment/types.rs` - EnrichedNote and related types
7. `crucible-core/src/processing/document_processor.rs` - Five-phase orchestrator

---

## Risks and Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| **Circular dependencies** | HIGH | MEDIUM | Move config to core FIRST (Phase 0.2) |
| **Test failures during migration** | HIGH | MEDIUM | Maintain test baseline, TDD for new code |
| **Breaking API changes** | MEDIUM | HIGH | Solo developer - acceptable, tests ensure correctness |
| **Provider compatibility issues** | MEDIUM | LOW | Providers already use correct trait |
| **Performance regression** | MEDIUM | LOW | Benchmark each phase, Merkle diff improves performance |
| **Data migration complexity** | LOW | LOW | Regenerate embeddings, no complex migration |

---

## Next Steps (Phase 0.2)

1. **Create crucible-core/src/enrichment/config.rs**
   - Copy types from embedding_config.rs
   - Remove Fastembed-specific details
   - Write tests

2. **Update imports across codebase**
   - Update crucible-surrealdb to use core config
   - Update crucible-llm provider factory
   - Update crucible-watch events

3. **Verify no regressions**
   - Run full test suite
   - Ensure all 13 config tests pass
   - Check compilation across workspace

4. **Delete crucible-surrealdb/src/embedding_config.rs**
   - After successful migration
   - Update lib.rs module exports

**Estimated Effort**: 4-6 hours
**Success Criteria**: All tests pass, zero compilation errors, config accessible from all crates

---

## Conclusion

**Architecture Violations**: SEVERE but well-documented
**Migration Complexity**: HIGH but manageable with phased approach
**Existing Infrastructure**: GOOD - trait already exists, providers implemented
**Risk Level**: MEDIUM - circular dependencies biggest concern, mitigated by config-first approach

**Recommendation**: PROCEED with Phase 0.2 (Config Migration) immediately. The foundation (EmbeddingProvider trait) already exists in crucible-llm, which significantly reduces implementation effort. Main work is moving configuration and creating clean orchestration layer.

**Timeline Impact**: Existing trait reduces work by ~1 week. Estimated 3 weeks instead of 4.

**READY FOR PHASE 0.2: Extract Configuration to Core**
