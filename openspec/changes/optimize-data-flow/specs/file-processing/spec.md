## ADDED Requirements

### Requirement: File-Level Change Detection
The system SHALL detect file changes before expensive parsing operations using content hashing.

#### Scenario: Skip Unchanged Files
- **WHEN** scanning kiln directory for files
- **THEN** system computes content hash for each discovered file
- **AND** queries database for previously stored file hash
- **AND** skips parsing and processing for files with unchanged content hashes

#### Scenario: Quick Hash Computation
- **WHEN** computing file content hashes during discovery
- **THEN** system uses efficient streaming hash algorithm (BLAKE3)
- **AND** stores hash in KilnFileInfo for later comparison
- **AND** completes hashing within milliseconds for typical file sizes

### Requirement: AST Block-Based Hashing and Merkle Trees
The system SHALL generate hashes for AST node blocks and construct Merkle trees for efficient change detection.

#### Scenario: AST Block Hash Generation
- **WHEN** parsing markdown document into AST blocks
- **THEN** system uses parsed AST nodes as natural block boundaries
- **AND** each AST node (heading, paragraph, list, code block, callout) becomes one block
- **AND** generates cryptographic hash for each AST block's content
- **AND** stores block hashes in ParsedDocument structure

#### Scenario: Semantic Block Boundaries
- **WHEN** extracting blocks from parsed document
- **THEN** system uses AST node boundaries without additional chunking logic
- **AND** preserves complete semantic units (entire paragraphs, complete lists, full code blocks)
- **AND** aligns blocks with HTML rendering (one AST node = one HTML element)
- **AND** maintains user mental model (edit paragraph = one block changed)

#### Scenario: Merkle Tree Construction from AST Blocks
- **WHEN** all AST blocks in a document have been hashed
- **THEN** system constructs binary Merkle tree from block hashes
- **AND** computes parent node hashes from child node hashes
- **AND** stores complete Merkle tree structure in content-addressed storage
- **AND** associates tree with document ID for future comparisons

### Requirement: Merkle Tree Diffing
The system SHALL compare Merkle trees to identify granular changes between document versions.

#### Scenario: Efficient Tree Comparison
- **WHEN** processing a file with detected file-level changes
- **THEN** system loads previously stored Merkle tree for document
- **AND** compares new tree root hash against stored tree root hash
- **AND** if roots match, skips further processing (no changes)
- **AND** if roots differ, traverses tree to identify changed blocks

#### Scenario: Granular Change Set Generation
- **WHEN** Merkle tree differences are detected
- **THEN** system generates precise change set with block indices
- **AND** identifies added blocks (new AST nodes)
- **AND** identifies removed blocks (deleted AST nodes)
- **AND** identifies modified blocks (changed AST node content)
- **AND** excludes unchanged blocks from further processing

### Requirement: Content-Addressed Block Storage and Deduplication
The system SHALL store blocks content-addressed by hash, enabling deduplication across documents.

#### Scenario: Content-Addressed Block Storage
- **WHEN** storing document blocks
- **THEN** system stores each block indexed by its content hash
- **AND** multiple documents can reference same block hash
- **AND** identical content (same hash) is stored only once
- **AND** reduces storage requirements proportional to duplication rate

#### Scenario: Cross-Document Deduplication
- **WHEN** processing multiple documents with identical blocks
- **THEN** system detects duplicate content via hash matching
- **AND** reuses existing stored blocks instead of creating duplicates
- **AND** maintains references from documents to shared block hashes
- **AND** enables "find all documents containing this block" queries

#### Scenario: Deduplication Examples
- **WHEN** common content appears across documents (quotes, code examples, definitions)
- **THEN** system stores content once and references from multiple documents
- **AND** reduces embedding generation costs (one embedding per unique block)
- **AND** maintains consistency (same content always has same embedding)

### Requirement: Incremental Block-Level Embedding Updates
The system SHALL generate embeddings only for changed AST blocks, avoiding redundant processing.

#### Scenario: Content-Addressed Embedding Lookup
- **WHEN** processing document blocks for embedding
- **THEN** system checks if embedding exists for each block hash
- **AND** reuses existing embedding if block hash found in storage
- **AND** generates new embedding only if block hash not found
- **AND** stores embeddings indexed by block hash (content-addressed)

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

#### Scenario: Block-Based Query Matching
- **WHEN** user performs semantic search query
- **THEN** system generates query embedding
- **AND** compares query embedding against all block embeddings (not document embeddings)
- **AND** returns matching blocks with similarity scores
- **AND** includes document context for each matching block

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

#### Scenario: Intelligent File Processing
- **WHEN** CLI starts up and scans kiln directory
- **THEN** system performs file-level change detection first
- **AND** only processes files that have actually changed
- **AND** processes changed files using block-level diffing
- **AND** completes processing significantly faster than full reprocessing

#### Scenario: Database Consistency with Incremental Updates
- **WHEN** incremental processing completes
- **THEN** database state reflects all file changes accurately
- **AND** Merkle trees are updated to reflect current document state
- **AND** embeddings are synchronized with block content
- **AND** CLI commands operate on fully up-to-date data