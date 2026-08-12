//! SQLite connection pool management
//!
//! Uses a simple Arc<Mutex<Connection>> pattern instead of r2d2 to avoid
//! version conflicts with other workspace crates.

use crate::storage::sqlite::config::SqliteConfig;
use crate::storage::sqlite::error_ext::SqliteResultExt;
use crate::storage::sqlite::schema::{self, MigrationOutcome};
use crucible_core::storage::{StorageError, StorageResult};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::sync::Arc;
use tracing::{debug, info};

/// Thread-safe SQLite connection wrapper
///
/// For SQLite in WAL mode, we can have multiple readers but only one writer.
/// This simple wrapper uses a mutex for thread safety.
#[derive(Clone)]
pub struct SqlitePool {
    conn: Arc<Mutex<Connection>>,
    /// What the migration ladder reported when this pool opened the database.
    /// By value, not behind a lock: it is written once, before the pool
    /// exists, and only read afterwards.
    migration_outcome: MigrationOutcome,
}

impl SqlitePool {
    /// Create a new connection pool with the given configuration
    pub fn new(config: SqliteConfig) -> StorageResult<Self> {
        info!(path = ?config.path, "Creating SQLite connection");

        let conn = if config.path.to_str() == Some(":memory:") {
            Connection::open_in_memory().sql()?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = config.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    StorageError::Backend(format!("Failed to create directory: {}", e))
                })?;
            }
            Connection::open(&config.path).sql()?
        };

        // Pragmas and migrations run before the pool exists, so the outcome can
        // be stored by value rather than mutated through the mutex.
        let migration_outcome = initialize(&conn, &config)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            migration_outcome,
        })
    }

    /// What the migration ladder reported when this database was opened.
    pub fn migration_outcome(&self) -> &MigrationOutcome {
        &self.migration_outcome
    }

    /// Create an in-memory pool for testing
    pub fn memory() -> StorageResult<Self> {
        Self::new(SqliteConfig::memory())
    }

    /// Execute a closure with the connection
    pub fn with_connection<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Connection) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }

    /// Execute a closure with mutable access to the connection
    pub fn with_connection_mut<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&mut Connection) -> StorageResult<T>,
    {
        let mut conn = self.conn.lock();
        f(&mut conn)
    }

    /// Execute a closure within a transaction
    ///
    /// If the closure returns `Ok`, the transaction is committed.
    /// If the closure returns `Err`, the transaction is rolled back.
    pub fn with_transaction<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Connection) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        conn.execute("BEGIN TRANSACTION", []).sql()?;

        match f(&conn) {
            Ok(result) => {
                conn.execute("COMMIT", []).sql()?;
                Ok(result)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    }

    /// Get database statistics
    pub fn stats(&self) -> StorageResult<DbStats> {
        self.with_connection(|conn| {
            let page_count: i64 = conn
                .query_row("PRAGMA page_count;", [], |row| row.get(0))
                .sql()?;

            let page_size: i64 = conn
                .query_row("PRAGMA page_size;", [], |row| row.get(0))
                .sql()?;

            let freelist_count: i64 = conn
                .query_row("PRAGMA freelist_count;", [], |row| row.get(0))
                .sql()?;

            Ok(DbStats {
                page_count: page_count as u64,
                page_size: page_size as u64,
                freelist_count: freelist_count as u64,
                total_size_bytes: (page_count * page_size) as u64,
            })
        })
    }
}

/// Configure pragmas and run the migration ladder.
///
/// Free rather than a method because it runs *before* the pool exists: the
/// migration outcome it produces is one of the pool's fields.
fn initialize(conn: &Connection, config: &SqliteConfig) -> StorageResult<MigrationOutcome> {
    configure_pragmas(conn, config)?;
    let outcome = schema::apply_migrations(conn)?;
    info!("SQLite database initialized successfully");
    Ok(outcome)
}

/// Configure SQLite PRAGMA settings for optimal performance
fn configure_pragmas(conn: &Connection, config: &SqliteConfig) -> StorageResult<()> {
    debug!("Configuring SQLite pragmas");

    // WAL mode for better concurrency
    if config.wal_mode {
        conn.execute_batch("PRAGMA journal_mode = WAL;").sql()?;
        conn.execute_batch("PRAGMA synchronous = NORMAL;").sql()?;
    }

    // Foreign key enforcement
    if config.foreign_keys {
        conn.execute_batch("PRAGMA foreign_keys = ON;").sql()?;
    }

    // Busy timeout
    conn.execute_batch(&format!(
        "PRAGMA busy_timeout = {};",
        config.busy_timeout_ms
    ))
    .sql()?;

    // Cache size
    conn.execute_batch(&format!("PRAGMA cache_size = {};", config.cache_size))
        .sql()?;

    // MMAP for faster reads (if configured)
    if config.mmap_size > 0 {
        conn.execute_batch(&format!("PRAGMA mmap_size = {};", config.mmap_size))
            .sql()?;
    }

    // Use memory for temp tables
    conn.execute_batch("PRAGMA temp_store = MEMORY;").sql()?;

    Ok(())
}

/// Database statistics
#[derive(Debug, Clone)]
pub struct DbStats {
    pub page_count: u64,
    pub page_size: u64,
    pub freelist_count: u64,
    pub total_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_memory_pool() {
        let pool = SqlitePool::memory().expect("Failed to create memory pool");

        pool.with_connection(|conn| {
            let result: i64 = conn.query_row("SELECT 1 + 1", [], |row| row.get(0)).sql()?;
            assert_eq!(result, 2);
            Ok(())
        })
        .expect("Query failed");
    }

    #[test]
    fn test_file_pool() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");

        let config = SqliteConfig::new(&db_path);
        let pool = SqlitePool::new(config).expect("Failed to create pool");

        // Verify WAL mode is enabled
        pool.with_connection(|conn| {
            let mode: String = conn
                .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
                .sql()?;
            assert_eq!(mode.to_lowercase(), "wal");
            Ok(())
        })
        .expect("Query failed");
    }

    #[test]
    fn test_pool_stats() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let stats = pool.stats().expect("Failed to get stats");

        assert!(stats.page_size > 0);
    }

    /// A new kiln gets the tables it uses and none of the four `SCHEMA_V1`
    /// created but nothing ever wrote. The negative half is the point: this
    /// test used to assert all six existed, which defended an accident rather
    /// than a decision.
    ///
    /// `entities` is still here deliberately. It holds no rows either, but it
    /// stays one release so a binary downgraded across the v3 migration still
    /// finds the foreign-key target its `properties` table declares.
    #[test]
    fn a_new_kiln_gets_the_live_tables_and_none_of_the_vestigial_ones() {
        let pool = SqlitePool::memory().expect("Failed to create pool");

        pool.with_connection(|conn| {
            let tables: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                    .sql()?;
                let rows = stmt.query_map([], |row| row.get(0)).sql()?;
                rows.filter_map(Result::ok).collect()
            };

            for live in ["entities", "properties", "notes", "note_links", "notes_fts"] {
                assert!(
                    tables.contains(&live.to_string()),
                    "{live} is missing: {tables:?}"
                );
            }

            for vestigial in ["relations", "blocks", "tags", "entity_tags"] {
                assert!(
                    !tables.contains(&vestigial.to_string()),
                    "{vestigial} has no production reader or writer and must not \
                     be created for a new kiln: {tables:?}"
                );
            }

            Ok(())
        })
        .expect("Failed to verify schema");
    }
}
