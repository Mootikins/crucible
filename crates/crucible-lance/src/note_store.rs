//! NoteStore implementation for LanceDB
//!
//! Provides LanceDB-backed storage for note metadata with native vector search.
//! LanceDB is a vector-native database optimized for embedding search, making
//! it an excellent choice for semantic search over notes.
//!
//! # Design Notes
//!
//! - LanceDB is append-only, so upsert = delete + insert
//! - Vector search is native and highly optimized
//! - Tags/links/properties are stored as JSON strings for simplicity
//! - Uses Arrow RecordBatch for data transfer

use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::{
    Array, BinaryArray, FixedSizeListArray, Float32Array, Int64Array, RecordBatch,
    RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use futures::TryStreamExt;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::{LanceError, LanceResult};
use crucible_core::parser::BlockHash;
use crucible_core::storage::{Filter, NoteRecord, NoteStore, Op, SearchResult, StorageResult};

// ============================================================================
// Constants
// ============================================================================

const TABLE_NAME: &str = "notes";
const DEFAULT_EMBEDDING_DIM: usize = 768;

// ============================================================================
// Schema Definition
// ============================================================================

/// Create the Arrow schema for the notes table
fn notes_schema(embedding_dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("content_hash", DataType::Binary, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                embedding_dim as i32,
            ),
            true, // nullable - some notes may not have embeddings
        ),
        Field::new("title", DataType::Utf8, false),
        Field::new("tags", DataType::Utf8, false), // JSON array
        Field::new("links_to", DataType::Utf8, false), // JSON array
        Field::new("properties", DataType::Utf8, false), // JSON object
        Field::new("updated_at", DataType::Int64, false), // Unix timestamp ms
    ])
}

// ============================================================================
// Filter Translation
// ============================================================================

/// Translate a Filter to LanceDB SQL-like filter expression
///
/// LanceDB uses a SQL-like syntax for filtering. Tags and properties are
/// stored as JSON strings, so we use string matching for contains operations.
fn filter_to_lance(filter: &Filter) -> String {
    match filter {
        Filter::Tag(tag) => {
            // Tags are stored as JSON array string, e.g., '["rust","test"]' or '["rust"]'
            // We need to match the tag as a complete JSON string element.
            // Possible patterns:
            // - Single tag: '["rust"]'
            // - First in list: '["rust",...]'
            // - Middle in list: '[...,"rust",...]'
            // - Last in list: '[...,"rust"]'
            let escaped = escape_sql_string(tag);
            format!(
                "(tags = '[\"{e}\"]' OR tags LIKE '[\"{e}\",%' OR tags LIKE '%,\"{e}\"]' OR tags LIKE '%,\"{e}\",%')",
                e = escaped
            )
        }
        Filter::Path(prefix) => {
            let escaped = escape_sql_string(prefix);
            format!("path LIKE '{}%'", escaped)
        }
        Filter::Property(key, op, value) => {
            // Properties are stored as a JSON object string
            // Since LanceDB may not support json_extract, we use string matching
            // This approach checks if the key exists and the value matches
            let escaped_key = escape_sql_string(key);

            let value_str = match value {
                Value::String(s) => {
                    let escaped = escape_sql_string(s);
                    if matches!(op, Op::Contains) {
                        // For contains, just check if value appears anywhere
                        escaped
                    } else {
                        escaped
                    }
                }
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                _ => escape_sql_string(&value.to_string()),
            };

            // For equality checks on string values, look for the pattern "key":"value"
            // For numeric values, look for "key":value (without quotes)
            match (op, value) {
                (Op::Eq, Value::String(_)) => {
                    format!("properties LIKE '%\"{}\":\"{}\"%'", escaped_key, value_str)
                }
                (Op::Eq, Value::Number(_) | Value::Bool(_)) => {
                    format!("properties LIKE '%\"{}\":{}%'", escaped_key, value_str)
                }
                (Op::Ne, _) => {
                    // For not-equal, we just check the key exists but negate the value match
                    format!(
                        "properties LIKE '%\"{}\":%' AND properties NOT LIKE '%{}%'",
                        escaped_key, value_str
                    )
                }
                (Op::Contains, _) => {
                    // Check if property contains the substring
                    format!("properties LIKE '%\"{}\":%{}%'", escaped_key, value_str)
                }
                _ => {
                    // For comparison operators (Gt, Lt, Gte, Lte, Matches), fall back to basic key check
                    // These are difficult to implement with string matching on JSON
                    format!("properties LIKE '%\"{}\":%'", escaped_key)
                }
            }
        }
        Filter::And(filters) => {
            if filters.is_empty() {
                return "1=1".to_string();
            }
            let clauses: Vec<_> = filters.iter().map(filter_to_lance).collect();
            format!("({})", clauses.join(" AND "))
        }
        Filter::Or(filters) => {
            if filters.is_empty() {
                return "1=0".to_string();
            }
            let clauses: Vec<_> = filters.iter().map(filter_to_lance).collect();
            format!("({})", clauses.join(" OR "))
        }
    }
}

/// Escape a string for use in SQL expressions
fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''").replace('\\', "\\\\")
}

// ============================================================================
// Arrow Conversion Utilities
// ============================================================================

/// Convert a NoteRecord to an Arrow RecordBatch
fn note_to_batch(
    note: &NoteRecord,
    schema: &Schema,
    embedding_dim: usize,
) -> LanceResult<RecordBatch> {
    // Path column
    let path = StringArray::from(vec![note.path.as_str()]);

    // Content hash column (32 bytes)
    let hash_bytes: Vec<&[u8]> = vec![note.content_hash.as_bytes()];
    let content_hash = BinaryArray::from(hash_bytes);

    // Embedding column (FixedSizeList of Float32)
    let embedding: Arc<dyn Array> = match &note.embedding {
        Some(emb) => {
            if emb.len() != embedding_dim {
                return Err(LanceError::Schema(format!(
                    "Embedding dimension mismatch: expected {}, got {}",
                    embedding_dim,
                    emb.len()
                )));
            }
            let values = Float32Array::from(emb.clone());
            let field = Arc::new(Field::new("item", DataType::Float32, true));
            Arc::new(
                FixedSizeListArray::try_new(field, embedding_dim as i32, Arc::new(values), None)
                    .map_err(|e| LanceError::Arrow(e.to_string()))?,
            )
        }
        None => {
            // Create a null embedding
            let values = Float32Array::from(vec![0.0f32; embedding_dim]);
            let field = Arc::new(Field::new("item", DataType::Float32, true));
            let list = FixedSizeListArray::try_new(
                field,
                embedding_dim as i32,
                Arc::new(values),
                Some(vec![false].into()),
            )
            .map_err(|e| LanceError::Arrow(e.to_string()))?;
            Arc::new(list)
        }
    };

    // Title column
    let title = StringArray::from(vec![note.title.as_str()]);

    // Tags column (JSON array)
    let tags_json = serde_json::to_string(&note.tags)?;
    let tags = StringArray::from(vec![tags_json.as_str()]);

    // Links column (JSON array)
    let links_json = serde_json::to_string(&note.links_to)?;
    let links_to = StringArray::from(vec![links_json.as_str()]);

    // Properties column (JSON object)
    let props_json = serde_json::to_string(&note.properties)?;
    let properties = StringArray::from(vec![props_json.as_str()]);

    // Updated at column (Unix timestamp in milliseconds)
    let updated_at = Int64Array::from(vec![note.updated_at.timestamp_millis()]);

    RecordBatch::try_new(
        Arc::new(schema.clone()),
        vec![
            Arc::new(path),
            Arc::new(content_hash),
            embedding,
            Arc::new(title),
            Arc::new(tags),
            Arc::new(links_to),
            Arc::new(properties),
            Arc::new(updated_at),
        ],
    )
    .map_err(|e| LanceError::Arrow(e.to_string()))
}

/// Convert an Arrow RecordBatch to NoteRecords
fn batch_to_notes(batch: &RecordBatch) -> LanceResult<Vec<NoteRecord>> {
    let num_rows = batch.num_rows();
    let mut notes = Vec::with_capacity(num_rows);

    // Extract columns
    let path_col = batch
        .column_by_name("path")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| LanceError::Conversion("path column not found or wrong type".to_string()))?;

    let hash_col = batch
        .column_by_name("content_hash")
        .and_then(|c| c.as_any().downcast_ref::<BinaryArray>())
        .ok_or_else(|| {
            LanceError::Conversion("content_hash column not found or wrong type".to_string())
        })?;

    let embedding_col = batch
        .column_by_name("embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>());

    let title_col = batch
        .column_by_name("title")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| {
            LanceError::Conversion("title column not found or wrong type".to_string())
        })?;

    let tags_col = batch
        .column_by_name("tags")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| LanceError::Conversion("tags column not found or wrong type".to_string()))?;

    let links_col = batch
        .column_by_name("links_to")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| {
            LanceError::Conversion("links_to column not found or wrong type".to_string())
        })?;

    let props_col = batch
        .column_by_name("properties")
        .and_then(|c| c.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| {
            LanceError::Conversion("properties column not found or wrong type".to_string())
        })?;

    let updated_col = batch
        .column_by_name("updated_at")
        .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
        .ok_or_else(|| {
            LanceError::Conversion("updated_at column not found or wrong type".to_string())
        })?;

    for i in 0..num_rows {
        // Path
        let path = path_col.value(i).to_string();

        // Content hash
        let hash_bytes = hash_col.value(i);
        let content_hash = if hash_bytes.len() == 32 {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(hash_bytes);
            BlockHash::new(arr)
        } else {
            BlockHash::zero()
        };

        // Embedding
        let embedding = embedding_col.and_then(|emb_col| {
            if emb_col.is_null(i) {
                None
            } else {
                let list_value = emb_col.value(i);
                list_value
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .map(|float_arr| float_arr.values().to_vec())
            }
        });

        // Title
        let title = title_col.value(i).to_string();

        // Tags
        let tags_json = tags_col.value(i);
        let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();

        // Links
        let links_json = links_col.value(i);
        let links_to: Vec<String> = serde_json::from_str(links_json).unwrap_or_default();

        // Properties
        let props_json = props_col.value(i);
        let properties: HashMap<String, Value> =
            serde_json::from_str(props_json).unwrap_or_default();

        // Updated at
        let timestamp_ms = updated_col.value(i);
        let updated_at = Utc
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .unwrap_or_else(Utc::now);

        notes.push(NoteRecord {
            path,
            content_hash,
            embedding,
            title,
            tags,
            links_to,
            properties,
            updated_at,
        });
    }

    Ok(notes)
}

/// Extract distance/score from a search result batch
fn extract_distance(batch: &RecordBatch, row: usize) -> f32 {
    // LanceDB adds a _distance column for vector search results
    batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .map(|arr| arr.value(row))
        .unwrap_or(0.0)
}

/// Convert L2 distance to cosine similarity score
///
/// LanceDB uses L2 distance by default. For normalized vectors,
/// we can convert to cosine similarity: similarity = 1 - (distance^2 / 2)
fn distance_to_similarity(distance: f32) -> f32 {
    // For L2 distance on normalized vectors:
    // ||a - b||^2 = 2 - 2*cos(a,b)
    // cos(a,b) = 1 - ||a - b||^2 / 2
    let sim = 1.0 - (distance * distance) / 2.0;
    sim.clamp(0.0, 1.0)
}

// ============================================================================
// LanceNoteStore Implementation
// ============================================================================

/// LanceDB implementation of NoteStore
///
/// Provides efficient vector search using LanceDB's native capabilities.
/// LanceDB is optimized for embedding search and provides sub-linear
/// query times through approximate nearest neighbor indexing.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_lance::note_store::{LanceNoteStore, create_note_store};
/// use crucible_core::storage::NoteStore;
///
/// let store = create_note_store("/path/to/lance.db").await?;
///
/// // Use via NoteStore trait
/// let note = store.get("notes/example.md").await?;
/// ```
pub struct LanceNoteStore {
    connection: Arc<RwLock<Connection>>,
    table: Arc<RwLock<Option<Table>>>,
    schema: Schema,
    embedding_dim: usize,
    db_path: String,
}

impl LanceNoteStore {
    /// Create a new LanceNoteStore at the given path
    ///
    /// Opens or creates a LanceDB database at the specified path.
    /// If the notes table doesn't exist, it will be created on first write.
    pub async fn new(db_path: &str) -> LanceResult<Self> {
        Self::with_dimensions(db_path, DEFAULT_EMBEDDING_DIM).await
    }

    /// Create a new LanceNoteStore with custom embedding dimensions
    pub async fn with_dimensions(db_path: &str, embedding_dim: usize) -> LanceResult<Self> {
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let connection = lancedb::connect(db_path)
            .execute()
            .await
            .map_err(|e| LanceError::Connection(e.to_string()))?;

        let schema = notes_schema(embedding_dim);

        // Try to open existing table
        let table = match connection.open_table(TABLE_NAME).execute().await {
            Ok(t) => {
                debug!("Opened existing notes table");
                Some(t)
            }
            Err(_) => {
                debug!("Notes table doesn't exist yet, will create on first write");
                None
            }
        };

        Ok(Self {
            connection: Arc::new(RwLock::new(connection)),
            table: Arc::new(RwLock::new(table)),
            schema,
            embedding_dim,
            db_path: db_path.to_string(),
        })
    }

    /// Get or create the notes table
    async fn ensure_table(&self) -> LanceResult<Table> {
        // First check if we already have the table
        {
            let table_guard = self.table.read().await;
            if let Some(ref table) = *table_guard {
                return Ok(table.clone());
            }
        }

        // Need to create the table
        let mut table_guard = self.table.write().await;

        // Double-check after acquiring write lock
        if let Some(ref table) = *table_guard {
            return Ok(table.clone());
        }

        let conn = self.connection.read().await;

        // Try to open again (might have been created by another process)
        match conn.open_table(TABLE_NAME).execute().await {
            Ok(t) => {
                *table_guard = Some(t.clone());
                Ok(t)
            }
            Err(_) => {
                // Create an empty table with schema
                debug!("Creating new notes table");

                let schema = Arc::new(self.schema.clone());
                let table = conn
                    .create_empty_table(TABLE_NAME, schema)
                    .execute()
                    .await
                    .map_err(|e| LanceError::Table(e.to_string()))?;

                *table_guard = Some(table.clone());
                Ok(table)
            }
        }
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.db_path
    }

    /// Get the embedding dimensions
    pub fn embedding_dimensions(&self) -> usize {
        self.embedding_dim
    }

    /// Create or rebuild the vector index on the embedding column
    ///
    /// This should be called after bulk loading data for optimal search performance.
    /// LanceDB uses IVF-PQ indexing which requires a minimum number of rows
    /// (typically 256+) to be effective.
    ///
    /// Without an index, vector search falls back to brute-force scan.
    pub async fn create_index(&self) -> LanceResult<()> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Err(LanceError::Table("Table not created yet".to_string())),
        };

        // Create IVF-PQ index on the embedding column
        // Index::Auto will select appropriate parameters based on data
        table
            .create_index(&["embedding"], Index::Auto)
            .execute()
            .await
            .map_err(|e| LanceError::Table(format!("Failed to create index: {}", e)))?;

        debug!("Created vector index on embedding column");
        Ok(())
    }

    /// Check if the table has a vector index
    pub async fn has_index(&self) -> bool {
        let table_guard = self.table.read().await;
        if let Some(table) = &*table_guard {
            // Try to list indices - if embedding index exists, we have one
            if let Ok(indices) = table.list_indices().await {
                return indices
                    .iter()
                    .any(|idx| idx.columns.contains(&"embedding".to_string()));
            }
        }
        false
    }
}

#[async_trait]
impl NoteStore for LanceNoteStore {
    async fn upsert(&self, note: NoteRecord) -> StorageResult<()> {
        // LanceDB is append-only, so we need to delete first then insert
        // This is the standard pattern for LanceDB upserts
        self.delete(&note.path).await?;

        let table = self.ensure_table().await?;
        let batch = note_to_batch(&note, &self.schema, self.embedding_dim)?;
        let schema = Arc::new(self.schema.clone());

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| LanceError::Table(e.to_string()))?;

        debug!("Upserted note: {}", note.path);
        Ok(())
    }

    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Ok(None), // Table doesn't exist yet
        };

        let escaped_path = escape_sql_string(path);
        let filter = format!("path = '{}'", escaped_path);

        let results = table
            .query()
            .only_if(&filter)
            .limit(1)
            .execute()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?;

        if results.is_empty() {
            return Ok(None);
        }

        let batch = &results[0];
        if batch.num_rows() == 0 {
            return Ok(None);
        }

        let notes = batch_to_notes(batch)?;
        Ok(notes.into_iter().next())
    }

    async fn delete(&self, path: &str) -> StorageResult<()> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Ok(()), // Table doesn't exist, nothing to delete
        };

        let escaped_path = escape_sql_string(path);
        let filter = format!("path = '{}'", escaped_path);

        // LanceDB delete is idempotent - deleting non-existent rows is fine
        if let Err(e) = table.delete(&filter).await {
            // Log but don't fail - delete should be idempotent
            warn!("Delete warning for {}: {}", path, e);
        }

        debug!("Deleted note: {}", path);
        Ok(())
    }

    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Ok(vec![]), // Table doesn't exist yet
        };

        let results = table
            .query()
            .execute()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?;

        let mut notes = Vec::new();
        for batch in results {
            notes.extend(batch_to_notes(&batch)?);
        }

        // Sort by updated_at descending
        notes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(notes)
    }

    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Ok(None),
        };

        // Query all and filter in Rust since binary comparison in SQL can be tricky
        let results = table
            .query()
            .execute()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?;

        for batch in results {
            let notes = batch_to_notes(&batch)?;
            for note in notes {
                if note.content_hash == *hash {
                    return Ok(Some(note));
                }
            }
        }

        Ok(None)
    }

    async fn search(
        &self,
        embedding: &[f32],
        k: usize,
        filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>> {
        let table_guard = self.table.read().await;
        let table = match &*table_guard {
            Some(t) => t,
            None => return Ok(vec![]), // No table = no results
        };

        // Build the vector search query
        let query = table
            .vector_search(embedding.to_vec())
            .map_err(|e| LanceError::Query(e.to_string()))?;

        // Apply filter if provided
        let query = if let Some(ref f) = filter {
            let filter_expr = filter_to_lance(f);
            query.only_if(&filter_expr)
        } else {
            query
        };

        // Execute with limit
        // Request more than k to account for null embeddings that we'll filter out
        let results = query
            .limit(k * 2)
            .execute()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?
            .try_collect::<Vec<_>>()
            .await
            .map_err(|e| LanceError::Query(e.to_string()))?;

        let mut search_results = Vec::new();

        for batch in results {
            let notes = batch_to_notes(&batch)?;
            for (i, note) in notes.into_iter().enumerate() {
                // Skip notes without embeddings
                if note.embedding.is_none() {
                    continue;
                }

                let distance = extract_distance(&batch, i);
                let score = distance_to_similarity(distance);

                search_results.push(SearchResult::new(note, score));

                if search_results.len() >= k {
                    break;
                }
            }

            if search_results.len() >= k {
                break;
            }
        }

        // Sort by score descending (should already be sorted, but ensure it)
        search_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top k
        search_results.truncate(k);

        Ok(search_results)
    }
}

// ============================================================================
// Factory Functions
// ============================================================================

/// Create a new LanceNoteStore at the given path
///
/// This is a convenience function for creating a store with default settings.
pub async fn create_note_store(db_path: &str) -> StorageResult<LanceNoteStore> {
    LanceNoteStore::new(db_path)
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))
}

/// Create a new LanceNoteStore with custom embedding dimensions
pub async fn create_note_store_with_dimensions(
    db_path: &str,
    dimensions: usize,
) -> StorageResult<LanceNoteStore> {
    LanceNoteStore::with_dimensions(db_path, dimensions)
        .await
        .map_err(|e| crucible_core::storage::StorageError::Backend(e.to_string()))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test embedding dimensions for contract compatibility
    const TEST_EMBEDDING_DIM: usize = 8;

    async fn setup() -> (TempDir, LanceNoteStore) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.lance");
        let store = LanceNoteStore::with_dimensions(db_path.to_str().unwrap(), TEST_EMBEDDING_DIM)
            .await
            .unwrap();
        (dir, store)
    }

    fn make_note(path: &str, title: &str) -> NoteRecord {
        NoteRecord::new(path.to_string(), BlockHash::zero()).with_title(title.to_string())
    }

    fn make_test_embedding(seed: f32) -> Vec<f32> {
        let mut embedding: Vec<f32> = (0..TEST_EMBEDDING_DIM)
            .map(|i| seed + (i as f32) * 0.1)
            .collect();

        // Normalize for consistent cosine similarity behavior
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut embedding {
                *x /= norm;
            }
        }
        embedding
    }

    fn make_note_with_embedding(path: &str, title: &str, seed: f32) -> NoteRecord {
        NoteRecord::new(path.to_string(), BlockHash::zero())
            .with_title(title.to_string())
            .with_embedding(make_test_embedding(seed))
    }

    #[test]
    fn test_filter_to_lance_tag() {
        let filter = Filter::Tag("rust".to_string());
        let sql = filter_to_lance(&filter);
        assert!(sql.contains("tags LIKE"));
        assert!(sql.contains("rust"));
    }

    #[test]
    fn test_filter_to_lance_path() {
        let filter = Filter::Path("projects/".to_string());
        let sql = filter_to_lance(&filter);
        assert!(sql.contains("path LIKE"));
        assert!(sql.contains("projects/"));
    }

    #[test]
    fn test_filter_to_lance_and() {
        let filter = Filter::And(vec![
            Filter::Tag("rust".to_string()),
            Filter::Path("notes/".to_string()),
        ]);
        let sql = filter_to_lance(&filter);
        assert!(sql.contains(" AND "));
    }

    #[test]
    fn test_filter_to_lance_or() {
        let filter = Filter::Or(vec![
            Filter::Tag("rust".to_string()),
            Filter::Tag("python".to_string()),
        ]);
        let sql = filter_to_lance(&filter);
        assert!(sql.contains(" OR "));
    }

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("hello"), "hello");
        assert_eq!(escape_sql_string("it's"), "it''s");
        assert_eq!(escape_sql_string("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_distance_to_similarity() {
        // Distance 0 should give similarity 1
        assert!((distance_to_similarity(0.0) - 1.0).abs() < 0.001);

        // Larger distance should give lower similarity
        let sim1 = distance_to_similarity(0.5);
        let sim2 = distance_to_similarity(1.0);
        assert!(sim1 > sim2);
    }

    #[tokio::test]
    async fn test_create_store() {
        let (_dir, store) = setup().await;
        assert_eq!(store.embedding_dimensions(), TEST_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_upsert_and_get() {
        let (_dir, store) = setup().await;

        let note = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Test Note")
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()]);

        store.upsert(note.clone()).await.expect("Failed to upsert");

        let retrieved = store
            .get("test/note.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");

        assert_eq!(retrieved.path, "test/note.md");
        assert_eq!(retrieved.title, "Test Note");
        assert_eq!(retrieved.tags, vec!["rust", "test"]);
        assert_eq!(retrieved.links_to, vec!["other/note.md"]);
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let (_dir, store) = setup().await;

        // Initial insert
        let note1 = make_note("test/update.md", "Original Title");
        store.upsert(note1).await.expect("Failed to upsert");

        // Update with same path
        let note2 = NoteRecord::new("test/update.md", BlockHash::zero())
            .with_title("Updated Title")
            .with_tags(vec!["new-tag".to_string()]);
        store.upsert(note2).await.expect("Failed to update");

        let retrieved = store
            .get("test/update.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");

        assert_eq!(retrieved.title, "Updated Title");
        assert_eq!(retrieved.tags, vec!["new-tag"]);

        // Verify no duplicates
        let all = store.list().await.expect("Failed to list");
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let (_dir, store) = setup().await;

        let note = make_note("test/delete.md", "To Be Deleted");
        store.upsert(note).await.expect("Failed to upsert");

        assert!(store.get("test/delete.md").await.unwrap().is_some());

        store
            .delete("test/delete.md")
            .await
            .expect("Failed to delete");

        let result = store.get("test/delete.md").await.expect("Failed to get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_is_idempotent() {
        let (_dir, store) = setup().await;

        // Delete non-existent note - should not error
        let result = store.delete("does/not/exist.md").await;
        assert!(result.is_ok());

        // Delete twice
        let note = make_note("test/twice.md", "Delete Twice");
        store.upsert(note).await.expect("Failed to upsert");

        store.delete("test/twice.md").await.expect("First delete");
        let result = store.delete("test/twice.md").await;
        assert!(result.is_ok(), "Second delete should also succeed");
    }

    #[tokio::test]
    async fn test_get_nonexistent_returns_none() {
        let (_dir, store) = setup().await;

        let result = store.get("never/existed.md").await.expect("Failed to get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list() {
        let (_dir, store) = setup().await;

        for i in 0..3 {
            let note = make_note(&format!("note{}.md", i), &format!("Note {}", i));
            store.upsert(note).await.expect("Failed to upsert");
        }

        let all = store.list().await.expect("Failed to list");
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn test_list_empty() {
        let (_dir, store) = setup().await;

        let all = store.list().await.expect("Failed to list");
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn test_get_by_hash() {
        let (_dir, store) = setup().await;

        let hash = BlockHash::new([1u8; 32]);
        let note = NoteRecord::new("test/hashed.md", hash).with_title("Hashed Note");
        store.upsert(note).await.expect("Failed to upsert");

        let found = store
            .get_by_hash(&hash)
            .await
            .expect("Failed to get by hash")
            .expect("Note should be found");

        assert_eq!(found.path, "test/hashed.md");
        assert_eq!(found.content_hash, hash);
    }

    #[tokio::test]
    async fn test_get_by_hash_nonexistent() {
        let (_dir, store) = setup().await;

        let hash = BlockHash::new([99u8; 32]);
        let result = store
            .get_by_hash(&hash)
            .await
            .expect("Failed to get by hash");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_search_by_embedding() {
        let (_dir, store) = setup().await;

        let note1 = make_note_with_embedding("note1.md", "Rust", 1.0);
        let note2 = make_note_with_embedding("note2.md", "Python", 5.0);
        let note3 = make_note_with_embedding("note3.md", "Mixed", 2.5);

        store.upsert(note1).await.expect("upsert 1");
        store.upsert(note2).await.expect("upsert 2");
        store.upsert(note3).await.expect("upsert 3");

        let query = make_test_embedding(1.0);
        let results = store.search(&query, 10, None).await.expect("search");

        assert_eq!(results.len(), 3);
        // First result should be note1 (exact match)
        assert_eq!(results[0].note.path, "note1.md");
        // Scores should be descending
        for i in 1..results.len() {
            assert!(
                results[i - 1].score >= results[i].score,
                "Results should be sorted by score descending"
            );
        }
    }

    #[tokio::test]
    async fn test_search_respects_k_limit() {
        let (_dir, store) = setup().await;

        for i in 0..10 {
            let note = make_note_with_embedding(
                &format!("note{}.md", i),
                &format!("Note {}", i),
                i as f32,
            );
            store.upsert(note).await.expect("upsert");
        }

        let query = make_test_embedding(5.0);
        let results = store.search(&query, 3, None).await.expect("search");

        assert_eq!(results.len(), 3, "Should return exactly k results");
    }

    #[tokio::test]
    async fn test_search_excludes_notes_without_embedding() {
        let (_dir, store) = setup().await;

        let note_with = make_note_with_embedding("with.md", "Has Embedding", 1.0);
        store.upsert(note_with).await.expect("upsert");

        let note_without = make_note("without.md", "No Embedding");
        store.upsert(note_without).await.expect("upsert");

        let query = make_test_embedding(1.0);
        let results = store.search(&query, 10, None).await.expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.path, "with.md");
    }

    #[tokio::test]
    async fn test_properties_preserved() {
        let (_dir, store) = setup().await;

        let mut props = HashMap::new();
        props.insert("string".to_string(), Value::String("value".to_string()));
        props.insert("number".to_string(), Value::Number(42.into()));
        props.insert("bool".to_string(), Value::Bool(true));

        let note = NoteRecord::new("props.md", BlockHash::zero()).with_properties(props.clone());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("props.md").await.unwrap().unwrap();

        assert_eq!(retrieved.properties.get("string"), props.get("string"));
        assert_eq!(retrieved.properties.get("number"), props.get("number"));
        assert_eq!(retrieved.properties.get("bool"), props.get("bool"));
    }

    #[tokio::test]
    async fn test_embedding_preserved() {
        let (_dir, store) = setup().await;

        let embedding = make_test_embedding(1.5);
        let note = NoteRecord::new("embed.md", BlockHash::zero())
            .with_title("Embedded")
            .with_embedding(embedding.clone());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("embed.md").await.unwrap().unwrap();
        let retrieved_embedding = retrieved.embedding.expect("Should have embedding");

        assert_eq!(embedding.len(), retrieved_embedding.len());
        for (original, stored) in embedding.iter().zip(retrieved_embedding.iter()) {
            assert!(
                (original - stored).abs() < 1e-5,
                "Embedding values should be preserved"
            );
        }
    }

    #[tokio::test]
    async fn test_empty_collections_handled() {
        let (_dir, store) = setup().await;

        let note = NoteRecord::new("empty.md", BlockHash::zero())
            .with_title("Empty Collections")
            .with_tags(vec![])
            .with_links(vec![])
            .with_properties(HashMap::new());

        store.upsert(note).await.expect("upsert");

        let retrieved = store.get("empty.md").await.unwrap().unwrap();
        assert!(retrieved.tags.is_empty());
        assert!(retrieved.links_to.is_empty());
        assert!(retrieved.properties.is_empty());
    }
}
