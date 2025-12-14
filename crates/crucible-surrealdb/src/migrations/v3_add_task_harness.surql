-- ============================================================================
-- Migration: v3_add_task_harness
-- ============================================================================
-- Adds task harness tables for long-running agent task management
-- Version: 3.0.0
-- Created: 2025-12-14
-- ============================================================================

-- ============================================================================
-- TABLE: task_files
-- ============================================================================
-- Stores TASKS.md file metadata and frontmatter

DEFINE TABLE task_files SCHEMAFULL;

-- File path (relative to workspace)
DEFINE FIELD path ON TABLE task_files
    TYPE string
    ASSERT $value != NONE;

-- Title from frontmatter
DEFINE FIELD title ON TABLE task_files
    TYPE option<string>;

-- Description from frontmatter
DEFINE FIELD description ON TABLE task_files
    TYPE option<string>;

-- Context files for agent prompts
DEFINE FIELD context_files ON TABLE task_files
    TYPE array<string>
    DEFAULT [];

-- Verification command
DEFINE FIELD verify ON TABLE task_files
    TYPE option<string>;

-- TDD mode flag
DEFINE FIELD tdd ON TABLE task_files
    TYPE bool
    DEFAULT false;

-- Additional frontmatter metadata
DEFINE FIELD metadata ON TABLE task_files
    TYPE object
    DEFAULT {};

-- File content hash for change detection
DEFINE FIELD file_hash ON TABLE task_files
    TYPE option<string>;

-- Timestamps
DEFINE FIELD created_at ON TABLE task_files
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD synced_at ON TABLE task_files
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX task_files_path_idx ON TABLE task_files
    COLUMNS path
    UNIQUE;

DEFINE INDEX task_files_hash_idx ON TABLE task_files
    COLUMNS file_hash;

-- ============================================================================
-- TABLE: tasks
-- ============================================================================
-- Stores individual task items

DEFINE TABLE tasks SCHEMAFULL;

-- Task ID from inline metadata (e.g., "1.1.1")
DEFINE FIELD task_id ON TABLE tasks
    TYPE string
    ASSERT $value != NONE;

-- Reference to parent task file
DEFINE FIELD task_file ON TABLE tasks
    TYPE record<task_files>
    ASSERT $value != NONE;

-- Task content/description
DEFINE FIELD content ON TABLE tasks
    TYPE string
    ASSERT $value != NONE;

-- Current status
DEFINE FIELD status ON TABLE tasks
    TYPE string
    ASSERT $value IN ["pending", "done", "in_progress", "cancelled", "blocked"];

-- Dependency IDs (task_ids)
DEFINE FIELD deps ON TABLE tasks
    TYPE array<string>
    DEFAULT [];

-- Additional inline metadata
DEFINE FIELD metadata ON TABLE tasks
    TYPE object
    DEFAULT {};

-- Timestamps
DEFINE FIELD created_at ON TABLE tasks
    TYPE datetime
    DEFAULT time::now();

DEFINE FIELD updated_at ON TABLE tasks
    TYPE datetime
    DEFAULT time::now();

-- Indexes
DEFINE INDEX tasks_task_id_idx ON TABLE tasks
    COLUMNS task_id;

DEFINE INDEX tasks_status_idx ON TABLE tasks
    COLUMNS status;

DEFINE INDEX tasks_file_idx ON TABLE tasks
    COLUMNS task_file;

-- Composite index for task lookup within file
DEFINE INDEX tasks_file_task_id_idx ON TABLE tasks
    COLUMNS task_file, task_id
    UNIQUE;

-- ============================================================================
-- RELATION: task_dependency
-- ============================================================================
-- Edge representing task→task dependencies

DEFINE TABLE task_dependency SCHEMAFULL
    TYPE RELATION
    FROM tasks TO tasks;

-- Dependency type
DEFINE FIELD dep_type ON TABLE task_dependency
    TYPE string
    DEFAULT "requires";

-- Creation timestamp
DEFINE FIELD created_at ON TABLE task_dependency
    TYPE datetime
    DEFAULT time::now();

-- Index for dependency queries
DEFINE INDEX task_dep_type_idx ON TABLE task_dependency
    COLUMNS dep_type;

-- ============================================================================
-- TABLE: task_history
-- ============================================================================
-- Records task status changes for audit trail

DEFINE TABLE task_history SCHEMAFULL;

-- Reference to the task
DEFINE FIELD task ON TABLE task_history
    TYPE record<tasks>
    ASSERT $value != NONE;

-- Status transition
DEFINE FIELD from_status ON TABLE task_history
    TYPE string
    ASSERT $value IN ["pending", "done", "in_progress", "cancelled", "blocked", "none"];

DEFINE FIELD to_status ON TABLE task_history
    TYPE string
    ASSERT $value IN ["pending", "done", "in_progress", "cancelled", "blocked"];

-- Actor who made the change
DEFINE FIELD actor ON TABLE task_history
    TYPE string
    ASSERT $value != NONE;

-- Optional reason
DEFINE FIELD reason ON TABLE task_history
    TYPE option<string>;

-- Timestamp
DEFINE FIELD timestamp ON TABLE task_history
    TYPE datetime
    DEFAULT time::now();

-- Indexes for history queries
DEFINE INDEX task_history_task_idx ON TABLE task_history
    COLUMNS task;

DEFINE INDEX task_history_timestamp_idx ON TABLE task_history
    COLUMNS timestamp;

DEFINE INDEX task_history_actor_idx ON TABLE task_history
    COLUMNS actor;

-- ============================================================================
-- HELPER FUNCTIONS
-- ============================================================================

-- Function to find ready tasks (all deps done)
DEFINE FUNCTION fn::ready_tasks($file_id: record<task_files>) {
    RETURN SELECT * FROM tasks
        WHERE task_file = $file_id
        AND status = "pending"
        AND (
            array::len(deps) = 0
            OR array::all(deps, |$dep| {
                (SELECT VALUE status FROM tasks WHERE task_file = $file_id AND task_id = $dep)[0] = "done"
            })
        );
};

-- ============================================================================
-- EVENTS
-- ============================================================================

-- Update timestamp on status change
DEFINE EVENT task_status_changed ON TABLE tasks
    WHEN $before.status != $after.status
    THEN (
        UPDATE $after.id SET updated_at = time::now()
    );

-- ============================================================================
-- Update schema version
-- ============================================================================

UPDATE metadata:system SET
    schema_version = 3,
    updated_at = time::now()
WHERE schema_version < 3;
