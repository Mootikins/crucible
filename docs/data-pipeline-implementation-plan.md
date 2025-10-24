# Data Pipeline Implementation Plan: Files → Embeddings → Search → Queries

## Overview
Start with the data pipeline foundation and work our way up to user queries. This ensures data flows correctly from file parsing through embedding generation to user-facing search functionality.

## Phase 1: File Parsing & Change Detection

### Agent 1: Implement Real Vault File Parsing
**Objective**: Replace mock file discovery with real vault directory scanning

**Tasks:**
- Scan actual vault directory (configured vault path) for .md files
- Parse markdown files and extract frontmatter properties and content
- Implement file change detection using modified timestamps and file size
- Track which files need re-processing vs. which are unchanged
- Handle file system errors and edge cases (corrupted files, permissions)

**Files to Modify:**
- Replace mock implementations in `crates/crucible-tools/src/vault_tools.rs`
- Update file discovery logic to use real filesystem operations
- Build file parsing functions for markdown and frontmatter

### Agent 2: Build Metadata Extraction System
**Objective**: Extract real metadata from actual vault files

**Tasks:**
- Extract real tags, titles, folders from actual vault files
- Parse YAML frontmatter correctly with error handling
- Build document metadata structure for each processed file
- Store metadata in database for search and filtering
- Handle various frontmatter formats and missing metadata

**Files to Modify:**
- `crates/crucible-tools/src/vault_tools.rs` - metadata extraction functions
- Database schema for storing document metadata
- Error handling for malformed frontmatter

## Phase 2: Embedding Generation & Storage

### Agent 3: Implement Real Embedding Generation
**Objective**: Connect to embedding service and generate embeddings for actual content

**Tasks:**
- Connect to configured embedding service (EMBEDDING_ENDPOINT, EMBEDDING_MODEL)
- Generate embeddings for actual document content (not mock data)
- Handle embedding service errors, retries, and rate limiting
- Store embeddings in proper vector format in DuckDB
- Batch embedding generation for efficiency

### Agent 4: Build Re-embedding Trigger System
**Objective**: Keep embeddings in sync with file changes

**Tasks:**
- Detect file changes and trigger re-embedding for modified files
- Update embeddings in database when files change
- Remove embeddings for deleted files
- Ensure embeddings stay in sync with vault state

## Phase 3: Database Search Implementation

### Agent 5: Implement Real Vector Search
**Objective**: Replace mock semantic search with actual vector similarity search

**Tasks:**
- Replace mock `semantic_search()` with actual vector similarity search
- Query embedding service for user query embeddings
- Find similar documents using cosine similarity in DuckDB
- Return ranked results with real similarity scores

### Agent 6: Implement Metadata and Content Search
**Objective**: Replace remaining mock search functions

**Tasks:**
- Replace mock `search_by_content()` with real full-text search
- Implement `search_by_filename()` with actual file pattern matching
- Build search functions that use real document metadata
- Ensure search functions query actual database content

## Phase 4: User Query Integration

### Agent 7: Connect CLI to Real Search Functions
**Objective**: Update CLI commands to use real search results

**Tasks:**
- Update `crucible-cli/src/commands/semantic.rs` to use real semantic search
- Ensure CLI commands use actual database search results
- Test with your real vault data and verify different queries work
- Update error handling for real search failures

### Agent 8: Connect REPL to Real Search Functions
**Objective**: Verify REPL uses real search functions

**Tasks:**
- Verify REPL `:run semantic_search` uses real vector search
- Test REPL queries return actual vault documents, not mock data
- Ensure results are correctly formatted for display
- Test end-to-end: file change → re-embed → search → query result

## Data Flow Testing

**Test Scenarios:**
- Test: Modify file in vault → Check embedding is updated → Verify search finds new content
- Test: Delete file → Check embedding is removed → Verify search no longer returns deleted content
- Test: New file → Check embedding is generated → Verify search returns new content
- Test: Query different terms → Get different, relevant results from your actual vault

## Key Implementation Order

1. **Foundation**: Real file parsing and metadata extraction
2. **Processing**: Embedding generation and storage with change detection
3. **Retrieval**: Real database search functions (vector, text, metadata)
4. **Interface**: CLI and REPL use real search functions with actual vault data

## Success Criteria

- No mock data remains in production execution pipeline
- All tools work with configured vault path (your vault or test vault)
- File changes correctly trigger re-processing and embedding updates
- User queries return results from actual vault content
- Integration tests use test vault for predictable data