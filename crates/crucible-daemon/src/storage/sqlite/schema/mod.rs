//! Schema management and migrations — the **only** owner of DDL for the kiln
//! database.
//!
//! Every `CREATE`, `ALTER` and `DROP` against `<kiln>/.crucible/crucible-sqlite.db`
//! runs from the numbered ladder in [`apply_migrations`] and is recorded in
//! `schema_migrations`. The DDL *constants* still live next to the logic that
//! owns each table's shape (`note_store.rs`, `link_index.rs`, `fts.rs`); only
//! their *execution* is here.
//!
//! To add a column, follow the procedure in
//! `docs/Meta/Analysis/Storage Schema.md` — it is six steps and the first one
//! (classify the table as derived or canonical) is the one that matters.

use crate::storage::sqlite::error_ext::SqliteResultExt;
use crucible_core::storage::{StorageError, StorageResult};
use rusqlite::Connection;
use tracing::{debug, info};

/// Schema version — tied to the crucible binary version.
/// Bump when adding tables, columns, or data migrations.
/// The daemon auto-migrates on startup; no user intervention needed.
const SCHEMA_VERSION: i32 = 6;

/// Tables the daemon may drop and recreate because their contents are a
/// function of files on disk. Anything NOT listed here holds data with no
/// other copy; a migration touching it must preserve rows.
///
/// `properties` is deliberately absent: `cru.storage` is its only writer and
/// markdown is not its source, so the database is the only copy. See
/// `docs/Meta/Analysis/Storage Schema.md`.
///
/// `notes` is listed, but "rebuildable" means *reparse from markdown*, not
/// *truncate*: it carries `embedding`, `embedding_model` and
/// `embedding_dimensions`, which are recoverable only by re-paying an
/// embedding provider.
pub(crate) const DERIVED_TABLES: &[&str] = &["notes", "notes_fts", "note_links"];

/// What a migration run needs to tell its caller. Grows only when a migration
/// genuinely cannot finish its own job.
#[derive(Debug, Default, Clone)]
pub struct MigrationOutcome {
    /// A derived-table rebuild the pipeline must perform, because the data is
    /// only recoverable from the markdown on disk. Set when the `note_links`
    /// v1→v2 step dropped span-less legacy rows, or when a kiln has notes and
    /// an empty link index.
    pub needs_link_reindex: bool,
}

/// Apply all pending migrations
pub fn apply_migrations(conn: &Connection) -> StorageResult<MigrationOutcome> {
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

    let mut outcome = MigrationOutcome::default();

    if current_version < 1 {
        apply_migration_v1(conn)?;
    }
    if current_version < 2 {
        apply_migration_v2(conn)?;
    }
    if current_version < 3 {
        apply_migration_v3(conn)?;
    }
    if current_version < 4 {
        apply_migration_v4(conn)?;
    }
    apply_migration_v5(conn, current_version, &mut outcome)?;
    if current_version < 6 {
        apply_migration_v6(conn)?;
    }

    Ok(outcome)
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

/// Migration v1: the `entities` catalog and the `properties` table
///
/// Named for history rather than content: it once created six tables, four of
/// which nothing ever wrote. See the note inside `SCHEMA_V1`.
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
///
/// The `sqlite_master` probe below is why v4 has to come *after* this step:
/// when this migration was written, `notes` was created by a separate owner
/// that ran later, so on a fresh database there was nothing to deduplicate.
/// v4 now creates `notes`, and keeping it below v2 preserves that meaning.
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

/// Migration v3: Drop the `properties` → `entities` foreign key
///
/// `properties.entity_id` was declared `REFERENCES entities(id) ON DELETE
/// CASCADE`, but nothing in production ever inserts into `entities`. The only
/// writer of `properties` is Lua's `cru.storage.set`, which supplies a
/// caller-chosen opaque id, so every plugin write failed with `FOREIGN KEY
/// constraint failed`. `entities` is vestigial — zero production reads, zero
/// production writes — so the constraint goes rather than the dead catalog
/// gaining a writer.
///
/// `properties` is the one canonical table in the kiln database: its values are
/// plugin-authored with no on-disk source, so they cannot be rebuilt from
/// markdown the way `notes` and `note_links` can. The rebuild therefore copies
/// inside a transaction and asserts the row counts match *before* anything is
/// dropped.
///
/// Rollback: an older binary sees a `properties` table with identical columns
/// and one constraint fewer — one it never satisfied and never needed.
fn apply_migration_v3(conn: &Connection) -> StorageResult<()> {
    // Shape-based rather than version-based, because `SCHEMA_V1` no longer
    // declares the foreign key: a database created fresh by this binary
    // already has the target shape and skips the round trip, while one created
    // by an older binary is rebuilt. Both converge.
    let entities_fk_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('properties') WHERE \"table\" = 'entities'",
            [],
            |row| row.get(0),
        )
        .sql()?;

    if entities_fk_count == 0 {
        debug!("Migration v3: properties has no FK into entities, nothing to rebuild");
        record_migration(conn, 3)?;
        return Ok(());
    }

    info!("Applying migration v3: rebuilding properties without the entities foreign key");

    // `SqlitePool::with_transaction` is unreachable here: the pool's mutex is
    // already held by the caller (`initialize` → `with_connection`), and
    // parking_lot's Mutex is not reentrant. This gives the same guarantee —
    // rollback on drop, commit only after the copy is verified.
    let tx = conn.unchecked_transaction().sql()?;

    // Counted inside the transaction, so the figure the copy is checked against
    // is the same one the copy read.
    let expected_rows: i64 = tx
        .query_row("SELECT COUNT(*) FROM properties", [], |row| row.get(0))
        .sql()?;

    tx.execute_batch(
        r#"
        DROP TABLE IF EXISTS properties_new;

        CREATE TABLE properties_new (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entity_id TEXT NOT NULL,
            namespace TEXT NOT NULL DEFAULT 'core',
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            source TEXT NOT NULL DEFAULT 'parser',
            confidence REAL NOT NULL DEFAULT 1.0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(entity_id, namespace, key)
        );

        INSERT INTO properties_new
            (id, entity_id, namespace, key, value, source, confidence, created_at, updated_at)
        SELECT id, entity_id, namespace, key, value, source, confidence, created_at, updated_at
        FROM properties;
        "#,
    )
    .map_err(|e| StorageError::Backend(format!("v3 properties copy: {}", e)))?;

    let copied_rows: i64 = tx
        .query_row("SELECT COUNT(*) FROM properties_new", [], |row| row.get(0))
        .sql()?;

    if copied_rows != expected_rows {
        // Dropping the transaction rolls back; `properties` is untouched.
        return Err(StorageError::Backend(format!(
            "v3 aborted: copied {} of {} properties rows; original table left intact",
            copied_rows, expected_rows
        )));
    }

    tx.execute_batch(
        r#"
        DROP TABLE properties;
        ALTER TABLE properties_new RENAME TO properties;
        CREATE INDEX IF NOT EXISTS idx_properties_entity ON properties(entity_id);
        CREATE INDEX IF NOT EXISTS idx_properties_namespace_key ON properties(namespace, key);
        "#,
    )
    .map_err(|e| StorageError::Backend(format!("v3 properties swap: {}", e)))?;

    record_migration(&tx, 3)?;
    tx.commit().sql()?;

    info!(
        rows = expected_rows,
        "Migration v3 applied successfully — properties rebuilt without the entities FK"
    );
    Ok(())
}

/// Migration v4: the `notes` table joins the ladder
///
/// `notes` used to be created by `SqliteNoteStore::apply_schema`, which ran
/// *after* `apply_migrations`. That is why v2 above has to probe
/// `sqlite_master` for the table it wants to deduplicate, and why no migration
/// could ever `ALTER TABLE notes`: the one place that records migrations ran
/// before the table existed.
///
/// Positioned after v1–v3 so v2's probe keeps its current meaning — on a fresh
/// database v2 still finds no `notes` table and still skips its dedup pass.
///
/// `notes` is derived (reparse the markdown), but it carries `embedding`,
/// `embedding_model` and `embedding_dimensions`, which are recoverable only by
/// re-paying an embedding provider. So this step is additive only: a
/// `CREATE TABLE IF NOT EXISTS` plus the two idempotent column repairs an
/// older database needs.
fn apply_migration_v4(conn: &Connection) -> StorageResult<()> {
    debug!("Applying migration v4: notes table + embedding metadata columns");

    conn.execute_batch(crate::storage::sqlite::note_store::NOTES_SCHEMA)
        .map_err(|e| StorageError::Backend(format!("v4 notes schema: {}", e)))?;
    ensure_embedding_metadata_columns(conn)?;

    record_migration(conn, 4)?;
    info!("Migration v4 applied successfully");
    Ok(())
}

/// Add the embedding metadata columns if an older database lacks them.
///
/// Idempotent by probe rather than by `IF NOT EXISTS`, which SQLite's
/// `ALTER TABLE ADD COLUMN` does not support. Additive and nullable, so a
/// downgraded binary still reads the table.
fn ensure_embedding_metadata_columns(conn: &Connection) -> StorageResult<()> {
    for column in ["embedding_model", "embedding_dimensions"] {
        let exists = conn
            .query_row(
                "SELECT 1 FROM pragma_table_info('notes') WHERE name = ?1",
                [column],
                |_| Ok(()),
            )
            .is_ok();
        if !exists {
            let sql_type = if column == "embedding_dimensions" {
                "INTEGER"
            } else {
                "TEXT"
            };
            conn.execute(
                &format!("ALTER TABLE notes ADD COLUMN {column} {sql_type}"),
                [],
            )
            .sql()?;
        }
    }
    Ok(())
}

/// Migration v5: the resolved-link index (`note_links` v2) joins the ladder
///
/// Two things happen here and only one of them is a migration:
///
/// 1. The DDL — and, for a database still carrying the v1 raw-text table, the
///    drop-and-recreate. That is recorded as v5. Dropping is legitimate
///    because `note_links` is derived: v1 rows have no spans and cannot be
///    upgraded in place, so the index is rebuilt from the markdown on disk.
/// 2. The answer to "does this kiln owe a relink pass?", which is **not** a
///    one-time question and therefore is not behind the version gate.
///    `ensure_note_links_v2` is idempotent and costs two `COUNT(*)`s; a kiln
///    whose relink pass was interrupted has notes and an empty index, and
///    change detection never reprocesses unchanged files, so a version-gated
///    check would leave it empty forever. `apply_schema` ran this on every
///    open before the ladder owned the DDL, and that behaviour is preserved.
///
/// Ordering dependency: `ensure_note_links_v2` does
/// `SELECT COUNT(*) FROM notes`, so it must run after v4 creates `notes`.
/// Reorder the two and a fresh database throws.
fn apply_migration_v5(
    conn: &Connection,
    current_version: i32,
    outcome: &mut MigrationOutcome,
) -> StorageResult<()> {
    outcome.needs_link_reindex = crate::storage::sqlite::link_index::ensure_note_links_v2(conn)
        .map_err(|e| StorageError::Backend(format!("v5 note_links schema: {}", e)))?;

    if current_version < 5 {
        record_migration(conn, 5)?;
        info!("Migration v5 applied successfully");
    }
    Ok(())
}

/// Migration v6: the FTS5 index (`notes_fts`) joins the ladder
///
/// Was `FtsIndex::setup`, called from `create_kiln_storage` — the fourth DDL
/// owner. `notes_fts` is derived: `note_pipeline` re-reads each note body and
/// re-indexes it, and `kiln_manager`'s open path already backfills a kiln with
/// rows in `notes` and none here.
fn apply_migration_v6(conn: &Connection) -> StorageResult<()> {
    debug!("Applying migration v6: notes_fts virtual table");

    conn.execute_batch(crate::storage::sqlite::fts::NOTES_FTS_SCHEMA)
        .map_err(|e| StorageError::Backend(format!("v6 notes_fts schema: {}", e)))?;

    record_migration(conn, 6)?;
    info!("Migration v6 applied successfully");
    Ok(())
}

/// v1 DDL — the `entities` catalog and the canonical `properties` table.
const SCHEMA_V1: &str = r#"
-- ============================================================================
-- TABLE: entities
-- ============================================================================
-- Vestigial. Designed as a universal catalog for all tracked objects; nothing
-- in production has ever inserted or selected a row. Still created for one
-- release so a binary downgraded across migration v3 finds the foreign-key
-- target its `properties` table declares.

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
--
-- entity_id is deliberately NOT a foreign key into entities: nothing in
-- production writes that catalog, and the only writer of this table is Lua's
-- cru.storage.set, which supplies an opaque caller-chosen id. The FK that used
-- to be here made every plugin write fail; migration v3 removes it from
-- databases created before this change.

CREATE TABLE IF NOT EXISTS properties (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id TEXT NOT NULL,
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
-- REMOVED: relations, blocks, tags, entity_tags
-- ============================================================================
-- `SCHEMA_V1` used to create four more tables here. Production never read or
-- wrote a single row of any of them: `relations` and `blocks` had no SQL at
-- all outside the deleted graph-query subsystem's tests, and `entity_tags`
-- had none anywhere. Wikilinks live in `note_links`, tags live in
-- `notes.tags`, and block hashes live in the parser.
--
-- Existing kilns keep their empty copies: `SCHEMA_V1` runs only under
-- `current_version < 1`, so a database that already recorded v1 never
-- re-reads this constant. Dropping them from existing kilns is a later
-- release's migration, guarded on a zero row count.
--
-- `entities` above is in the same position — no production reads, no
-- production writes — and is deliberately still created. It stays one release
-- so a binary downgraded across migration v3 still finds the foreign-key
-- target its `properties` table declares. It joins the drop list once v3 has
-- shipped for a release.
"#;

#[cfg(test)]
mod tests;
