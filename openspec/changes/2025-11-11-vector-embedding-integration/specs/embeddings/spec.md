# Embeddings Capability Specification

## ADDED Requirements

### Requirement: Embedding Provider Abstraction (SOLID/Dependency Inversion)
The system SHALL define an `EmbeddingProvider` trait in the core domain layer with concrete implementations in the infrastructure layer, following SOLID principles.

**Architecture**:
- **Trait Definition**: `crucible-core/src/enrichment/embedding.rs` - Abstract interface
- **Implementations**: `crucible-llm/src/providers/` - Fastembed, OpenAI, custom providers
- **Dependency Flow**: Core defines contract, infrastructure provides implementation

#### Scenario: Provider switching with dependency injection
- **WHEN** user configures different embedding provider in configuration
- **THEN** EnrichmentService receives appropriate implementation via dependency injection
- **AND** no code changes are required in core domain logic

#### Scenario: Local embedding generation (Fastembed)
- **WHEN** offline operation is required or privacy mode is enabled
- **THEN** FastembedProvider (crucible-llm) generates embeddings locally using ONNX models
- **AND** no network calls are made
- **AND** models are loaded once and reused across batches

#### Scenario: Cloud embedding generation (OpenAI)
- **WHEN** higher quality embeddings are needed
- **THEN** OpenAIProvider (crucible-llm) generates embeddings via API with error handling
- **AND** appropriate rate limiting and retry logic is applied
- **AND** API credentials are managed securely

### Requirement: Enrichment Service Orchestration
The system SHALL provide an `EnrichmentService` in crucible-core that orchestrates all enrichment operations including embedding generation, metadata extraction, and relation inference.

**Architecture**:
- **Location**: `crucible-core/src/enrichment/service.rs`
- **Responsibilities**: Coordinate parallel enrichment operations using Merkle diff results
- **Dependencies**: Receives `EmbeddingProvider` trait implementation via dependency injection

#### Scenario: Enrichment orchestration with parallel operations
- **WHEN** EnrichmentService receives ParsedNote and changed block list from Merkle diff
- **THEN** service executes embedding generation, metadata extraction, and relation inference in parallel
- **AND** collects all results into EnrichedNote structure
- **AND** passes EnrichedNote to storage layer for persistence

### Requirement: Block-Level Embedding Generation (Incremental Only)
The system SHALL generate vector embeddings ONLY for blocks identified as changed by Merkle tree diff, following the five-phase data flow.

**Five-Phase Data Flow**:
1. **Quick Filter**: Check file modified date + BLAKE3 hash (skip if unchanged)
2. **Parsing**: Full file parse to AST
3. **Merkle Diff**: Build tree from AST, compare to stored tree, identify changed blocks
4. **Enrichment**: Generate embeddings for changed blocks only (>5 words)
5. **Storage**: Transactional persistence

#### Scenario: Initial document processing (all blocks new)
- **WHEN** new markdown document is processed for first time
- **THEN** Phase 1 detects no existing hash (proceed to parse)
- **AND** Phase 3 Merkle diff identifies all blocks as "changed" (no existing tree)
- **AND** Phase 4 generates embeddings for all blocks >5 words
- **AND** Phase 5 stores all embeddings with new Merkle tree

#### Scenario: Incremental embedding updates (only changed blocks)
- **WHEN** existing document content is modified
- **THEN** Phase 1 detects hash difference (proceed to parse)
- **AND** Phase 2 parses full file to AST
- **AND** Phase 3 builds new Merkle tree and diffs against stored tree
- **AND** Merkle diff identifies specific changed block IDs
- **AND** Phase 4 generates embeddings ONLY for changed blocks >5 words
- **AND** Phase 5 deletes old embeddings for changed blocks, stores new embeddings

#### Scenario: File unchanged (skip all processing)
- **WHEN** file modified date matches DB and hash matches DB
- **THEN** Phase 1 skips all further processing
- **AND** no parsing, Merkle diffing, or embedding generation occurs

#### Scenario: Embedding exclusion for short content
- **WHEN** content block contains 5 or fewer words
- **THEN** EnrichmentService filters out block from embedding batch
- **AND** no embedding is generated to avoid noise in search results

### Requirement: Embedding Storage (Pure I/O Layer)
The storage layer in crucible-surrealdb SHALL handle ONLY vector persistence and retrieval operations, with NO business logic for embedding generation.

**Architecture**:
- **Location**: `crucible-surrealdb/src/embedding.rs` (refactored to storage-only)
- **Responsibilities**: Store vectors, search by similarity, delete embeddings
- **NOT Responsible For**: Embedding generation, provider management, enrichment orchestration

#### Scenario: Vector storage with metadata
- **WHEN** EnrichedNote is passed to storage layer from EnrichmentService
- **THEN** storage layer persists embeddings with metadata (model, dimensions, timestamp)
- **AND** associates embeddings with block IDs
- **AND** NO embedding generation occurs in storage layer

#### Scenario: Vector similarity search
- **WHEN** user performs semantic search query
- **THEN** query embedding is generated by EnrichmentService (not storage)
- **AND** storage layer performs vector similarity search using SurrealDB
- **AND** returns blocks with most similar embeddings ranked by cosine similarity

#### Scenario: Embedding deletion for changed blocks
- **WHEN** Merkle diff identifies changed blocks during Phase 3
- **THEN** Phase 5 storage deletes old embeddings for those block IDs
- **AND** stores new embeddings generated in Phase 4
- **AND** deletion and insertion occur in single transaction

### Requirement: Hybrid Search Integration
The system SHALL combine semantic similarity with existing graph and text-based search methods.

#### Scenario: Combined search query
- **WHEN** user searches with natural language query
- **THEN** results combine semantic similarity, graph relationships, and keyword matching

#### Scenario: Relevance scoring
- **WHEN** search results are ranked
- **THEN** composite score combines semantic, graph, and text relevance with configurable weights

#### Scenario: Search result diversity
- **WHEN** multiple similar documents match
- **THEN** results include diverse sources to avoid echo chambers

### Requirement: Configuration and Management
The system SHALL provide configuration options for embedding models and search parameters.

#### Scenario: Model selection
- **WHEN** user specifies embedding model preference
- **THEN** system uses the selected model for all operations

#### Scenario: Search tuning
- **WHEN** search results need adjustment
- **THEN** weights for semantic vs graph vs text components can be configured

#### Scenario: Performance optimization
- **WHEN** system resources are limited
- **THEN** batch sizes and caching strategies can be tuned

## MODIFIED Requirements

### Requirement: Document Processing Pipeline (Five-Phase Architecture)
The document processing pipeline SHALL be restructured into five distinct phases with clear separation of concerns, integrating the new EnrichmentService.

**Pipeline Architecture**:
```
Phase 1: Quick Filter → Phase 2: Parse → Phase 3: Merkle Diff →
Phase 4: Enrichment → Phase 5: Storage
```

#### Scenario: End-to-end document processing with enrichment layer
- **WHEN** file system detects markdown document change
- **THEN** Phase 1 checks file modified date and BLAKE3 hash
- **AND** Phase 2 parses full file to AST (if not skipped)
- **AND** Phase 3 builds Merkle tree from AST and diffs against stored tree
- **AND** Phase 4 EnrichmentService generates embeddings for changed blocks + metadata + relations
- **AND** Phase 5 storage persists enriched data in single transaction
- **AND** all phases execute with proper error handling and logging

#### Scenario: Error handling with graceful degradation
- **WHEN** embedding provider fails during Phase 4
- **THEN** EnrichmentService logs error with provider details
- **AND** enrichment continues with metadata and relation extraction
- **AND** storage persists partial enrichment (no embeddings for failed blocks)
- **AND** system tracks failed blocks for retry
- **AND** document remains queryable via graph/text search

#### Scenario: Progress reporting for bulk processing
- **WHEN** processing large knowledge base (many files)
- **THEN** system reports progress for each phase per file
- **AND** provides estimated completion time based on average phase durations
- **AND** tracks and reports Merkle diff efficiency (blocks skipped vs processed)

## REMOVED Requirements

None - all existing functionality preserved.