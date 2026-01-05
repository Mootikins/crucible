//! Factory Functions and Adapters for SQLite Backend
//!
//! Mirrors the SurrealDB adapters module for daemon compatibility.

use crate::connection::SqlitePool;
use crate::note_store::SqliteNoteStore;
use crate::SqliteConfig;
use anyhow::Result;
use crucible_core::database::{QueryResult, Record, RecordId};
use crucible_core::storage::NoteStore;
use std::collections::HashMap;
use std::sync::Arc;

/// Opaque handle to a SQLite client.
///
/// Mirrors SurrealClientHandle for daemon compatibility.
#[derive(Clone)]
pub struct SqliteClientHandle {
    pool: SqlitePool,
    note_store: Arc<SqliteNoteStore>,
}

impl SqliteClientHandle {
    /// Create a handle from a SqlitePool
    pub fn new(pool: SqlitePool, note_store: SqliteNoteStore) -> Self {
        Self {
            pool,
            note_store: Arc::new(note_store),
        }
    }

    /// Get the pool for direct access
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get a trait object for NoteStore
    pub fn as_note_store(&self) -> Arc<dyn NoteStore> {
        self.note_store.clone()
    }

    /// Execute a SQL query and return results
    ///
    /// This mirrors SurrealClient::query() for daemon compatibility.
    pub async fn query(&self, sql: &str, _params: &[serde_json::Value]) -> Result<QueryResult> {
        use rusqlite::params_from_iter;
        use std::time::Instant;

        let sql_owned = sql.to_string();
        let pool = self.pool.clone();

        // Execute query using spawn_blocking for async compatibility
        let result = tokio::task::spawn_blocking(move || {
            let start = Instant::now();

            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(&sql_owned)?;
                let column_count = stmt.column_count();
                let column_names: Vec<String> = (0..column_count)
                    .map(|i| stmt.column_name(i).unwrap_or("").to_string())
                    .collect();

                let mut records = Vec::new();
                let mut rows = stmt.query(params_from_iter(std::iter::empty::<&str>()))?;

                while let Some(row) = rows.next()? {
                    let mut data = HashMap::new();
                    let mut record_id = None;

                    for (i, name) in column_names.iter().enumerate() {
                        let value = row_value_to_json(row, i);

                        // Use 'id' or 'path' column as record ID
                        if name == "id" || name == "path" {
                            if let Some(s) = value.as_str() {
                                record_id = Some(RecordId(s.to_string()));
                            }
                        }

                        data.insert(name.clone(), value);
                    }

                    records.push(Record {
                        id: record_id,
                        data,
                    });
                }

                let total_count = records.len() as u64;
                let execution_time_ms = start.elapsed().as_millis() as u64;

                Ok(QueryResult {
                    records,
                    total_count: Some(total_count),
                    execution_time_ms: Some(execution_time_ms),
                    has_more: false,
                })
            })
        })
        .await??;

        Ok(result)
    }
}

/// Convert a rusqlite row value to serde_json::Value
fn row_value_to_json(row: &rusqlite::Row, idx: usize) -> serde_json::Value {
    // Try different types in order
    if let Ok(v) = row.get::<_, i64>(idx) {
        return serde_json::Value::Number(v.into());
    }
    if let Ok(v) = row.get::<_, f64>(idx) {
        if let Some(n) = serde_json::Number::from_f64(v) {
            return serde_json::Value::Number(n);
        }
    }
    if let Ok(v) = row.get::<_, String>(idx) {
        // Try parsing as JSON for complex types
        if v.starts_with('[') || v.starts_with('{') {
            if let Ok(json) = serde_json::from_str(&v) {
                return json;
            }
        }
        return serde_json::Value::String(v);
    }
    if let Ok(v) = row.get::<_, bool>(idx) {
        return serde_json::Value::Bool(v);
    }
    if let Ok(v) = row.get::<_, Vec<u8>>(idx) {
        return serde_json::Value::String(format!("[blob {} bytes]", v.len()));
    }

    serde_json::Value::Null
}

/// Create a SQLite client from configuration.
pub async fn create_sqlite_client(config: SqliteConfig) -> Result<SqliteClientHandle> {
    let pool = SqlitePool::new(config)?;
    let note_store = crate::create_note_store(pool.clone()).await?;

    Ok(SqliteClientHandle::new(pool, note_store))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_sqlite_client() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let config = SqliteConfig::new(&db_path);

        let client = create_sqlite_client(config).await.unwrap();

        // Verify we can get a note store
        let _store = client.as_note_store();
    }

    #[tokio::test]
    async fn test_query_basic() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let config = SqliteConfig::new(&db_path);

        let client = create_sqlite_client(config).await.unwrap();

        // Simple query
        let result = client.query("SELECT 1 + 1 AS result", &[]).await.unwrap();

        assert_eq!(result.records.len(), 1);
        assert_eq!(
            result.records[0].data.get("result"),
            Some(&serde_json::Value::Number(2.into()))
        );
    }

    #[tokio::test]
    async fn test_query_notes_table() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let config = SqliteConfig::new(&db_path);

        let client = create_sqlite_client(config).await.unwrap();

        // Query the notes table (should be empty but exist)
        let result = client.query("SELECT * FROM notes LIMIT 10", &[]).await;
        assert!(result.is_ok());
    }
}
