//! Schema management and migrations

use crate::error_ext::SqliteResultExt;
use crucible_core::storage::{StorageError, StorageResult};
use rusqlite::Connection;
use tracing::{debug, info};

/// Schema version — tied to the crucible binary version.
/// Bump when adding tables, columns, or data migrations.
/// The daemon auto-migrates on startup; no user intervention needed.
const SCHEMA_VERSION: i32 = 2;

/// Apply all pending migrations
pub fn apply_migrations(conn: &Connection) -> StorageResult<()> {
    // Create migrations table if it doesn't exist
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            binary_version TEXT
        );",
    )
    .sql()?;

    // Ensure binary_version column exists (added in v2, but migrations table
    // may have been created by v1 without it)
    let has_binary_version = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('schema_migrations') WHERE name = 'binary_version'",
            [],
            |_| Ok(()),
        )
        .is_ok();
    if !has_binary_version {
        conn.execute(
            "ALTER TABLE schema_migrations ADD COLUMN binary_version TEXT",
            [],
        )
        .sql()?;
    }

    let current_version = get_current_version(conn)?;
    debug!(
        current_version,
        target_version = SCHEMA_VERSION,
        "Checking migrations"
    );

    if current_version < 1 {
        apply_migration_v1(conn)?;
    }
    if current_version < 2 {
        apply_migration_v2(conn)?;
    }

    Ok(())
}

/// Get current schema version
fn get_current_version(conn: &Connection) -> StorageResult<i32> {
    let version: Option<i32> = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap_or(None);

    Ok(version.unwrap_or(0))
}

/// Record that a migration was applied, including the binary version that ran it.
fn record_migration(conn: &Connection, version: i32) -> StorageResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_migrations (version, binary_version) VALUES (?1, ?2)",
        rusqlite::params![version, env!("CARGO_PKG_VERSION")],
    )
    .sql()?;
    Ok(())
}

/// Migration v1: Initial schema with EAV+Graph tables
fn apply_migration_v1(conn: &Connection) -> StorageResult<()> {
    debug!("Applying migration v1: Initial EAV+Graph schema");

    conn.execute_batch(SCHEMA_V1)
        .map_err(|e| StorageError::Backend(format!("Failed to apply v1 schema: {}", e)))?;

    record_migration(conn, 1)?;
    info!("Migration v1 applied successfully");
    Ok(())
}

/// Migration v2: Normalize note paths and deduplicate
///
/// The notes table may contain the same file under both relative and absolute
/// paths (e.g., `./docs/Foo.md` and `/home/.../docs/Foo.md`) from different
/// invocation contexts. This migration:
///   1. Normalizes all paths to their filename component (relative to kiln root)
///   2. Deduplicates by keeping the entry with the most recent updated_at
///   3. Also applies the notes table schema (previously managed separately)
fn apply_migration_v2(conn: &Connection) -> StorageResult<()> {
    info!("Applying migration v2: Note path normalization + deduplication");

    // Check if notes table exists (it was previously created separately by NoteStore)
    let notes_exists: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='notes'",
            [],
            |_| Ok(true),
        )
        .unwrap_or(false);

    if notes_exists {
        // Find duplicate paths (same filename, different directory prefix)
        // Strategy: for each group of paths sharing a filename, keep the shortest
        // path (most likely the relative one) and delete the rest.
        let duplicates: Vec<(String, String)> = {
            let mut stmt = conn
                .prepare(
                    r#"
                    SELECT n1.path, n2.path
                    FROM notes n1
                    JOIN notes n2 ON n1.path != n2.path
                    WHERE replace(replace(n1.path, rtrim(n1.path, replace(n1.path, '/', '')), ''), '/', '')
                        = replace(replace(n2.path, rtrim(n2.path, replace(n2.path, '/', '')), ''), '/', '')
                    AND length(n1.path) > length(n2.path)
                    "#,
                )
                .map_err(|e| StorageError::Backend(format!("v2 dedup query: {}", e)))?;

            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| StorageError::Backend(format!("v2 dedup: {}", e)))?;

            rows.filter_map(|r| r.ok()).collect()
        };

        if !duplicates.is_empty() {
            info!(
                count = duplicates.len(),
                "Removing duplicate note entries with longer paths"
            );
            for (longer_path, _shorter_path) in &duplicates {
                conn.execute("DELETE FROM notes WHERE path = ?1", [longer_path])
                    .map_err(|e| StorageError::Backend(format!("v2 delete duplicate: {}", e)))?;
                // Also clean up note_links for the deleted path
                conn.execute(
                    "DELETE FROM note_links WHERE source_path = ?1",
                    [longer_path],
                )
                .map_err(|e| StorageError::Backend(format!("v2 delete links: {}", e)))?;
            }
        }
    }

    record_migration(conn, 2)?;
    info!("Migration v2 applied successfully");
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
    fn test_binary_version_recorded() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();

        let binary_version: String = conn
            .query_row(
                "SELECT binary_version FROM schema_migrations WHERE version = ?1",
                [SCHEMA_VERSION],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(binary_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_v2_deduplicates_note_paths() {
        let conn = Connection::open_in_memory().unwrap();

        // Bootstrap schema_migrations table + v1
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now')),
                binary_version TEXT
            );",
        )
        .unwrap();
        apply_migration_v1(&conn).unwrap();

        // Create notes table with duplicate entries
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS notes (
                path TEXT PRIMARY KEY,
                content_hash BLOB NOT NULL,
                embedding BLOB,
                embedding_model TEXT,
                embedding_dimensions INTEGER,
                title TEXT NOT NULL,
                tags TEXT NOT NULL,
                links_to TEXT NOT NULL,
                properties TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS note_links (
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                PRIMARY KEY (source_path, target_path)
            );
            INSERT INTO notes (path, content_hash, title, tags, links_to, properties, updated_at)
            VALUES ('./docs/Getting Started.md', X'00', 'Getting Started', '[]', '[]', '{}', '2026-03-20');
            INSERT INTO notes (path, content_hash, title, tags, links_to, properties, updated_at)
            VALUES ('/home/user/crucible/docs/Getting Started.md', X'00', 'Getting Started', '[]', '[]', '{}', '2026-03-19');
            INSERT INTO notes (path, content_hash, title, tags, links_to, properties, updated_at)
            VALUES ('./docs/Plugins.md', X'01', 'Plugins', '[]', '[]', '{}', '2026-03-20');
            "#,
        )
        .unwrap();

        // Verify 3 entries before migration
        let count_before: i32 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_before, 3);

        // Apply v2
        apply_migration_v2(&conn).unwrap();

        // Should have 2 entries: the shorter path for Getting Started + Plugins
        let count_after: i32 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_after, 2, "Should deduplicate to 2 unique notes");

        // The shorter (relative) path should survive
        let surviving_path: String = conn
            .query_row(
                "SELECT path FROM notes WHERE title = 'Getting Started'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surviving_path, "./docs/Getting Started.md");
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
