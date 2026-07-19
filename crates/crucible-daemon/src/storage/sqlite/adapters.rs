//! Factory Functions and Adapters for SQLite Backend
//!
//! Storage adapters for daemon compatibility.

use crate::storage::sqlite::connection::SqlitePool;
#[cfg(test)]
use crate::storage::sqlite::error_ext::SqliteResultExt;
use crate::storage::sqlite::note_store::SqliteNoteStore;
use crate::storage::sqlite::SqliteConfig;
use anyhow::Result;
use crucible_core::storage::{NoteStore, PropertyStore};
#[cfg(test)]
use crucible_core::{QueryResult, Record, RecordId};
#[cfg(test)]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Opaque handle to a SQLite client.
#[derive(Clone)]
pub struct SqliteClientHandle {
    pool: SqlitePool,
    note_store: Arc<SqliteNoteStore>,
    /// The kiln this handle is bound to. `None` means an unbound handle
    /// (test-only; `as_knowledge_repository()` falls back to `Scope::Global`
    /// authority). Production `KilnManager::open` always calls
    /// [`Self::with_kiln_path`] so reads are scope-enforced.
    kiln_path: Option<PathBuf>,
}

impl SqliteClientHandle {
    /// Create a handle from a SqlitePool. Use [`Self::with_kiln_path`] to
    /// bind it to a kiln so [`Self::as_knowledge_repository`] enforces
    /// workspace-scoped reads — without that the repo falls back to
    /// `Scope::Global` authority, which leaks user-scoped notes.
    pub fn new(pool: SqlitePool, note_store: SqliteNoteStore) -> Self {
        Self {
            pool,
            note_store: Arc::new(note_store),
            kiln_path: None,
        }
    }

    /// Builder: bind this handle to a kiln so reads through
    /// [`Self::as_knowledge_repository`] enforce `Scope::Workspace(kiln_path)`
    /// authority. `KilnManager::open` calls this for every production
    /// handle; only test setups skip it.
    #[must_use]
    pub fn with_kiln_path(mut self, kiln_path: impl Into<PathBuf>) -> Self {
        self.kiln_path = Some(kiln_path.into());
        self
    }

    /// The kiln this handle is bound to, if any.
    pub fn kiln_path(&self) -> Option<&Path> {
        self.kiln_path.as_deref()
    }

    /// Get the pool for direct access
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get a trait object for NoteStore
    pub fn as_note_store(&self) -> Arc<dyn NoteStore> {
        self.note_store.clone()
    }

    /// Get a trait object for PropertyStore (EAV properties)
    pub fn as_property_store(&self) -> Arc<dyn PropertyStore> {
        self.note_store.clone()
    }

    /// Get a trait object for KnowledgeRepository.
    ///
    /// The repository's read authority is derived from this handle's
    /// `kiln_path`:
    /// - Bound handle → `Scope::Workspace(kiln_path)` (user-scoped notes
    ///   from other tenants are filtered out).
    /// - Unbound handle → `Scope::Global` (test/admin only).
    pub fn as_knowledge_repository(&self) -> Arc<dyn crucible_core::traits::KnowledgeRepository> {
        match &self.kiln_path {
            Some(p) => Arc::new(
                crate::storage::sqlite::repository::SqliteKnowledgeRepository::with_kiln_path(
                    self.note_store.clone(),
                    p.clone(),
                ),
            ),
            None => Arc::new(
                crate::storage::sqlite::repository::SqliteKnowledgeRepository::new(
                    self.note_store.clone(),
                ),
            ),
        }
    }

    /// Execute a raw SQL query against the kiln's SQLite store.
    ///
    /// # SECURITY — test-only escape hatch
    ///
    /// This bypasses every typed-storage safety net, **including memory
    /// scoping**. A SELECT here returns rows regardless of
    /// `properties.scope` because nothing is appending the scope filter
    /// for you. Production code paths MUST go through the [`NoteStore`]
    /// trait (which carries a `Scope` authority) or one of the typed
    /// `note.*` RPC handlers — never this method.
    ///
    /// Gated behind `#[cfg(test)]` so no production build ever links
    /// this symbol. The Lua `cru.storage` surface intentionally exposes
    /// only the property-store (EAV) API, not raw SQL.
    #[cfg(test)]
    pub(crate) async fn query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<QueryResult> {
        use rusqlite::params_from_iter;
        use std::time::Instant;

        let sql_owned = sql.to_string();
        let pool = self.pool.clone();

        // Execute query using spawn_blocking for async compatibility
        let result = tokio::task::spawn_blocking(move || {
            let start = Instant::now();

            pool.with_connection(|conn| {
                let mut stmt = conn.prepare(&sql_owned).sql()?;
                let column_count = stmt.column_count();
                let column_names: Vec<String> = (0..column_count)
                    .map(|i| stmt.column_name(i).unwrap_or("").to_string())
                    .collect();

                let mut records = Vec::new();
                let mut rows = stmt
                    .query(params_from_iter(std::iter::empty::<&str>()))
                    .sql()?;

                while let Some(row) = rows.next().sql()? {
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

/// Convert a rusqlite row value to serde_json::Value (test-only helper
/// for [`SqliteClientHandle::query`]).
#[cfg(test)]
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
    let note_store = crate::storage::sqlite::create_note_store(pool.clone()).await?;

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

    /// Memory-scoping regression: `SqliteClientHandle::as_knowledge_repository()`
    /// MUST bind to the handle's kiln path so reads enforce same-workspace
    /// authority. Pre-fix this method dropped the kiln path and the
    /// resulting repo defaulted to an unbound authority — leaking
    /// sibling-workspace notes into every precognition turn.
    #[tokio::test]
    async fn as_knowledge_repository_is_kiln_scoped() {
        use crucible_core::parser::BlockHash;
        use crucible_core::storage::note_store::NoteRecord;
        use crucible_core::storage::NoteStore;

        let tempdir = TempDir::new().unwrap();
        let kiln_root = tempdir.path().to_path_buf();
        let sibling_root = tempdir.path().join("sibling-elsewhere");
        let db_path = kiln_root.join("test.db");
        let config = SqliteConfig::new(&db_path);

        let client = create_sqlite_client(config)
            .await
            .unwrap()
            .with_kiln_path(kiln_root.clone());

        // Seed a sibling-workspace note that this kiln's authority must NOT see.
        let alien = NoteRecord {
            path: "notes/alien.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: Some(vec![1.0, 0.0]),
            title: "Alien".to_string(),
            tags: vec![],
            links_to: vec![],
            links: Vec::new(),
            properties: Default::default(),
            updated_at: chrono::Utc::now(),
            ..Default::default()
        }
        .with_scope(crucible_core::storage::Scope::workspace_unchecked(
            &sibling_root,
        ));
        NoteStore::upsert(client.as_note_store().as_ref(), alien)
            .await
            .unwrap();

        // Seed an own-workspace note that authority CAN see.
        let ws_note = NoteRecord {
            path: "notes/visible.md".to_string(),
            content_hash: BlockHash::zero(),
            embedding: Some(vec![0.0, 1.0]),
            title: "Visible".to_string(),
            tags: vec![],
            links_to: vec![],
            links: Vec::new(),
            properties: Default::default(),
            updated_at: chrono::Utc::now(),
            ..Default::default()
        }
        .with_scope(crucible_core::storage::Scope::workspace_unchecked(
            &kiln_root,
        ));
        NoteStore::upsert(client.as_note_store().as_ref(), ws_note)
            .await
            .unwrap();

        let repo = client.as_knowledge_repository();
        let listed = repo.list_notes(None).await.unwrap();
        assert_eq!(
            listed.len(),
            1,
            "workspace-bound repo leaked notes: {:?}",
            listed.iter().map(|n| &n.path).collect::<Vec<_>>()
        );
        assert_eq!(listed[0].path, "notes/visible.md");
    }
}
