# Vector Embedding Integration for Semantic Search

**Change ID**: `2025-11-11-vector-embedding-integration`
**Status**: Ready for Implementation (Revised Architecture)
**Created**: 2025-11-11
**Updated**: 2025-11-16
**Author**: Matthew Krohn

## Why

The parser now produces block-level entities ready for vector embeddings, but the embedding generation and semantic search infrastructure has architectural issues that violate clean architecture principles documented in ARCHITECTURE.md.

**Current Problems**:
1. **Layer Violation**: Embedding logic exists in `crucible-surrealdb` (infrastructure) instead of core domain layer
2. **Missing Enrichment Layer**: No orchestrator coordinates embedding + Merkle + metadata enrichment
3. **Unclear Data Flow**: When/where embeddings are generated is not well-defined in ingestion pipeline
4. **Tight Coupling**: Embedding provider implementations coupled to storage layer

This change corrects the architecture to match the clean enrichment pipeline documented in ARCHITECTURE.md:136-157.

## What Changes

### Architecture Corrections

**NEW: Enrichment Layer in crucible-core**:
- `EnrichmentService` orchestrator coordinates all enrichment operations
- `EmbeddingProvider` trait abstraction (SOLID/Dependency Inversion)
- Parallel enrichment: embeddings, metadata extraction, relation inference
- Clear separation: Parser (pure) → Enrichment (business logic) → Storage (pure I/O)

**Refactored: Embedding Components**:
- Trait definitions moved to `crucible-core/src/enrichment/embedding.rs`
- Provider implementations in `crucible-llm/src/providers/` (Fastembed, OpenAI)
- Storage layer only handles vector persistence/search (stays in `crucible-surrealdb`)

**NEW: Five-Phase Data Flow (Merkle-Driven Efficiency)**:
1. **Quick Filter**: Check file modified date + BLAKE3 hash (skip if unchanged)
2. **Parse to AST**: Build tree structure from pulldown-cmark event stream (unavoidable)
3. **Merkle Diff**: Single tree traversal to build Merkle tree + diff with stored tree → identify changed blocks
4. **Enrich Changed Blocks**: Process ONLY changed blocks (per Merkle diff):
   - Generate embeddings (>5 words)
   - Extract relations (wikilinks, tags)
   - Compute metadata (word counts, language)
5. **Storage**: Transactional persistence of enriched data

**Efficiency Strategy**: Merkle diff identifies changed blocks, avoiding redundant processing of unchanged content. Multiple logical passes are acceptable - real efficiency comes from processing only what changed.

### Technical Changes

- **crucible-core/src/enrichment/**: New module with service orchestration
- **crucible-llm/src/providers/**: Embedding provider implementations
- **crucible-surrealdb**: Refactor to pure storage layer (remove business logic)
- **Merkle Integration**: Tree built from AST (Phase 3), used for block-level change detection
- **Incremental Processing**: Only re-embed blocks identified by Merkle diff

## Impact

### Files Created
- `crucible-core/src/enrichment/mod.rs` - Module entry point
- `crucible-core/src/enrichment/service.rs` - EnrichmentService orchestrator
- `crucible-core/src/enrichment/embedding.rs` - EmbeddingProvider trait
- `crucible-core/src/enrichment/metadata.rs` - MetadataExtractor
- `crucible-core/src/enrichment/relations.rs` - RelationInferrer
- `crucible-core/src/enrichment/types.rs` - EnrichedNote and related types
- `crucible-llm/src/providers/mod.rs` - Provider module
- `crucible-llm/src/providers/fastembed.rs` - Fastembed implementation
- `crucible-llm/src/providers/openai.rs` - OpenAI implementation (future)

### Files Refactored
- `crucible-surrealdb/src/embedding.rs` - Reduce to storage-only operations
- `crucible-surrealdb/src/eav_graph/ingest.rs` - Integrate with EnrichmentService
- `crucible-core/src/processing/mod.rs` - Add enrichment phase to pipeline

### Files Deleted
- `crucible-surrealdb/src/embedding_pipeline.rs` - Business logic moves to core
- `crucible-surrealdb/src/embedding_pool.rs` - Thread management moves to provider impl
- `crucible-surrealdb/src/embedding_config.rs` - Config moves to core or provider

### Affected Specs
- `embeddings` - Updated with correct architecture and data flow
- `parser` - Clarified integration with enrichment layer

## Success Criteria

1. **Clean Architecture**: Enrichment layer in core, storage has no business logic
2. **SOLID Compliance**: EmbeddingProvider trait in core, implementations in LLM crate
3. **Correct Data Flow**: File check → Parse → Merkle diff → Enrich (changed only) → Store
4. **Incremental Processing**: Only re-embed blocks identified by Merkle tree diff
5. **Performance**: Process 1000 blocks in <30 seconds (Fastembed), only embed changed content
6. **Provider Flexibility**: Easy to swap Fastembed/OpenAI/custom via trait
7. **Integration**: Seamless integration with existing Merkle tree and parser infrastructure