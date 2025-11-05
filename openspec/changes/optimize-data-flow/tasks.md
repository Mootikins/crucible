## 1. File-Level Change Detection
- [ ] 1.1 Add content hash field to KilnFileInfo structure
- [ ] 1.2 Implement BLAKE3 streaming file hashing during discovery phase
- [ ] 1.3 Add file_hash column to database schema
- [ ] 1.4 Query database for stored file hashes during scan
- [ ] 1.5 Skip parsing and processing for files with unchanged content hashes

## 2. AST Block-Based Hashing Infrastructure
- [ ] 2.1 Add method to ParsedDocument to extract AST blocks as hashable units
- [ ] 2.2 Implement AST block → string serialization for hashing
- [ ] 2.3 Implement block hashing using BLAKE3 (AST node content → hash)
- [ ] 2.4 Extend ParsedDocument to store block hashes alongside blocks
- [ ] 2.5 Add Merkle tree construction from AST block hashes
- [ ] 2.6 Store Merkle trees in content-addressed storage (already implemented)

## 3. Content-Addressed Block Storage
- [ ] 3.1 Add document_blocks table to database schema (document_id, block_index, block_hash)
- [ ] 3.2 Store blocks in ContentAddressedStorage by hash
- [ ] 3.3 Implement block → document mapping queries
- [ ] 3.4 Add "find documents containing block hash" query
- [ ] 3.5 Implement deduplication tracking and statistics

## 4. Merkle Tree Diffing
- [ ] 4.1 Load existing Merkle trees for changed files from storage
- [ ] 4.2 Implement tree comparison using existing compare_enhanced method
- [ ] 4.3 Generate change sets (added/removed/modified block indices)
- [ ] 4.4 Map block indices back to AST nodes for processing
- [ ] 4.5 Test diffing accuracy with various edit scenarios

## 5. Block-Level Embedding Infrastructure
- [ ] 5.1 Add block_embeddings table to database schema
- [ ] 5.2 Implement content-addressed embedding storage (hash → embedding)
- [ ] 5.3 Add embedding lookup by block hash
- [ ] 5.4 Implement embedding reuse for duplicate blocks
- [ ] 5.5 Update embedding generation to check for existing embeddings first

## 6. Incremental Embedding Pipeline
- [ ] 6.1 Modify embedding pipeline to accept change sets
- [ ] 6.2 Implement selective embedding generation for changed blocks only
- [ ] 6.3 Skip embedding generation for unchanged blocks
- [ ] 6.4 Update only changed embeddings in database
- [ ] 6.5 Maintain document → block → embedding relationships

## 7. Block-Level Semantic Search
- [ ] 7.1 Update search queries to operate on block embeddings not document embeddings
- [ ] 7.2 Implement block similarity search with document context
- [ ] 7.3 Add result aggregation (group blocks by document)
- [ ] 7.4 Implement block-level result ranking
- [ ] 7.5 Add search result presentation with block highlighting

## 8. Integration and Testing
- [ ] 8.1 Integrate new pipeline into kiln_processor.rs
- [ ] 8.2 Update CLI startup processing to use incremental pipeline
- [ ] 8.3 Add performance benchmarks comparing old vs new approach
- [ ] 8.4 Test with various vault sizes (100, 1000, 10000 documents)
- [ ] 8.5 Test incremental scenarios (edit 1 block, edit 10 files, etc.)
- [ ] 8.6 Ensure graceful migration from legacy document-level embeddings
- [ ] 8.7 Add progress reporting for long-running operations