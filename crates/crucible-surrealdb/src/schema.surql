-- ============================================================================
-- Crucible Knowledge Kiln - SurrealDB Schema
-- ============================================================================
-- Version: 2.0.0
-- Created: 2025-10-19
-- Updated: 2025-11-06 (Added document_blocks table for block-level storage)
-- Purpose: Define tables, relations, and indexes for knowledge kiln indexing
--
-- Usage:
--   surreal import --conn http://localhost:8000 --user root --pass root \
--     --ns crucible --db kiln schema.surql
-- ============================================================================

-- Namespace and Database
-- Note: Set via connection, not in schema file

-- ============================================================================
-- ANALYZERS (for full-text search)
-- ============================================================================

DEFINE ANALYZER content_analyzer
    TOKENIZERS blank, class
    FILTERS lowercase, snowball(english);

DEFINE ANALYZER title_analyzer
    TOKENIZERS blank, class
    FILTERS lowercase;

-- ============================================================================
-- TABLE: notes
-- ============================================================================
-- Primary table for all markdown documents in the kiln

DEFINE TABLE notes SCHEMAFULL;

-- Core identifiers
DEFINE FIELD path ON TABLE notes
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD title ON TABLE notes
    TYPE option<string>;

DEFINE FIELD content ON TABLE notes
    TYPE string;

-- File hash for change detection
DEFINE FIELD file_hash ON TABLE notes
    TYPE string
    ASSERT $value != NONE AND string::len($value) == 64;

-- Timestamps
DEFINE FIELD created_at ON TABLE notes
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD modified_at ON TABLE notes
    TYPE datetime
    DEFAULT time::now();

-- Full-text search fields (copies for indexing)
DEFINE FIELD content_text ON TABLE notes
    TYPE string
    FLEXIBLE;

DEFINE FIELD title_text ON TABLE notes
    TYPE option<string>
    FLEXIBLE;

-- Tags (array for fast filtering)
DEFINE FIELD tags ON TABLE notes
    TYPE array<string>
    DEFAULT [];

-- Semantic search
DEFINE FIELD embedding ON TABLE notes
    TYPE option<array<float>>;

DEFINE FIELD embedding_model ON TABLE notes
    TYPE option<string>;

DEFINE FIELD embedding_updated_at ON TABLE notes
    TYPE option<datetime>;

-- Flexible metadata (frontmatter properties)
DEFINE FIELD metadata ON TABLE notes
    TYPE object
    DEFAULT {};

-- Computed fields
DEFINE FIELD status ON TABLE notes
    VALUE $this.metadata.status OR "none";

DEFINE FIELD folder ON TABLE notes
    VALUE string::split($this.path, "/")[0];

DEFINE FIELD file_name ON TABLE notes
    VALUE array::last(string::split($this.path, "/"));

-- Constraints and indexes
DEFINE INDEX unique_path ON TABLE notes
    COLUMNS path
    UNIQUE;

DEFINE INDEX file_hash_idx ON TABLE notes
    COLUMNS file_hash;

DEFINE INDEX tags_idx ON TABLE notes
    COLUMNS tags;

DEFINE INDEX folder_idx ON TABLE notes
    COLUMNS folder;

DEFINE INDEX modified_at_idx ON TABLE notes
    COLUMNS modified_at;

-- Full-text search indexes
DEFINE INDEX content_search ON TABLE notes
    COLUMNS content_text
    SEARCH ANALYZER content_analyzer BM25 HIGHLIGHTS;

DEFINE INDEX title_search ON TABLE notes
    COLUMNS title_text
    SEARCH ANALYZER title_analyzer BM25;

-- Vector search index (384 dimensions for all-MiniLM-L6-v2, adjust as needed)
DEFINE INDEX embedding_idx ON TABLE notes
    COLUMNS embedding
    MTREE DIMENSION 384 DISTANCE COSINE;

-- ============================================================================
-- TABLE: tags
-- ============================================================================
-- Metadata and hierarchy for tags

DEFINE TABLE tags SCHEMAFULL;

-- Core fields
DEFINE FIELD name ON TABLE tags
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD description ON TABLE tags
    TYPE option<string>;

DEFINE FIELD color ON TABLE tags
    TYPE option<string>;

-- Statistics
DEFINE FIELD usage_count ON TABLE tags
    TYPE int
    DEFAULT 0;

DEFINE FIELD last_used ON TABLE tags
    TYPE option<datetime>;

-- Hierarchy (for nested tags like #project/crucible)
DEFINE FIELD parent_tag ON TABLE tags
    TYPE option<record<tags>>;

-- Metadata
DEFINE FIELD created_at ON TABLE tags
    TYPE datetime
    DEFAULT time::now();

-- Constraints
DEFINE INDEX unique_tag_name ON TABLE tags
    COLUMNS name
    UNIQUE;

-- ============================================================================
-- RELATION: wikilink
-- ============================================================================
-- Represents [[Document Title]] links between notes

DEFINE TABLE wikilink SCHEMAFULL
    TYPE RELATION
    FROM notes TO notes;

-- Edge metadata
DEFINE FIELD link_text ON TABLE wikilink
    TYPE string;

DEFINE FIELD context ON TABLE wikilink
    TYPE option<string>;

DEFINE FIELD position ON TABLE wikilink
    TYPE int;

DEFINE FIELD created_at ON TABLE wikilink
    TYPE datetime
    DEFAULT time::now();

-- For weighted graph algorithms
DEFINE FIELD weight ON TABLE wikilink
    TYPE float
    DEFAULT 1.0;

-- Index for filtering
DEFINE INDEX link_text_idx ON TABLE wikilink
    COLUMNS link_text;

-- ============================================================================
-- RELATION: tagged_with
-- ============================================================================
-- Links notes to tags (alternative to array field for rich queries)

DEFINE TABLE tagged_with SCHEMAFULL
    TYPE RELATION
    FROM notes TO tags;

-- Edge metadata
DEFINE FIELD added_at ON TABLE tagged_with
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD added_by ON TABLE tagged_with
    TYPE option<string>;  -- "user" or "auto"

-- ============================================================================
-- RELATION: embeds
-- ============================================================================
-- Represents ![[Document]] embed relationships between notes

DEFINE TABLE embeds SCHEMAFULL
    TYPE RELATION
    FROM notes TO notes;

-- Edge metadata for embeds
DEFINE FIELD embed_type ON TABLE embeds
    TYPE string
    DEFAULT "simple";

-- Reference target (heading name or block ID)
DEFINE FIELD reference_target ON TABLE embeds
    TYPE option<string>;

-- Display alias for embeds
DEFINE FIELD display_alias ON TABLE embeds
    TYPE option<string>;

-- Context where the embed appears
DEFINE FIELD context ON TABLE embeds
    TYPE option<string>;

-- Position in source document
DEFINE FIELD position ON TABLE embeds
    TYPE int;

-- Creation timestamp
DEFINE FIELD created_at ON TABLE embeds
    TYPE datetime
    DEFAULT time::now();

-- For weighted graph algorithms
DEFINE FIELD weight ON TABLE embeds
    TYPE float
    DEFAULT 1.0;

-- Indexes for filtering
DEFINE INDEX embed_type_idx ON TABLE embeds
    COLUMNS embed_type;

DEFINE INDEX reference_target_idx ON TABLE embeds
    COLUMNS reference_target;

-- ============================================================================
-- RELATION: relates_to
-- ============================================================================
-- Semantic similarity and other computed relationships

DEFINE TABLE relates_to SCHEMAFULL
    TYPE RELATION
    FROM notes TO notes;

-- Relationship type
DEFINE FIELD relation_type ON TABLE relates_to
    TYPE string;

-- Similarity score (for semantic search)
DEFINE FIELD score ON TABLE relates_to
    TYPE float
    DEFAULT 0.0;

-- Metadata
DEFINE FIELD computed_at ON TABLE relates_to
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD metadata ON TABLE relates_to
    TYPE option<object>;

-- Index for filtering by type
DEFINE INDEX relation_type_idx ON TABLE relates_to
    COLUMNS relation_type;

DEFINE INDEX score_idx ON TABLE relates_to
    COLUMNS score;

-- ============================================================================
-- TABLE: document_blocks
-- ============================================================================
-- Maps documents to their blocks for content-addressed storage and deduplication

DEFINE TABLE document_blocks SCHEMAFULL;

-- Reference to source document
DEFINE FIELD document_id ON TABLE document_blocks
    TYPE string
    ASSERT $value != NONE;

-- Block index within the document (0-based)
DEFINE FIELD block_index ON TABLE document_blocks
    TYPE int
    ASSERT $value >= 0;

-- Content hash of the block (BLAKE3, 64-char hex)
DEFINE FIELD block_hash ON TABLE document_blocks
    TYPE string
    ASSERT $value != NONE AND string::len($value) == 64;

-- Block type for context and analysis
DEFINE FIELD block_type ON TABLE document_blocks
    TYPE string
    ASSERT $value != NONE;  -- "heading", "paragraph", "list", "code", etc.

-- Start and end positions in the source document
DEFINE FIELD start_offset ON TABLE document_blocks
    TYPE int
    ASSERT $value >= 0;

DEFINE FIELD end_offset ON TABLE document_blocks
    TYPE int
    ASSERT $value >= 0;

-- Block content (for retrieval and analysis)
DEFINE FIELD block_content ON TABLE document_blocks
    TYPE string
    ASSERT $value != NONE;

-- Block metadata (preserved from AST)
DEFINE FIELD block_metadata ON TABLE document_blocks
    TYPE object
    DEFAULT {};

-- Creation timestamp
DEFINE FIELD created_at ON TABLE document_blocks
    TYPE datetime
    DEFAULT time::now();

-- Last updated timestamp
DEFINE FIELD updated_at ON TABLE document_blocks
    TYPE datetime
    DEFAULT time::now();

-- Indexes for efficient queries
DEFINE INDEX document_blocks_idx ON TABLE document_blocks
    COLUMNS document_id, block_index;

DEFINE INDEX block_hash_idx ON TABLE document_blocks
    COLUMNS block_hash;

DEFINE INDEX document_id_idx ON TABLE document_blocks
    COLUMNS document_id;

DEFINE INDEX block_type_idx ON TABLE document_blocks
    COLUMNS block_type;

DEFINE INDEX created_at_idx ON TABLE document_blocks
    COLUMNS created_at;

-- Unique constraint to prevent duplicate blocks in same document
DEFINE INDEX unique_document_block ON TABLE document_blocks
    COLUMNS document_id, block_index
    UNIQUE;

-- ============================================================================
-- TABLE: hybrid_tree
-- ============================================================================
-- Metadata for Hybrid Merkle trees (document-level)

DEFINE TABLE hybrid_tree SCHEMAFULL;

DEFINE FIELD document_path ON TABLE hybrid_tree TYPE string ASSERT $value != NONE;
DEFINE FIELD root_hash ON TABLE hybrid_tree TYPE string ASSERT $value != NONE;
DEFINE FIELD section_count ON TABLE hybrid_tree TYPE int ASSERT $value >= 0;
DEFINE FIELD total_blocks ON TABLE hybrid_tree TYPE int ASSERT $value >= 0;
DEFINE FIELD is_virtualized ON TABLE hybrid_tree TYPE bool DEFAULT false;
DEFINE FIELD virtual_section_count ON TABLE hybrid_tree TYPE option<int>;
DEFINE FIELD created_at ON TABLE hybrid_tree TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON TABLE hybrid_tree TYPE datetime DEFAULT time::now();
DEFINE FIELD metadata ON TABLE hybrid_tree TYPE option<object> FLEXIBLE;

DEFINE INDEX root_hash_idx ON TABLE hybrid_tree COLUMNS root_hash;
DEFINE INDEX updated_at_idx ON TABLE hybrid_tree COLUMNS updated_at;

-- ============================================================================
-- TABLE: section
-- ============================================================================
-- Individual sections of Merkle trees (uses composite record IDs)

DEFINE TABLE section SCHEMAFULL;

DEFINE FIELD tree_id ON TABLE section TYPE string ASSERT $value != NONE;
DEFINE FIELD section_index ON TABLE section TYPE int ASSERT $value >= 0;
DEFINE FIELD section_hash ON TABLE section TYPE string ASSERT $value != NONE;
DEFINE FIELD heading ON TABLE section TYPE option<string>;
DEFINE FIELD depth ON TABLE section TYPE int ASSERT $value >= 0 AND $value <= 6;
DEFINE FIELD start_block ON TABLE section TYPE int ASSERT $value >= 0;
DEFINE FIELD end_block ON TABLE section TYPE int ASSERT $value >= 0;
DEFINE FIELD section_data ON TABLE section TYPE bytes ASSERT $value != NONE;
DEFINE FIELD created_at ON TABLE section TYPE datetime DEFAULT time::now();

DEFINE INDEX tree_id_idx ON TABLE section COLUMNS tree_id;
DEFINE INDEX section_hash_idx ON TABLE section COLUMNS section_hash;

-- ============================================================================
-- TABLE: virtual_section
-- ============================================================================
-- Virtual sections for large documents (uses composite record IDs)

DEFINE TABLE virtual_section SCHEMAFULL;

DEFINE FIELD tree_id ON TABLE virtual_section TYPE string ASSERT $value != NONE;
DEFINE FIELD virtual_index ON TABLE virtual_section TYPE int ASSERT $value >= 0;
DEFINE FIELD hash ON TABLE virtual_section TYPE string ASSERT $value != NONE;
DEFINE FIELD primary_heading ON TABLE virtual_section TYPE option<string>;
DEFINE FIELD min_depth ON TABLE virtual_section TYPE int ASSERT $value >= 0 AND $value <= 6;
DEFINE FIELD max_depth ON TABLE virtual_section TYPE int ASSERT $value >= 0 AND $value <= 6;
DEFINE FIELD section_count ON TABLE virtual_section TYPE int ASSERT $value >= 0;
DEFINE FIELD total_blocks ON TABLE virtual_section TYPE int ASSERT $value >= 0;
DEFINE FIELD start_index ON TABLE virtual_section TYPE int ASSERT $value >= 0;
DEFINE FIELD end_index ON TABLE virtual_section TYPE int ASSERT $value >= 0;
DEFINE FIELD created_at ON TABLE virtual_section TYPE datetime DEFAULT time::now();

DEFINE INDEX virtual_tree_id_idx ON TABLE virtual_section COLUMNS tree_id;
DEFINE INDEX virtual_hash_idx ON TABLE virtual_section COLUMNS hash;

-- ============================================================================
-- TABLE: metadata (system metadata)
-- ============================================================================
-- Stores schema version and system configuration

DEFINE TABLE metadata SCHEMAFULL;

DEFINE FIELD schema_version ON TABLE metadata
    TYPE int
    DEFAULT 1;

DEFINE FIELD updated_at ON TABLE metadata
    TYPE datetime
    DEFAULT time::now();

-- Initialize system metadata
CREATE metadata:system SET
    schema_version = 2,
    updated_at = time::now();

-- Schema migration to version 2.0.0
-- Add document_blocks table and update version
UPDATE metadata:system SET
    schema_version = 2,
    updated_at = time::now()
WHERE schema_version < 2;

-- ============================================================================
-- OPTIONAL: metadata_index table
-- ============================================================================
-- Uncomment if you need fast queries on specific frontmatter properties
-- for large kilns (>10K documents)

/*
DEFINE TABLE metadata_index SCHEMAFULL;

-- Reference to source note
DEFINE FIELD note ON TABLE metadata_index
    TYPE record<notes>
    ASSERT $value != NONE;

-- Property key-value
DEFINE FIELD key ON TABLE metadata_index
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD value ON TABLE metadata_index
    TYPE string;

DEFINE FIELD value_type ON TABLE metadata_index
    TYPE string;  -- "string", "number", "boolean", "date"

-- Typed values (for performance)
DEFINE FIELD value_number ON TABLE metadata_index
    TYPE option<float>;

DEFINE FIELD value_date ON TABLE metadata_index
    TYPE option<datetime>;

DEFINE FIELD value_bool ON TABLE metadata_index
    TYPE option<bool>;

-- Indexes
DEFINE INDEX key_value ON TABLE metadata_index
    COLUMNS key, value;

DEFINE INDEX note_key ON TABLE metadata_index
    COLUMNS note, key;
*/

-- ============================================================================
-- HELPER FUNCTIONS (optional convenience functions)
-- ============================================================================

-- Function to calculate cosine similarity between embeddings
DEFINE FUNCTION fn::cosine_similarity($a: array<float>, $b: array<float>) {
    RETURN vector::distance::cosine($a, $b);
};

-- Function to normalize tag names (lowercase, remove #)
DEFINE FUNCTION fn::normalize_tag($tag: string) {
    RETURN string::lowercase(string::trim(string::replace($tag, "#", "")));
};

-- Function to extract folder from path
DEFINE FUNCTION fn::get_folder($path: string) {
    LET $parts = string::split($path, "/");
    RETURN IF array::len($parts) > 1 THEN $parts[0] ELSE "";
};

-- Function to get file extension
DEFINE FUNCTION fn::get_extension($path: string) {
    LET $parts = string::split($path, ".");
    RETURN IF array::len($parts) > 1 THEN array::last($parts) ELSE "";
};

-- ============================================================================
-- EVENTS (optional triggers for auto-updates)
-- ============================================================================

-- Update modified_at timestamp on content changes
DEFINE EVENT update_modified_at ON TABLE notes WHEN $before.content != $after.content THEN (
    UPDATE $after.id SET modified_at = time::now()
);

-- Sync content to content_text for full-text search
DEFINE EVENT sync_content_text ON TABLE notes WHEN $before.content != $after.content THEN (
    UPDATE $after.id SET content_text = $after.content
);

-- Sync title to title_text for full-text search
DEFINE EVENT sync_title_text ON TABLE notes WHEN $before.title != $after.title THEN (
    UPDATE $after.id SET title_text = $after.title
);

-- ============================================================================
-- PERMISSIONS (optional access control)
-- ============================================================================

-- For now, allow all access (adjust based on your security model)
-- Uncomment and customize if you need user-based permissions

/*
DEFINE SCOPE user SESSION 24h;

-- Notes permissions
DEFINE PERMISSION read ON TABLE notes FOR select WHERE true;
DEFINE PERMISSION create ON TABLE notes FOR create WHERE true;
DEFINE PERMISSION update ON TABLE notes FOR update WHERE true;
DEFINE PERMISSION delete ON TABLE notes FOR delete WHERE true;

-- Tags permissions
DEFINE PERMISSION read ON TABLE tags FOR select WHERE true;
DEFINE PERMISSION create ON TABLE tags FOR create WHERE true;
DEFINE PERMISSION update ON TABLE tags FOR update WHERE true;

-- Relations permissions
DEFINE PERMISSION read ON TABLE wikilink FOR select WHERE true;
DEFINE PERMISSION create ON TABLE wikilink FOR create WHERE true;
DEFINE PERMISSION delete ON TABLE wikilink FOR delete WHERE true;

DEFINE PERMISSION read ON TABLE embeds FOR select WHERE true;
DEFINE PERMISSION create ON TABLE embeds FOR create WHERE true;
DEFINE PERMISSION delete ON TABLE embeds FOR delete WHERE true;
*/

-- ============================================================================
-- TABLE: file_state
-- ============================================================================
-- Stores file state for pipeline change detection (Phase 1 quick filter)
-- This is a cache table - can be cleared and rebuilt from source files

DEFINE TABLE file_state SCHEMAFULL;

-- Relative path from kiln root (used as primary lookup key)
DEFINE FIELD relative_path ON TABLE file_state
    TYPE string
    ASSERT $value != NONE;

-- BLAKE3 hash of file content
DEFINE FIELD file_hash ON TABLE file_state
    TYPE string
    ASSERT $value != NONE;

-- File modification time (stored as datetime)
DEFINE FIELD modified_time ON TABLE file_state
    TYPE datetime
    ASSERT $value != NONE;

-- File size in bytes
DEFINE FIELD file_size ON TABLE file_state
    TYPE int
    ASSERT $value != NONE;

-- When this state was recorded (for debugging/auditing)
DEFINE FIELD updated_at ON TABLE file_state
    TYPE datetime
    DEFAULT time::now();

-- Indexes for fast lookups
DEFINE INDEX file_state_path_idx ON TABLE file_state COLUMNS relative_path UNIQUE;
DEFINE INDEX file_state_hash_idx ON TABLE file_state COLUMNS file_hash;

-- ============================================================================
-- END OF SCHEMA
-- ============================================================================
