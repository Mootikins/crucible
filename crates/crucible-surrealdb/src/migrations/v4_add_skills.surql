-- ============================================================================
-- Migration: v4_add_skills
-- ============================================================================
-- Adds agent skills storage
-- Version: 4.0.0
-- Created: 2025-12-22
-- ============================================================================

DEFINE TABLE skills SCHEMAFULL;

-- Identity
DEFINE FIELD name ON TABLE skills
    TYPE string
    ASSERT $value != NONE AND string::len($value) >= 1 AND string::len($value) <= 64;

DEFINE FIELD description ON TABLE skills
    TYPE string
    ASSERT $value != NONE AND string::len($value) >= 1;

-- Source tracking
DEFINE FIELD scope ON TABLE skills
    TYPE string
    ASSERT $value IN ["personal", "workspace", "kiln"];

DEFINE FIELD source_path ON TABLE skills
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD source_agent ON TABLE skills
    TYPE option<string>;

DEFINE FIELD content_hash ON TABLE skills
    TYPE string
    ASSERT $value != NONE;

-- Content
DEFINE FIELD body ON TABLE skills
    TYPE string
    DEFAULT "";

DEFINE FIELD license ON TABLE skills
    TYPE option<string>;

DEFINE FIELD compatibility ON TABLE skills
    TYPE option<string>;

DEFINE FIELD allowed_tools ON TABLE skills
    TYPE array<string>
    DEFAULT [];

DEFINE FIELD metadata ON TABLE skills
    TYPE object
    DEFAULT {};

-- Embedding for semantic search
DEFINE FIELD embedding ON TABLE skills
    TYPE option<array<float>>;

DEFINE FIELD embedding_model ON TABLE skills
    TYPE option<string>;

-- Shadow tracking
DEFINE FIELD shadowed_paths ON TABLE skills
    TYPE array<string>
    DEFAULT [];

-- Timestamps
DEFINE FIELD indexed_at ON TABLE skills
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE skills
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX skills_name_scope_idx ON TABLE skills
    COLUMNS name, scope
    UNIQUE;

DEFINE INDEX skills_scope_idx ON TABLE skills
    COLUMNS scope;

DEFINE INDEX skills_content_hash_idx ON TABLE skills
    COLUMNS content_hash;

-- ============================================================================
-- Update schema version
-- ============================================================================

UPDATE metadata:system SET
    schema_version = 4,
    updated_at = time::now()
WHERE schema_version < 4;
