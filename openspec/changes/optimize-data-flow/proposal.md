## Why
The current data flow processes entire files on every run, missing critical optimizations for incremental updates, block-level embeddings, and cross-document deduplication. Character-based chunking splits content at arbitrary boundaries, preventing semantic coherence and efficient reuse of embeddings across documents. We need change detection at file and AST block levels to avoid expensive reprocessing of unchanged content, enable block-level semantic search, and provide foundation for future sync capabilities.

## What Changes
- **ADD** file-level change detection using BLAKE3 content hashing before expensive parsing
- **ADD** AST block-based hashing using natural semantic boundaries (headings, paragraphs, lists, code blocks)
- **ADD** Merkle tree construction and storage for efficient change detection
- **ADD** Merkle tree diffing to identify changed blocks at granular AST node level
- **ADD** content-addressed block storage enabling cross-document deduplication
- **ADD** block-level embedding generation with content-addressed reuse
- **ADD** incremental embedding updates only for changed blocks
- **ADD** block-level semantic search for improved accuracy and context
- **REMOVE** character-based chunking in favor of AST node boundaries
- **REMOVE** full-file processing behavior that reprocesses unchanged content
- **MODIFY** data flow to support incremental updates and block-level operations

## Impact
- **Affected specs**: file-processing, cli-architecture, content-addressed-storage
- **Affected code**:
  - kiln_processor.rs (incremental pipeline integration)
  - kiln_scanner.rs (file-level change detection)
  - embedding_pipeline.rs (block-level embedding generation)
  - ParsedDocument structure (block hash storage)
  - Database schema (block tables, embedding tables)
  - Search queries (block-level instead of document-level)
- **Performance**:
  - 90% reduction in embedding API calls for incremental edits
  - <500ms processing for single-block edits (vs ~2000ms full reprocessing)
  - <5s processing for 10-file edits in 1000-document vault (vs ~60s)
  - Deduplication reduces storage and costs proportional to content reuse
- **User Experience**:
  - More accurate semantic search (AST blocks vs arbitrary chunks)
  - Block-level result highlighting in search results
  - Sub-second response on consumer hardware
- **Architecture**:
  - Merkle tree infrastructure provides foundation for future sync features
  - Content-addressed storage enables efficient cross-device synchronization (future)
  - AST block boundaries align with user mental model and HTML rendering