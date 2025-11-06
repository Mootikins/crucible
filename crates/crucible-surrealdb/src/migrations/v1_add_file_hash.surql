-- Migration v1: Add file_hash column to notes table
-- This migration adds file-level change detection support
-- Created: 2025-11-05

-- Add file_hash field to notes table (if it doesn't already exist)
-- Using DEFINE FIELD with FLEXIBLE allows the field to be added to existing records
DEFINE FIELD file_hash ON TABLE notes
    TYPE option<string>
    FLEXIBLE;

-- Add index for file_hash for efficient change detection lookups
DEFINE INDEX file_hash_idx ON TABLE notes
    COLUMNS file_hash;

-- Add validation constraint to ensure file_hash is always a valid 64-character hex string when present
DEFINE FIELD file_hash ON TABLE notes
    ASSERT $value = NONE OR (type::is::string($value) AND string::len($value) = 64);

-- Update metadata to reflect schema version
UPDATE metadata:system SET
    schema_version = 1,
    updated_at = time::now()
WHERE schema_version < 1 OR schema_version = NONE;