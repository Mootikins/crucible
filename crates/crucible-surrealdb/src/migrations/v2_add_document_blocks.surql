-- ============================================================================
-- Migration: Add document_blocks table for content-addressed block storage
-- ============================================================================
-- Version: 2.0.0
-- Created: 2025-11-06
-- Purpose: Add document_blocks table to support block-level change detection
--          and content-addressed storage for deduplication
--
-- Usage:
--   surreal import --conn http://localhost:8000 --user root --pass root \
--     --ns crucible --db kiln migrations/v2_add_document_blocks.surql
-- ============================================================================

-- ============================================================================
-- TABLE: document_blocks
-- ============================================================================
-- Maps documents to their blocks for content-addressed storage and deduplication

DEFINE TABLE document_blocks SCHEMAFULL;

-- Reference to source document (using the document's file path as ID)
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

-- ============================================================================
-- INDEXES for efficient queries
-- ============================================================================

-- Primary lookup index: document + block index
DEFINE INDEX document_blocks_idx ON TABLE document_blocks
    COLUMNS document_id, block_index;

-- Hash-based lookup for deduplication
DEFINE INDEX block_hash_idx ON TABLE document_blocks
    COLUMNS block_hash;

-- Document-based queries
DEFINE INDEX document_id_idx ON TABLE document_blocks
    COLUMNS document_id;

-- Block type filtering
DEFINE INDEX block_type_idx ON TABLE document_blocks
    COLUMNS block_type;

-- Time-based queries
DEFINE INDEX created_at_idx ON TABLE document_blocks
    COLUMNS created_at;

-- Unique constraint to prevent duplicate blocks in same document
DEFINE INDEX unique_document_block ON TABLE document_blocks
    COLUMNS document_id, block_index
    UNIQUE;

-- ============================================================================
-- SCHEMA VERSION UPDATE
-- ============================================================================

-- Update schema version to 2
UPDATE metadata:system SET
    schema_version = 2,
    updated_at = time::now()
WHERE schema_version < 2;

-- ============================================================================
-- HELPER FUNCTIONS for block operations
-- ============================================================================

-- Function to find documents containing a specific block hash
DEFINE FUNCTION fn::documents_with_block($block_hash: string) {
    RETURN SELECT document_id FROM document_blocks WHERE block_hash = $block_hash;
};

-- Function to get all blocks for a document
DEFINE FUNCTION fn::document_blocks($document_id: string) {
    RETURN * FROM document_blocks WHERE document_id = $document_id ORDER BY block_index;
};

-- Function to get block content by hash
DEFINE FUNCTION fn::block_by_hash($block_hash: string) {
    RETURN * FROM document_blocks WHERE block_hash = $block_hash LIMIT 1;
};

-- Function to count duplicate blocks (for deduplication stats)
DEFINE FUNCTION fn::block_duplicate_count($block_hash: string) {
    RETURN count() FROM document_blocks WHERE block_hash = $block_hash;
};

-- ============================================================================
-- EVENTS for automatic updates
-- ============================================================================

-- Update updated_at timestamp on block content changes
DEFINE EVENT update_block_timestamp ON TABLE document_blocks
WHEN $before.block_content != $after.block_content THEN (
    UPDATE $after.id SET
        updated_at = time::now(),
        block_hash = $after.block_hash  -- Hash should be recalculated
);

-- ============================================================================
-- VIEWS for common queries
-- ============================================================================

-- View: Block deduplication statistics
DEFINE VIEW block_deduplication_stats AS
    SELECT
        block_hash,
        count() as duplicate_count,
        array::agg(document_id) as documents,
        min(created_at) as first_seen
    FROM document_blocks
    GROUP BY block_hash
    WHERE duplicate_count > 1;

-- View: Document block summary
DEFINE VIEW document_block_summary AS
    SELECT
        document_id,
        count() as total_blocks,
        array::distinct(block_type) as block_types,
        min(created_at) as first_block_created,
        max(updated_at) as last_block_updated
    FROM document_blocks
    GROUP BY document_id;

-- ============================================================================
-- END OF MIGRATION
-- ============================================================================