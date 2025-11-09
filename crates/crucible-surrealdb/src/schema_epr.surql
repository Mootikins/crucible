-- ============================================================================
-- Crucible Knowledge Kiln - Entity/Property/Relation Schema
-- ============================================================================
-- Version: 0.2.0 (experimental)
-- Created: 2025-11-08
-- Updated: 2025-11-08
-- Purpose: Flexible, plugin-ready storage model for all entities, metadata,
--          relations, blocks, embeddings, and tags.
--
-- Changelog v0.2.0:
-- - Renamed entity_type → type (default: "note")
-- - Added properties.value_json for complex metadata
-- - Added relations.context for breadcrumbs/hash references
-- - Added entity_tags.created_at for provenance tracking
-- ============================================================================

-- ============================================================================
-- TABLE: entities
-- ============================================================================
-- Universal catalog for every thing tracked by Crucible

DEFINE TABLE entities SCHEMAFULL;

DEFINE FIELD id ON TABLE entities
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD type ON TABLE entities
    TYPE string
    DEFAULT "note"
    ASSERT $value IN ["note", "block", "tag", "section", "media", "person"];

DEFINE FIELD created_at ON TABLE entities
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE entities
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD deleted_at ON TABLE entities
    TYPE option<datetime>;

DEFINE FIELD version ON TABLE entities
    TYPE int
    DEFAULT 1;

DEFINE FIELD content_hash ON TABLE entities
    TYPE option<string>;

DEFINE FIELD created_by ON TABLE entities
    TYPE option<string>;

DEFINE FIELD vault_id ON TABLE entities
    TYPE option<string>;

DEFINE FIELD data ON TABLE entities
    TYPE option<object>;

DEFINE FIELD search_text ON TABLE entities
    TYPE option<string>
    FLEXIBLE;

DEFINE INDEX unique_entity_id ON TABLE entities COLUMNS id UNIQUE;
DEFINE INDEX entity_type_idx ON TABLE entities COLUMNS type;
DEFINE INDEX content_hash_idx ON TABLE entities COLUMNS content_hash;

-- ============================================================================
-- TABLE: properties
-- ============================================================================
-- Extensible metadata using namespace/key/value triples

DEFINE TABLE properties SCHEMAFULL;

DEFINE FIELD entity_id ON TABLE properties
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD namespace ON TABLE properties
    TYPE string
    DEFAULT "core";

DEFINE FIELD key ON TABLE properties
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD value ON TABLE properties
    TYPE string;

DEFINE FIELD value_type ON TABLE properties
    TYPE string
    ASSERT $value IN ["text", "number", "boolean", "date", "json"];

DEFINE FIELD value_text ON TABLE properties
    TYPE option<string>;

DEFINE FIELD value_number ON TABLE properties
    TYPE option<float>;

DEFINE FIELD value_bool ON TABLE properties
    TYPE option<bool>;

DEFINE FIELD value_date ON TABLE properties
    TYPE option<datetime>;

DEFINE FIELD value_json ON TABLE properties
    TYPE option<object>;

DEFINE FIELD source ON TABLE properties
    TYPE string
    DEFAULT "parser";

DEFINE FIELD confidence ON TABLE properties
    TYPE float
    DEFAULT 1.0;

DEFINE FIELD created_at ON TABLE properties
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE properties
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX entity_namespace_key_idx ON TABLE properties
    COLUMNS entity_id, namespace, key;

DEFINE INDEX namespace_key_idx ON TABLE properties
    COLUMNS namespace, key;

-- ============================================================================
-- TABLE: relations
-- ============================================================================
-- Typed, directed graph edges between entities

DEFINE TABLE relations SCHEMAFULL
    TYPE RELATION
    FROM entities TO entities;

DEFINE FIELD relation_type ON TABLE relations
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD weight ON TABLE relations
    TYPE float
    DEFAULT 1.0;

DEFINE FIELD directed ON TABLE relations
    TYPE bool
    DEFAULT true;

DEFINE FIELD confidence ON TABLE relations
    TYPE float
    DEFAULT 1.0;

DEFINE FIELD source ON TABLE relations
    TYPE string
    DEFAULT "parser";

DEFINE FIELD position ON TABLE relations
    TYPE option<int>;

DEFINE FIELD metadata ON TABLE relations
    TYPE object
    DEFAULT {};

DEFINE FIELD context ON TABLE relations
    TYPE option<string>;

DEFINE FIELD created_at ON TABLE relations
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX relation_type_idx ON TABLE relations COLUMNS relation_type;
DEFINE INDEX relation_source_idx ON TABLE relations COLUMNS source;
DEFINE INDEX relation_from_idx ON TABLE relations COLUMNS in, relation_type;
DEFINE INDEX relation_to_idx ON TABLE relations COLUMNS out, relation_type;

-- ============================================================================
-- TABLE: blocks
-- ============================================================================
-- AST blocks for section-aware Merkle trees and diffing

DEFINE TABLE blocks SCHEMAFULL;

DEFINE FIELD id ON TABLE blocks
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD entity_id ON TABLE blocks
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD block_index ON TABLE blocks
    TYPE int;

DEFINE FIELD block_type ON TABLE blocks
    TYPE string;

DEFINE FIELD content ON TABLE blocks
    TYPE string;

DEFINE FIELD content_hash ON TABLE blocks
    TYPE string;

DEFINE FIELD start_offset ON TABLE blocks
    TYPE option<int>;

DEFINE FIELD end_offset ON TABLE blocks
    TYPE option<int>;

DEFINE FIELD start_line ON TABLE blocks
    TYPE option<int>;

DEFINE FIELD end_line ON TABLE blocks
    TYPE option<int>;

DEFINE FIELD parent_block_id ON TABLE blocks
    TYPE option<record<blocks>>;

DEFINE FIELD depth ON TABLE blocks
    TYPE option<int>;

DEFINE FIELD metadata ON TABLE blocks
    TYPE object
    DEFAULT {};

DEFINE FIELD created_at ON TABLE blocks
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE blocks
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX block_lookup_idx ON TABLE blocks
    COLUMNS entity_id, block_index;

DEFINE INDEX block_hash_idx ON TABLE blocks
    COLUMNS content_hash;

-- ============================================================================
-- TABLE: embeddings
-- ============================================================================

DEFINE TABLE embeddings SCHEMAFULL;

DEFINE FIELD entity_id ON TABLE embeddings
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD block_id ON TABLE embeddings
    TYPE option<record<blocks>>;

DEFINE FIELD embedding ON TABLE embeddings
    TYPE array<float>;

DEFINE FIELD dimensions ON TABLE embeddings
    TYPE int;

DEFINE FIELD model ON TABLE embeddings
    TYPE string;

DEFINE FIELD model_version ON TABLE embeddings
    TYPE string;

DEFINE FIELD content_used ON TABLE embeddings
    TYPE string;

DEFINE FIELD created_at ON TABLE embeddings
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX embedding_unique_idx ON TABLE embeddings
    COLUMNS entity_id, model UNIQUE;

DEFINE INDEX embedding_vector_384 ON TABLE embeddings
    COLUMNS embedding
    MTREE DIMENSION 384 DISTANCE COSINE;

-- ============================================================================
-- TABLE: tags
-- ============================================================================

DEFINE TABLE tags SCHEMAFULL;

DEFINE FIELD id ON TABLE tags
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD name ON TABLE tags
    TYPE string
    ASSERT $value != NONE;

DEFINE FIELD parent_id ON TABLE tags
    TYPE option<record<tags>>;

DEFINE FIELD path ON TABLE tags
    TYPE string;

DEFINE FIELD depth ON TABLE tags
    TYPE int
    DEFAULT 0;

DEFINE FIELD description ON TABLE tags
    TYPE option<string>;

DEFINE FIELD color ON TABLE tags
    TYPE option<string>;

DEFINE FIELD icon ON TABLE tags
    TYPE option<string>;

DEFINE INDEX tag_name_unique ON TABLE tags COLUMNS name UNIQUE;
DEFINE INDEX tag_path_idx ON TABLE tags COLUMNS path;

-- ============================================================================
-- TABLE: entity_tags
-- ============================================================================

DEFINE TABLE entity_tags SCHEMAFULL;

DEFINE FIELD entity_id ON TABLE entity_tags
    TYPE record<entities>
    ASSERT $value != NONE;

DEFINE FIELD tag_id ON TABLE entity_tags
    TYPE record<tags>
    ASSERT $value != NONE;

DEFINE FIELD source ON TABLE entity_tags
    TYPE string
    DEFAULT "parser";

DEFINE FIELD confidence ON TABLE entity_tags
    TYPE float
    DEFAULT 1.0;

DEFINE FIELD created_at ON TABLE entity_tags
    TYPE datetime
    DEFAULT time::now();

DEFINE INDEX entity_tag_unique ON TABLE entity_tags
    COLUMNS entity_id, tag_id UNIQUE;
