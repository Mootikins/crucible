//! Migration-ladder tests.
//!
//! Split out of `schema/mod.rs` to keep that file inside the 1000-line
//! module budget as the ladder grows.

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

    seed_schema_migrations(&conn);
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

/// The `properties` DDL as binaries up to 0.23.0 created it: `entity_id`
/// carried a foreign key into the `entities` catalog. Kept here, in tests
/// only, so the migration can be exercised against the shape it has to
/// migrate from.
const LEGACY_PROPERTIES_WITH_FK: &str = r#"
CREATE TABLE properties (
id INTEGER PRIMARY KEY AUTOINCREMENT,
entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
namespace TEXT NOT NULL DEFAULT 'core',
key TEXT NOT NULL,
value TEXT NOT NULL,
source TEXT NOT NULL DEFAULT 'parser',
confidence REAL NOT NULL DEFAULT 1.0,
created_at TEXT NOT NULL DEFAULT (datetime('now')),
updated_at TEXT NOT NULL DEFAULT (datetime('now')),
UNIQUE(entity_id, namespace, key)
);
CREATE INDEX idx_properties_entity ON properties(entity_id);
CREATE INDEX idx_properties_namespace_key ON properties(namespace, key);
"#;

/// Bring `conn` to the state an older binary would have left it in: schema
/// version 2, with the foreign-key-bearing `properties` table.
fn seed_legacy_v2_db(conn: &Connection) {
    seed_schema_migrations(conn);
    apply_migration_v1(conn).unwrap();
    conn.execute_batch("DROP TABLE properties;").unwrap();
    conn.execute_batch(LEGACY_PROPERTIES_WITH_FK).unwrap();
    record_migration(conn, 2).unwrap();

    let fks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('properties') WHERE \"table\" = 'entities'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fks, 1, "fixture must start from the FK-bearing shape");
}

#[tokio::test]
async fn a_kiln_from_an_older_binary_migrates_and_accepts_a_property_write() {
    use crate::storage::sqlite::{SqliteConfig, SqlitePool, SqlitePropertyStore};
    use crucible_core::storage::PropertyStore;

    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("kiln.db");

    // The old binary's database, closed before the new binary opens it.
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        seed_legacy_v2_db(&conn);
    }

    // The new binary opens it: pragmas, then migrations.
    let pool = SqlitePool::new(SqliteConfig::new(&db_path)).unwrap();
    let store = SqlitePropertyStore::new(pool);

    store
        .property_set("note:foo", "plugin:x", "k", "v")
        .await
        .unwrap();
    let val = store
        .property_get("note:foo", "plugin:x", "k")
        .await
        .unwrap();
    assert_eq!(val, Some("v".to_string()));
}

#[test]
fn migrating_off_the_foreign_key_preserves_existing_property_rows() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    seed_legacy_v2_db(&conn);

    // Rows an older binary could actually have written: the FK forced a
    // catalog row to exist first.
    conn.execute_batch(
        r#"
        INSERT INTO entities (id, type) VALUES ('note:a', 'note'), ('note:b', 'note');
        INSERT INTO properties (entity_id, namespace, key, value, source, confidence)
        VALUES ('note:a', 'plugin:x', 'k1', '"v1"', 'plugin', 0.5),
               ('note:a', 'core', 'k1', '"v2"', 'parser', 1.0),
               ('note:b', 'plugin:x', 'k1', '"v3"', 'plugin', 1.0);
        "#,
    )
    .unwrap();

    let rows_before = read_properties(&conn);
    assert_eq!(rows_before.len(), 3);

    apply_migrations(&conn).unwrap();

    assert_eq!(get_current_version(&conn).unwrap(), SCHEMA_VERSION);
    let fks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('properties') WHERE \"table\" = 'entities'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fks, 0, "the rebuilt table must carry no FK into entities");
    assert_eq!(
        read_properties(&conn),
        rows_before,
        "every column of every row must survive the rebuild"
    );

    // Both indexes came back.
    let indexes: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index' AND tbl_name = 'properties' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .unwrap();
        let rows = stmt.query_map([], |row| row.get(0)).unwrap();
        rows.filter_map(Result::ok).collect()
    };
    assert_eq!(
        indexes,
        vec!["idx_properties_entity", "idx_properties_namespace_key"]
    );
}

/// The rebuild's safety argument is that nothing is dropped until the copy
/// has been verified. Force the copy to fail — something already occupying
/// the scratch name, which `DROP TABLE IF EXISTS` cannot clear because it
/// is a view — and assert the original table and the recorded version are
/// both untouched.
#[test]
fn a_failed_rebuild_leaves_properties_and_the_schema_version_untouched() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    seed_legacy_v2_db(&conn);
    conn.execute_batch(
        r#"
        INSERT INTO entities (id, type) VALUES ('note:a', 'note');
        INSERT INTO properties (entity_id, namespace, key, value)
        VALUES ('note:a', 'plugin:x', 'k', '"v"');
        CREATE VIEW properties_new AS SELECT 1;
        "#,
    )
    .unwrap();

    let rows_before = read_properties(&conn);
    assert_eq!(rows_before.len(), 1);

    let err = apply_migration_v3(&conn).expect_err("the rebuild must fail");
    assert!(
        err.to_string().contains("v3 properties copy"),
        "unexpected error: {err}"
    );

    assert_eq!(read_properties(&conn), rows_before, "rows must survive");
    assert_eq!(
        get_current_version(&conn).unwrap(),
        2,
        "an aborted migration must not be recorded as applied"
    );
    let fks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('properties') WHERE \"table\" = 'entities'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fks, 1, "the original table, constraint and all, is intact");
}

/// Every column of every `properties` row, ordered, for equality checks.
/// Concatenated because the point is whole-row equality, not field access —
/// and every column is `NOT NULL`, so nothing can vanish into a NULL.
fn read_properties(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT id || '|' || entity_id || '|' || namespace || '|' || key || '|' ||
                    value || '|' || source || '|' || confidence || '|' ||
                    created_at || '|' || updated_at
             FROM properties ORDER BY id",
        )
        .unwrap();
    let rows = stmt.query_map([], |row| row.get(0)).unwrap();
    rows.filter_map(Result::ok).collect()
}

/// `entities` is a catalog nothing in production writes, so `properties`
/// must not depend on it. This replaces a test that asserted the opposite
/// (an `ON DELETE CASCADE` from `entities` into `properties`) — that
/// cascade was the bug, not the contract.
#[test]
fn properties_are_independent_of_the_entities_catalog() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    apply_migrations(&conn).unwrap();

    // No catalog row: an insert naming an unknown entity still succeeds.
    conn.execute(
        "INSERT INTO properties (entity_id, namespace, key, value)
         VALUES ('note:unknown', 'plugin:x', 'title', '\"Test\"')",
        [],
    )
    .unwrap();

    // And a property outlives the deletion of its would-be catalog row.
    conn.execute_batch(
        r#"
        INSERT INTO entities (id, type) VALUES ('note:known', 'note');
        INSERT INTO properties (entity_id, namespace, key, value)
        VALUES ('note:known', 'plugin:x', 'title', '"Test"');
        DELETE FROM entities WHERE id = 'note:known';
        "#,
    )
    .unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM properties", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

// ===========================================================================
// Single DDL ownership (Phase A of
// [[2026-08-11-dead-code-and-schema-migrations]])
//
// The kiln database used to have four DDL owners: `apply_migrations` here,
// `SqliteNoteStore::apply_schema`, `link_index::ensure_note_links_v2` and
// `FtsIndex::setup`. The tests below pin the property that fixed it — every
// table a kiln needs exists after `apply_migrations` **alone**, so a future
// migration can `ALTER TABLE notes` in the one place that records migrations.
// ===========================================================================

/// Create the bare `schema_migrations` ledger, as `apply_migrations` does on
/// its first line, so a fixture can record versions before calling it.
fn seed_schema_migrations(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now')),
            binary_version TEXT
        );",
    )
    .unwrap();
}

/// Column names of `table_name`, in declaration order.
fn columns_of(conn: &Connection, table_name: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
        .unwrap();
    let rows = stmt.query_map([table_name], |row| row.get(0)).unwrap();
    rows.filter_map(Result::ok).collect()
}

fn table_exists(conn: &Connection, table_name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE name = ?1 AND type IN ('table', 'view')",
        [table_name],
        |_| Ok(()),
    )
    .is_ok()
}

#[test]
fn a_fresh_database_has_every_kiln_table_after_migrations_alone() {
    let conn = Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();

    assert_eq!(
        columns_of(&conn, "notes"),
        vec![
            "path",
            "content_hash",
            "embedding",
            "embedding_model",
            "embedding_dimensions",
            "title",
            "tags",
            "links_to",
            "properties",
            "updated_at",
        ],
        "the notes table must be complete without SqliteNoteStore::apply_schema"
    );

    assert_eq!(
        columns_of(&conn, "note_links"),
        vec![
            "source_path",
            "resolved_target",
            "raw_target",
            "target_key",
            "span_start",
            "span_end",
            "kind",
            "is_ambiguous",
        ],
        "note_links v2 must exist without SqliteNoteStore::apply_schema"
    );

    assert!(
        table_exists(&conn, "notes_fts"),
        "the FTS5 virtual table must exist without FtsIndex::setup"
    );
}

#[test]
fn a_database_from_before_the_embedding_columns_gains_them_and_keeps_its_rows() {
    let conn = Connection::open_in_memory().unwrap();
    seed_schema_migrations(&conn);
    apply_migration_v1(&conn).unwrap();
    // The `notes` shape shipped before embedding metadata was recorded.
    conn.execute_batch(
        r#"
        CREATE TABLE notes (
            path TEXT PRIMARY KEY,
            content_hash BLOB NOT NULL,
            embedding BLOB,
            title TEXT NOT NULL,
            tags TEXT NOT NULL,
            links_to TEXT NOT NULL,
            properties TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        INSERT INTO notes (path, content_hash, embedding, title, tags, links_to, properties, updated_at)
        VALUES ('docs/A.md', X'0102', X'0000803f', 'A', '[]', '[]', '{}', '2026-08-11');
        "#,
    )
    .unwrap();
    record_migration(&conn, 3).unwrap();

    apply_migrations(&conn).unwrap();

    let cols = columns_of(&conn, "notes");
    assert!(cols.contains(&"embedding_model".to_string()), "{cols:?}");
    assert!(
        cols.contains(&"embedding_dimensions".to_string()),
        "{cols:?}"
    );

    // The costly column is derived-but-expensive: a migration must never
    // destroy it. Assert the byte pattern, not just the row count.
    let (path, embedding): (String, Vec<u8>) = conn
        .query_row("SELECT path, embedding FROM notes", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(path, "docs/A.md");
    assert_eq!(embedding, vec![0x00, 0x00, 0x80, 0x3f]);
}

#[test]
fn a_v1_note_links_table_is_replaced_and_the_outcome_asks_for_a_relink() {
    let conn = Connection::open_in_memory().unwrap();
    seed_schema_migrations(&conn);
    apply_migration_v1(&conn).unwrap();
    // v1 note_links: raw text, no `target_key`, no spans — unrecoverable in
    // place, so the ladder drops it and reports that a relink pass is owed.
    conn.execute_batch(
        r#"
        CREATE TABLE notes (
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
        CREATE TABLE note_links (
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            PRIMARY KEY (source_path, target_path)
        );
        INSERT INTO notes (path, content_hash, title, tags, links_to, properties, updated_at)
        VALUES ('docs/A.md', X'01', 'A', '[]', '["B"]', '{}', '2026-08-11');
        INSERT INTO note_links (source_path, target_path) VALUES ('docs/A.md', 'B');
        "#,
    )
    .unwrap();
    record_migration(&conn, 3).unwrap();

    let outcome = apply_migrations(&conn).unwrap();

    assert!(
        outcome.needs_link_reindex,
        "dropping v1 note_links leaves the index rebuildable only from disk, \
         so the ladder must say so"
    );
    assert!(columns_of(&conn, "note_links").contains(&"target_key".to_string()));
    // The note itself is untouched: only the derived link rows were dropped.
    let notes: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(notes, 1);
}

#[test]
fn a_fresh_database_owes_no_relink() {
    let conn = Connection::open_in_memory().unwrap();
    let outcome = apply_migrations(&conn).unwrap();
    assert!(
        !outcome.needs_link_reindex,
        "an empty kiln has nothing to relink"
    );
}

/// T-A6. The test that could not have passed before Phase A: `apply_migrations`
/// used to run *before* `notes` existed, so a migration step that alters
/// `notes` had nowhere to live. This runs a throwaway v7-style step to prove
/// the ladder can now do it, and is the guard against regressing to split
/// ownership.
#[test]
fn a_migration_step_can_alter_notes_on_an_existing_database() {
    let conn = Connection::open_in_memory().unwrap();
    apply_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO notes (path, content_hash, title, tags, links_to, properties, updated_at)
         VALUES ('docs/A.md', X'01', 'A', '[]', '[]', '{}', '2026-08-11')",
        [],
    )
    .unwrap();

    // Stand-in for the next real migration: additive column on a table the
    // ladder now owns.
    conn.execute("ALTER TABLE notes ADD COLUMN word_count INTEGER", [])
        .unwrap();
    record_migration(&conn, SCHEMA_VERSION + 1).unwrap();

    assert!(columns_of(&conn, "notes").contains(&"word_count".to_string()));
    let surviving: String = conn
        .query_row("SELECT path FROM notes", [], |row| row.get(0))
        .unwrap();
    assert_eq!(surviving, "docs/A.md");
    assert_eq!(get_current_version(&conn).unwrap(), SCHEMA_VERSION + 1);
}

/// T-A7. `DERIVED_TABLES` is a claim about which tables a migration may drop
/// and recreate. Keep it in sync with the DDL constants, and keep the one
/// canonical table out of it.
#[test]
fn derived_tables_each_appear_in_exactly_one_ddl_constant() {
    use crate::storage::sqlite::fts::NOTES_FTS_SCHEMA;
    use crate::storage::sqlite::link_index::NOTE_LINKS_V2_SCHEMA;
    use crate::storage::sqlite::note_store::NOTES_SCHEMA;

    let ddl = [
        ("NOTES_SCHEMA", NOTES_SCHEMA),
        ("NOTE_LINKS_V2_SCHEMA", NOTE_LINKS_V2_SCHEMA),
        ("NOTES_FTS_SCHEMA", NOTES_FTS_SCHEMA),
    ];

    for table in DERIVED_TABLES {
        let creators: Vec<&str> = ddl
            .iter()
            .filter(|(_, sql)| creates_table(sql, table))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            creators.len(),
            1,
            "derived table `{table}` must be created by exactly one DDL constant, found {creators:?}"
        );
    }

    // The one canonical table in the kiln database. A migration that rebuilds
    // it loses plugin-authored data that has no other copy.
    assert!(
        !DERIVED_TABLES.contains(&"properties"),
        "`properties` is canonical, not derived"
    );
    for (name, sql) in ddl {
        assert!(
            !creates_table(sql, "properties"),
            "`properties` must not be created by {name}: its shape belongs to \
             SCHEMA_V1, and listing it beside derived tables invites a rebuild"
        );
    }
}

/// Does `sql` contain a `CREATE [VIRTUAL] TABLE [IF NOT EXISTS] <table>`?
fn creates_table(sql: &str, table: &str) -> bool {
    sql.lines().any(|line| {
        let line = line.trim().to_lowercase();
        let Some(rest) = line.strip_prefix("create ") else {
            return false;
        };
        let rest = rest.strip_prefix("virtual ").unwrap_or(rest);
        let Some(rest) = rest.strip_prefix("table ") else {
            return false;
        };
        let rest = rest.strip_prefix("if not exists ").unwrap_or(rest);
        rest.split(['(', ' '])
            .next()
            .is_some_and(|name| name == table.to_lowercase())
    })
}
