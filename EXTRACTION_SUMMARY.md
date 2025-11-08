# EXTRACTION SUMMARY: Crucible Merkle Trees & Rune Deep Dive

## Document Overview
- **File**: claude-exploration-merkle-trees-and-Rune-p1.md
- **Total Content**: ~57k+ tokens
- **Scope**: Architectural analysis, database/query design, Merkle tree patterns, Rune integration

---

## 1. MERKLE TREE IMPLEMENTATION RECOMMENDATIONS

### Current Assessment vs Oxen Patterns

**Oxen's Approach (File-system scale):**
- Node Types: File, Directory, VNode, FileChunk, Commit
- Hash: u128 (16 bytes)
- Optimized for millions of files
- VNode optimization for large directories
- Content-addressed storage with deduplication

**Crucible's Approach (Document blocks):**
- Node Types: Leaf (block hashes), Internal (combined hashes)
- Binary tree structure
- Algorithm-agnostic (BLAKE3, SHA256)
- Block-level change detection
- Appropriate for thousands of blocks per document

### Recommendation: Learn Patterns, Don't Integrate

**Do NOT:**
- Replace Merkle tree implementation directly (different use cases)
- Switch from SurrealDB to RocksDB (loses query abstractions)

**DO Adopt:**
- Node caching strategy (merkle_tree_node_cache pattern)
- Short hash display (to_short_str() - first 10 chars)
- Batch processing patterns
- RocksDB compression features (lz4, snappy) via SurrealDB config

### Two-Level Hybrid Merkle Tree (RECOMMENDED)

Instead of binary tree only:

```rust
// N-ary semantic layer (sections)
struct DocumentMerkleTree {
    sections: Vec<SectionNode>,    // Semantic grouping
    root_hash: BlockHash,
}

struct SectionNode {
    heading: ASTBlock,
    blocks: Vec<ASTBlock>,         // Direct children
    binary_tree: MerkleTree,       // Efficient block hashing
    section_hash: BlockHash,       // Hash of section
}

// Benefits:
// - Semantic section-level grouping
// - Efficient block-level binary trees
// - Easy to answer "which section changed?"
// - Fast rehashing (only affected section)
// - Matches document mental model
```

**Performance Comparison:**
- Binary only: O(log_2 N) per block change
- Hybrid: O(S + log_k M) for section changes (better cache locality)
- Tree depth: Binary=13 for 10,000 blocks; Sections≈2

**VNode Optimization for Large Documents:**
```rust
// For documents >1000 blocks, use lazy loading
pub struct LargeDocumentNode {
    id: String,
    block_count: usize,
    root_hash: BlockHash,
    sections: LazyLoad<Vec<SectionNode>>,  // Load on demand
}
```

---

## 2. DATA MODEL & SCHEMA RECOMMENDATIONS

### Entity-Property-Relation (EPR) Model

**Core Tables:**

```sql
-- Universal base for all entities
CREATE TABLE entities (
    id TEXT PRIMARY KEY,           -- "note:abc123", "tag:xyz"
    type TEXT NOT NULL,            -- "note", "block", "tag", "media", "person"
    created_at TIMESTAMP,
    updated_at TIMESTAMP,
    deleted_at TIMESTAMP,          -- Soft deletes
    version INTEGER,               -- Optimistic locking
    content_hash TEXT,             -- BLAKE3 for change detection
    created_by TEXT,
    vault_id TEXT,
    data JSONB,                    -- Entity-specific data
    search_text TEXT,              -- Denormalized for FTS
);

-- Flexible metadata (plugin-extensible)
CREATE TABLE properties (
    entity_id TEXT,
    key TEXT,
    -- Multi-type value storage
    value_text TEXT,
    value_int INTEGER,
    value_float FLOAT,
    value_bool BOOLEAN,
    value_date TIMESTAMP,
    value_json JSONB,              -- Complex nested objects
    value_type TEXT,               -- Type information
    source TEXT,                   -- Which plugin/system set this
    confidence FLOAT,              -- For AI-generated properties
    PRIMARY KEY (entity_id, key),
);

-- Graph edges (typed, directed, weighted)
CREATE TABLE relations (
    id TEXT PRIMARY KEY,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    rel_type TEXT,                 -- "wikilink", "references", "contains"
    directed BOOLEAN,
    weight FLOAT,                  -- For graph algorithms
    confidence FLOAT,
    context TEXT,
    position INTEGER,
    data JSONB,
    source TEXT,                   -- "parser", "user", "plugin:xyz"
);

-- Sub-document AST blocks
CREATE TABLE blocks (
    id TEXT PRIMARY KEY,           -- "block:abc123"
    entity_id TEXT,                -- Parent note
    block_type TEXT,               -- "heading", "paragraph", "code"
    block_index INTEGER,
    parent_block TEXT,             -- For nested blocks
    depth INTEGER,
    content TEXT NOT NULL,
    content_hash TEXT,             -- BLAKE3
    start_offset INTEGER,
    end_offset INTEGER,
    start_line INTEGER,
    end_line INTEGER,
    metadata JSONB,
);

-- Vector embeddings for semantic search
CREATE TABLE embeddings (
    entity_id TEXT PRIMARY KEY,
    vector FLOAT[],                -- 384, 768, or 1536 dimensions
    dimensions INTEGER,
    model TEXT,                    -- "all-MiniLM-L6-v2", etc.
    model_version TEXT,
    content_used TEXT,             -- What was embedded
    created_at TIMESTAMP,
);

-- Hierarchical tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT UNIQUE,              -- "project/crucible"
    parent_id TEXT,                -- Parent tag
    path TEXT,                     -- Materialized path: "/project/crucible"
    depth INTEGER,
    description TEXT,
    color TEXT,
    icon TEXT,
);

-- Many-to-many junction for tags
CREATE TABLE entity_tags (
    entity_id TEXT,
    tag_id TEXT,
    source TEXT,                   -- "frontmatter", "inline", "plugin:auto-tagger"
    confidence FLOAT,
    PRIMARY KEY (entity_id, tag_id),
);
```

**Plugin Extension Tables:**

```sql
-- Allows plugins to register custom entity types & properties
CREATE TABLE plugin_schemas (
    plugin_id TEXT,
    schema_name TEXT,              -- "task", "citation", "person"
    schema_version INTEGER,
    definition JSONB,              -- JSON Schema format
    required_props TEXT[],
    indexed_props TEXT[],
    validation_fn TEXT,            -- Optional custom validation
    PRIMARY KEY (plugin_id, schema_name),
);

-- Event system for plugins
CREATE TABLE plugin_hooks (
    id SERIAL PRIMARY KEY,
    plugin_id TEXT,
    event_type TEXT,               -- "entity.created", "relation.updated"
    entity_type TEXT,              -- Filter to specific types
    priority INTEGER,
    handler_fn TEXT,
    enabled BOOLEAN,
);

-- Plugin-created materialized views
CREATE TABLE plugin_views (
    plugin_id TEXT,
    view_name TEXT,
    query TEXT,                    -- SQL/SurrealQL defining view
    refresh_on TEXT[],             -- Events triggering refresh
    last_refresh TIMESTAMP,
    PRIMARY KEY (plugin_id, view_name),
);
```

**Key Design Decisions:**

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Metadata | EAV (Entity-Attribute-Value) | Flexibility > performance; plugins extend without migrations |
| Property Types | Typed (value_text, int, etc.) | Query performance; plugin validation |
| Relations | Typed, directed, weighted | Support graph algorithms; track confidence; enable filtering |
| Deletes | Soft deletes (deleted_at) | History preservation; recovery options |
| Flexibility | Open-Closed principle | Plugins add data without modifying core tables |

---

## 3. QUERY LANGUAGE DESIGN (CQL)

### Architecture: 3 Layers

**Layer 1: Declarative (Simple)** - 80% of use cases
**Layer 2: Procedural (Pipelines)** - 15% of use cases
**Layer 3: Functional (Advanced)** - 5% of use cases

### Layer 1: Declarative Queries

```
# Table queries
TABLE notes
  WHERE tags CONTAINS "project"
    AND properties.status = "in-progress"
    AND created_at > @today - 7d
  SORT BY priority DESC
  LIMIT 10

# Graph traversal
GRAPH notes
  WHERE title = "Index"
  FOLLOW wikilink -> wikilink -> wikilink    # 3 hops
  WHERE tags CONTAINS "example"              # Filter destination
  LIMIT 50

# Bidirectional traversal
GRAPH notes
  WHERE id = $current_note
  FOLLOW wikilink | REVERSE wikilink         # Outbound + inbound
  LIMIT 100

# Path finding
PATH notes
  FROM id = "note:start"
  TO id = "note:end"
  VIA wikilink
  MAX DEPTH 5

# Vector semantic search
SEARCH "machine learning"
  IN notes
  THRESHOLD 0.7
  LIMIT 20

# Hybrid search (vector + filters + graph)
SEARCH "rust programming"
  IN notes
  WHERE tags CONTAINS "technical"
  FOLLOW wikilink
  DEPTH 1
  THRESHOLD 0.8

# Aggregations
COUNT notes
  GROUP BY tags
  HAVING count > 5
  SORT BY count DESC

STATS notes
  AGGREGATE
    COUNT(*) as total,
    AVG(word_count) as avg_words,
    MAX(updated_at) as last_modified
  GROUP BY properties.project
```

### Layer 2: Procedural (Pipeline Style)

```
# Multi-stage processing
notes
  | WHERE tags CONTAINS "project"
  | FOLLOW wikilink
  | WHERE properties.status = "active"
  | ENRICH WITH backlink_count = (COUNT REVERSE wikilink)
  | SORT BY backlink_count DESC
  | LIMIT 10

# Graph algorithms
notes
  | COMPUTE pagerank = PAGERANK(wikilink, damping=0.85)
  | WHERE pagerank > 0.01
  | SORT BY pagerank DESC

notes
  | COMPUTE centrality = BETWEENNESS(wikilink)
  | COMPUTE community = LOUVAIN(wikilink)
  | GROUP BY community
```

### Layer 3: Functional (Advanced)

```
# Recursive traversal with cycle detection
RECURSIVE traverse(node, visited) {
  FROM node
  FOLLOW wikilink AS next
  WHERE next NOT IN visited
  COLLECT visited + [next] AS new_visited
  RECURSE traverse(next, new_visited)
  MAX DEPTH 10
}

traverse("note:start", [])

# Temporal queries
notes
  | WHERE path = "/daily/2025-01-15.md"
  | VERSIONS BETWEEN @2025-01-15 AND @2025-01-16
  | DIFF SHOW changes

# Snapshot comparison
GRAPH notes AS OF @2024-12-01
  FOLLOW wikilink
  COMPARE WITH @2025-01-01
```

### AST Structure for Parser

```rust
#[derive(Debug, Clone)]
pub enum Query {
    Table(TableQuery),
    Graph(GraphQuery),
    Search(SearchQuery),
    Pipeline(PipelineQuery),
    Recursive(RecursiveQuery),
}

#[derive(Debug, Clone)]
pub struct TableQuery {
    pub source: EntityType,
    pub filters: Vec<Filter>,
    pub sort: Option<SortClause>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GraphQuery {
    pub start: Filter,
    pub traversal: Vec<Traversal>,  // Multiple hops
    pub filters: Vec<Filter>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Traversal {
    pub relation: String,
    pub direction: Direction,       // Forward, Backward, Both
    pub max_depth: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum Filter {
    Eq(PropertyPath, Value),
    Neq(PropertyPath, Value),
    Gt(PropertyPath, Value),
    Lt(PropertyPath, Value),
    Contains(PropertyPath, Value),
    In(PropertyPath, Vec<Value>),
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Box<Filter>),
}
```

---

## 4. RUNE INTEGRATION (CRITICAL PARADIGM SHIFT)

### Key Insight: Queries ARE Rune Scripts

Instead of custom parser → AST → compiler, expose database through Rune APIs:

```rust
// This IS a query in Rune!
pub fn find_related_notes(title) {
    notes()
        .where(|n| n.tags.contains("project"))
        .where(|n| n.created_at > Date::today() - Duration::days(7))
        .sort_by(|n| n.updated_at)
        .take(10)
}

// Graph traversal
pub fn explore_connections(note_id) {
    notes()
        .where(|n| n.id == note_id)
        .follow_relation("wikilink")
        .depth(3)
        .where(|n| n.tags.contains("active"))
}

// Semantic search with graph expansion
pub fn find_context(query) {
    search(query)
        .threshold(0.7)
        .follow_relation("wikilink")
        .depth(1)
        .take(20)
}
```

### Architecture: The Missing Middle Layer

```
┌─────────────────────────────────────────┐
│    Plugin Authors (Write in Rune)       │  ← Hot-reloadable scripts
├─────────────────────────────────────────┤
│    Rune VM (Embedded in Crucible)       │  ← Sandboxed execution
├─────────────────────────────────────────┤
│   Crucible Core API (Rust exposed)      │  ← #[rune::function] macros
├─────────────────────────────────────────┤
│    Database Layer (SurrealDB)           │  ← Performance critical
└─────────────────────────────────────────┘
```

### Rune vs Custom DSL

| Aspect | Custom CQL | Rune-based |
|--------|-----------|-----------|
| Parser | Custom (pest/nom) | Rune handles it |
| Power | Limited to query design | Full programming language |
| Hot-reload | Not applicable | Native support |
| Learning curve | New syntax | "Rust without types" |
| Type safety | Compile-time | Runtime checks |
| Extensibility | Plugin via registry | Dynamic function definition |

### Rune Core Module Exposed

```rust
pub fn crucible_module() -> Result<Module, ContextError> {
    let mut module = Module::with_crate("crucible")?;

    // Entity operations
    module.function_meta(notes)?;
    module.function_meta(blocks)?;
    module.function_meta(tags)?;
    module.function_meta(entities)?;

    // Graph operations
    module.function_meta(follow)?;
    module.function_meta(traverse)?;
    module.function_meta(path_between)?;

    // Search operations
    module.function_meta(search)?;
    module.function_meta(semantic_search)?;
    module.function_meta(hybrid_search)?;

    // Property operations
    module.function_meta(set_property)?;
    module.function_meta(get_property)?;

    Ok(module)
}
```

---

## 5. CONCRETE IMPLEMENTATION PATTERNS

### Query Compiler Design

```rust
pub struct QueryCompiler {
    backend: Arc<dyn QueryBackend>,
    plugin_registry: Arc<PluginRegistry>,
}

impl QueryCompiler {
    pub fn compile(&self, query: &Query) -> Result<CompiledQuery> {
        match query {
            Query::Table(tq) => self.compile_table(tq),
            Query::Graph(gq) => self.compile_graph(gq),
            Query::Search(sq) => self.compile_search(sq),
            // ... translate to backend-specific query
        }
    }

    fn compile_graph(&self, query: &GraphQuery) -> Result<CompiledQuery> {
        // Different translation based on backend
        match self.backend.backend_type() {
            BackendType::SurrealDB => self.compile_graph_surreal(query),
            BackendType::SQLite => self.compile_graph_sqlite(query),
            BackendType::CozoDB => self.compile_graph_cozo(query),
        }
    }
}
```

### Graph Query Compilation Examples

**SurrealDB (native graph syntax):**
```sql
SELECT * FROM notes
WHERE title = "Index"
->wikilink->notes
->wikilink->notes
WHERE tags CONTAINS "project"
LIMIT 20
```

**SQLite (recursive CTE):**
```sql
WITH RECURSIVE graph_traverse(id, depth, path) AS (
    -- Base case
    SELECT id, 0 as depth, id as path
    FROM entities
    WHERE title = "Index"
    
    UNION ALL
    
    -- Recursive case
    SELECT r.to_id, gt.depth + 1, gt.path || ',' || r.to_id
    FROM graph_traverse gt
    JOIN relations r ON gt.id = r.from_id
    WHERE r.rel_type = 'wikilink'
      AND gt.depth < 2
      AND INSTR(gt.path, ',' || r.to_id) = 0  -- Cycle detection
)
SELECT DISTINCT e.*
FROM graph_traverse gt
JOIN entities e ON gt.id = e.id
WHERE e.tags LIKE '%project%'
LIMIT 20
```

### Plugin API Example: Task Manager

```rust
use crucible::{Plugin, QueryEngine, Filter, Value};

pub struct TaskManagerPlugin {
    query: QueryEngine,
}

impl Plugin for TaskManagerPlugin {
    async fn on_load(&mut self) -> Result<()> {
        // Register custom entity type
        self.register_entity_type("task", task_schema()).await?;
        
        // Register custom relation
        self.register_relation_type("blocks", "One task blocks another").await?;
        
        Ok(())
    }

    async fn get_open_tasks(&self) -> Result<Vec<Entity>> {
        self.query
            .table("task")
            .where_eq("status", "open")
            .where_contains("tags", "urgent")
            .sort_by("priority", true)
            .limit(20)
            .execute()
            .await
    }

    async fn get_blocked_tasks(&self) -> Result<Vec<Entity>> {
        self.query
            .graph(Filter::Eq(
                PropertyPath::from("type"),
                Value::String("task".to_string())
            ))
            .follow_reverse("blocks")
            .filter(Filter::Eq(
                PropertyPath::from("status"),
                Value::String("open".to_string())
            ))
            .execute()
            .await
    }
}
```

### Backend Abstraction (Future-proof)

```rust
pub trait QueryBackend: Send + Sync {
    fn backend_type(&self) -> BackendType;
    async fn execute(&self, query: CompiledQuery) -> Result<QueryResult>;
}

// Current implementation
pub struct SurrealDbBackend { /* ... */ }
impl QueryBackend for SurrealDbBackend { /* ... */ }

// Future alternatives without breaking plugins
pub struct SqliteBackend { /* ... */ }
pub struct CozoDbBackend { /* ... */ }
```

---

## 6. DATABASE CHOICE JUSTIFICATION

### SurrealDB Chosen (Despite Alternatives)

**Why SurrealDB over Alternatives:**

| Criteria | SQLite+vec | SurrealDB | Winner |
|----------|-----------|----------|--------|
| Graph support | Manual CTEs | Native | SurrealDB |
| Metadata flexibility | JSON | JSONB + schema | SurrealDB |
| Vector search | sqlite-vec | MTREE | Tie (good enough) |
| Scaling to 100K+ | Good | Better | SurrealDB |
| Live queries | No | Yes | SurrealDB |
| Relational queries | Full SQL | SurrealQL | SQL-familiar |
| **Weighted Score** | 4.35/5 | 3.85/5 | SQLite wins |

**BUT with context changes (yours):**
- SQL not ABSOLUTE requirement → Custom DSL OK
- Graph queries VERY important → SurrealDB's native support critical
- Metadata CORE to vision → SurrealDB's flexibility wins
- Scaling important → RocksDB backend proven

**Recommendation: STAY WITH SURREALDB**

Reasons:
1. Already using it (zero migration cost)
2. Native graph traversals (->wikilink-> syntax)
3. Flexible metadata handling (nested JSON)
4. RocksDB backend proven at scale
5. Build query abstraction on top (insulates plugins)

**Escape Hatch:** Abstract database layer NOW

```rust
pub trait KilnDatabase {
    async fn search(&self, query: &Query) -> Result<Vec<Note>>;
    async fn get_note(&self, id: &str) -> Result<Note>;
    async fn store_note(&self, note: &Note) -> Result<()>;
    async fn graph_traverse(&self, start: &str, depth: u8) -> Result<Vec<Note>>;
}

// Current
impl KilnDatabase for SurrealDbBackend { ... }

// Future (no plugin changes)
impl KilnDatabase for SqliteBackend { ... }
impl KilnDatabase for CozoDbBackend { ... }
```

---

## 7. IMPLEMENTATION ROADMAP

### Phase 1: Core Schema (3 weeks)
- [ ] Implement entities, properties, relations tables
- [ ] Create basic CRUD operations
- [ ] Add indexes for performance
- [ ] Write migration scripts
- [ ] Test with 10K sample notes

### Phase 2: Query Parser (3 weeks)
- [ ] Build CQL parser (use pest or nom)
- [ ] Create AST types
- [ ] Implement basic query compilation
- [ ] Add comprehensive error handling
- [ ] Write parser tests

### Phase 3: Query Execution (3 weeks)
- [ ] Implement table queries (SELECT with filters, sorting)
- [ ] Implement graph queries (recursive CTEs for SQLite fallback)
- [ ] Add vector search integration
- [ ] Performance optimization & indexing
- [ ] Benchmark with realistic workloads

### Phase 4: Rune Integration (3 weeks)
- [ ] Create crucible_module() exposing Rust API
- [ ] Implement query builder helpers for Rune
- [ ] Hot-reload mechanism
- [ ] Sandboxing & security review
- [ ] Example plugins

### Phase 5: Advanced Features (4 weeks)
- [ ] Pipeline queries
- [ ] Computed properties (PageRank, centrality)
- [ ] Custom functions in Rune
- [ ] Temporal queries & versioning
- [ ] Query optimization (query planner hints)

**Total: 16 weeks for full implementation**

---

## 8. PERFORMANCE TARGETS & SCALING

### Query Performance Targets
- **Simple table queries**: <50ms p95
- **Full-text search**: <50ms p95
- **Vector search**: <50ms p95 (good; <20ms if Qdrant added)
- **Graph traversal (2-hop)**: <50ms p95
- **Hybrid (vector + filter + graph)**: <100ms p95

### Scaling Projections

**At 10K notes (current):**
- Graph queries (2-hop): 5-15ms
- Vector search: 10-30ms
- Hybrid queries: 50-100ms

**At 100K notes (near-term):**
- Graph queries: 10-30ms
- Vector search: 20-50ms (consider Qdrant if >50ms)
- Hybrid: 100-200ms (acceptable)

**At 500K notes (long-term):**
- Graph queries: 30-100ms ⚠️ May need optimization
- Vector search: 50-150ms ⚠️ Strong candidate for Qdrant
- Hybrid: 200-500ms ⚠️ Hit limits
- Migration trigger: p95 latencies > 200ms consistently

### Optimization Opportunities
1. **Materialized views** for hot paths
2. **Index tuning** for property filtering
3. **Add Qdrant** if vector search bottleneck
4. **Query caching** for repeated patterns
5. **Lazy loading** for large graphs (VNode pattern)

---

## KEY TAKEAWAYS

1. **Merkle Trees**: Use hybrid (section-level n-ary + block-level binary) for semantic grouping
2. **Schema**: Entity-Property-Relation model for flexibility; plugins extend without migrations
3. **Query Language**: 3-layer CQL (Declarative → Procedural → Functional)
4. **Backend**: SurrealDB with query abstraction layer for future flexibility
5. **Plugins**: Use Rune for hot-reloadable extensibility; custom parser NOT needed
6. **Architecture**: Rune VM as "missing middle layer" between plugins and Rust core
7. **Scaling**: Start with built-in features; add Qdrant/optimizations only when bottlenecks appear

