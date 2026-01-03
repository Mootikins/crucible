//! SQLite connection pool management
//!
//! Uses a simple Arc<Mutex<Connection>> pattern instead of r2d2 to avoid
//! version conflicts with other workspace crates.

use crate::config::SqliteConfig;
use crate::error::{SqliteError, SqliteResult};
use crate::schema;
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
    config: SqliteConfig,
}

impl SqlitePool {
    /// Create a new connection pool with the given configuration
    pub fn new(config: SqliteConfig) -> SqliteResult<Self> {
        info!(path = ?config.path, "Creating SQLite connection");

        let conn = if config.path.to_str() == Some(":memory:") {
            Connection::open_in_memory()?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = config.path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SqliteError::Connection(format!("Failed to create directory: {}", e))
                })?;
            }
            Connection::open(&config.path)?
        };

        let sqlite_pool = Self {
            conn: Arc::new(Mutex::new(conn)),
            config,
        };

        // Configure and apply schema
        sqlite_pool.initialize()?;

        Ok(sqlite_pool)
    }

    /// Create an in-memory pool for testing
    pub fn memory() -> SqliteResult<Self> {
        Self::new(SqliteConfig::memory())
    }

    /// Execute a closure with the connection
    pub fn with_connection<F, T>(&self, f: F) -> SqliteResult<T>
    where
        F: FnOnce(&Connection) -> SqliteResult<T>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }

    /// Execute a closure with mutable access to the connection
    pub fn with_connection_mut<F, T>(&self, f: F) -> SqliteResult<T>
    where
        F: FnOnce(&mut Connection) -> SqliteResult<T>,
    {
        let mut conn = self.conn.lock();
        f(&mut conn)
    }

    /// Initialize the database (configure pragmas and apply schema)
    fn initialize(&self) -> SqliteResult<()> {
        self.with_connection(|conn| {
            // Apply PRAGMA settings
            self.configure_pragmas(conn)?;

            // Apply schema migrations
            schema::apply_migrations(conn)?;

            info!("SQLite database initialized successfully");
            Ok(())
        })
    }

    /// Configure SQLite PRAGMA settings for optimal performance
    fn configure_pragmas(&self, conn: &Connection) -> SqliteResult<()> {
        debug!("Configuring SQLite pragmas");

        // WAL mode for better concurrency
        if self.config.wal_mode {
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        }

        // Foreign key enforcement
        if self.config.foreign_keys {
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        }

        // Busy timeout
        conn.execute_batch(&format!(
            "PRAGMA busy_timeout = {};",
            self.config.busy_timeout_ms
        ))?;

        // Cache size
        conn.execute_batch(&format!("PRAGMA cache_size = {};", self.config.cache_size))?;

        // MMAP for faster reads (if configured)
        if self.config.mmap_size > 0 {
            conn.execute_batch(&format!("PRAGMA mmap_size = {};", self.config.mmap_size))?;
        }

        // Use memory for temp tables
        conn.execute_batch("PRAGMA temp_store = MEMORY;")?;

        Ok(())
    }

    /// Get database statistics
    pub fn stats(&self) -> SqliteResult<DbStats> {
        self.with_connection(|conn| {
            let page_count: i64 = conn.query_row("PRAGMA page_count;", [], |row| row.get(0))?;

            let page_size: i64 = conn.query_row("PRAGMA page_size;", [], |row| row.get(0))?;

            let freelist_count: i64 =
                conn.query_row("PRAGMA freelist_count;", [], |row| row.get(0))?;

            Ok(DbStats {
                page_count: page_count as u64,
                page_size: page_size as u64,
                freelist_count: freelist_count as u64,
                total_size_bytes: (page_count * page_size) as u64,
            })
        })
    }
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
            let result: i64 = conn.query_row("SELECT 1 + 1", [], |row| row.get(0))?;
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
            let mode: String = conn.query_row("PRAGMA journal_mode;", [], |row| row.get(0))?;
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

    #[test]
    fn test_schema_applied() {
        let pool = SqlitePool::memory().expect("Failed to create pool");

        // Verify tables exist
        pool.with_connection(|conn| {
            let tables: Vec<String> = {
                let mut stmt = conn
                    .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
                let rows = stmt.query_map([], |row| row.get(0))?;
                rows.filter_map(Result::ok).collect()
            };

            assert!(tables.contains(&"entities".to_string()));
            assert!(tables.contains(&"properties".to_string()));
            assert!(tables.contains(&"relations".to_string()));
            assert!(tables.contains(&"blocks".to_string()));
            assert!(tables.contains(&"tags".to_string()));
            assert!(tables.contains(&"entity_tags".to_string()));

            Ok(())
        })
        .expect("Failed to verify schema");
    }
}
