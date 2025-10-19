# SurrealDB Schema Design - Executive Summary

> **Created**: 2025-10-19
> **Status**: Design Complete, Ready for Implementation
> **Context**: Terminal-first PoC architecture for Crucible knowledge vault

## What Was Delivered

This design provides a complete SurrealDB schema for Crucible's knowledge vault indexing system, optimized for graph database patterns and the daemon-based architecture outlined in POC_ARCHITECTURE.md.

## Key Deliverables

### 1. Architecture Document
**File**: `/home/moot/crucible/docs/SURREALDB_SCHEMA.md`

Comprehensive design rationale covering:
- Graph-first approach (native edges vs junction tables)
- Schema flexibility for heterogeneous frontmatter
- Vector search integration (MTREE indexes for embeddings)
- Full-text search strategy (BM25 with analyzers)
- Migration and versioning strategy

**Key Decision**: Use GRAPH EDGES for wikilinks, enabling powerful traversal syntax:
```sql
SELECT ->wikilink->notes FROM notes:foo  -- Outgoing links
SELECT <-wikilink<-notes FROM notes:bar  -- Backlinks
```

### 2. SurrealQL Schema
**File**: `/home/moot/crucible/crates/crucible-surrealdb/src/schema.surql`

Production-ready schema with:
- **`notes` table**: Full-text indexed content, vector embeddings, flexible metadata
- **`tags` table**: Tag metadata, hierarchy, usage statistics
- **`wikilink` relation**: Graph edges with context and positioning
- **`tagged_with` relation**: Rich tag relationships
- **`relates_to` relation**: Semantic similarity, citations

Indexes optimized for:
- Path lookups (UNIQUE index)
- Full-text search (BM25 on content/title)
- Tag filtering (array containment)
- Vector similarity (MTREE for cosine distance)

### 3. Rust Type Definitions
**File**: `/home/moot/crucible/crates/crucible-surrealdb/src/schema_types.rs`

Type-safe abstractions including:
- `Note`: Document with embeddings and metadata
- `Tag`: Hierarchical tags with usage tracking
- `Wikilink`, `TaggedWith`, `RelatesTo`: Typed graph edges
- `RecordId<T>`: Type-safe record identifiers
- Query builders: `SemanticSearchQuery`, `FullTextSearchQuery`, `GraphTraversalQuery`

**Builder Pattern Example**:
```rust
let note = Note::new("test.md", "# Content")
    .with_title("Test Note")
    .with_tags(vec!["#rust".to_string()])
    .with_embedding(embedding, "all-MiniLM-L6-v2");
```

### 4. Query Examples
**File**: `/home/moot/crucible/crates/crucible-surrealdb/examples/queries.surql`

90+ example queries demonstrating:
- Tag filtering (simple, AND/OR, hierarchical)
- Graph traversal (backlinks, multi-hop, shortest path, hub analysis)
- Full-text search (basic, phrase, boolean, highlighted snippets)
- Semantic search (cosine similarity, hybrid with filters)
- Analytics (statistics, tag correlation, influence metrics)
- Maintenance (find stale embeddings, broken links, orphans)

### 5. Implementation Guide
**File**: `/home/moot/crucible/docs/SURREALDB_IMPLEMENTATION.md`

Step-by-step roadmap covering:
- Phase 1: Database setup and schema initialization
- Phase 2: Markdown parser (frontmatter, wikilinks, tags)
- Phase 3: File watcher integration with `notify-debouncer`
- Phase 4: REPL command implementation
- Phase 5: Embedding generation pipeline
- Phase 6: Testing strategy (unit, integration, performance)
- Phase 7: Optimization (batching, pooling)

## Architectural Decisions

### 1. Graph-First Design

**Decision**: Use native SurrealDB RELATE statements for wikilinks instead of junction tables.

**Rationale**:
- Cleaner syntax: `->wikilink->notes` vs complex JOINs
- Edge properties: Store link context, position, weight
- Bidirectional traversal: `<-wikilink<-notes` for backlinks
- Performance: Optimized for graph algorithms

**Trade-off**: Couples to SurrealDB (acceptable for PoC phase per architecture doc).

### 2. Flexible Metadata Schema

**Decision**: Use JSON `metadata` object for frontmatter, with computed fields for common properties.

**Rationale**:
- Notes have heterogeneous frontmatter (status, priority, author, etc.)
- No schema migrations needed when users add new properties
- Computed fields (`status`, `folder`) provide indexed access to common patterns

**Trade-off**: Complex property queries slower than dedicated columns (mitigated by optional `metadata_index` table for large vaults).

### 3. Vector Search Strategy

**Decision**: Store embeddings as `array<float>` with MTREE index for approximate nearest neighbor search.

**Rationale**:
- Native SurrealDB type (no external vector DB needed)
- MTREE provides HNSW-like performance for cosine similarity
- Co-located with document data (no JOINs for hybrid search)

**Configuration**:
```sql
DEFINE INDEX embedding_idx ON TABLE notes COLUMNS embedding
  MTREE DIMENSION 384 DISTANCE COSINE;
```

### 4. Full-Text Search

**Decision**: Use SurrealDB 2.0+ native full-text search with BM25 ranking.

**Rationale**:
- No external search index (Elasticsearch, Meilisearch) needed
- Integrated relevance scoring and highlighting
- Customizable analyzers (stemming, stop words)

**Example**:
```sql
SELECT
    search::score(1) AS relevance,
    search::highlight('<mark>', '</mark>', 1) AS snippet
FROM notes
WHERE content_text @1@ "knowledge graph"
ORDER BY relevance DESC;
```

### 5. Index Strategy

**Indexed**:
- Path (UNIQUE) - primary access pattern
- Tags (array) - frequent filter
- Folder (computed) - folder-based queries
- Modified timestamp - recency sorting
- Content/title (full-text) - search
- Embedding (MTREE) - semantic search

**Not Indexed Initially**:
- Metadata properties (schemaless, can add `metadata_index` table if needed)
- Edge properties (add if graph queries slow)

## Query Patterns Supported

### 1. Tag-Based Filtering
```sql
-- Multiple tags (AND)
SELECT * FROM notes
WHERE tags CONTAINSALL ["#rust", "#database"]
ORDER BY modified_at DESC;

-- Hierarchical tags
SELECT * FROM notes WHERE tags CONTAINS SOME (
    SELECT name FROM tags WHERE parent_tag = tags:project
);
```

### 2. Graph Traversal
```sql
-- Backlinks
SELECT <-wikilink<-notes.* FROM notes:foo.md;

-- Two-hop traversal
SELECT ->wikilink->notes->wikilink->notes.* FROM notes:start.md;

-- Hub analysis
SELECT path, count(<-wikilink) AS backlinks
FROM notes ORDER BY backlinks DESC;
```

### 3. Full-Text Search
```sql
-- With highlighting
SELECT
    search::highlight('<mark>', '</mark>', 1) AS snippet,
    search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "agent orchestration"
ORDER BY relevance DESC;
```

### 4. Semantic Search
```sql
-- Vector similarity
SELECT
    path,
    vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
ORDER BY similarity ASC
LIMIT 10;

-- Hybrid (semantic + tag filter)
SELECT * FROM notes
WHERE embedding IS NOT NONE
  AND tags CONTAINS "#rust"
ORDER BY vector::distance::cosine(embedding, $query_embedding) ASC;
```

### 5. Complex Queries
```sql
-- Context-aware search (boost linked notes)
SELECT
    n.path,
    search::score(1) AS text_score,
    IF(n.id IN $context->wikilink->notes, 1.5, 1.0) AS link_boost
FROM notes AS n
WHERE content_text @1@ "your query"
ORDER BY (text_score * link_boost) DESC;
```

## Performance Characteristics

### Expected Query Times (10K notes)

- Path lookup: <1ms (indexed)
- Tag filter: 5-10ms (array index)
- Full-text search: 10-50ms (BM25)
- Semantic search: 20-100ms (MTREE)
- Backlinks: 5-15ms (graph edge)
- Two-hop traversal: 20-50ms

### Scaling Thresholds

- **<1K notes**: All queries fast, no optimization needed
- **1K-10K**: Current design optimal
- **10K-100K**: Add `metadata_index` table, optimize embeddings
- **100K+**: Consider partitioning by folder

## Migration Strategy

### Schema Versioning
```sql
CREATE metadata:system SET schema_version = 1;
```

### Zero-Downtime Migrations
1. Add new fields as `option<T>` (nullable)
2. Populate gradually in background
3. Once complete, make required

### Rollback
- Export schema before migrations: `surreal export schema_backup.surql`
- Wrap migrations in transactions: `BEGIN TRANSACTION; ... COMMIT;`

## Implementation Phases

Per SURREALDB_IMPLEMENTATION.md:

1. **Phase 1**: Database setup (schema init, connection pool)
2. **Phase 2**: Markdown parser (frontmatter, wikilinks, tags)
3. **Phase 3**: File watcher integration
4. **Phase 4**: Query implementation (REPL commands)
5. **Phase 5**: Embedding generation pipeline
6. **Phase 6**: Testing (unit, integration, performance)
7. **Phase 7**: Optimization (batching, indexing, caching)

## Integration with PoC Architecture

Aligns with terminal-first daemon architecture:

1. **Watcher** → Parser → **SurrealDB** (indexing pipeline)
2. **REPL** → SurrealQL pass-through → **SurrealDB** (query layer)
3. **Logger** → Tracing events → **TUI** (visibility)

Direct SurrealQL in REPL enables:
- Graph traversal experiments
- Ad-hoc analytics
- Schema debugging

## Next Steps

1. Implement markdown parser (Phase 2)
2. Test schema initialization (Phase 1)
3. Build watcher integration (Phase 3)
4. Implement REPL commands (Phase 4)
5. Add embedding pipeline (Phase 5)

## Files Reference

All deliverables located in:
- `/home/moot/crucible/docs/SURREALDB_SCHEMA.md` - Design doc
- `/home/moot/crucible/docs/SURREALDB_IMPLEMENTATION.md` - Implementation guide
- `/home/moot/crucible/crates/crucible-surrealdb/src/schema.surql` - Schema
- `/home/moot/crucible/crates/crucible-surrealdb/src/schema_types.rs` - Rust types
- `/home/moot/crucible/crates/crucible-surrealdb/examples/queries.surql` - Query examples

## Questions Answered

### Graph vs Relational?
**Graph edges** for wikilinks. Native SurrealDB RELATE enables elegant traversal syntax.

### Embedding storage?
**Native `array<float>`** with MTREE index. No external vector DB needed.

### Flexible frontmatter?
**JSON `metadata` object** + computed fields for common properties. Optional `metadata_index` table for large vaults.

### Full-text search?
**Native SurrealDB search** with BM25, analyzers, highlighting. No external index needed.

### Schema evolution?
**Versioned migrations** with optional fields. Rollback via schema exports and transactions.

---

**Status**: Design complete. Ready for implementation.
**Recommendation**: Start with Phase 1 (database setup) and Phase 2 (parser).
