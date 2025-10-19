# SurrealDB Schema Design for Crucible Knowledge Vault

> **Status**: Design Document
> **Created**: 2025-10-19
> **Purpose**: Define the graph database schema for indexing and querying the knowledge vault

## Table of Contents

1. [Design Philosophy](#design-philosophy)
2. [Schema Overview](#schema-overview)
3. [Table Definitions](#table-definitions)
4. [Graph Relations](#graph-relations)
5. [Indexing Strategy](#indexing-strategy)
6. [Query Patterns](#query-patterns)
7. [Migration Strategy](#migration-strategy)

## Design Philosophy

### Graph-First Approach

SurrealDB's native graph capabilities are leveraged throughout this schema. Unlike traditional relational databases, we model relationships as first-class GRAPH EDGES rather than junction tables. This enables:

1. **Efficient Graph Traversal**: Native `->relation->table` syntax for multi-hop queries
2. **Flexible Relations**: Edges can have properties (metadata, timestamps, weights)
3. **Recursive Queries**: Built-in support for transitive closure operations
4. **Path Discovery**: Find all paths between documents naturally

### Schema Flexibility for Frontmatter

Knowledge vault documents have heterogeneous frontmatter properties. The schema handles this via:

1. **Core Fields**: Strongly-typed fields for common properties (path, title, tags)
2. **Metadata Object**: Flexible JSON object for custom frontmatter properties
3. **Property Extraction**: Computed fields for commonly-queried custom properties

This balances query performance (indexed core fields) with flexibility (schemaless metadata).

### Vector Search Integration

Embeddings are stored natively as `array<float>` with dedicated indexing:

1. **Embedding Vectors**: 384-1536 dimensions (model-dependent)
2. **HNSW Indexing**: Hierarchical Navigable Small World for approximate nearest neighbor search
3. **Metadata Co-location**: Embeddings stored alongside document data to avoid JOINs
4. **Hybrid Search**: Combine semantic similarity with filters (tags, properties)

### Full-Text Search Strategy

SurrealDB 2.0+ provides native full-text search capabilities:

1. **Tokenized Indexes**: BM25-based search on content and title fields
2. **Analyzer Configuration**: Stemming, stop words, case-insensitive matching
3. **Ranking**: Relevance scoring with configurable weights
4. **Hybrid Queries**: Combine full-text with graph traversal and filters

## Schema Overview

```
┌──────────────┐
│   notes      │  (Core document table)
│  - id        │
│  - path      │
│  - title     │
│  - content   │
│  - embedding │
│  - tags      │
│  - metadata  │
└──────┬───────┘
       │
       ├─────→ wikilink ─────→ notes  (Graph edges for [[links]])
       │
       ├─────→ tagged_with ──→ tags   (Tag relationships)
       │
       └─────→ relates_to ───→ notes  (Semantic similarity, backlinks)
```

### Design Rationale

**Why `notes` instead of `documents`?**
The term "notes" aligns with knowledge management terminology (Obsidian, Logseq, Roam). It's user-facing and semantically accurate.

**Why separate `tags` table?**
Tags are stored both inline (array field) for fast filtering AND as a separate table for:
- Tag metadata (usage count, description, color)
- Tag hierarchy (parent tags)
- Tag suggestions and autocomplete

**Why graph edges for wikilinks?**
Traditional approach: Junction table with (from_id, to_id) pairs.
Graph approach: Native SurrealDB RELATE statements create typed edges.

Benefits:
- Cleaner syntax: `SELECT ->wikilink->notes FROM notes:foo`
- Edge properties: Store link context, position in document
- Bidirectional traversal: `<-wikilink<-notes` for backlinks
- Performance: Optimized for graph queries

## Table Definitions

### 1. Notes Table

Primary table for all markdown documents in the vault.

```surql
DEFINE TABLE notes SCHEMAFULL;

-- Core identifiers
DEFINE FIELD path ON TABLE notes TYPE string ASSERT $value != NONE;
DEFINE FIELD title ON TABLE notes TYPE option<string>;
DEFINE FIELD content ON TABLE notes TYPE string;

-- Timestamps
DEFINE FIELD created_at ON TABLE notes TYPE datetime DEFAULT time::now();
DEFINE FIELD modified_at ON TABLE notes TYPE datetime DEFAULT time::now();

-- Full-text search fields
DEFINE FIELD content_text ON TABLE notes TYPE string FLEXIBLE;
DEFINE FIELD title_text ON TABLE notes TYPE option<string> FLEXIBLE;

-- Tags (array for fast filtering)
DEFINE FIELD tags ON TABLE notes TYPE array<string> DEFAULT [];

-- Semantic search
DEFINE FIELD embedding ON TABLE notes TYPE option<array<float>>;
DEFINE FIELD embedding_model ON TABLE notes TYPE option<string>;
DEFINE FIELD embedding_updated_at ON TABLE notes TYPE option<datetime>;

-- Flexible metadata (frontmatter properties)
DEFINE FIELD metadata ON TABLE notes TYPE object DEFAULT {};

-- Computed fields (common frontmatter properties)
DEFINE FIELD status ON TABLE notes VALUE $this.metadata.status OR "none";
DEFINE FIELD folder ON TABLE notes VALUE string::split($this.path, "/")[0];
DEFINE FIELD file_name ON TABLE notes VALUE array::last(string::split($this.path, "/"));

-- Constraints
DEFINE INDEX unique_path ON TABLE notes COLUMNS path UNIQUE;
```

**Field Rationale:**

- **`path` as primary key**: File paths are unique, stable, and user-meaningful
- **`content` vs `content_text`**: Original content preserved; `content_text` for full-text indexing
- **`embedding` optional**: Not all documents have embeddings initially
- **`metadata` object**: Schemaless JSON for frontmatter flexibility
- **Computed `folder`**: Extracted from path for fast folder filtering

### 2. Tags Table

Metadata and hierarchy for tags used across the vault.

```surql
DEFINE TABLE tags SCHEMAFULL;

-- Core fields
DEFINE FIELD name ON TABLE tags TYPE string ASSERT $value != NONE;
DEFINE FIELD description ON TABLE tags TYPE option<string>;
DEFINE FIELD color ON TABLE tags TYPE option<string>;

-- Statistics
DEFINE FIELD usage_count ON TABLE tags TYPE int DEFAULT 0;
DEFINE FIELD last_used ON TABLE tags TYPE option<datetime>;

-- Hierarchy (for nested tags like #project/crucible)
DEFINE FIELD parent_tag ON TABLE tags TYPE option<record<tags>>;

-- Metadata
DEFINE FIELD created_at ON TABLE tags TYPE datetime DEFAULT time::now();

-- Constraints
DEFINE INDEX unique_tag_name ON TABLE tags COLUMNS name UNIQUE;
```

**Design Notes:**

- **`usage_count`**: Incremented when tag added to note; decremented when removed
- **`parent_tag`**: Enables hierarchical tags (`#project/crucible` → parent: `#project`)
- **Optional fields**: Tags can be lightweight (just name) or rich (description, color)

### 3. Metadata Index Table (Optional)

Dedicated table for fast queries on specific frontmatter properties.

```surql
DEFINE TABLE metadata_index SCHEMAFULL;

-- Reference to source note
DEFINE FIELD note ON TABLE metadata_index TYPE record<notes> ASSERT $value != NONE;

-- Property key-value
DEFINE FIELD key ON TABLE metadata_index TYPE string ASSERT $value != NONE;
DEFINE FIELD value ON TABLE metadata_index TYPE string;
DEFINE FIELD value_type ON TABLE metadata_index TYPE string;  -- "string", "number", "boolean", "date"

-- Typed values (for performance)
DEFINE FIELD value_number ON TABLE metadata_index TYPE option<float>;
DEFINE FIELD value_date ON TABLE metadata_index TYPE option<datetime>;
DEFINE FIELD value_bool ON TABLE metadata_index TYPE option<bool>;

-- Indexes
DEFINE INDEX key_value ON TABLE metadata_index COLUMNS key, value;
DEFINE INDEX note_key ON TABLE metadata_index COLUMNS note, key;
```

**When to Use:**

- **With index table**: Fast queries like `WHERE metadata.status = "published"` across 10K+ documents
- **Without index table**: Simpler schema; acceptable for <5K documents with JSON queries

Trade-off: Storage overhead vs query performance. Recommend starting WITHOUT this table; add if needed.

## Graph Relations

### 1. Wikilink Edge

Represents `[[Document Title]]` links between notes.

```surql
DEFINE TABLE wikilink SCHEMAFULL TYPE RELATION
  FROM notes TO notes;

-- Edge metadata
DEFINE FIELD link_text ON TABLE wikilink TYPE string;  -- The text inside [[ ]]
DEFINE FIELD context ON TABLE wikilink TYPE option<string>;  -- Surrounding paragraph
DEFINE FIELD position ON TABLE wikilink TYPE int;  -- Character offset in source document
DEFINE FIELD created_at ON TABLE wikilink TYPE datetime DEFAULT time::now();

-- For weighted graph algorithms
DEFINE FIELD weight ON TABLE wikilink TYPE float DEFAULT 1.0;
```

**Usage Examples:**

```surql
-- Create wikilink
RELATE notes:foo->wikilink->notes:bar SET
  link_text = "Related Document",
  position = 1234;

-- Find all outgoing links from a note
SELECT ->wikilink->notes.* FROM notes:foo;

-- Find backlinks (who links to this note?)
SELECT <-wikilink<-notes.* FROM notes:bar;

-- Find notes that link to same targets (related notes)
SELECT ->wikilink->notes<-wikilink<-notes.* FROM notes:foo;
```

### 2. Tagged_With Edge

Links notes to tags (alternative to array field for rich queries).

```surql
DEFINE TABLE tagged_with SCHEMAFULL TYPE RELATION
  FROM notes TO tags;

-- Edge metadata
DEFINE FIELD added_at ON TABLE tagged_with TYPE datetime DEFAULT time::now();
DEFINE FIELD added_by ON TABLE tagged_with TYPE option<string>;  -- "user" or "auto"
```

**Why Both Array AND Edge?**

- **Array (`notes.tags`)**: Fast filtering with `WHERE tags CONTAINS "#rust"`
- **Edge (`tagged_with`)**: Rich metadata, bidirectional queries, tag statistics

Update both in tandem:

```surql
-- Add tag to note
UPDATE notes:foo SET tags += "#rust";
RELATE notes:foo->tagged_with->tags:rust SET added_by = "user";
UPDATE tags:rust SET usage_count += 1, last_used = time::now();
```

### 3. Relates_To Edge

Semantic similarity and other computed relationships.

```surql
DEFINE TABLE relates_to SCHEMAFULL TYPE RELATION
  FROM notes TO notes;

-- Relationship type
DEFINE FIELD relation_type ON TABLE relates_to TYPE string;  -- "similar", "references", "contradicts"

-- Similarity score (for semantic search)
DEFINE FIELD score ON TABLE relates_to TYPE float DEFAULT 0.0;

-- Metadata
DEFINE FIELD computed_at ON TABLE relates_to TYPE datetime DEFAULT time::now();
DEFINE FIELD metadata ON TABLE relates_to TYPE option<object>;

-- Index for filtering by type
DEFINE INDEX relation_type_idx ON TABLE relates_to COLUMNS relation_type;
```

**Use Cases:**

1. **Semantic Similarity**: Store top-K similar documents as edges
2. **Citation Graphs**: Explicit references extracted from content
3. **Topic Clusters**: Documents grouped by semantic topic

```surql
-- Find semantically similar notes
SELECT ->relates_to[WHERE relation_type = "similar"]->notes.*
FROM notes:foo
ORDER BY score DESC
LIMIT 10;
```

## Indexing Strategy

### Performance-Critical Indexes

```surql
-- 1. Path lookup (primary access pattern)
DEFINE INDEX unique_path ON TABLE notes COLUMNS path UNIQUE;

-- 2. Full-text search on content
DEFINE ANALYZER content_analyzer TOKENIZERS blank,class FILTERS lowercase,snowball(english);
DEFINE INDEX content_search ON TABLE notes COLUMNS content_text
  SEARCH ANALYZER content_analyzer BM25 HIGHLIGHTS;

-- 3. Full-text search on titles
DEFINE INDEX title_search ON TABLE notes COLUMNS title_text
  SEARCH ANALYZER content_analyzer BM25;

-- 4. Tag filtering (array containment)
DEFINE INDEX tags_idx ON TABLE notes COLUMNS tags;

-- 5. Folder filtering
DEFINE INDEX folder_idx ON TABLE notes COLUMNS folder;

-- 6. Timestamp range queries
DEFINE INDEX modified_at_idx ON TABLE notes COLUMNS modified_at;

-- 7. Vector search (HNSW for approximate nearest neighbor)
DEFINE INDEX embedding_idx ON TABLE notes COLUMNS embedding
  MTREE DIMENSION 384 DISTANCE COSINE;
```

**Index Selection Rationale:**

- **UNIQUE on path**: Enforces constraint; O(log n) lookups
- **SEARCH indexes**: BM25 algorithm for relevance ranking
- **MTREE on embeddings**: Metric tree for vector similarity (HNSW under the hood)
- **Array index on tags**: Optimizes `CONTAINS` and `CONTAINSALL` queries
- **Skip metadata indexes initially**: JSON queries acceptable for small-medium vaults

### When to Add More Indexes

Monitor query performance. Add indexes when:

1. **Metadata queries slow**: Add `metadata_index` table
2. **Graph queries slow**: Add indexes on edge tables
3. **Complex filters slow**: Add composite indexes

## Query Patterns

### 1. Tag-Based Filtering

```surql
-- Simple tag filter
SELECT path, title, tags
FROM notes
WHERE tags CONTAINS "#project"
ORDER BY modified_at DESC;

-- Multiple tags (AND logic)
SELECT path, title, tags
FROM notes
WHERE tags CONTAINSALL ["#rust", "#database"]
ORDER BY modified_at DESC;

-- Tag OR filter
SELECT path, title, tags
FROM notes
WHERE tags CONTAINSANY ["#project", "#task"]
ORDER BY modified_at DESC;

-- Tag with property filter
SELECT path, title, tags, metadata.status
FROM notes
WHERE tags CONTAINS "#project"
  AND metadata.status = "active"
ORDER BY modified_at DESC;

-- Hierarchical tags (using parent_tag)
SELECT path, title, tags
FROM notes
WHERE tags CONTAINS SOME (
  SELECT name FROM tags
  WHERE name = "#project"
    OR parent_tag = tags:project
);
```

### 2. Graph Traversal (Wikilinks)

```surql
-- Direct outgoing links
SELECT path, title, ->wikilink->notes.title AS linked_notes
FROM notes:projects/crucible.md;

-- Backlinks (who links here?)
SELECT path, title, <-wikilink<-notes.(path, title) AS referring_notes
FROM notes:projects/crucible.md;

-- Two-hop traversal (notes linked by linked notes)
SELECT path, title,
  ->wikilink->notes->wikilink->notes.(path, title) AS second_hop
FROM notes:projects/crucible.md;

-- Find orphan notes (no incoming or outgoing links)
SELECT path, title
FROM notes
WHERE array::len(<-wikilink) = 0
  AND array::len(->wikilink) = 0;

-- Most linked notes (hub analysis)
SELECT path, title, count(<-wikilink) AS backlink_count
FROM notes
ORDER BY backlink_count DESC
LIMIT 20;

-- Shortest path between two notes
SELECT path FROM notes:A TO notes:B
  VIA wikilink
  LIMIT 1;

-- All paths up to depth 3
SELECT path, title,
  ->wikilink->notes AS hop1,
  ->wikilink->notes->wikilink->notes AS hop2,
  ->wikilink->notes->wikilink->notes->wikilink->notes AS hop3
FROM notes:start.md;
```

### 3. Full-Text Search

```surql
-- Basic full-text search
SELECT path, title, search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "knowledge management"
ORDER BY relevance DESC
LIMIT 20;

-- Full-text with tag filter
SELECT path, title, search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "SurrealDB schema"
  AND tags CONTAINS "#database"
ORDER BY relevance DESC;

-- Multi-field search (title weighted higher)
SELECT path, title,
  (search::score(1) * 2 + search::score(2)) AS relevance
FROM notes
WHERE title_text @1@ "architecture"
   OR content_text @2@ "architecture"
ORDER BY relevance DESC;

-- Search with highlights
SELECT path, title,
  search::highlight('<mark>', '</mark>', 1) AS snippet,
  search::score(1) AS relevance
FROM notes
WHERE content_text @1@ "graph database"
ORDER BY relevance DESC
LIMIT 10;

-- Phrase search (exact match)
SELECT path, title
FROM notes
WHERE content_text @@ '"knowledge graph"'  -- Quoted phrase
ORDER BY modified_at DESC;
```

### 4. Semantic Search (Embeddings)

```surql
-- Vector similarity search
LET $query_embedding = [0.12, -0.45, 0.78, ...];  -- 384-dim vector
SELECT path, title,
  vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
ORDER BY similarity ASC  -- Lower cosine distance = higher similarity
LIMIT 10;

-- Hybrid search (semantic + keyword filter)
LET $query_embedding = [0.12, -0.45, 0.78, ...];
SELECT path, title, tags,
  vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE embedding IS NOT NONE
  AND tags CONTAINS "#rust"
ORDER BY similarity ASC
LIMIT 10;

-- Semantic search with full-text fallback
LET $query_embedding = [0.12, -0.45, 0.78, ...];
SELECT path, title,
  vector::distance::cosine(embedding, $query_embedding) AS semantic_score,
  search::score(1) AS text_score
FROM notes
WHERE embedding IS NOT NONE
  AND content_text @1@ "agent orchestration"
ORDER BY (semantic_score * 0.7 + text_score * 0.3) ASC
LIMIT 10;

-- Find similar notes using precomputed edges
SELECT path, title,
  ->relates_to[WHERE relation_type = "similar"]->notes.(path, title, score)
FROM notes:foo.md
ORDER BY score DESC
LIMIT 10;
```

### 5. Metadata Property Queries

```surql
-- Simple property filter
SELECT path, title, metadata.status
FROM notes
WHERE metadata.status = "published"
ORDER BY modified_at DESC;

-- Property existence check
SELECT path, title
FROM notes
WHERE metadata.author IS NOT NONE;

-- Nested property access
SELECT path, title, metadata.project.name
FROM notes
WHERE metadata.project.status = "active";

-- Property type conversion
SELECT path, title, metadata.priority
FROM notes
WHERE <int>metadata.priority > 5;

-- Date range filter
SELECT path, title, metadata.due_date
FROM notes
WHERE <datetime>metadata.due_date > time::now()
  AND <datetime>metadata.due_date < time::now() + 7d;

-- Array property filter
SELECT path, title, metadata.contributors
FROM notes
WHERE metadata.contributors CONTAINS "alice";

-- Using metadata_index table (if implemented)
SELECT note.path, note.title, value
FROM metadata_index
WHERE key = "status"
  AND value = "published"
FETCH note;
```

### 6. Complex Queries (Combining Patterns)

```surql
-- Find active projects with recent updates and backlinks
SELECT path, title,
  modified_at,
  tags,
  count(<-wikilink) AS backlinks
FROM notes
WHERE tags CONTAINS "#project"
  AND metadata.status = "active"
  AND modified_at > time::now() - 7d
ORDER BY backlinks DESC, modified_at DESC;

-- Discover related notes through tags and links
SELECT path, title,
  -- Notes with same tags
  (SELECT path FROM notes WHERE tags CONTAINSANY $parent.tags LIMIT 5) AS similar_by_tags,
  -- Notes linked from this note
  ->wikilink->notes.(path, title) AS linked_notes,
  -- Notes linking to this note
  <-wikilink<-notes.(path, title) AS backlinks
FROM notes:foo.md;

-- Build a topic graph (notes + their connections)
SELECT path, title, tags,
  array::len(->wikilink) AS outgoing_links,
  array::len(<-wikilink) AS incoming_links,
  ->wikilink->notes.(path, title) AS connections
FROM notes
WHERE tags CONTAINS "#knowledge-graph"
ORDER BY (incoming_links + outgoing_links) DESC;

-- Find entry points (highly linked, no tags)
SELECT path, title, count(<-wikilink) AS popularity
FROM notes
WHERE array::len(tags) = 0
  AND count(<-wikilink) > 10
ORDER BY popularity DESC;

-- Semantic search within a folder
LET $query_embedding = [0.12, -0.45, 0.78, ...];
SELECT path, title,
  vector::distance::cosine(embedding, $query_embedding) AS similarity
FROM notes
WHERE string::starts_with(path, "Projects/")
  AND embedding IS NOT NONE
ORDER BY similarity ASC
LIMIT 10;
```

## Migration Strategy

### Schema Versioning

Use SurrealDB's `INFO` commands to track schema version:

```surql
-- Store schema version
DEFINE FIELD schema_version ON TABLE metadata TYPE int DEFAULT 1;
CREATE metadata:system SET schema_version = 1, updated_at = time::now();

-- Check current version
SELECT schema_version FROM metadata:system;
```

### Migration Scripts

Store migrations as numbered SurrealQL files:

```
migrations/
  001_initial_schema.surql
  002_add_embedding_indexes.surql
  003_add_metadata_index_table.surql
```

### Backwards Compatibility

When evolving schema:

1. **Add Optional Fields**: Use `DEFINE FIELD ... TYPE option<T>` for new fields
2. **Computed Fields**: Derive from existing data rather than requiring migration
3. **Edge Properties**: Add new edge metadata without breaking existing edges
4. **Index Additions**: Can be added without downtime

### Zero-Downtime Migrations

```surql
-- 1. Add new field (optional)
DEFINE FIELD new_field ON TABLE notes TYPE option<string>;

-- 2. Populate gradually (background job)
UPDATE notes SET new_field = "default_value" WHERE new_field IS NONE LIMIT 1000;

-- 3. Once complete, make required
REMOVE FIELD new_field ON TABLE notes;
DEFINE FIELD new_field ON TABLE notes TYPE string DEFAULT "default_value";
```

### Rollback Strategy

1. **Schema Snapshots**: Export schema before migrations
   ```bash
   surreal export --ns crucible --db vault schema_backup.surql
   ```

2. **Transaction-based Migrations**: Wrap in `BEGIN TRANSACTION` / `COMMIT`
   ```surql
   BEGIN TRANSACTION;
   -- Migration steps
   COMMIT TRANSACTION;
   ```

3. **Feature Flags**: Enable new schema features gradually

## Performance Considerations

### Query Optimization

1. **Use Record IDs**: `notes:path/to/doc.md` faster than `WHERE path = "path/to/doc.md"`
2. **Limit Graph Depth**: Unbounded graph traversal can be expensive
3. **Index Coverage**: Ensure filtered fields are indexed
4. **Fetch vs Inline**: Use `FETCH` to avoid N+1 queries

```surql
-- BAD: N+1 queries
SELECT path, (SELECT * FROM ->wikilink->notes) AS links FROM notes;

-- GOOD: Single query with FETCH
SELECT path, ->wikilink->notes.* FROM notes;
```

### Storage Optimization

1. **Embedding Compression**: Consider quantization (float32 → int8) for large vaults
2. **Content Deduplication**: Store content in separate table if same content appears multiple times
3. **Archival Strategy**: Move old/rarely-accessed notes to cold storage table

### Scaling Thresholds

- **<1K notes**: All queries fast, no optimization needed
- **1K-10K notes**: Add indexes, monitor slow queries
- **10K-100K notes**: Consider `metadata_index` table, optimize embeddings
- **100K+ notes**: Partition by folder, implement sharding strategy

## Implementation Checklist

### Phase 1: Core Schema
- [ ] Define `notes` table with core fields
- [ ] Add `path` unique index
- [ ] Define `wikilink` relation
- [ ] Test basic CRUD operations

### Phase 2: Search Capabilities
- [ ] Add full-text search indexes
- [ ] Define `tags` table
- [ ] Add `tagged_with` relation
- [ ] Test tag filtering queries

### Phase 3: Semantic Search
- [ ] Add `embedding` field
- [ ] Create MTREE index on embeddings
- [ ] Define `relates_to` relation
- [ ] Test vector similarity queries

### Phase 4: Advanced Features
- [ ] Add metadata indexes
- [ ] Implement tag hierarchy
- [ ] Create computed fields
- [ ] Optimize query performance

### Phase 5: Production Hardening
- [ ] Set up migration system
- [ ] Add schema versioning
- [ ] Implement backup/restore
- [ ] Monitor query performance

## References

- [SurrealDB Documentation](https://surrealdb.com/docs)
- [SurrealQL Graph Relations](https://surrealdb.com/docs/surrealql/statements/relate)
- [Full-Text Search](https://surrealdb.com/docs/surrealql/statements/define/indexes#full-text-search)
- [Vector Search](https://surrealdb.com/docs/surrealql/functions/vector)

---

**Next Steps**: Implement schema in `/home/moot/crucible/crates/crucible-surrealdb/src/schema.surql`
