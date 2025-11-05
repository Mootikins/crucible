# Crucible SurrealDB Backend

> Graph database schema for Crucible knowledge kiln indexing

## Overview

This crate provides a SurrealDB backend for Crucible's knowledge management system, optimized for graph-based queries, semantic search, and full-text indexing.

## Key Features

- **Graph Relations**: Native wikilink traversal with `->wikilink->notes` syntax
- **Full-Text Search**: BM25 ranking with highlighting and analyzers
- **Semantic Search**: Vector embeddings with MTREE indexing
- **Flexible Schema**: JSON metadata for heterogeneous frontmatter
- **Type-Safe Queries**: Rust builders for common query patterns

## Quick Start

### 1. Initialize Database

```rust
use crucible_surrealdb::{SurrealDatabase, Note};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to local RocksDB-backed instance
    let db = SurrealDatabase::new("./kiln.db").await?;

    // Initialize schema
    db.initialize_schema().await?;

    Ok(())
}
```

### 2. Index a Note

```rust
let note = Note::new("projects/crucible.md", "# Crucible\n\nKnowledge kiln")
    .with_title("Crucible")
    .with_tags(vec!["#project".to_string(), "#rust".to_string()])
    .with_metadata("status", serde_json::json!("active"));

db.upsert_note(&note).await?;
```

### 3. Query by Tags

```rust
let notes = db.search_by_tags(&["#project", "#rust"]).await?;

for note in notes {
    println!("{}: {}", note.path, note.title.unwrap_or_default());
}
```

### 4. Graph Traversal

```rust
// Get backlinks
let backlinks = db.get_backlinks("projects/crucible.md").await?;

// Direct SurrealQL for complex queries
let result = db.query(r#"
    SELECT
        path,
        ->wikilink->notes.(path, title) AS links,
        <-wikilink<-notes.(path, title) AS backlinks
    FROM notes:projects/crucible.md
"#).await?;
```

### 5. Semantic Search

```rust
let embedding = embedding_service.embed("knowledge graph systems")?;

let results = db.semantic_search(&embedding, 10, Some(0.3)).await?;

for result in results {
    println!("{}: similarity={}", result.note.path, result.score);
}
```

## Schema Overview

### Tables

- **`notes`**: Documents with content, embeddings, tags, metadata
- **`tags`**: Tag metadata, hierarchy, usage statistics

### Relations (Graph Edges)

- **`wikilink`**: Note → Note (document links)
- **`tagged_with`**: Note → Tag (tag associations)
- **`relates_to`**: Note → Note (semantic similarity, citations)

### Indexes

- Path (UNIQUE)
- Full-text (content, title)
- Tags (array containment)
- Embeddings (MTREE vector search)
- Timestamps (modified_at)

## Example Queries

### Tag Filtering

```sql
-- Notes with multiple tags (AND)
SELECT * FROM notes
WHERE tags CONTAINSALL ["#rust", "#database"]
ORDER BY modified_at DESC;

-- Hierarchical tags
SELECT * FROM notes WHERE tags CONTAINS SOME (
    SELECT name FROM tags WHERE parent_tag = tags:project
);
```

### Graph Traversal

```sql
-- Backlinks (who links here?)
SELECT <-wikilink<-notes.* FROM notes:foo.md;

-- Two-hop traversal
SELECT ->wikilink->notes->wikilink->notes.* FROM notes:start.md;

-- Hub analysis
SELECT path, count(<-wikilink) AS backlinks
FROM notes ORDER BY backlinks DESC LIMIT 20;
```

### Full-Text Search

```sql
-- With snippets
SELECT
    path,
    search::highlight('<mark>', '</mark>', 1) AS snippet,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "knowledge management"
ORDER BY relevance DESC;
```

### Semantic Search

```sql
-- Vector similarity
SELECT
    path,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
ORDER BY similarity ASC
LIMIT 10;
```

## Type-Safe Builders

```rust
use crucible_surrealdb::{SemanticSearchQuery, FullTextSearchQuery, GraphTraversalQuery};

// Semantic search
let query = SemanticSearchQuery::new(embedding)
    .limit(10)
    .min_similarity(0.7)
    .filter_tags(vec!["#rust".to_string()])
    .filter_folder("Projects");

let results = db.semantic_search_query(&query).await?;

// Full-text search
let query = FullTextSearchQuery::new("graph database")
    .filter_tags(vec!["#database".to_string()])
    .limit(20);

let results = db.full_text_search_query(&query).await?;

// Graph traversal
let query = GraphTraversalQuery::new(RecordId::new("notes", "start.md"))
    .max_depth(3)
    .backlinks();

let nodes = db.graph_traversal_query(&query).await?;
```

## Documentation

- **[Schema Design](../../docs/SURREALDB_SCHEMA.md)** - Architecture and design rationale
- **[Example Queries](examples/queries.surql)** - 90+ query patterns

## Schema Files

- `src/schema.surql` - SurrealQL schema definition
- `src/schema_types.rs` - Rust type definitions
- `examples/queries.surql` - Query examples

## ContentAddressedStorage Backend

This crate also provides a SurrealDB backend for the Crucible content-addressed storage system. It implements the `ContentAddressedStorage` trait using SurrealDB as the underlying database.

### Features

- **Persistent Storage**: Content blocks and Merkle trees are stored in SurrealDB
- **ACID Transactions**: Full transaction support for data consistency
- **Efficient Indexing**: Hash-based lookups with optimized indexes
- **Async/Await Support**: Full async/await support with Tokio integration
- **Connection Pooling**: Efficient connection management
- **RocksDB Backend**: Uses RocksDB for high-performance persistence

### Usage

```rust
use crucible_surrealdb::ContentAddressedStorageSurrealDB;
use crucible_core::storage::ContentAddressedStorage;

// Create an in-memory storage for testing
let storage = ContentAddressedStorageSurrealDB::new_memory().await?;

// Create a file-based storage
let storage = ContentAddressedStorageSurrealDB::new_file("/path/to/database").await?;

// Store and retrieve content blocks
let hash = "content_hash_123";
let data = b"Hello, World!";

storage.store_block(hash, data).await?;
let retrieved = storage.get_block(hash).await?;
assert_eq!(retrieved, Some(data.to_vec()));
```

### Integration with Storage Builder

Due to the async nature of SurrealDB initialization, use the `Custom` backend:

```rust
use crucible_core::storage::builder::{ContentAddressedStorageBuilder, StorageBackendType, HasherConfig};
use crucible_core::hashing::blake3::Blake3Hasher;
use crucible_surrealdb::ContentAddressedStorageSurrealDB;
use std::sync::Arc;

// Create the SurrealDB storage instance
let surrealdb_storage = ContentAddressedStorageSurrealDB::new_file("./my_db").await?;
let storage_arc = Arc::new(surrealdb_storage) as Arc<dyn ContentAddressedStorage>;

// Use it with the builder
let storage = ContentAddressedStorageBuilder::new()
    .with_backend(StorageBackendType::Custom(storage_arc))
    .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
    .build()?;
```

## Implementation Status

### Completed
- [x] Schema design and documentation
- [x] SurrealQL schema definitions
- [x] Rust type definitions
- [x] Query examples
- [x] Type-safe builders
- [x] ContentAddressedStorage trait implementation
- [x] Async/await support
- [x] Connection pooling and configuration

### In Progress
- [ ] Database connection implementation
- [ ] Query execution layer
- [ ] Markdown parser integration
- [ ] File watcher pipeline

### Planned
- [ ] Embedding generation
- [ ] REPL integration
- [ ] Performance optimization
- [ ] Migration system

## Testing

```bash
# Run unit tests
cargo test -p crucible-surrealdb

# Run integration tests (requires SurrealDB)
cargo test -p crucible-surrealdb --features integration-tests

# Check schema validity
surreal import --conn http://localhost:8000 \
  --user root --pass root --ns crucible --db kiln \
  src/schema.surql
```

## Performance

Expected query times (10K notes):

- Path lookup: <1ms
- Tag filter: 5-10ms
- Full-text search: 10-50ms
- Semantic search: 20-100ms
- Backlinks: 5-15ms
- Two-hop traversal: 20-50ms

## Dependencies

```toml
[dependencies]
surrealdb = "2.0"
tokio = { version = "1.0", features = ["full"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
```

## Architecture

Aligns with Crucible's terminal-first daemon architecture:

```
File System
    ↓
Watcher (notify-debouncer)
    ↓
Parser (frontmatter, wikilinks, tags)
    ↓
SurrealDB (this crate)
    ↓
REPL / MCP Server
```

## Configuration

```yaml
database:
  backend: "surrealdb"
  path: "~/.crucible/kiln.db"
  namespace: "crucible"
  database: "kiln"
  max_connections: 10
```

## Contributing

See [Schema Design](../../docs/SURREALDB_SCHEMA.md) for architecture details and contribution guidelines.

## License

Same as Crucible parent project.

---

**Status**: Design complete, implementation in progress
**Last Updated**: 2025-10-19
