# Entity-Property-Relation (EPR) Schema Design Summary

**Reference Documents:**
- EXTRACTION_SUMMARY.md (sections 2 & 4)
- REFACTORING_PLAN.md (Goal 1)

---

## Core Design Philosophy

The EPR model separates **structure** (entities, relations) from **semantics** (properties), enabling:
- Plugin extensibility without schema migrations
- Flexible metadata storage
- Natural graph query patterns
- Future-proof architecture

**Key Principle:** Plugins add data by creating new property keys and relations, not by modifying core tables.

---

## Core Tables & Field Definitions

### 1. ENTITIES (Universal Base)

Represents all named "things" in the system: notes, blocks, tags, media, persons, etc.

```sql
CREATE TABLE entities {
    id: string UNIQUE,              -- "note:abc123", "tag:xyz", "block:def456"
    entity_type: string,            -- "note", "block", "tag", "section", "media", "person"
    created_at: datetime,           -- Auto-set to time::now()
    updated_at: datetime,           -- Auto-updated
    deleted_at: option<datetime>,   -- Soft deletes for recovery
    version: int,                   -- Optimistic locking (default: 1)
    content_hash: option<string>,   -- BLAKE3 hash for change detection
    created_by: option<string>,     -- User/plugin that created it
    vault_id: option<string>,       -- Vault ownership (multi-vault support)
    data: option<object>,           -- Entity-specific JSON data
    search_text: option<string>,    -- Denormalized FTS field
}

INDEXES:
- entity_id (UNIQUE)
- entity_type
- content_hash
```

**Why this design?**
- Single base table eliminates type-specific tables
- Version field enables optimistic locking
- content_hash tracks changes at entity level
- data field allows custom fields per entity type

---

### 2. PROPERTIES (Flexible Metadata - EAV Model)

Stores arbitrary key-value metadata, plugin-extensible by namespace.

```sql
CREATE TABLE properties {
    entity_id: record<entities>,    -- Links to parent entity
    namespace: string,              -- "core", "user", "plugin:task-manager"
    key: string,                    -- "title", "path", "status", "custom_field"
    
    -- Multi-type value storage (one populated per property)
    value: string,                  -- JSON-encoded for flexibility
    value_type: string,             -- "text", "json", "number", "boolean", "date"
    value_text: option<string>,     -- Optimized for text queries
    value_number: option<float>,    -- Optimized for numeric queries
    value_bool: option<bool>,       -- Optimized for boolean filters
    value_date: option<datetime>,   -- Optimized for time queries
    
    -- Metadata
    source: string,                 -- "parser", "user", "ml", "plugin:name"
    confidence: float,              -- For AI-generated properties (0-1)
    created_at: datetime,
    updated_at: datetime,
    
    PRIMARY KEY (entity_id, namespace, key)
}

INDEXES:
- (entity_id, namespace, key) - Primary lookup
- (namespace, key) - Cross-entity queries
```

**Multi-type Design:**
- Stores value as JSON string for max flexibility
- Also stores in typed columns (value_text, value_number, etc.)
- Queries use typed columns for performance
- Plugins register expected types in plugin_schemas

**Example Usage:**
```rust
// Core property (system-managed)
Property {
    entity_id: "note:abc123",
    namespace: "core",
    key: "title",
    value_text: "My Note",
    value_type: "text",
}

// Plugin property (plugin-managed)
Property {
    entity_id: "note:abc123",
    namespace: "plugin:task-manager",
    key: "status",
    value_text: "in-progress",
    value_type: "text",
}

// AI-generated property (low confidence)
Property {
    entity_id: "note:abc123",
    namespace: "plugin:auto-tagger",
    key: "topic",
    value_text: "rust-programming",
    confidence: 0.85,
    value_type: "text",
}
```

---

### 3. RELATIONS (Typed Graph Edges)

Directed, typed, weighted edges connecting entities.

```sql
CREATE TABLE relations {
    id: string UNIQUE,              -- "rel:xyz789"
    from_id: record<entities>,      -- Source entity
    to_id: record<entities>,        -- Target entity
    relation_type: string,          -- "wikilink", "embeds", "child_of", "tagged_with", "blocks"
    
    -- Graph properties
    weight: float,                  -- 1.0 default; for PageRank, centrality, etc.
    directed: bool,                 -- True if unidirectional
    confidence: float,              -- For ML-extracted relations (0-1)
    
    -- Tracking
    source: string,                 -- "user", "parser", "ml", "plugin:name"
    position: int,                  -- Order if relation is positional (e.g., list items)
    metadata: object,               -- Custom data per relation
    created_at: datetime,
    
    PRIMARY KEY (from_id, to_id, relation_type)
}

INDEXES:
- relation_type
- source
- (from_id, relation_type)
- (to_id, relation_type)
```

**Graph Query Support:**
```sql
-- Find all wikilinks from note:abc123
SELECT to_id FROM relations 
WHERE from_id = "note:abc123" 
  AND relation_type = "wikilink"

-- Native SurrealDB traversal syntax
SELECT * FROM (SELECT ->wikilink->entities FROM "note:abc123") 
WHERE entity_type = "note"

-- Reverse traversal (backlinks)
SELECT from_id FROM relations 
WHERE to_id = "note:abc123" 
  AND relation_type = "wikilink"
```

---

### 4. BLOCKS (AST Nodes for Merkle Trees)

Sub-document blocks representing AST nodes; enables block-level change detection.

```sql
CREATE TABLE blocks {
    id: string UNIQUE,              -- "block:abc123"
    entity_id: record<entities>,    -- Parent note
    block_index: int,               -- Position in document (0-indexed)
    block_type: string,             -- "heading", "paragraph", "list_item", "code_block"
    
    -- Content
    content: string,                -- Raw text of block
    content_hash: string,           -- BLAKE3 hash (64 hex chars)
    
    -- Positioning
    start_offset: int,              -- Character offset in source
    end_offset: int,
    start_line: int,                -- Line number in source
    end_line: int,
    
    -- Hierarchy (for nested blocks)
    parent_block_id: option<record<blocks>>,  -- For nested lists, quotes, etc.
    depth: int,                     -- Nesting depth
    
    -- Metadata
    metadata: object,               -- AST-specific: heading_level, language (for code)
    created_at: datetime,
    updated_at: datetime,
    
    PRIMARY KEY (entity_id, block_index)
}

INDEXES:
- (entity_id, block_index) - Primary lookup
- content_hash - Change detection
- block_type - Type-based queries
```

**Block Hierarchy Example:**
```
Document: note:abc123
├─ block:0 (heading level 1) "Introduction"
├─ block:1 (paragraph) "Content here"
├─ block:2 (list) 
│  ├─ block:2a (list_item, parent=2, depth=1) "Item 1"
│  ├─ block:2b (list_item, parent=2, depth=1) "Item 2"
│  │  └─ block:2b1 (list_item, parent=2b, depth=2) "Subitem"
```

---

### 5. EMBEDDINGS (Vector Search)

Stores vector embeddings for semantic search; supports block and entity-level indexing.

```sql
CREATE TABLE embeddings {
    entity_id: record<entities>,    -- Primary entity
    block_id: option<record<blocks>>,  -- Optional block-level embedding
    
    embedding: array<float>,        -- Vector (384, 768, or 1536 dims)
    dimensions: int,                -- Actual dimension count
    model: string,                  -- "all-MiniLM-L6-v2", "text-embedding-3-small"
    model_version: string,          -- Track model updates for reindexing
    content_used: string,           -- What was embedded (title, summary, full content)
    
    created_at: datetime,
    
    PRIMARY KEY (entity_id, model)  -- One embedding per model per entity
}

INDEXES:
- (entity_id, model) - UNIQUE
- MTREE on embedding column (dimension-specific):
    DIMENSION 384 DISTANCE COSINE (for MiniLM models)
    DIMENSION 768 DISTANCE COSINE (for larger models)
    DIMENSION 1536 DISTANCE COSINE (for OpenAI models)
```

**Query Example:**
```sql
-- Semantic search
SELECT entity_id, 1 - vector::similarity(embedding, @query_vec) AS distance
FROM embeddings
WHERE model = "all-MiniLM-L6-v2"
ORDER BY embedding <+> @query_vec  -- MTREE index accelerates this
LIMIT 20
THRESHOLD 0.7
```

---

### 6. TAGS (Hierarchical Taxonomy)

Structured tag hierarchy with materialized path for efficient queries.

```sql
CREATE TABLE tags {
    id: string UNIQUE,              -- "tag:abc123"
    name: string UNIQUE,            -- "project/crucible" (slash-delimited)
    parent_id: option<record<tags>>,-- Parent tag for nesting
    path: string,                   -- Materialized path "/project/crucible"
    depth: int,                     -- 0 for root, 1+ for children
    
    -- Metadata
    description: option<string>,
    color: option<string>,          -- Hex color for UI
    icon: option<string>,           -- Icon name for UI
    
    PRIMARY KEY (name)  -- Unique by full name
}

INDEXES:
- name (UNIQUE)
- path (materialized path queries)
```

**Hierarchy Example:**
```
tag:root1 (name="project", path="/project", depth=0)
├─ tag:c1 (name="project/crucible", path="/project/crucible", depth=1)
├─ tag:c2 (name="project/personal", path="/project/personal", depth=1)
│  └─ tag:c2a (name="project/personal/home", path="/project/personal/home", depth=2)
```

---

### 7. ENTITY_TAGS (Many-to-Many)

Junction table linking entities to tags with source tracking.

```sql
CREATE TABLE entity_tags {
    entity_id: record<entities>,
    tag_id: record<tags>,
    source: string,                 -- "frontmatter", "inline", "plugin:auto-tagger"
    confidence: float,              -- For AI-generated tags
    
    PRIMARY KEY (entity_id, tag_id)
}

INDEXES:
- (entity_id, tag_id) - Primary lookup
- tag_id - "Find all notes with tag X"
```

---

### 8. PLUGIN_SCHEMAS (Extensibility Registry)

Allows plugins to register custom entity types, properties, and relations.

```sql
CREATE TABLE plugin_schemas {
    plugin_id: string,              -- "task-manager", "auto-tagger", "zettelkasten"
    schema_name: string,            -- "task", "citation", "person"
    schema_version: int,            -- Track schema evolution
    
    -- JSON Schema format for validation
    definition: object,             -- Full JSON Schema document
    required_props: array<string>,  -- Required property keys
    indexed_props: array<string>,   -- Which properties should be indexed
    validation_fn: option<string>,  -- Custom validation function (Rune?)
    
    PRIMARY KEY (plugin_id, schema_name)
}
```

**Example:** Task Manager Plugin
```rust
PluginSchema {
    plugin_id: "task-manager",
    schema_name: "task",
    definition: {
        "type": "object",
        "properties": {
            "status": {"type": "string", "enum": ["open", "done", "blocked"]},
            "priority": {"type": "int", "minimum": 1, "maximum": 5},
            "due_date": {"type": "datetime"},
            "assignee": {"type": "string"}
        }
    },
    required_props: ["status"],
    indexed_props: ["status", "priority"],
}
```

---

## Key Benefits

### 1. Plugin Extensibility Without Migrations
- Plugins add properties in their own namespace
- No schema changes needed
- Safe multi-version deployments
- Properties are validated against plugin_schemas

### 2. Flexible Metadata
- EAV model vs fixed schema
- Custom fields per entity type
- AI confidence scores built-in
- Source tracking for provenance

### 3. Natural Graph Queries
- Relations are first-class
- Type filtering with relation_type
- SurrealDB native support: `->wikilink->entities`
- Weight/confidence for algorithms

### 4. Change Detection
- content_hash at entity level
- content_hash at block level
- version field for optimistic locking
- Enables efficient diff algorithms

### 5. Multi-Vault Support
- vault_id field for isolation
- Extend for permission boundaries
- Supports shared vaults

---

## Differences from Current Schema

| Aspect | Current | EPR |
|--------|---------|-----|
| **Structure** | Type-specific tables (notes, tags, media) | Single entities table + properties |
| **Metadata** | Fixed columns (title, author, etc.) | EAV model (any key-value) |
| **Relations** | Implicit (tags, links) | Explicit relations table |
| **Graph Queries** | Manual joins or scripting | Native graph traversal |
| **Extensibility** | Requires schema migration | Add property key to properties |
| **Plugin Support** | Hardcoded features | plugins register schemas |
| **Version Tracking** | Manual field | Built-in optimistic locking |
| **Change Detection** | Timestamps only | content_hash + version |

---

## Migration Strategy

### Phase 1: Parallel Schema (Zero Downtime)
```
Application
    ↓
[Write Adaptor]
    ↓ ↓
[Old Schema] [New EPR Schema]
    ↓ ↓
[Read Adaptor - Prefer EPR, fall back to old]
```

### Phase 2: Data Migration
```rust
// Pseudo-code
for note in old_schema.notes {
    // Create entity
    entities.create(Entity {
        id: format!("note:{}", note.id),
        entity_type: "note",
        content_hash: note.file_hash,
        ...
    })
    
    // Create properties
    properties.create(Property {
        entity_id: "note:...",
        namespace: "core",
        key: "path",
        value_text: note.path,
        ...
    })
    
    // Migrate relations
    for tag in note.tags {
        relations.create(Relation {
            from_id: "note:...",
            to_id: format!("tag:{}", tag),
            relation_type: "tagged_with",
        })
    }
    
    // Migrate blocks
    for block in note.blocks {
        blocks.create(Block {
            entity_id: "note:...",
            block_index: block.index,
            block_type: block.type,
            content: block.content,
            content_hash: block.hash,
        })
    }
}
```

### Phase 3: Cutover
1. Stop writing to old schema
2. Verify data integrity (sample checks)
3. Keep old schema as backup (don't drop immediately)
4. Monitor for issues
5. Drop old schema after validation period

---

## Integration with Merkle Trees

### Hybrid Merkle Tree Structure
```
Document (entity:abc123)
  ├─ Section 1: "Introduction" (Heading H1)
  │  └─ [paragraph, list] → BinaryMerkleTree (blocks 0-5)
  ├─ Section 2: "Methods" (Heading H1)
  │  └─ [paragraph, code_block, list] → BinaryMerkleTree (blocks 6-12)
  └─ Section 3: "Results" (Heading H1)
     └─ [table, paragraph] → BinaryMerkleTree (blocks 13-20)

Root Hash = Combine(Section1.hash, Section2.hash, Section3.hash)
```

### Change Detection Flow
```
1. Parse document → AST blocks
2. Group AST by headings → Sections
3. Build BinaryMerkleTree per section
4. Combine section hashes → Root hash
5. Compare with previous root hash:
   - If root matches: no changes
   - If root differs: find changed sections (O(S) where S = # sections)
   - For each changed section: binary tree diff (O(log N) per block)
```

### VNode Optimization
```sql
-- For large documents (>1000 blocks)
CREATE VIRTUAL NODE vnode:huge-doc {
    node_id: "vnode:huge-doc",
    block_count: 15000,
    root_hash: "abc123...",
    sections: LazyLoad::NotLoaded {
        storage_key: "vnode:huge-doc"
    }
}

-- Load sections on demand:
let sections = vnode.load_sections(db).await?;
```

---

## Performance Characteristics

### Query Performance Targets
| Query Type | Target p95 | Scaling to 100K notes |
|------------|-----------|----------------------|
| Simple table (WHERE) | <50ms | 10-30ms |
| Full-text search | <50ms | 20-50ms |
| Vector search | <50ms | 20-50ms |
| Graph traversal (2-hop) | <50ms | 10-30ms |
| Hybrid (vector + filter + graph) | <100ms | 100-200ms |

### Optimization Opportunities
1. Materialized views for hot paths
2. Index tuning for property filtering
3. Vector search delegation to Qdrant (if bottleneck)
4. Query result caching
5. Lazy loading via VNodes

---

## Implementation Roadmap

### Phase 1: Archive CLI Tests (0.5 days)
- Archive 26,372 test lines → 500 (98% reduction)
- Keep 3-4 core e2e tests

### Phase 2: EPR Schema Migration (2.5 days)
- Create schema file
- Create Rust types
- Write migration script
- Test on sample data

### Phase 3: Hybrid Merkle Trees (2.5 days)
- Implement SectionNode grouping
- Implement BinaryMerkleTree
- Add diff algorithm

### Phase 4: VNode Optimization (1 day)
- Lazy loading for large documents
- Threshold-based activation

### Phase 5: DB Layer Cleanup (2.5 days)
- Consolidate 28 files → 11 files
- Remove duplicate logic

### Phase 6: ACP Integration (1.5 days)
- Implement chat with context injection

**Total: ~11 days**

---

## Key Takeaways

1. **EPR is fundamentally about separation of concerns**: Structure (entities, relations) vs semantics (properties)

2. **Plugins don't migrate schemas**: They register custom property keys and relation types in their namespace

3. **No artificial limits**: Properties and relations are infinitely extensible

4. **Multi-type properties enable performance**: Store value as JSON for flexibility, also in typed columns for query optimization

5. **Merkle tree integration is seamless**: Blocks table directly supports change detection; hybrid structure matches document semantics

6. **Migration is non-breaking**: Run both schemas in parallel, migrate data, then cutover

7. **SurrealDB native features shine here**: Graph traversal, flexible schemas, MTREE for vectors

---

**For detailed implementation, see:**
- EXTRACTION_SUMMARY.md (771 lines)
- REFACTORING_PLAN.md (full schema definitions)

