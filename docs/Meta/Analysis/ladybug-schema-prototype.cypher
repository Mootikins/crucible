-- ============================================================================
-- Crucible Knowledge Kiln - LadybugDB Schema Prototype
-- ============================================================================
-- Version: 0.1.0 (prototype)
-- Created: 2025-12-30
-- Purpose: Minimal graph-native schema for Crucible on LadybugDB/Cypher
--
-- Design principles:
-- 1. Graph-first: relationships are first-class citizens
-- 2. Minimal: only essential fields, no EAV complexity
-- 3. LLM-friendly: Cypher syntax has good training data coverage
-- 4. Block-level: embeddings at block granularity for precise retrieval
-- ============================================================================

-- ============================================================================
-- NODE TABLES
-- ============================================================================

-- Primary document storage
CREATE NODE TABLE Note(
    path STRING,                    -- relative path from kiln root (PK)
    title STRING,                   -- extracted title (first H1 or filename)
    content STRING,                 -- full markdown content
    file_hash STRING,               -- BLAKE3 hash for change detection
    modified_at TIMESTAMP,          -- file modification time
    created_at TIMESTAMP,           -- first indexed time
    folder STRING,                  -- computed: first path segment
    status STRING,                  -- from frontmatter (optional)
    PRIMARY KEY(path)
);

-- Content blocks (paragraphs, headings, code, lists, etc.)
CREATE NODE TABLE Block(
    id STRING,                      -- {note_path}#{block_index}
    block_type STRING,              -- "paragraph", "heading", "code", "list", etc.
    content STRING,                 -- block text content
    content_hash STRING,            -- BLAKE3 hash for deduplication
    heading_level INT64,            -- 1-6 for headings, NULL otherwise
    start_line INT64,               -- source location
    end_line INT64,
    PRIMARY KEY(id)
);

-- Hierarchical tags
CREATE NODE TABLE Tag(
    name STRING,                    -- normalized tag name (no #)
    path STRING,                    -- full path for nested tags: "project/crucible"
    depth INT64,                    -- nesting level (0 = root)
    color STRING,                   -- optional display color
    PRIMARY KEY(name)
);

-- Embeddings stored separately for flexibility
-- (allows multiple models, block vs note level)
CREATE NODE TABLE Embedding(
    id STRING,                      -- {entity_type}:{entity_id}:{model}
    entity_type STRING,             -- "note" or "block"
    entity_id STRING,               -- path or block id
    model STRING,                   -- e.g., "all-MiniLM-L6-v2"
    model_version STRING,           -- for cache invalidation
    vector FLOAT[384],              -- embedding vector (dimension varies by model)
    content_hash STRING,            -- hash of content that was embedded
    created_at TIMESTAMP,
    PRIMARY KEY(id)
);

-- File state cache for incremental processing
CREATE NODE TABLE FileState(
    path STRING,                    -- relative path
    file_hash STRING,               -- BLAKE3 hash
    modified_time TIMESTAMP,        -- filesystem mtime
    file_size INT64,                -- bytes
    updated_at TIMESTAMP,           -- when we last checked
    PRIMARY KEY(path)
);

-- ============================================================================
-- RELATIONSHIP TABLES
-- ============================================================================

-- Wikilinks: [[Target Note]] or [[Target Note|alias]]
CREATE REL TABLE LINKS_TO(
    FROM Note TO Note,
    link_text STRING,               -- the text inside [[ ]]
    display_alias STRING,           -- text after | if present
    context STRING,                 -- surrounding text for relevance
    position INT64,                 -- character offset in source
    weight FLOAT DEFAULT 1.0        -- for graph algorithms
);

-- Embeds: ![[Target Note]] or ![[Target Note#heading]]
CREATE REL TABLE EMBEDS(
    FROM Note TO Note,
    embed_type STRING,              -- "full", "heading", "block"
    target_anchor STRING,           -- heading name or block ID if specified
    position INT64
);

-- Note contains blocks (ordered)
CREATE REL TABLE HAS_BLOCK(
    FROM Note TO Block,
    block_index INT64,              -- 0-based order within note
    section_path STRING             -- breadcrumb: "## Intro > ### Details"
);

-- Note/Block tagged with Tag
CREATE REL TABLE TAGGED_WITH(
    FROM Note TO Tag,
    source STRING DEFAULT 'frontmatter',  -- "frontmatter", "inline", "auto"
    confidence FLOAT DEFAULT 1.0          -- 1.0 for explicit, <1.0 for inferred
);

-- Tag hierarchy
CREATE REL TABLE TAG_PARENT(
    FROM Tag TO Tag                 -- child -> parent
);

-- Semantic similarity (computed, sparse)
CREATE REL TABLE SIMILAR_TO(
    FROM Note TO Note,
    score FLOAT,                    -- cosine similarity
    model STRING,                   -- which embedding model
    computed_at TIMESTAMP
);

-- Block-level similarity for precise retrieval
CREATE REL TABLE BLOCK_SIMILAR_TO(
    FROM Block TO Block,
    score FLOAT,
    model STRING,
    computed_at TIMESTAMP
);

-- ============================================================================
-- INDEXES
-- ============================================================================

-- Note lookups
CREATE INDEX note_folder_idx ON Note(folder);
CREATE INDEX note_modified_idx ON Note(modified_at);
CREATE INDEX note_hash_idx ON Note(file_hash);

-- Block lookups
CREATE INDEX block_type_idx ON Block(block_type);
CREATE INDEX block_hash_idx ON Block(content_hash);

-- Tag lookups
CREATE INDEX tag_path_idx ON Tag(path);

-- Embedding lookups
CREATE INDEX embedding_entity_idx ON Embedding(entity_type, entity_id);
CREATE INDEX embedding_model_idx ON Embedding(model);

-- Vector similarity index (LadybugDB supports this)
CREATE VECTOR INDEX embedding_vector_idx ON Embedding(vector)
    USING HNSW(M=16, ef_construction=200);

-- ============================================================================
-- EXAMPLE QUERIES (for LLM training/prompting)
-- ============================================================================

-- Get all outgoing links from a note
-- MATCH (source:Note {path: 'Projects/Crucible.md'})-[l:LINKS_TO]->(target:Note)
-- RETURN target.path, l.link_text;

-- Get backlinks to a note
-- MATCH (source:Note)-[l:LINKS_TO]->(target:Note {path: 'Projects/Crucible.md'})
-- RETURN source.path, l.link_text, l.context;

-- 2-hop neighborhood (notes within 2 links)
-- MATCH (n:Note {path: $path})-[:LINKS_TO*1..2]-(related:Note)
-- RETURN DISTINCT related.path, related.title;

-- Find notes by tag (including child tags)
-- MATCH (n:Note)-[:TAGGED_WITH]->(t:Tag)
-- WHERE t.path STARTS WITH 'project'
-- RETURN n.path, t.name;

-- Semantic search (find similar blocks)
-- MATCH (b:Block)<-[:HAS_BLOCK]-(n:Note)
-- WHERE b.id IN $candidate_ids  -- from vector search
-- RETURN n.path, b.content, b.block_type
-- ORDER BY $scores[b.id] DESC
-- LIMIT 10;

-- Graph analytics: most connected notes
-- MATCH (n:Note)-[l:LINKS_TO]-()
-- RETURN n.path, COUNT(l) as connections
-- ORDER BY connections DESC
-- LIMIT 20;

-- Find orphan notes (no incoming or outgoing links)
-- MATCH (n:Note)
-- WHERE NOT (n)-[:LINKS_TO]-() AND NOT ()-[:LINKS_TO]->(n)
-- RETURN n.path;

-- Tag co-occurrence
-- MATCH (n:Note)-[:TAGGED_WITH]->(t1:Tag),
--       (n)-[:TAGGED_WITH]->(t2:Tag)
-- WHERE t1.name < t2.name
-- RETURN t1.name, t2.name, COUNT(n) as co_occurrences
-- ORDER BY co_occurrences DESC;

-- ============================================================================
-- SCHEMA STATISTICS (vs current SurrealDB)
-- ============================================================================
--
-- This schema:
--   - 5 node tables (Note, Block, Tag, Embedding, FileState)
--   - 7 relationship tables
--   - ~40 fields total
--   - ~100 lines
--
-- Current SurrealDB schema:
--   - 15+ tables across 2 competing schemas
--   - ~150 fields total
--   - ~600 lines
--
-- Reduction: ~75% fewer lines, clearer semantics
-- ============================================================================
