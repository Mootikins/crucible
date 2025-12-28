//! Factory Functions and Adapters for Trait Compatibility
//!
//! This module provides the public API for creating SurrealDB-backed implementations
//! of core traits. It enforces the Dependency Inversion Principle by hiding concrete
//! types and exposing only trait objects.
//!
//! ## Architecture (SOLID Phase 5)
//!
//! - Concrete types (SurrealClient, EAVGraphStore, etc.) are private to the crate
//! - This module provides factory functions that return trait objects
//! - CLI code depends on abstractions, not implementations
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_surrealdb::adapters;
//! use crucible_surrealdb::SurrealDbConfig;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = SurrealDbConfig {
//!         path: "./cache.db".to_string(),
//!         namespace: "crucible".to_string(),
//!         database: "kiln".to_string(),
//!         max_connections: Some(10),
//!         timeout_seconds: Some(30),
//!     };
//!
//!     // Create client (opaque handle)
//!     let client = adapters::create_surreal_client(config).await?;
//!
//!     // Create trait objects
//!     let enriched_store = adapters::create_enriched_note_store(client.clone());
//!     let merkle_store = adapters::create_merkle_store(client.clone());
//!
//!     Ok(())
//! }
//! ```

use crate::event_handlers::{StorageHandler, TagHandler};
use crate::{EAVGraphStore, MerklePersistence, NoteIngestor, SurrealClient, SurrealDbConfig};
use anyhow::Result;
use async_trait::async_trait;
use crucible_core::enrichment::{EnrichedNote, EnrichedNoteStore};
use crucible_core::events::{SessionEvent, SharedEventBus};
use crucible_merkle::MerkleStore;
use std::sync::Arc;

// ============================================================================
// Opaque Handle for SurrealDB Client
// ============================================================================

/// Opaque handle to a SurrealDB client.
///
/// This type is intentionally opaque - you cannot access the underlying SurrealClient.
/// Use the factory functions in this module to create trait objects that depend on it.
///
/// # Cloning
///
/// This handle is cheap to clone (Arc-wrapped internally).
#[derive(Clone)]
pub struct SurrealClientHandle {
    client: Arc<SurrealClient>,
}

impl SurrealClientHandle {
    /// Create a handle from a SurrealClient (internal use only)
    pub(crate) fn new(client: SurrealClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    /// Get the inner client for special operations (kiln integration, etc.)
    ///
    /// This is exposed publicly but should be used sparingly. Most operations
    /// should go through the factory functions in this module.
    pub fn inner(&self) -> &SurrealClient {
        &self.client
    }

    /// Get an Arc to the inner client (internal use only)
    pub(crate) fn inner_arc(&self) -> Arc<SurrealClient> {
        Arc::clone(&self.client)
    }

    /// Get a trait object for KnowledgeRepository
    ///
    /// This is needed when you need to pass the client as a trait object
    /// to code that depends on the trait, not the implementation.
    pub fn as_knowledge_repository(&self) -> Arc<dyn crucible_core::traits::KnowledgeRepository> {
        Arc::clone(&self.client) as Arc<dyn crucible_core::traits::KnowledgeRepository>
    }
}

// ============================================================================
// Factory Functions - Public API
// ============================================================================

/// Create a SurrealDB client from configuration.
///
/// This is the main entry point for creating a database connection.
///
/// # Returns
///
/// An opaque handle that can be passed to other factory functions.
pub async fn create_surreal_client(config: SurrealDbConfig) -> Result<SurrealClientHandle> {
    let client = SurrealClient::new(config).await?;
    Ok(SurrealClientHandle::new(client))
}

/// Create an enriched note store backed by SurrealDB.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
///
/// # Returns
///
/// A trait object implementing `EnrichedNoteStore`
pub fn create_enriched_note_store(client: SurrealClientHandle) -> Arc<dyn EnrichedNoteStore> {
    Arc::new(EnrichedNoteStoreAdapter::new(client.inner_arc()))
}

/// Create a Merkle tree store backed by SurrealDB.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
///
/// # Returns
///
/// A trait object implementing `MerkleStore`
pub fn create_merkle_store(client: SurrealClientHandle) -> Arc<dyn MerkleStore> {
    Arc::new(MerklePersistence::new((*client.inner()).clone()))
}

/// Create a SurrealDB-backed change detection store
///
/// Returns a trait object implementing `ChangeDetectionStore` for file state tracking.
/// This is used by the pipeline's Phase 1 quick filter to avoid reprocessing unchanged files.
///
/// # Arguments
///
/// * `client` - SurrealDB client handle
///
/// # Returns
///
/// A trait object implementing `ChangeDetectionStore`
pub fn create_change_detection_store(
    client: SurrealClientHandle,
) -> Arc<dyn crucible_core::processing::ChangeDetectionStore> {
    Arc::new(
        crate::change_detection_store::SurrealChangeDetectionStore::new((*client.inner()).clone()),
    )
}

// ============================================================================
// Adapter Implementations
// ============================================================================

/// Adapter that owns EAVGraphStore and implements EnrichedNoteStore.
///
/// This solves the lifetime issue with NoteIngestor<'a> by owning the store
/// and creating NoteIngestor instances on-demand for each operation.
///
/// # Architecture
///
/// The adapter pattern is used here because `NoteIngestor<'a>` has a lifetime
/// parameter and cannot be directly wrapped in `Arc<dyn Trait>`. By owning
/// the `EAVGraphStore`, we can create short-lived `NoteIngestor` instances
/// that borrow from our owned store.
struct EnrichedNoteStoreAdapter {
    store: EAVGraphStore,
}

impl EnrichedNoteStoreAdapter {
    /// Create a new adapter with the given SurrealDB client.
    ///
    /// # Note
    ///
    /// SurrealClient is cheap to clone (it's Arc-wrapped internally), so we
    /// can safely clone it here.
    fn new(client: Arc<SurrealClient>) -> Self {
        Self {
            store: EAVGraphStore::new((*client).clone()),
        }
    }
}

#[async_trait]
impl EnrichedNoteStore for EnrichedNoteStoreAdapter {
    async fn store_enriched(&self, enriched: &EnrichedNote, relative_path: &str) -> Result<()> {
        // Create NoteIngestor on-demand (borrows from self.store)
        let ingestor = NoteIngestor::new(&self.store);
        ingestor.store_enriched(enriched, relative_path).await
    }

    async fn note_exists(&self, relative_path: &str) -> Result<bool> {
        // Create NoteIngestor on-demand (borrows from self.store)
        let ingestor = NoteIngestor::new(&self.store);
        ingestor.note_exists(relative_path).await
    }
}

// ============================================================================
// Event Handler Factory Functions
// ============================================================================

/// Create a storage handler for database event processing.
///
/// The storage handler subscribes to `NoteParsed`, `FileDeleted`, and `FileMoved` events
/// to store/update/delete entities in the EAV graph database.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
/// * `emitter` - Event emitter for emitting storage events
///
/// # Returns
///
/// A `StorageHandler` that can be registered with the event bus.
pub fn create_storage_handler(
    client: SurrealClientHandle,
    emitter: SharedEventBus<SessionEvent>,
) -> StorageHandler {
    let store = Arc::new(EAVGraphStore::new((*client.inner()).clone()));
    StorageHandler::new(store, emitter)
}

/// Create a tag handler for processing entity tags.
///
/// The tag handler subscribes to `NoteParsed` events to extract and associate tags
/// with entities in the database.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
/// * `emitter` - Event emitter for emitting tag events
///
/// # Returns
///
/// A `TagHandler` that can be registered with the event bus.
pub fn create_tag_handler(
    client: SurrealClientHandle,
    emitter: SharedEventBus<SessionEvent>,
) -> TagHandler {
    let store = Arc::new(EAVGraphStore::new((*client.inner()).clone()));
    TagHandler::new(store, emitter)
}

// ============================================================================
// Graph Query Executor
// ============================================================================

use crucible_core::traits::{GraphQueryError, GraphQueryExecutor, GraphQueryResult};

/// Create a graph query executor backed by SurrealDB.
///
/// The executor translates jaq-like query syntax to SurrealQL and executes
/// against the database.
///
/// # Arguments
///
/// * `client` - Opaque handle to a SurrealDB client
///
/// # Returns
///
/// A trait object implementing `GraphQueryExecutor`
///
/// # Example
///
/// ```rust,ignore
/// let executor = create_graph_executor(client);
/// let results = executor.execute(r#"outlinks("Index")"#).await?;
/// ```
pub fn create_graph_executor(client: SurrealClientHandle) -> Arc<dyn GraphQueryExecutor> {
    Arc::new(SurrealGraphExecutor::new(client.inner_arc()))
}

/// SurrealDB-backed implementation of GraphQueryExecutor.
///
/// Uses the `graph_query` module to translate jaq-like syntax to SurrealQL.
struct SurrealGraphExecutor {
    client: Arc<SurrealClient>,
}

impl SurrealGraphExecutor {
    fn new(client: Arc<SurrealClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl GraphQueryExecutor for SurrealGraphExecutor {
    async fn execute(&self, query: &str) -> GraphQueryResult<Vec<serde_json::Value>> {
        use crate::graph_query::create_default_pipeline;

        // Use the new composable query pipeline
        // Supports: MATCH patterns (priority 50), SQL sugar (40), jaq-style (30)
        let pipeline = create_default_pipeline();
        let rendered = pipeline
            .execute(query)
            .map_err(|e| GraphQueryError::with_query(e.to_string(), query))?;

        // Execute against SurrealDB
        let result = self
            .client
            .query(&rendered.sql, &[serde_json::to_value(&rendered.params).unwrap_or_default()])
            .await
            .map_err(|e| GraphQueryError::with_query(e.to_string(), query))?;

        // Convert records to JSON values
        let values: Vec<serde_json::Value> = result
            .records
            .into_iter()
            .map(|record| {
                let mut obj = serde_json::Map::new();
                if let Some(id) = record.id {
                    obj.insert("id".to_string(), serde_json::Value::String(id.0));
                }
                for (key, value) in record.data {
                    obj.insert(key, value);
                }
                serde_json::Value::Object(obj)
            })
            .collect();

        Ok(values)
    }
}

#[cfg(test)]
mod graph_executor_tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::EAVGraphStore;
    use crate::SurrealClient;
    use crucible_core::storage::RelationStorage;

    /// Helper to set up a test database with notes and relations using EAVGraphStore
    async fn setup_test_graph() -> (SurrealClient, Arc<dyn GraphQueryExecutor>) {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();

        // Create test entities (notes) - include required 'type' field
        client
            .db()
            .query(r#"CREATE entities:index CONTENT { type: "note", title: "Index", path: "Index.md", content_hash: "hash1" }"#)
            .await
            .unwrap();
        client
            .db()
            .query(r#"CREATE entities:project_a CONTENT { type: "note", title: "Project A", path: "projects/a.md", content_hash: "hash2" }"#)
            .await
            .unwrap();
        client
            .db()
            .query(r#"CREATE entities:project_b CONTENT { type: "note", title: "Project B", path: "projects/b.md", content_hash: "hash3" }"#)
            .await
            .unwrap();
        client
            .db()
            .query(r#"CREATE entities:sub_page CONTENT { type: "note", title: "Sub Page", path: "projects/sub.md", content_hash: "hash4" }"#)
            .await
            .unwrap();

        // Create relations using EAVGraphStore (the proper way that creates graph edges)
        let store = EAVGraphStore::new(client.clone());

        // Index -> Project A, Index -> Project B
        // Project A -> Sub Page
        // Sub Page -> Index (creates a cycle for testing)
        let relations = vec![
            crucible_core::storage::Relation::wikilink("entities:index", "entities:project_a"),
            crucible_core::storage::Relation::wikilink("entities:index", "entities:project_b"),
            crucible_core::storage::Relation::wikilink("entities:project_a", "entities:sub_page"),
            crucible_core::storage::Relation::wikilink("entities:sub_page", "entities:index"),
        ];

        for relation in relations {
            store.store_relation(relation).await.unwrap();
        }

        let handle = SurrealClientHandle::new(client.clone());
        let executor = create_graph_executor(handle);
        (client, executor)
    }

    /// Verify the test data is actually in the database
    #[tokio::test]
    async fn test_data_setup() {
        let (client, _executor) = setup_test_graph().await;

        // Verify entity count
        let mut result = client
            .db()
            .query("SELECT count() as c FROM entities GROUP ALL")
            .await
            .unwrap();
        let counts: Vec<serde_json::Value> = result.take(0).unwrap();
        let count = counts.first().and_then(|v| v.get("c")).and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 4, "Should have 4 entities");

        // Verify relation count
        let mut result = client
            .db()
            .query("SELECT count() as c FROM relations GROUP ALL")
            .await
            .unwrap();
        let counts: Vec<serde_json::Value> = result.take(0).unwrap();
        let count = counts.first().and_then(|v| v.get("c")).and_then(|v| v.as_i64()).unwrap_or(0);
        assert_eq!(count, 4, "Should have 4 relations");

        // Verify relation structure: `in`.title accesses the linked entity's title field
        let mut result = client
            .db()
            .query(r#"SELECT out.title as target FROM relations WHERE `in`.title = "Index" AND relation_type = "wikilink""#)
            .await
            .unwrap();
        let outlinks: Vec<serde_json::Value> = result.take(0).unwrap();
        assert_eq!(outlinks.len(), 2, "Index should have 2 outlinks");
    }

    #[tokio::test]
    async fn test_executor_find() {
        let (_client, executor) = setup_test_graph().await;

        let results = executor.execute(r#"find("Index")"#).await.unwrap();

        assert_eq!(results.len(), 1, "find(Index) should return 1 result, got: {:?}", results);
        assert_eq!(results[0]["title"], "Index");
        assert_eq!(results[0]["path"], "Index.md");
    }

    #[tokio::test]
    async fn test_executor_find_not_found() {
        let (_client, executor) = setup_test_graph().await;

        let results = executor.execute(r#"find("Nonexistent")"#).await.unwrap();

        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_executor_outlinks() {
        let (_client, executor) = setup_test_graph().await;

        let results = executor.execute(r#"outlinks("Index")"#).await.unwrap();

        assert_eq!(results.len(), 2, "outlinks(Index) should return 2, got {:?}", results);

        // The results are entities nested under "out" key from the SELECT out ... FETCH out pattern
        let mut titles: Vec<&str> = results
            .iter()
            .filter_map(|r| r["out"]["title"].as_str())
            .collect();
        titles.sort();

        assert_eq!(titles, vec!["Project A", "Project B"]);
    }

    #[tokio::test]
    async fn test_executor_inlinks() {
        let (_client, executor) = setup_test_graph().await;

        // Index has inlink from Sub Page
        let results = executor.execute(r#"inlinks("Index")"#).await.unwrap();

        assert_eq!(results.len(), 1, "inlinks(Index) should return 1, got {:?}", results);
        // Result is nested under "in" key from SELECT `in` ... FETCH `in` pattern
        assert_eq!(results[0]["in"]["title"], "Sub Page");
    }

    #[tokio::test]
    async fn test_executor_error_on_invalid_query() {
        let (_client, executor) = setup_test_graph().await;

        // Invalid query syntax
        let result = executor.execute("not_a_valid_function()").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[tokio::test]
    async fn test_executor_neighbors() {
        let (_client, executor) = setup_test_graph().await;

        // Project A has: outlinks to Sub Page, inlinks from Index
        let results = executor.execute(r#"neighbors("Project A")"#).await.unwrap();

        assert_eq!(results.len(), 2, "neighbors(Project A) should return 2, got {:?}", results);

        // Neighbors returns mixed "out" and "in" keys due to UNION
        let mut titles: Vec<&str> = results
            .iter()
            .filter_map(|r| {
                r["out"]["title"].as_str().or_else(|| r["in"]["title"].as_str())
            })
            .collect();
        titles.sort();

        assert_eq!(titles, vec!["Index", "Sub Page"]);
    }
}
