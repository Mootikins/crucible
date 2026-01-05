//! SurrealDB NoteStore Implementation
//!
//! This module provides a SurrealDB-backed implementation of the [`NoteStore`] trait
//! from `crucible-core`. It handles storage, retrieval, and semantic search of note
//! metadata with efficient vector similarity queries.
//!
//! ## Features
//!
//! - **CRUD operations**: Upsert, get, delete, and list note records
//! - **Content-addressed lookup**: Find notes by their BLAKE3 content hash
//! - **Semantic search**: Vector similarity search with cosine distance
//! - **Filtering**: Tag and path-based filters for search queries
//!
//! ## Usage
//!
//! ```ignore
//! use crucible_surrealdb::SurrealNoteStore;
//! use crucible_surrealdb::SurrealClient;
//! use crucible_core::storage::{NoteStore, NoteRecord, Filter};
//!
//! async fn example() -> anyhow::Result<()> {
//!     let client = SurrealClient::new_memory().await?;
//!     let store = SurrealNoteStore::new(client);
//!     store.apply_schema().await?;
//!
//!     // Use NoteStore trait methods...
//!     let notes = store.list().await?;
//!     Ok(())
//! }
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};

use crucible_core::events::{NoteChangeType, SessionEvent};
use crucible_core::parser::BlockHash;
use crucible_core::storage::{
    Filter, NoteRecord, NoteStore, SearchResult, StorageError, StorageResult,
};

use crate::SurrealClient;

/// Schema version for the NoteStore tables
const SCHEMA_VERSION: &str = "note_store_v1.0.0";

/// Default embedding dimensions (all-MiniLM-L6-v2)
const DEFAULT_EMBEDDING_DIMENSIONS: usize = 384;

/// Static flag to track if schema has been applied in this process
static SCHEMA_APPLIED: AtomicBool = AtomicBool::new(false);

/// SurrealDB-backed implementation of the NoteStore trait
///
/// This struct wraps a [`SurrealClient`] and provides efficient storage and
/// retrieval of note metadata. It uses the `note_records` table defined in
/// `schema_notes.surql`.
#[derive(Clone)]
pub struct SurrealNoteStore {
    /// The underlying SurrealDB client
    client: SurrealClient,
    /// Embedding dimensions for vector index (configurable)
    embedding_dimensions: usize,
}

impl std::fmt::Debug for SurrealNoteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurrealNoteStore")
            .field("embedding_dimensions", &self.embedding_dimensions)
            .finish()
    }
}

/// Internal record format for SurrealDB serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SurrealNoteRecord {
    path: String,
    content_hash: String,
    embedding: Option<Vec<f32>>,
    title: String,
    tags: Vec<String>,
    links_to: Vec<String>,
    properties: serde_json::Value,
    updated_at: DateTime<Utc>,
}

impl From<NoteRecord> for SurrealNoteRecord {
    fn from(note: NoteRecord) -> Self {
        Self {
            path: note.path,
            content_hash: note.content_hash.to_hex(),
            embedding: note.embedding,
            title: note.title,
            tags: note.tags,
            links_to: note.links_to,
            properties: serde_json::to_value(&note.properties).unwrap_or_default(),
            updated_at: note.updated_at,
        }
    }
}

impl TryFrom<SurrealNoteRecord> for NoteRecord {
    type Error = StorageError;

    fn try_from(record: SurrealNoteRecord) -> Result<Self, Self::Error> {
        let content_hash = BlockHash::from_hex(&record.content_hash)
            .map_err(|e| StorageError::Deserialization(format!("Invalid content hash: {}", e)))?;

        let properties = match record.properties {
            serde_json::Value::Object(map) => map.into_iter().collect(),
            _ => std::collections::HashMap::new(),
        };

        Ok(Self {
            path: record.path,
            content_hash,
            embedding: record.embedding,
            title: record.title,
            tags: record.tags,
            links_to: record.links_to,
            properties,
            updated_at: record.updated_at,
        })
    }
}

impl SurrealNoteStore {
    /// Create a new SurrealNoteStore with the given client
    pub fn new(client: SurrealClient) -> Self {
        Self {
            client,
            embedding_dimensions: DEFAULT_EMBEDDING_DIMENSIONS,
        }
    }

    /// Create a new SurrealNoteStore with custom embedding dimensions
    pub fn with_dimensions(client: SurrealClient, dimensions: usize) -> Self {
        Self {
            client,
            embedding_dimensions: dimensions,
        }
    }

    /// Get a reference to the underlying client
    pub fn client(&self) -> &SurrealClient {
        &self.client
    }

    /// Get the configured embedding dimensions
    pub fn embedding_dimensions(&self) -> usize {
        self.embedding_dimensions
    }

    /// Apply the NoteStore schema to the database
    ///
    /// This method is idempotent and safe to call multiple times.
    /// It uses process-level caching to avoid redundant schema checks.
    pub async fn apply_schema(&self) -> anyhow::Result<()> {
        // Fast path: if we've already applied schema in this process, skip
        if SCHEMA_APPLIED.load(Ordering::Relaxed) {
            trace!("NoteStore schema already applied in this process, skipping");
            return Ok(());
        }

        // Check if schema version exists in database
        if self.check_schema_version().await? {
            debug!(
                "NoteStore schema version {} already present, skipping initialization",
                SCHEMA_VERSION
            );
            SCHEMA_APPLIED.store(true, Ordering::Relaxed);
            return Ok(());
        }

        // Apply schema using batched approach
        self.apply_schema_batched().await?;

        // Ensure vector index exists
        self.ensure_vector_index().await?;

        // Mark schema version in database
        self.mark_schema_version().await?;

        // Update process-level cache
        SCHEMA_APPLIED.store(true, Ordering::Relaxed);
        debug!("NoteStore schema {} applied successfully", SCHEMA_VERSION);

        Ok(())
    }

    /// Check if the current schema version is already applied
    async fn check_schema_version(&self) -> anyhow::Result<bool> {
        let query = "SELECT * FROM _note_store_schema WHERE version = $version LIMIT 1";
        let params = vec![serde_json::json!({ "version": SCHEMA_VERSION })];

        let result = self.client.query(query, &params).await;

        match result {
            Ok(r) => Ok(!r.records.is_empty()),
            Err(_) => Ok(false),
        }
    }

    /// Mark the current schema version as applied
    async fn mark_schema_version(&self) -> anyhow::Result<()> {
        let queries = format!(
            r#"
            DEFINE TABLE IF NOT EXISTS _note_store_schema SCHEMAFULL;
            DEFINE FIELD IF NOT EXISTS version ON TABLE _note_store_schema TYPE string;
            DEFINE FIELD IF NOT EXISTS applied_at ON TABLE _note_store_schema TYPE datetime DEFAULT time::now();
            DELETE _note_store_schema;
            CREATE _note_store_schema SET version = '{}', applied_at = time::now();
            "#,
            SCHEMA_VERSION
        );

        for query in queries.split(';') {
            let trimmed = query.trim();
            if !trimmed.is_empty() {
                let _ = self.client.query(trimmed, &[]).await;
            }
        }

        Ok(())
    }

    /// Apply schema using batched statements
    async fn apply_schema_batched(&self) -> anyhow::Result<()> {
        let schema = include_str!("schema_notes.surql");
        let start = std::time::Instant::now();

        // Collect valid statements
        let statements: Vec<&str> = schema
            .split(';')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.starts_with("--"))
            .collect();

        debug!("Applying {} NoteStore schema statements", statements.len());

        // Try batched execution first
        let batch_query = statements.join(";\n");
        let batch_result = self.client.query(&batch_query, &[]).await;

        if batch_result.is_ok() {
            debug!(
                "NoteStore schema applied via batch in {:?}",
                start.elapsed()
            );
            return Ok(());
        }

        // Fallback: execute statements individually
        debug!("Batch failed, falling back to individual statement execution");
        for statement in statements {
            let result = self.client.query(statement, &[]).await;
            if let Err(e) = result {
                let err_msg = format!("{}", e);
                // Ignore "already exists" errors for idempotency
                if !err_msg.contains("already exists")
                    && !err_msg.contains("already defined")
                    && !err_msg.contains("IF NOT EXISTS")
                {
                    return Err(anyhow::anyhow!(
                        "Failed to execute NoteStore schema statement '{}...': {}",
                        &statement[..statement.len().min(50)],
                        e
                    ));
                }
                trace!(
                    "Schema element already exists (ignoring): {}...",
                    &statement[..statement.len().min(30)]
                );
            }
        }

        debug!(
            "NoteStore schema applied via individual statements in {:?}",
            start.elapsed()
        );
        Ok(())
    }

    /// Ensure the vector index exists with the correct dimensions
    async fn ensure_vector_index(&self) -> anyhow::Result<()> {
        let query = format!(
            "DEFINE INDEX IF NOT EXISTS note_embedding_idx ON TABLE note_records \
             COLUMNS embedding MTREE DIMENSION {} DISTANCE COSINE",
            self.embedding_dimensions
        );

        self.client
            .query(&query, &[])
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

        debug!(
            "Vector index created/verified with {} dimensions",
            self.embedding_dimensions
        );
        Ok(())
    }

    /// Clear the schema cache (useful for testing)
    #[cfg(test)]
    pub fn clear_schema_cache() {
        SCHEMA_APPLIED.store(false, Ordering::Relaxed);
    }

    /// Convert a Filter to a SurrealQL WHERE clause fragment
    ///
    /// Note: The actual parameter values are provided via `build_filter_params`.
    /// This method only generates the clause structure with parameter placeholders.
    fn filter_to_where_clause(filter: &Filter) -> String {
        match filter {
            Filter::Tag(_) => {
                // Tag filter: check if $tag is in the tags array
                "$tag INSIDE tags".to_string()
            }
            Filter::Path(_) => {
                // Path prefix filter: check if path starts with $path_prefix
                "string::starts_with(path, $path_prefix)".to_string()
            }
            Filter::Property(key, op, _) => {
                // Property filter: compare a frontmatter property
                let op_str = match op {
                    crucible_core::storage::Op::Eq => "=",
                    crucible_core::storage::Op::Ne => "!=",
                    crucible_core::storage::Op::Gt => ">",
                    crucible_core::storage::Op::Lt => "<",
                    crucible_core::storage::Op::Gte => ">=",
                    crucible_core::storage::Op::Lte => "<=",
                    crucible_core::storage::Op::Contains => "CONTAINS",
                    crucible_core::storage::Op::Matches => "~",
                };
                format!("properties.{} {} $prop_value", key, op_str)
            }
            Filter::And(filters) => {
                let clauses: Vec<String> =
                    filters.iter().map(Self::filter_to_where_clause).collect();
                format!("({})", clauses.join(" AND "))
            }
            Filter::Or(filters) => {
                let clauses: Vec<String> =
                    filters.iter().map(Self::filter_to_where_clause).collect();
                format!("({})", clauses.join(" OR "))
            }
        }
    }

    /// Build filter parameters for a query
    fn build_filter_params(filter: &Filter) -> serde_json::Value {
        match filter {
            Filter::Tag(tag) => serde_json::json!({ "tag": tag }),
            Filter::Path(prefix) => serde_json::json!({ "path_prefix": prefix }),
            Filter::Property(_, _, value) => serde_json::json!({ "prop_value": value }),
            Filter::And(filters) | Filter::Or(filters) => {
                // For compound filters, merge all parameter objects
                let mut merged = serde_json::Map::new();
                for f in filters {
                    if let serde_json::Value::Object(map) = Self::build_filter_params(f) {
                        for (k, v) in map {
                            merged.insert(k, v);
                        }
                    }
                }
                serde_json::Value::Object(merged)
            }
        }
    }
}

#[async_trait]
impl NoteStore for SurrealNoteStore {
    /// Insert or update a note record
    async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
        // First check if the note exists to determine which event to emit
        let existed = self.get(&note.path).await?.is_some();

        let surreal_note = SurrealNoteRecord::from(note);

        let query = r#"
            UPSERT note_records:[$path] CONTENT {
                path: $path,
                content_hash: $content_hash,
                embedding: $embedding,
                title: $title,
                tags: $tags,
                links_to: $links_to,
                properties: $properties,
                updated_at: $updated_at
            }
        "#;

        let params = vec![serde_json::json!({
            "path": surreal_note.path,
            "content_hash": surreal_note.content_hash,
            "embedding": surreal_note.embedding,
            "title": surreal_note.title,
            "tags": surreal_note.tags,
            "links_to": surreal_note.links_to,
            "properties": surreal_note.properties,
            "updated_at": surreal_note.updated_at.to_rfc3339()
        })];

        self.client
            .query(query, &params)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to upsert note: {}", e)))?;

        // Return appropriate event based on whether the note existed before
        let event = if existed {
            SessionEvent::NoteModified {
                path: surreal_note.path.into(),
                change_type: NoteChangeType::Content,
            }
        } else {
            SessionEvent::NoteCreated {
                path: surreal_note.path.into(),
                title: Some(surreal_note.title),
            }
        };

        Ok(vec![event])
    }

    /// Get a note record by path
    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        let query = "SELECT * FROM note_records WHERE path = $path LIMIT 1";
        let params = vec![serde_json::json!({ "path": path })];

        let result = self
            .client
            .query(query, &params)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to get note: {}", e)))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let record = &result.records[0];
        let surreal_record: SurrealNoteRecord = serde_json::from_value(
            serde_json::to_value(&record.data)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?,
        )
        .map_err(|e| StorageError::Deserialization(e.to_string()))?;

        Ok(Some(surreal_record.try_into()?))
    }

    /// Delete a note record by path (idempotent)
    async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
        // Check if the note exists before deletion
        let existed = self.get(path).await?.is_some();

        let query = "DELETE note_records WHERE path = $path";
        let params = vec![serde_json::json!({ "path": path })];

        self.client
            .query(query, &params)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to delete note: {}", e)))?;

        Ok(SessionEvent::NoteDeleted {
            path: path.into(),
            existed,
        })
    }

    /// List all note records
    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        let query = "SELECT * FROM note_records ORDER BY updated_at DESC";

        let result = self
            .client
            .query(query, &[])
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to list notes: {}", e)))?;

        let mut notes = Vec::with_capacity(result.records.len());
        for record in &result.records {
            let surreal_record: SurrealNoteRecord = serde_json::from_value(
                serde_json::to_value(&record.data)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?,
            )
            .map_err(|e| StorageError::Deserialization(e.to_string()))?;
            notes.push(surreal_record.try_into()?);
        }

        Ok(notes)
    }

    /// Find a note by its content hash
    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        let query = "SELECT * FROM note_records WHERE content_hash = $hash LIMIT 1";
        let params = vec![serde_json::json!({ "hash": hash.to_hex() })];

        let result = self
            .client
            .query(query, &params)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to get note by hash: {}", e)))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let record = &result.records[0];
        let surreal_record: SurrealNoteRecord = serde_json::from_value(
            serde_json::to_value(&record.data)
                .map_err(|e| StorageError::Deserialization(e.to_string()))?,
        )
        .map_err(|e| StorageError::Deserialization(e.to_string()))?;

        Ok(Some(surreal_record.try_into()?))
    }

    /// Search notes by embedding similarity
    async fn search(
        &self,
        embedding: &[f32],
        k: usize,
        filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>> {
        // Build the base vector search query
        let (where_clause, params) = if let Some(ref f) = filter {
            let clause = Self::filter_to_where_clause(f);
            let mut filter_params = Self::build_filter_params(f);

            // Add embedding and limit params
            if let serde_json::Value::Object(ref mut map) = filter_params {
                map.insert("query_embedding".to_string(), serde_json::json!(embedding));
                map.insert("k".to_string(), serde_json::json!(k));
            }

            (
                format!("WHERE embedding IS NOT NONE AND {}", clause),
                vec![filter_params],
            )
        } else {
            (
                "WHERE embedding IS NOT NONE".to_string(),
                vec![serde_json::json!({
                    "query_embedding": embedding,
                    "k": k
                })],
            )
        };

        // Use SurrealDB's vector similarity function
        let query = format!(
            r#"
            SELECT *,
                   vector::similarity::cosine(embedding, $query_embedding) AS score
            FROM note_records
            {}
            ORDER BY score DESC
            LIMIT $k
            "#,
            where_clause
        );

        let result = self
            .client
            .query(&query, &params)
            .await
            .map_err(|e| StorageError::Backend(format!("Failed to search notes: {}", e)))?;

        let mut search_results = Vec::with_capacity(result.records.len());
        for record in &result.records {
            // Extract score from the result
            let score = record
                .data
                .get("score")
                .and_then(|v| v.as_f64())
                .map(|s| s as f32)
                .unwrap_or(0.0);

            // Remove score from data before deserializing
            let mut data = record.data.clone();
            data.remove("score");

            let surreal_record: SurrealNoteRecord = serde_json::from_value(
                serde_json::to_value(&data)
                    .map_err(|e| StorageError::Deserialization(e.to_string()))?,
            )
            .map_err(|e| StorageError::Deserialization(e.to_string()))?;

            let note: NoteRecord = surreal_record.try_into()?;
            search_results.push(SearchResult::new(note, score));
        }

        Ok(search_results)
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Create a `SurrealNoteStore` from a SurrealDB client with default embedding dimensions.
///
/// This factory function creates the store and applies the schema, ensuring the
/// database is ready for use immediately.
///
/// # Arguments
///
/// * `client` - A SurrealDB client connection
///
/// # Returns
///
/// A configured and initialized `SurrealNoteStore` ready for CRUD operations
/// and semantic search.
///
/// # Errors
///
/// Returns an error if schema application fails.
///
/// # Example
///
/// ```ignore
/// use crucible_surrealdb::{SurrealClient, create_note_store};
///
/// let client = SurrealClient::new_memory().await?;
/// let store = create_note_store(client).await?;
/// ```
pub async fn create_note_store(client: SurrealClient) -> StorageResult<SurrealNoteStore> {
    let store = SurrealNoteStore::new(client);
    store
        .apply_schema()
        .await
        .map_err(|e| StorageError::Backend(format!("Failed to apply NoteStore schema: {}", e)))?;
    Ok(store)
}

/// Create a `SurrealNoteStore` with custom embedding dimensions.
///
/// Use this variant when your embedding model produces vectors with a different
/// dimension than the default (384 for all-MiniLM-L6-v2).
///
/// # Arguments
///
/// * `client` - A SurrealDB client connection
/// * `dimensions` - The number of dimensions for the embedding vector index
///
/// # Returns
///
/// A configured and initialized `SurrealNoteStore` with the specified embedding
/// dimensions.
///
/// # Errors
///
/// Returns an error if schema application fails.
///
/// # Example
///
/// ```ignore
/// use crucible_surrealdb::{SurrealClient, create_note_store_with_dimensions};
///
/// let client = SurrealClient::new_memory().await?;
/// // Use 768 dimensions for a larger embedding model
/// let store = create_note_store_with_dimensions(client, 768).await?;
/// ```
pub async fn create_note_store_with_dimensions(
    client: SurrealClient,
    dimensions: usize,
) -> StorageResult<SurrealNoteStore> {
    let store = SurrealNoteStore::with_dimensions(client, dimensions);
    store
        .apply_schema()
        .await
        .map_err(|e| StorageError::Backend(format!("Failed to apply NoteStore schema: {}", e)))?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_surreal_note_store_new() {
        let client = SurrealClient::new_memory().await.unwrap();
        let store = SurrealNoteStore::new(client);
        assert_eq!(store.embedding_dimensions, DEFAULT_EMBEDDING_DIMENSIONS);
    }

    #[tokio::test]
    async fn test_surreal_note_store_with_dimensions() {
        let client = SurrealClient::new_memory().await.unwrap();
        let store = SurrealNoteStore::with_dimensions(client, 768);
        assert_eq!(store.embedding_dimensions, 768);
    }

    #[tokio::test]
    async fn test_filter_to_where_clause() {
        let tag_filter = Filter::Tag("rust".to_string());
        assert!(SurrealNoteStore::filter_to_where_clause(&tag_filter).contains("INSIDE tags"));

        let path_filter = Filter::Path("projects/".to_string());
        assert!(SurrealNoteStore::filter_to_where_clause(&path_filter).contains("starts_with"));

        let and_filter = Filter::And(vec![
            Filter::Tag("rust".to_string()),
            Filter::Path("notes/".to_string()),
        ]);
        let clause = SurrealNoteStore::filter_to_where_clause(&and_filter);
        assert!(clause.contains("AND"));
    }

    #[tokio::test]
    async fn test_surreal_note_record_conversion() {
        let note = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Test Note")
            .with_tags(vec!["rust".to_string()]);

        let surreal: SurrealNoteRecord = note.clone().into();
        assert_eq!(surreal.path, "test/note.md");
        assert_eq!(surreal.title, "Test Note");
        assert_eq!(surreal.content_hash.len(), 64); // hex string

        let back: NoteRecord = surreal.try_into().unwrap();
        assert_eq!(back.path, note.path);
        assert_eq!(back.title, note.title);
    }

    #[tokio::test]
    async fn test_create_note_store_factory() {
        // Clear schema cache to ensure fresh test
        SurrealNoteStore::clear_schema_cache();

        let client = SurrealClient::new_memory().await.unwrap();
        let store = create_note_store(client).await.unwrap();

        // Verify default dimensions
        assert_eq!(store.embedding_dimensions(), DEFAULT_EMBEDDING_DIMENSIONS);

        // Verify store is usable (schema was applied)
        let notes = store.list().await.unwrap();
        assert!(notes.is_empty());
    }

    #[tokio::test]
    async fn test_create_note_store_with_dimensions_factory() {
        // Clear schema cache to ensure fresh test
        SurrealNoteStore::clear_schema_cache();

        let client = SurrealClient::new_memory().await.unwrap();
        let store = create_note_store_with_dimensions(client, 1536)
            .await
            .unwrap();

        // Verify custom dimensions
        assert_eq!(store.embedding_dimensions(), 1536);

        // Verify store is usable (schema was applied)
        let notes = store.list().await.unwrap();
        assert!(notes.is_empty());
    }
}
