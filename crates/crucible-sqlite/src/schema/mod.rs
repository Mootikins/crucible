//! Schema management and migrations

use crate::error::{SqliteError, SqliteResult};
use rusqlite::Connection;
use tracing::{debug, info};

/// Schema version - increment when making schema changes
const SCHEMA_VERSION: i32 = 1;

/// Apply all pending migrations
pub fn apply_migrations(conn: &Connection) -> SqliteResult<()> {
    // Create migrations table if it doesn't exist
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current_version = get_current_version(conn)?;
    debug!(current_version, target_version = SCHEMA_VERSION, "Checking migrations");

    if current_version < SCHEMA_VERSION {
        info!(
            from = current_version,
            to = SCHEMA_VERSION,
            "Applying schema migrations"
        );
        apply_migration_v1(conn)?;
    }

    Ok(())
}

/// Get current schema version
fn get_current_version(conn: &Connection) -> SqliteResult<i32> {
    let version: Option<i32> = conn
        .query_row(
            "SELECT MAX(version) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(None);

    Ok(version.unwrap_or(0))
}

/// Record that a migration was applied
fn record_migration(conn: &Connection, version: i32) -> SqliteResult<()> {
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?)",
        [version],
    )?;
    Ok(())
}

/// Migration v1: Initial schema with EAV+Graph tables
fn apply_migration_v1(conn: &Connection) -> SqliteResult<()> {
    debug!("Applying migration v1: Initial EAV+Graph schema");

    conn.execute_batch(SCHEMA_V1).map_err(|e| {
        SqliteError::Schema(format!("Failed to apply v1 schema: {}", e))
    })?;

    record_migration(conn, 1)?;
    info!("Migration v1 applied successfully");
    Ok(())
}

/// Initial schema SQL
const SCHEMA_V1: &str = r#"
-- ============================================================================
-- TABLE: entities
-- ============================================================================
-- Universal catalog for all tracked objects

CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY NOT NULL,
    type TEXT NOT NULL CHECK (type IN ('note', 'block', 'tag', 'section', 'media', 'person')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    content_hash TEXT,
    created_by TEXT,
    vault_id TEXT,
    data TEXT  -- JSON blob for flexible data storage
);

CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(type);
CREATE INDEX IF NOT EXISTS idx_entities_content_hash ON entities(content_hash);
CREATE INDEX IF NOT EXISTS idx_entities_vault ON entities(vault_id);
CREATE INDEX IF NOT EXISTS idx_entities_deleted ON entities(deleted_at);

-- ============================================================================
-- TABLE: properties
-- ============================================================================
-- EAV-style extensible metadata with namespacing

CREATE TABLE IF NOT EXISTS properties (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL DEFAULT 'core',
    key TEXT NOT NULL,
    value TEXT NOT NULL,  -- JSON: {"type": "text", "value": "..."}
    source TEXT NOT NULL DEFAULT 'parser',
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(entity_id, namespace, key)
);

CREATE INDEX IF NOT EXISTS idx_properties_entity ON properties(entity_id);
CREATE INDEX IF NOT EXISTS idx_properties_namespace_key ON properties(namespace, key);

-- ============================================================================
-- TABLE: relations
-- ============================================================================
-- Typed directed graph edges (wikilinks, embeds, links)

CREATE TABLE IF NOT EXISTS relations (
    id TEXT PRIMARY KEY NOT NULL,
    from_entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_entity_id TEXT REFERENCES entities(id) ON DELETE SET NULL,
    relation_type TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1.0,
    directed INTEGER NOT NULL DEFAULT 1,
    confidence REAL NOT NULL DEFAULT 1.0,
    source TEXT NOT NULL DEFAULT 'parser',
    position INTEGER,
    metadata TEXT DEFAULT '{}',  -- JSON
    content_category TEXT CHECK (content_category IS NULL OR content_category IN (
        'note', 'image', 'video', 'audio', 'pdf', 'document',
        'other', 'web', 'youtube', 'github', 'wikipedia', 'stackoverflow', 'external'
    )),
    context TEXT,
    block_offset INTEGER,
    block_hash BLOB,  -- 32 bytes for BLAKE3
    heading_occurrence INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_relations_from ON relations(from_entity_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_relations_to ON relations(to_entity_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_relations_type ON relations(relation_type);

-- ============================================================================
-- TABLE: blocks
-- ============================================================================
-- AST sections for hierarchical content

CREATE TABLE IF NOT EXISTS blocks (
    id TEXT PRIMARY KEY NOT NULL,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    block_index INTEGER NOT NULL,
    block_type TEXT NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT,
    start_offset INTEGER,
    end_offset INTEGER,
    start_line INTEGER,
    end_line INTEGER,
    parent_block_id TEXT REFERENCES blocks(id) ON DELETE CASCADE,
    depth INTEGER,
    metadata TEXT DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_blocks_entity ON blocks(entity_id, block_index);
CREATE INDEX IF NOT EXISTS idx_blocks_hash ON blocks(content_hash);
CREATE INDEX IF NOT EXISTS idx_blocks_parent ON blocks(parent_block_id);

-- ============================================================================
-- TABLE: tags
-- ============================================================================
-- Hierarchical tag taxonomy

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    parent_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
    path TEXT NOT NULL,  -- Full path: "project/ai/agents"
    depth INTEGER NOT NULL DEFAULT 0,
    description TEXT,
    color TEXT,
    icon TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_tags_path ON tags(path);
CREATE INDEX IF NOT EXISTS idx_tags_parent ON tags(parent_id);

-- ============================================================================
-- TABLE: entity_tags
-- ============================================================================
-- Many-to-many entity-tag associations

CREATE TABLE IF NOT EXISTS entity_tags (
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    source TEXT NOT NULL DEFAULT 'parser',
    confidence REAL NOT NULL DEFAULT 1.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (entity_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_entity_tags_tag ON entity_tags(tag_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_applies_cleanly() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();

        // Verify version was recorded
        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_schema_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Apply twice - should not error
        apply_migrations(&conn).unwrap();
        apply_migrations(&conn).unwrap();

        let version = get_current_version(&conn).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn test_foreign_keys_work() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        apply_migrations(&conn).unwrap();

        // Insert an entity
        conn.execute(
            "INSERT INTO entities (id, type) VALUES ('test:1', 'note')",
            [],
        )
        .unwrap();

        // Insert a property referencing the entity
        conn.execute(
            "INSERT INTO properties (entity_id, namespace, key, value) VALUES ('test:1', 'core', 'title', '\"Test\"')",
            [],
        )
        .unwrap();

        // Delete the entity - should cascade to properties
        conn.execute("DELETE FROM entities WHERE id = 'test:1'", [])
            .unwrap();

        // Verify property was deleted
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM properties WHERE entity_id = 'test:1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
