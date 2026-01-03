//! Factory Functions and Adapters for Trait Compatibility
//!
//! This module provides the public API for creating SurrealDB-backed implementations
//! of core traits. It enforces the Dependency Inversion Principle by hiding concrete
//! types and exposing only trait objects.
//!
//! ## Architecture (SOLID Phase 5)
//!
//! - Concrete types (SurrealClient, MerklePersistence, etc.) are private to the crate
//! - Public API provides trait objects and factory functions via the `adapters` module
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
//!     let merkle_store = adapters::create_merkle_store(client.clone());
//!
//!     Ok(())
//! }
//! ```

use crate::{MerklePersistence, SurrealClient, SurrealDbConfig};
use anyhow::Result;
use async_trait::async_trait;
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
        Arc::new(SurrealKnowledgeRepository::new(self.inner_arc()))
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
            .query(
                &rendered.sql,
                &[serde_json::to_value(&rendered.params).unwrap_or_default()],
            )
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

// NOTE: Graph executor tests were removed during Phase 4 cleanup.
// They depended on the old EAV graph system which has been replaced by NoteStore.
// New tests should be added using the NoteStore-based graph view.

// ============================================================================
// KnowledgeRepository Adapter
// ============================================================================

use crucible_core::parser::ParsedNote;
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_core::{CrucibleError, Result as CrucibleResult};

/// SurrealDB-backed implementation of KnowledgeRepository.
///
/// Uses the notes table from the NoteStore schema to provide knowledge access.
struct SurrealKnowledgeRepository {
    client: Arc<SurrealClient>,
}

impl SurrealKnowledgeRepository {
    fn new(client: Arc<SurrealClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl KnowledgeRepository for SurrealKnowledgeRepository {
    async fn get_note_by_name(&self, name: &str) -> CrucibleResult<Option<ParsedNote>> {
        // Query notes table for note with matching path or title
        let sql = r#"
            SELECT * FROM notes
            WHERE path CONTAINS $name
               OR title = $name
            LIMIT 1
        "#;

        let result = self
            .client
            .query(sql, &[serde_json::json!({ "name": name })])
            .await
            .map_err(|e| CrucibleError::DatabaseError(format!("Query failed: {}", e)))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        // Parse the result into a minimal ParsedNote
        let record = &result.records[0];
        let path: &str = record
            .data
            .get("path")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("");
        let content: &str = record
            .data
            .get("content")
            .and_then(|v: &serde_json::Value| v.as_str())
            .unwrap_or("");

        // Create a minimal ParsedNote from the stored data
        let mut note = ParsedNote::new(std::path::PathBuf::from(path));
        note.content.plain_text = content.to_string();

        Ok(Some(note))
    }

    async fn list_notes(&self, path: Option<&str>) -> CrucibleResult<Vec<NoteInfo>> {
        let (sql, params) = if let Some(path_filter) = path {
            (
                r#"SELECT path, title, tags, updated_at FROM notes WHERE path CONTAINS $path"#,
                serde_json::json!({ "path": path_filter }),
            )
        } else {
            (
                r#"SELECT path, title, tags, updated_at FROM notes"#,
                serde_json::json!({}),
            )
        };

        let result = self
            .client
            .query(sql, &[params])
            .await
            .map_err(|e| CrucibleError::DatabaseError(format!("Query failed: {}", e)))?;

        let notes = result
            .records
            .iter()
            .map(|r| {
                let path: &str = r
                    .data
                    .get("path")
                    .and_then(|v: &serde_json::Value| v.as_str())
                    .unwrap_or("");
                let title: Option<&str> = r
                    .data
                    .get("title")
                    .and_then(|v: &serde_json::Value| v.as_str());
                let tags: Vec<String> = r
                    .data
                    .get("tags")
                    .and_then(|v: &serde_json::Value| v.as_array())
                    .map(|arr: &Vec<serde_json::Value>| {
                        arr.iter()
                            .filter_map(|v: &serde_json::Value| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                // Extract name from path (filename without extension)
                let name = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path)
                    .to_string();

                NoteInfo {
                    name,
                    path: path.to_string(),
                    title: title.map(String::from),
                    tags,
                    created_at: None,
                    updated_at: None,
                }
            })
            .collect();

        Ok(notes)
    }

    async fn search_vectors(&self, _vector: Vec<f32>) -> CrucibleResult<Vec<crucible_core::types::SearchResult>> {
        // Vector search requires an embedding provider, which is not available here.
        // This is a placeholder - callers should use semantic_search directly with a provider.
        Ok(Vec::new())
    }
}
