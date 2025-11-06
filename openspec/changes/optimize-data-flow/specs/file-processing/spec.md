## ADDED Requirements

### Requirement: File-Level Change Detection
The system SHALL detect file changes before expensive parsing operations using content hashing.

**Implementation:** `crucible-watch/src/file_scanner.rs`, `crucible-core/src/hashing/file_hasher.rs`

#### Scenario: Skip Unchanged Files
- **WHEN** scanning kiln directory for files (via `FileScanner` in `crucible-watch`)
- **THEN** system computes content hash for each discovered file (using `FileHasher` from `crucible-core`)
- **AND** queries database for previously stored file hash (via `HashLookupStorage` trait implemented by `crucible-surrealdb`)
- **AND** skips parsing and processing for files with unchanged content hashes (in `ChangeDetector` from `crucible-watch`)

#### Scenario: Quick Hash Computation
- **WHEN** computing file content hashes during discovery
- **THEN** system uses efficient streaming hash algorithm (BLAKE3 implementation in `crucible-core/src/hashing/file_hasher.rs`)
- **AND** stores hash in `FileInfo` structure (defined in `crucible-watch`)
- **AND** completes hashing within milliseconds for typical file sizes

### Requirement: AST Block-Based Hashing and Merkle Trees
The system SHALL generate hashes for AST node blocks and construct Merkle trees for efficient change detection.

**Implementation:** `crucible-parser/src/block_extractor.rs`, `crucible-core/src/hashing/block_hasher.rs`

#### Scenario: AST Block Hash Generation
- **WHEN** parsing markdown document into AST blocks (via `BlockExtractor` in `crucible-parser`)
- **THEN** system uses parsed AST nodes as natural block boundaries
- **AND** each AST node (heading, paragraph, list, code block, callout) becomes one `ASTBlock`
- **AND** generates cryptographic hash for each AST block's content (using `BlockHasher` from `crucible-core`)
- **AND** stores block hashes in ParsedDocument structure (in `crucible-parser`)

#### Scenario: Semantic Block Boundaries
- **WHEN** extracting blocks from parsed document
- **THEN** system uses AST node boundaries without additional chunking logic
- **AND** preserves complete semantic units (entire paragraphs, complete lists, full code blocks)
- **AND** aligns blocks with HTML rendering (one AST node = one HTML element)
- **AND** maintains user mental model (edit paragraph = one block changed)

#### Scenario: Merkle Tree Construction from AST Blocks
- **WHEN** all AST blocks in a document have been hashed
- **THEN** system constructs binary Merkle tree from block hashes (using `BlockHasher::build_merkle_tree()` in `crucible-core`)
- **AND** computes parent node hashes from child node hashes (using existing `MerkleTree` type from `crucible-core/src/storage/merkle.rs`)
- **AND** stores complete Merkle tree structure in content-addressed storage (via `ContentAddressedStorage` trait implemented by `crucible-surrealdb`)
- **AND** associates tree with document ID for future comparisons (in database via `crucible-surrealdb`)

### Requirement: Merkle Tree Diffing
The system SHALL compare Merkle trees to identify granular changes between document versions.

**Implementation:** `crucible-core/src/storage/tree_differ.rs`, `crucible-watch/src/change_detector.rs`

#### Scenario: Efficient Tree Comparison
- **WHEN** processing a file with detected file-level changes (in `ChangeDetector` from `crucible-watch`)
- **THEN** system loads previously stored Merkle tree for document (via `ContentAddressedStorage` trait)
- **AND** compares new tree root hash against stored tree root hash (using `TreeDiffer` from `crucible-core`)
- **AND** if roots match, skips further processing (no changes)
- **AND** if roots differ, traverses tree to identify changed blocks (using `MerkleTree::compare_enhanced()` from `crucible-core`)

#### Scenario: Granular Change Set Generation
- **WHEN** Merkle tree differences are detected
- **THEN** system generates precise change set with block indices
- **AND** identifies added blocks (new AST nodes)
- **AND** identifies removed blocks (deleted AST nodes)
- **AND** identifies modified blocks (changed AST node content)
- **AND** excludes unchanged blocks from further processing

### Requirement: Content-Addressed Block Storage and Deduplication
The system SHALL store blocks content-addressed by hash, enabling deduplication across documents.

**Implementation:** `crucible-surrealdb/src/content_addressed_storage.rs`, `crucible-core/src/storage/deduplicator.rs`

#### Scenario: Content-Addressed Block Storage
- **WHEN** storing document blocks (via `ContentAddressedStorage::store_block()` in `crucible-surrealdb`)
- **THEN** system stores each block indexed by its content hash in database
- **AND** multiple documents can reference same block hash (via `document_blocks` table)
- **AND** identical content (same hash) is stored only once
- **AND** reduces storage requirements proportional to duplication rate

#### Scenario: Cross-Document Deduplication
- **WHEN** processing multiple documents with identical blocks
- **THEN** system detects duplicate content via hash matching (using `Deduplicator` from `crucible-core`)
- **AND** reuses existing stored blocks instead of creating duplicates
- **AND** maintains references from documents to shared block hashes (in `document_blocks` table in `crucible-surrealdb`)
- **AND** enables "find all documents containing this block" queries (via queries in `crucible-surrealdb`)

#### Scenario: Deduplication Examples
- **WHEN** common content appears across documents (quotes, code examples, definitions)
- **THEN** system stores content once and references from multiple documents
- **AND** reduces embedding generation costs (one embedding per unique block)
- **AND** maintains consistency (same content always has same embedding)

### Requirement: Incremental Block-Level Embedding Updates
The system SHALL generate embeddings only for changed AST blocks, avoiding redundant processing.

**Implementation:** `crucible-llm` (or `crucible-watch`), `crucible-surrealdb/src/block_embeddings.rs`

#### Scenario: Content-Addressed Embedding Lookup
- **WHEN** processing document blocks for embedding (in embedding pipeline)
- **THEN** system checks if embedding exists for each block hash (via `block_embeddings` table in `crucible-surrealdb`)
- **AND** reuses existing embedding if block hash found in storage
- **AND** generates new embedding only if block hash not found (via embedding API in `crucible-llm`)
- **AND** stores embeddings indexed by block hash (content-addressed in `crucible-surrealdb`)

#### Scenario: Selective Embedding Generation
- **WHEN** processing document with changed blocks identified by Merkle tree diff
- **THEN** system generates embeddings only for modified, added blocks
- **AND** skips embedding generation for unchanged blocks
- **AND** reuses embeddings for blocks with matching hashes
- **AND** updates only changed embeddings in database

#### Scenario: Efficient Resource Usage
- **WHEN** processing large documents (10KB, 10 AST blocks) with minimal changes (1 block edited)
- **THEN** system completes processing in <500ms (vs ~2000ms full reprocessing)
- **AND** reduces embedding API calls by 90% (1 new embedding vs 10 full re-embedding)
- **AND** maintains embedding quality for changed content
- **AND** provides sub-second user experience on consumer hardware

#### Scenario: Large Vault Incremental Processing
- **WHEN** processing vault with 1000 documents after editing 10 files
- **THEN** system completes in <5 seconds (vs ~60 seconds full reprocessing)
- **AND** processes only changed blocks (~10 files × 2 blocks average = ~20 embeddings)
- **AND** leverages deduplication across entire vault
- **AND** provides progress feedback for long-running operations

### Requirement: Block-Level Semantic Search
The system SHALL perform semantic search at AST block granularity, not document granularity.

**Implementation:** `crucible-surrealdb` (vector search queries), `crucible-cli` (result processing)

#### Scenario: Block-Based Query Matching
- **WHEN** user performs semantic search query (in `crucible-cli`)
- **THEN** system generates query embedding (via `crucible-llm`)
- **AND** compares query embedding against all block embeddings (vector similarity query in `crucible-surrealdb` on `block_embeddings` table)
- **AND** returns matching blocks with similarity scores
- **AND** includes document context for each matching block (via join with `document_blocks` table)

#### Scenario: Multi-Block Document Results
- **WHEN** single document contains multiple blocks matching query
- **THEN** system aggregates blocks into document result
- **AND** highlights best-matching blocks within document
- **AND** ranks document by highest block similarity score
- **AND** provides block-level context for why document matched

#### Scenario: Search Result Presentation
- **WHEN** displaying search results to user
- **THEN** system shows matching block content with document references
- **AND** enables highlighting of specific matching blocks in rendered document
- **AND** maps block IDs to HTML elements for visual highlighting
- **AND** allows user to navigate to exact matching section in document

## MODIFIED Requirements

### Requirement: Startup File Processing Workflow
The system SHALL provide efficient incremental file processing during CLI initialization using change detection.

**Implementation:** `crucible-cli` (orchestration), `crucible-watch` (scanning/detection), `crucible-surrealdb` (storage)

#### Scenario: Intelligent File Processing
- **WHEN** CLI starts up and scans kiln directory (in `crucible-cli`)
- **THEN** system performs file-level change detection first (using `FileScanner` + `ChangeDetector` from `crucible-watch`)
- **AND** only processes files that have actually changed (filtered by `ChangeSet` from `crucible-watch`)
- **AND** processes changed files using block-level diffing (using `TreeDiffer` from `crucible-core`)
- **AND** completes processing significantly faster than full reprocessing

#### Scenario: Database Consistency with Incremental Updates
- **WHEN** incremental processing completes
- **THEN** database state reflects all file changes accurately
- **AND** Merkle trees are updated to reflect current document state
- **AND** embeddings are synchronized with block content
- **AND** CLI commands operate on fully up-to-date data