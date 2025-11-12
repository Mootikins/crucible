# Vector Embedding Integration for Semantic Search

**Change ID**: `2025-11-11-vector-embedding-integration`
**Status**: Ready for Implementation
**Created**: 2025-11-11
**Author**: Matthew Krohn

## Why

The parser now produces block-level entities ready for vector embeddings, but the embedding generation and semantic search infrastructure is not yet implemented. This is critical for enabling intelligent knowledge discovery and is a core requirement for the ACP MVP.

## What Changes

- **Embedding Service Abstraction**: Trait-based provider system (Fastembed, OpenAI, custom)
- **Block-Level Embedding Generation**: Vectors for all content blocks (>5 words)
- **Embedding Storage & Indexing**: SurrealDB vector search integration
- **Semantic Search Implementation**: Hybrid search combining semantic + graph + fuzzy
- **CLI Integration**: Embedding commands and search capabilities
- **Performance Optimization**: Batch processing and incremental updates

## Impact

- **Affected specs**: `embeddings`, `parser` (extension for embedding workflow)
- **Affected code**:
  - `crates/crucible-core/src/embedding/` (new module)
  - `crates/crucible-surrealdb/src/embedding_store.rs` (new implementation)
  - `crates/crucible-cli/src/commands/embed.rs` (new command)
  - `crates/crucible-cli/src/commands/semantic_search.rs` (new command)

## Success Criteria

1. **Embedding Generation**: All content blocks (>5 words) automatically generate vectors
2. **Provider Flexibility**: Support multiple embedding providers with trait abstraction
3. **Semantic Search**: Hybrid search combining semantic similarity with graph traversal
4. **Performance**: Embed 1000 blocks in <30 seconds using local Fastembed
5. **Incremental Updates**: Only re-embed changed blocks (Merkle tree integration)
6. **Storage Efficiency**: Deduplication and compression for similar embeddings
7. **Error Handling**: Graceful fallback when embedding services are unavailable