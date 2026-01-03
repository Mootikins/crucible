-- ============================================================================
-- Crucible NoteStore Schema
-- ============================================================================
-- Version: 1.0.0
-- Created: 2025-01-03
-- Purpose: Simplified note storage for the NoteStore trait implementation.
--          This schema provides a flat, efficient table for note metadata
--          and semantic search.
--
-- Usage:
--   Applied programmatically via SurrealNoteStore::apply_schema()
-- ============================================================================

-- ============================================================================
-- TABLE: note_records
-- ============================================================================
-- Primary table for NoteStore implementation.
-- Each record represents a parsed note's metadata and embedding.

DEFINE TABLE note_records SCHEMAFULL;

-- Primary key: relative path to the note file
DEFINE FIELD path ON TABLE note_records
    TYPE string
    ASSERT $value != NONE;

-- BLAKE3 content hash (32 bytes, stored as hex string for compatibility)
DEFINE FIELD content_hash ON TABLE note_records
    TYPE string
    ASSERT $value != NONE AND string::len($value) == 64;

-- Optional embedding vector for semantic search
-- Dimensions vary by model (384 for MiniLM, 768 for others, etc.)
DEFINE FIELD embedding ON TABLE note_records
    TYPE option<array<float>>;

-- Note title (from frontmatter or first heading)
DEFINE FIELD title ON TABLE note_records
    TYPE string
    DEFAULT "";

-- Tags from inline tags and frontmatter
DEFINE FIELD tags ON TABLE note_records
    TYPE array<string>
    DEFAULT [];

-- Denormalized outlinks (paths this note links to)
DEFINE FIELD links_to ON TABLE note_records
    TYPE array<string>
    DEFAULT [];

-- Frontmatter properties as flexible JSON object
DEFINE FIELD properties ON TABLE note_records
    TYPE object
    DEFAULT {};

-- Last update timestamp
DEFINE FIELD updated_at ON TABLE note_records
    TYPE datetime
    DEFAULT time::now();

-- ============================================================================
-- INDEXES
-- ============================================================================

-- Unique path constraint (primary lookup key)
DEFINE INDEX note_path_idx ON TABLE note_records
    COLUMNS path
    UNIQUE;

-- Content hash index for deduplication and change detection
DEFINE INDEX note_hash_idx ON TABLE note_records
    COLUMNS content_hash;

-- Tags index for filtering
DEFINE INDEX note_tags_idx ON TABLE note_records
    COLUMNS tags;

-- Updated timestamp for ordering and incremental sync
DEFINE INDEX note_updated_idx ON TABLE note_records
    COLUMNS updated_at;

-- ============================================================================
-- VECTOR INDEX (Dynamic Creation)
-- ============================================================================
-- NOTE: The MTREE vector index is created dynamically at runtime
-- via ensure_vector_index() to match the configured embedding dimensions.
-- Default dimensions: 384 (all-MiniLM-L6-v2)
--
-- Example (created programmatically):
-- DEFINE INDEX note_embedding_idx ON TABLE note_records
--     COLUMNS embedding
--     MTREE DIMENSION 384 DISTANCE COSINE;

-- ============================================================================
-- END OF SCHEMA
-- ============================================================================
