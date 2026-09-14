//! Factory Functions and Adapters for SQLite Backend
//!
//! Storage adapters for daemon compatibility.

use crate::storage::sqlite::connection::SqlitePool;
use crate::storage::sqlite::note_store::SqliteNoteStore;
use crate::storage::sqlite::SqliteConfig;
use anyhow::Result;
use crucible_core::storage::{NoteStore, PropertyStore};
use std::path::PathBuf;
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

    /// Get the pool for direct access
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get a trait object for NoteStore
    pub fn as_note_store(&self) -> Arc<dyn NoteStore> {
        self.note_store.clone()
    }

    /// Get a trait object for PropertyStore (EAV properties)
    /// Block-granularity vector store (SQLite).
    pub fn as_block_store(&self) -> Arc<dyn crucible_core::storage::BlockStore> {
        Arc::new(super::block_store::SqliteBlockStore::new(self.pool.clone()))
    }

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
        let blocks = self.as_block_store();
        match &self.kiln_path {
            Some(p) => Arc::new(
                crate::storage::sqlite::repository::SqliteKnowledgeRepository::with_kiln_path(
                    self.note_store.clone(),
                    p.clone(),
                )
                .with_block_store(blocks),
            ),
            None => Arc::new(
                crate::storage::sqlite::repository::SqliteKnowledgeRepository::new(
                    self.note_store.clone(),
                )
                .with_block_store(blocks),
            ),
        }
    }
}

/// Create a SQLite client from configuration.
pub async fn create_sqlite_client(config: SqliteConfig) -> Result<SqliteClientHandle> {
    let pool = SqlitePool::new(config)?;
    let note_store = SqliteNoteStore::new(pool.clone());

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
