## 1. File-Level Change Detection

### 1.1 Refactor Architecture (PREREQUISITE)
- [ ] 1.1.1 Create `ContentHasher` trait in `crucible-core/src/traits/change_detection.rs`
- [ ] 1.1.2 Create `HashLookupStorage` trait in `crucible-core/src/traits/change_detection.rs`
- [ ] 1.1.3 Create `ChangeDetector` trait in `crucible-core/src/traits/change_detection.rs`

### 1.2 File Hashing (crucible-core)
- [ ] 1.2.1 Move BLAKE3 streaming hash to `crucible-core/src/hashing/file_hasher.rs`
- [ ] 1.2.2 Implement `ContentHasher` trait for `FileHasher`
- [ ] 1.2.3 Add batch hashing support for parallel processing
- [ ] 1.2.4 Add tests for file hashing in crucible-core

### 1.3 File Scanning (crucible-watch)
- [ ] 1.3.1 Create `FileInfo` type in `crucible-watch/src/types.rs` (with content_hash field)
- [ ] 1.3.2 Create `FileScanner` in `crucible-watch/src/file_scanner.rs`
- [ ] 1.3.3 Implement directory scanning using `FileHasher` from core
- [ ] 1.3.4 Add integration tests for file scanning

### 1.4 Change Detection (crucible-watch)
- [ ] 1.4.1 Create `ChangeDetector` in `crucible-watch/src/change_detector.rs`
- [ ] 1.4.2 Implement change detection logic (compare discovered vs stored hashes)
- [ ] 1.4.3 Return `ChangeSet` with unchanged/changed/new/deleted files
- [ ] 1.4.4 Add tests for change detection with mocked storage

### 1.5 Hash Lookup Storage (crucible-surrealdb)
- [ ] 1.5.1 Add file_hash column to database schema (`schema.surql`)
- [ ] 1.5.2 Create migration script (`v1_add_file_hash.surql`) ✅ DONE
- [ ] 1.5.3 Implement `HashLookupStorage` trait in `hash_lookup.rs` (keep existing implementation)
- [ ] 1.5.4 Keep batch query optimization and caching (already implemented) ✅ DONE

### 1.6 CLI Integration (crucible-cli)
- [ ] 1.6.1 Wire up `FileScanner` with `FileHasher` dependency injection
- [ ] 1.6.2 Wire up `ChangeDetector` with `SurrealHashLookup` dependency injection
- [ ] 1.6.3 Update processing pipeline to use `ChangeSet` for selective processing
- [ ] 1.6.4 Add end-to-end tests with real vault

## 2. AST Block-Based Hashing Infrastructure

### 2.1 Block Extraction (crucible-parser)
- [ ] 2.1.1 Create `ASTBlock` type in `crucible-parser/src/types.rs`
- [ ] 2.1.2 Create `BlockExtractor` in `crucible-parser/src/block_extractor.rs`
- [ ] 2.1.3 Implement extraction from `ParsedDocument` (headings, paragraphs, lists, code blocks)
- [ ] 2.1.4 Add tests for block extraction with various markdown structures

### 2.2 Block Hashing (crucible-core)
- [ ] 2.2.1 Create `BlockHasher` in `crucible-core/src/hashing/block_hasher.rs`
- [ ] 2.2.2 Implement AST block → string serialization for hashing
- [ ] 2.2.3 Implement block hashing using BLAKE3 (consistent with file hashing)
- [ ] 2.2.4 Add tests for block hashing and serialization

### 2.3 Merkle Tree Construction (crucible-core)
- [ ] 2.3.1 Extend `BlockHasher` to build Merkle trees from block hashes
- [ ] 2.3.2 Use existing `MerkleTree` type from `crucible-core/src/storage/merkle.rs`
- [ ] 2.3.3 Add tests for tree construction with various block counts

### 2.4 Document Integration (crucible-parser)
- [ ] 2.4.1 Extend `ParsedDocument` to store `block_hashes: Vec<BlockHash>`
- [ ] 2.4.2 Extend `ParsedDocument` to store optional `merkle_root: Option<String>`
- [ ] 2.4.3 Update parser to populate block hashes when available

### 2.5 Tree Storage (crucible-surrealdb)
- [ ] 2.5.1 Implement `ContentAddressedStorage::store_tree()` (use existing implementation in `content_addressed_storage.rs`)
- [ ] 2.5.2 Implement `ContentAddressedStorage::get_tree()` (use existing implementation)
- [ ] 2.5.3 Add database indexes for efficient tree retrieval

## 3. Content-Addressed Block Storage

### 3.1 Database Schema (crucible-surrealdb)
- [ ] 3.1.1 Add `document_blocks` table to `schema.surql` (document_id, block_index, block_hash)
- [ ] 3.1.2 Add indexes for efficient block lookups
- [ ] 3.1.3 Create migration script for schema update

### 3.2 Block Storage (crucible-surrealdb)
- [ ] 3.2.1 Implement `ContentAddressedStorage::store_block()` for block persistence
- [ ] 3.2.2 Implement `ContentAddressedStorage::get_block()` for block retrieval
- [ ] 3.2.3 Add block → document mapping in `document_blocks` table
- [ ] 3.2.4 Add tests for block storage and retrieval

### 3.3 Block Queries (crucible-surrealdb)
- [ ] 3.3.1 Implement "find documents containing block hash" query
- [ ] 3.3.2 Implement "get all blocks for document" query
- [ ] 3.3.3 Implement "get block by hash" query with content
- [ ] 3.3.4 Add batch query support for multiple block hashes

### 3.4 Deduplication (crucible-core + crucible-surrealdb)
- [ ] 3.4.1 Create `Deduplicator` in `crucible-core/src/storage/deduplicator.rs`
- [ ] 3.4.2 Implement duplicate block detection using storage queries
- [ ] 3.4.3 Compute deduplication statistics (storage saved, reuse count)
- [ ] 3.4.4 Add deduplication reporting in `crucible-surrealdb`

## 4. Merkle Tree Diffing

### 4.1 Tree Comparison (crucible-core)
- [ ] 4.1.1 Create `TreeDiffer` in `crucible-core/src/storage/tree_differ.rs`
- [ ] 4.1.2 Use existing `MerkleTree::compare_enhanced()` method
- [ ] 4.1.3 Generate block-level change sets (added/removed/modified indices)
- [ ] 4.1.4 Add tests for tree diffing with various edit patterns

### 4.2 Change Set Processing (crucible-watch)
- [ ] 4.2.1 Extend `ChangeDetector` to support block-level change sets
- [ ] 4.2.2 Load existing Merkle trees from storage via trait
- [ ] 4.2.3 Map block indices back to AST nodes for processing
- [ ] 4.2.4 Return `BlockChangeSet` with changed block details

### 4.3 Integration Testing (crucible-cli)
- [ ] 4.3.1 Test diffing with single block edits
- [ ] 4.3.2 Test diffing with multiple block edits
- [ ] 4.3.3 Test diffing with block additions/deletions
- [ ] 4.3.4 Validate accuracy with real markdown documents

## 5. Block-Level Embedding Infrastructure

### 5.1 Database Schema (crucible-surrealdb)
- [ ] 5.1.1 Add `block_embeddings` table to `schema.surql` (block_hash, embedding_vector, model)
- [ ] 5.1.2 Add indexes for efficient embedding lookups
- [ ] 5.1.3 Create migration script for schema update

### 5.2 Embedding Storage (crucible-surrealdb)
- [ ] 5.2.1 Implement content-addressed embedding storage (hash → embedding)
- [ ] 5.2.2 Implement embedding lookup by block hash
- [ ] 5.2.3 Add batch embedding storage for multiple blocks
- [ ] 5.2.4 Add tests for embedding storage and retrieval

### 5.3 Embedding Reuse (crucible-core or crucible-llm)
- [ ] 5.3.1 Create `EmbeddingCache` to check for existing embeddings before generation
- [ ] 5.3.2 Implement duplicate block embedding reuse logic
- [ ] 5.3.3 Track embedding reuse statistics
- [ ] 5.3.4 Add tests for embedding reuse scenarios

## 6. Incremental Embedding Pipeline

### 6.1 Pipeline Modification (crucible-watch or crucible-llm)
- [ ] 6.1.1 Extend embedding pipeline to accept `BlockChangeSet` instead of full documents
- [ ] 6.1.2 Implement selective embedding generation for changed blocks only
- [ ] 6.1.3 Skip embedding generation for unchanged blocks (reuse existing)
- [ ] 6.1.4 Add progress reporting for block-level embedding generation

### 6.2 Embedding Updates (crucible-surrealdb)
- [ ] 6.2.1 Update only changed block embeddings in database
- [ ] 6.2.2 Maintain document → block → embedding relationships
- [ ] 6.2.3 Clean up orphaned embeddings when blocks are deleted
- [ ] 6.2.4 Add transaction support for atomic embedding updates

### 6.3 Integration Testing (crucible-cli)
- [ ] 6.3.1 Test incremental embedding with single block edit
- [ ] 6.3.2 Test incremental embedding with multiple block edits
- [ ] 6.3.3 Measure embedding API call reduction (target: 90%+)
- [ ] 6.3.4 Validate embedding consistency across updates

## 7. Block-Level Semantic Search

### 7.1 Search Query Updates (crucible-surrealdb)
- [ ] 7.1.1 Update vector similarity queries to operate on `block_embeddings` table
- [ ] 7.1.2 Implement block similarity search with configurable threshold
- [ ] 7.1.3 Add document context retrieval for matching blocks
- [ ] 7.1.4 Optimize query performance with proper indexes

### 7.2 Result Processing (crucible-core or crucible-cli)
- [ ] 7.2.1 Implement result aggregation (group blocks by document)
- [ ] 7.2.2 Implement block-level result ranking (by similarity score)
- [ ] 7.2.3 Add result deduplication for blocks appearing in multiple documents
- [ ] 7.2.4 Calculate document-level scores from block matches

### 7.3 Presentation (crucible-cli or crucible-tauri)
- [ ] 7.3.1 Format search results with block highlighting
- [ ] 7.3.2 Show block context within document structure
- [ ] 7.3.3 Add block-level snippet extraction for previews
- [ ] 7.3.4 Test search result quality vs document-level search

## 8. Integration and Testing

### 8.1 Pipeline Integration (crucible-cli)
- [ ] 8.1.1 Wire all components together with dependency injection
- [ ] 8.1.2 Update CLI startup to use incremental file + block pipeline
- [ ] 8.1.3 Add configuration options for enabling/disabling block-level features
- [ ] 8.1.4 Ensure backward compatibility with file-level-only mode

### 8.2 Performance Benchmarking (crucible-cli)
- [ ] 8.2.1 Create benchmark suite comparing old vs new approach
- [ ] 8.2.2 Test with various vault sizes (100, 1000, 10000 documents)
- [ ] 8.2.3 Test incremental scenarios (edit 1 block, edit 10 files, etc.)
- [ ] 8.2.4 Measure: processing time, embedding API calls, database queries, storage usage

### 8.3 Migration Support (crucible-surrealdb)
- [ ] 8.3.1 Create migration script from document-level to block-level embeddings
- [ ] 8.3.2 Implement gradual migration (process files incrementally)
- [ ] 8.3.3 Ensure search works during migration (fallback to document-level)
- [ ] 8.3.4 Add rollback mechanism if migration fails

### 8.4 User Experience (crucible-cli)
- [ ] 8.4.1 Add progress reporting for long-running operations
- [ ] 8.4.2 Show change detection statistics (X unchanged, Y changed, Z new)
- [ ] 8.4.3 Show embedding reuse statistics (X blocks reused, Y API calls saved)
- [ ] 8.4.4 Add `--verbose` flag for detailed processing information

### 8.5 Documentation Updates
- [ ] 8.5.1 Update CLAUDE.md with block-level processing architecture
- [ ] 8.5.2 Create user guide for incremental processing features
- [ ] 8.5.3 Document performance characteristics and tuning options
- [ ] 8.5.4 Add troubleshooting guide for common issues