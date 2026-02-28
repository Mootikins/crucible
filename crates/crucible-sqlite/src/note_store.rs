//! NoteStore implementation for SQLite
//!
//! Provides SQLite-backed storage for note metadata with vector search support.
//! The vector search uses brute-force cosine similarity computed in Rust.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, params_from_iter, OptionalExtension, ToSql};
use serde_json::Value;
use tracing::debug;

use crate::connection::SqlitePool;
use crucible_core::events::SessionEvent;
use crucible_core::parser::BlockHash;
use crucible_core::storage::{
    Filter, NoteRecord, NoteStore, Op, SearchResult, StorageError, StorageResult,
};

// ============================================================================
// Schema
// ============================================================================

/// SQL schema for the notes table and note_links junction table
const NOTES_SCHEMA: &str = r#"
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

CREATE INDEX IF NOT EXISTS notes_hash_idx ON notes(content_hash);
CREATE INDEX IF NOT EXISTS notes_updated_idx ON notes(updated_at);

-- Junction table for fast inlinks queries
-- Denormalized from links_to JSON array for O(1) reverse lookups
CREATE TABLE IF NOT EXISTS note_links (
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    PRIMARY KEY (source_path, target_path),
    FOREIGN KEY (source_path) REFERENCES notes(path) ON DELETE CASCADE
);

-- Index for inlinks queries: "what notes link to this path?"
CREATE INDEX IF NOT EXISTS note_links_target_idx ON note_links(target_path);
"#;

// ============================================================================
// Embedding Serialization
// ============================================================================

/// Serialize an embedding vector to raw bytes (f32 little-endian)
fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Deserialize raw bytes to an embedding vector
fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().expect("chunk should be 4 bytes");
            f32::from_le_bytes(arr)
        })
        .collect()
}

// ============================================================================
// Schema Migration
// ============================================================================

/// Ensure embedding metadata columns exist in the notes table
///
/// This is an idempotent migration that checks if columns exist before adding them.
/// Uses PRAGMA table_info to avoid ALTER TABLE errors on existing columns.
fn ensure_embedding_metadata_columns(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    // Check if embedding_model column exists
    let has_embedding_model = conn.query_row(
        "SELECT 1 FROM pragma_table_info('notes') WHERE name = 'embedding_model'",
        [],
        |_| Ok(()),
    ).is_ok();

    // Check if embedding_dimensions column exists
    let has_embedding_dimensions = conn.query_row(
        "SELECT 1 FROM pragma_table_info('notes') WHERE name = 'embedding_dimensions'",
        [],
        |_| Ok(()),
    ).is_ok();

    // Add embedding_model column if it doesn't exist
    if !has_embedding_model {
        conn.execute(
            "ALTER TABLE notes ADD COLUMN embedding_model TEXT",
            [],
        )?;
    }

    // Add embedding_dimensions column if it doesn't exist
    if !has_embedding_dimensions {
        conn.execute(
            "ALTER TABLE notes ADD COLUMN embedding_dimensions INTEGER",
            [],
        )?;
    }

    Ok(())
}

// ============================================================================
// Cosine Similarity
// ============================================================================

/// Compute cosine similarity between two vectors
///
/// Returns a value in the range [-1, 1] for normalized vectors.
/// Returns 0.0 if either vector has zero magnitude.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

// ============================================================================
// Filter Translation
// ============================================================================

/// Validate that a property key is safe for interpolation into a JSON path.
///
/// Allows: alphanumeric, underscore, dot (for nested paths), hyphen.
/// Rejects: empty strings, SQL metacharacters, quotes, parens, semicolons.
fn validate_property_key(key: &str) -> StorageResult<()> {
    if key.is_empty() {
        return Err(StorageError::InvalidOperation(
            "Property key must not be empty".to_string(),
        ));
    }
    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
    {
        return Err(StorageError::InvalidOperation(format!(
            "Property key contains invalid characters: {:?}",
            key
        )));
    }
    Ok(())
}

fn filter_to_sql(
    filter: &Filter,
    params: &mut Vec<Box<dyn ToSql + Send>>,
) -> StorageResult<String> {
    match filter {
        Filter::Tag(tag) => {
            params.push(Box::new(tag.clone()));
            Ok("EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ?)".to_string())
        }
        Filter::Path(prefix) => {
            params.push(Box::new(format!("{}%", prefix)));
            Ok("path LIKE ?".to_string())
        }
        Filter::Property(key, op, value) => {
            validate_property_key(key)?;

            let op_str = match op {
                Op::Eq => "=",
                Op::Ne => "!=",
                Op::Gt => ">",
                Op::Lt => "<",
                Op::Gte => ">=",
                Op::Lte => "<=",
                Op::Contains => "LIKE",
                Op::Matches => "GLOB",
            };

            let param_value: Box<dyn ToSql + Send> = match op {
                Op::Contains => {
                    let pattern = match value {
                        Value::String(s) => format!("%{}%", s),
                        other => format!("%{}%", other),
                    };
                    Box::new(pattern)
                }
                _ => match value {
                    Value::String(s) => Box::new(s.clone()),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i)
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f)
                        } else {
                            Box::new(n.to_string())
                        }
                    }
                    Value::Bool(b) => Box::new(*b),
                    Value::Null => Box::new(Option::<String>::None),
                    _ => Box::new(value.to_string()),
                },
            };
            params.push(param_value);
            Ok(format!(
                "json_extract(properties, '$.{}') {} ?",
                key, op_str
            ))
        }
        Filter::And(filters) => {
            if filters.is_empty() {
                return Ok("1=1".to_string());
            }
            let clauses: Vec<_> = filters
                .iter()
                .map(|f| filter_to_sql(f, params))
                .collect::<StorageResult<Vec<_>>>()?;
            Ok(format!("({})", clauses.join(" AND ")))
        }
        Filter::Or(filters) => {
            if filters.is_empty() {
                return Ok("1=0".to_string());
            }
            let clauses: Vec<_> = filters
                .iter()
                .map(|f| filter_to_sql(f, params))
                .collect::<StorageResult<Vec<_>>>()?;
            Ok(format!("({})", clauses.join(" OR ")))
        }
    }
}

// ============================================================================
// Row Conversion
// ============================================================================

/// Convert a database row to a NoteRecord
fn row_to_note(row: &rusqlite::Row<'_>) -> Result<NoteRecord, rusqlite::Error> {
    let path: String = row.get(0)?;
    let content_hash_bytes: Vec<u8> = row.get(1)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(2)?;
    let embedding_model: Option<String> = row.get(3)?;
    let embedding_dimensions: Option<i32> = row.get(4)?;
    let title: String = row.get(5)?;
    let tags_json: String = row.get(6)?;
    let links_json: String = row.get(7)?;
    let properties_json: String = row.get(8)?;
    let updated_at_str: String = row.get(9)?;

    // Parse content hash
    let content_hash = if content_hash_bytes.len() == 32 {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&content_hash_bytes);
        BlockHash::new(arr)
    } else {
        BlockHash::zero()
    };

    // Parse embedding
    let embedding = embedding_bytes.map(|bytes| deserialize_embedding(&bytes));

    // Parse tags
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    // Parse links
    let links_to: Vec<String> = serde_json::from_str(&links_json).unwrap_or_default();

    // Parse properties
    let properties: HashMap<String, Value> =
        serde_json::from_str(&properties_json).unwrap_or_default();

    // Parse updated_at
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    Ok(NoteRecord {
        path,
        content_hash,
        embedding,
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        title,
        tags,
        links_to,
        properties,
        updated_at,
    })
}

// ============================================================================
// SqliteNoteStore
// ============================================================================

/// SQLite implementation of NoteStore
///
/// Stores note metadata in SQLite with JSON arrays for tags and links.
/// Vector search uses brute-force cosine similarity computed in Rust.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_sqlite::{SqliteConfig, SqlitePool, note_store::SqliteNoteStore};
/// use crucible_core::storage::NoteStore;
///
/// let pool = SqlitePool::new(SqliteConfig::memory())?;
/// let store = SqliteNoteStore::new(pool);
/// store.apply_schema().await?;
///
/// // Now use via the NoteStore trait
/// let note = store.get("notes/example.md").await?;
/// ```
#[derive(Clone)]
pub struct SqliteNoteStore {
    pool: SqlitePool,
}

impl SqliteNoteStore {
    /// Create a new NoteStore with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Apply the notes table schema
    ///
    /// This should be called once when initializing the store.
    pub async fn apply_schema(&self) -> StorageResult<()> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute_batch(NOTES_SCHEMA)
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                debug!("Notes schema applied successfully");

                // Apply idempotent migration for embedding metadata columns
                ensure_embedding_metadata_columns(conn)
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                debug!("Embedding metadata columns ensured");

                Ok(())
            })
        })
        .await??;

        Ok(())
}

}

#[async_trait]
impl NoteStore for SqliteNoteStore {
    async fn upsert(&self, note: NoteRecord) -> StorageResult<Vec<SessionEvent>> {
        let pool = self.pool.clone();

        let result = tokio::task::spawn_blocking(move || {
            pool.with_transaction(|conn| {
                // Serialize fields
                let content_hash_bytes = note.content_hash.as_bytes().to_vec();
                let embedding_bytes = note.embedding.as_ref().map(|e| serialize_embedding(e));
                let embedding_model = note.embedding_model.clone();
                let embedding_dimensions = note.embedding_dimensions;
                let tags_json = serde_json::to_string(&note.tags)?;
                let links_json = serde_json::to_string(&note.links_to)?;
                let properties_json = serde_json::to_string(&note.properties)?;
                let updated_at_str = note.updated_at.to_rfc3339();

                // Check if the note existed before to determine appropriate event
                let existed = conn.query_row(
                    "SELECT 1 FROM notes WHERE path = ?1",
                    [&note.path],
                    |row| row.get::<_, i32>(0),
                ).optional().is_ok_and(|opt| opt.is_some());

                // Upsert the note
                conn.execute(
                    r#"
                    INSERT INTO notes (path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                    ON CONFLICT(path) DO UPDATE SET
                        content_hash = excluded.content_hash,
                        embedding = excluded.embedding,
                        embedding_model = excluded.embedding_model,
                        embedding_dimensions = excluded.embedding_dimensions,
                        title = excluded.title,
                        tags = excluded.tags,
                        links_to = excluded.links_to,
                        properties = excluded.properties,
                        updated_at = excluded.updated_at
                    "#,
                    params![
                        note.path,
                        content_hash_bytes,
                        embedding_bytes,
                        embedding_model,
                        embedding_dimensions,
                        note.title,
                        tags_json,
                        links_json,
                        properties_json,
                        updated_at_str,
                    ],
                )
                .map_err(|e| StorageError::Backend(e.to_string()))?;

                // Update note_links junction table for fast inlinks queries
                conn.execute(
                    "DELETE FROM note_links WHERE source_path = ?1",
                    params![note.path],
                )
                .map_err(|e| StorageError::Backend(e.to_string()))?;

                // Insert new links
                if !note.links_to.is_empty() {
                    let mut stmt = conn
                        .prepare(
                        "INSERT OR IGNORE INTO note_links (source_path, target_path) VALUES (?1, ?2)",
                    )
                        .map_err(|e| StorageError::Backend(e.to_string()))?;
                    for target in &note.links_to {
                        stmt.execute(params![note.path, target])
                            .map_err(|e| StorageError::Backend(e.to_string()))?;
                    }
                }

                let event = if existed {
                    SessionEvent::NoteModified {
                        path: note.path.clone().into(),
                        change_type: crucible_core::events::NoteChangeType::Content,
                    }
                } else {
                    SessionEvent::NoteCreated {
                        path: note.path.clone().into(),
                        title: Some(note.title.clone()),
                    }
                };

                Ok(vec![event])
            })
        })
        .await??;

        Ok(result)
    }

    async fn get(&self, path: &str) -> StorageResult<Option<NoteRecord>> {
        let pool = self.pool.clone();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    WHERE path = ?1
                    "#,
                )
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                let note = stmt
                    .query_row([&path], row_to_note)
                    .optional()
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                Ok(note)
            })
        })
        .await?
    }

    async fn delete(&self, path: &str) -> StorageResult<SessionEvent> {
        let pool = self.pool.clone();
        let path_str = path.to_string();
        let path_for_event = path_str.clone();

        let existed = tokio::task::spawn_blocking(move || {
            pool.with_transaction(|conn| {
                // Check if the note exists before deletion
                let existed = conn
                    .query_row("SELECT 1 FROM notes WHERE path = ?1", [&path_str], |row| {
                        row.get::<_, i32>(0)
                    })
                    .optional()
                    .is_ok_and(|opt| opt.is_some());

                conn.execute("DELETE FROM notes WHERE path = ?1", [&path_str])
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                Ok(existed)
            })
        })
        .await??;

        // Return NoteDeleted event
        let event = SessionEvent::NoteDeleted {
            path: path_for_event.into(),
            existed,
        };
        Ok(event)
    }

    async fn list(&self) -> StorageResult<Vec<NoteRecord>> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    ORDER BY updated_at DESC
                    "#,
                )
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                let notes = stmt
                    .query_map([], row_to_note)
                    .map_err(|e| StorageError::Backend(e.to_string()))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                Ok(notes)
            })
        })
        .await?
    }

    async fn get_by_hash(&self, hash: &BlockHash) -> StorageResult<Option<NoteRecord>> {
        let pool = self.pool.clone();
        let hash_bytes = hash.as_bytes().to_vec();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    WHERE content_hash = ?1
                    LIMIT 1
                    "#,
                )
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                let note = stmt
                    .query_row([&hash_bytes], row_to_note)
                    .optional()
                    .map_err(|e| StorageError::Backend(e.to_string()))?;
                Ok(note)
            })
        })
        .await?
    }

    async fn search(
        &self,
        embedding: &[f32],
        k: usize,
        filter: Option<Filter>,
    ) -> StorageResult<Vec<SearchResult>> {
        let pool = self.pool.clone();
        let query_embedding = embedding.to_vec();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Build SQL query with optional filter
                let (sql, params) = if let Some(ref filter) = filter {
                    let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
                    let where_clause = filter_to_sql(filter, &mut params)?;
                    let sql = format!(
                        r#"
                        SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                        FROM notes
                        WHERE embedding IS NOT NULL AND {}
                        "#,
                        where_clause
                    );
                    (sql, params)
                } else {
                    let sql = r#"
                        SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                        FROM notes
                        WHERE embedding IS NOT NULL
                    "#
                    .to_string();
                    (sql, Vec::new())
                };

                // Execute query
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                // Collect notes with their embeddings
                let mut results: Vec<(NoteRecord, f32)> = Vec::new();

                // Build params slice for query
                let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref() as &dyn ToSql).collect();

                let rows = stmt
                    .query_map(params_from_iter(param_refs), |row| {
                    let note = row_to_note(row)?;
                    Ok(note)
                })
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                for row_result in rows {
                    let note = row_result.map_err(|e| StorageError::Backend(e.to_string()))?;
                    if let Some(ref note_embedding) = note.embedding {
                        let score = cosine_similarity(&query_embedding, note_embedding);
                        results.push((note, score));
                    }
                }

                // Sort by score descending
                results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

                // Take top k results
                let top_k: Vec<SearchResult> = results
                    .into_iter()
                    .take(k)
                    .map(|(note, score)| SearchResult::new(note, score))
                    .collect();

                Ok(top_k)
            })
        })
        .await?
    }
}

// ============================================================================
// Factory Function
// ============================================================================

/// Create a new SqliteNoteStore with schema applied
///
/// This is a convenience function that creates the store and applies the schema.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_sqlite::{SqliteConfig, SqlitePool, note_store::create_note_store};
///
/// let pool = SqlitePool::new(SqliteConfig::memory())?;
/// let store = create_note_store(pool).await?;
/// ```
pub async fn create_note_store(pool: SqlitePool) -> StorageResult<SqliteNoteStore> {
    let store = SqliteNoteStore::new(pool);
    store.apply_schema().await?;
    Ok(store)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_embedding() {
        let original = vec![1.0_f32, 2.5, -std::f32::consts::PI, 0.0, f32::MAX, f32::MIN];
        let bytes = serialize_embedding(&original);
        let restored = deserialize_embedding(&bytes);

        assert_eq!(original.len(), restored.len());
        for (a, b) in original.iter().zip(restored.iter()) {
            assert!((a - b).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_different_lengths() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_filter_to_sql_tag() {
        let filter = Filter::Tag("rust".to_string());
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let sql = filter_to_sql(&filter, &mut params).unwrap();

        assert!(sql.contains("json_each(tags)"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_filter_to_sql_path() {
        let filter = Filter::Path("projects/".to_string());
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let sql = filter_to_sql(&filter, &mut params).unwrap();

        assert!(sql.contains("LIKE"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_filter_to_sql_property() {
        let filter = Filter::Property(
            "status".to_string(),
            Op::Eq,
            Value::String("draft".to_string()),
        );
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let sql = filter_to_sql(&filter, &mut params).unwrap();

        assert!(sql.contains("json_extract"));
        assert!(sql.contains("status"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn test_filter_to_sql_and() {
        let filter = Filter::And(vec![
            Filter::Tag("rust".to_string()),
            Filter::Path("notes/".to_string()),
        ]);
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let sql = filter_to_sql(&filter, &mut params).unwrap();

        assert!(sql.contains(" AND "));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_filter_to_sql_or() {
        let filter = Filter::Or(vec![
            Filter::Tag("rust".to_string()),
            Filter::Tag("python".to_string()),
        ]);
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let sql = filter_to_sql(&filter, &mut params).unwrap();

        assert!(sql.contains(" OR "));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_filter_rejects_malicious_property_key_sql_injection() {
        // A key like this could escape the JSON path and inject SQL
        let filter = Filter::Property(
            "foo') OR ('1'='1".to_string(),
            Op::Eq,
            Value::String("bar".to_string()),
        );
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let result = filter_to_sql(&filter, &mut params);
        assert!(result.is_err(), "Malicious property key should be rejected");
    }

    #[test]
    fn test_filter_rejects_property_key_with_semicolon() {
        let filter = Filter::Property(
            "key; DROP TABLE notes; --".to_string(),
            Op::Eq,
            Value::String("val".to_string()),
        );
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let result = filter_to_sql(&filter, &mut params);
        assert!(result.is_err(), "Key with semicolon should be rejected");
    }

    #[test]
    fn test_filter_rejects_property_key_with_quotes() {
        let filter = Filter::Property(
            r#"key"value"#.to_string(),
            Op::Eq,
            Value::String("val".to_string()),
        );
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let result = filter_to_sql(&filter, &mut params);
        assert!(result.is_err(), "Key with quotes should be rejected");
    }

    #[test]
    fn test_filter_accepts_valid_property_keys() {
        for key in &["status", "my_key", "nested.path", "key123", "_private"] {
            let filter =
                Filter::Property(key.to_string(), Op::Eq, Value::String("val".to_string()));
            let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
            let result = filter_to_sql(&filter, &mut params);
            assert!(result.is_ok(), "Valid key '{}' should be accepted", key);
        }
    }

    #[test]
    fn test_filter_rejects_empty_property_key() {
        let filter = Filter::Property("".to_string(), Op::Eq, Value::String("val".to_string()));
        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        let result = filter_to_sql(&filter, &mut params);
        assert!(result.is_err(), "Empty key should be rejected");
    }

    #[tokio::test]
    async fn test_note_store_crud() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        // Create a note
        let note = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Test Note")
            .with_tags(vec!["rust".to_string(), "test".to_string()])
            .with_links(vec!["other/note.md".to_string()]);

        // Upsert
        store.upsert(note.clone()).await.expect("Failed to upsert");

        // Get
        let retrieved = store
            .get("test/note.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");
        assert_eq!(retrieved.path, "test/note.md");
        assert_eq!(retrieved.title, "Test Note");
        assert_eq!(retrieved.tags, vec!["rust", "test"]);
        assert_eq!(retrieved.links_to, vec!["other/note.md"]);

        // Update
        let updated = NoteRecord::new("test/note.md", BlockHash::zero())
            .with_title("Updated Note")
            .with_tags(vec!["updated".to_string()]);
        store.upsert(updated).await.expect("Failed to update");

        let retrieved = store
            .get("test/note.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");
        assert_eq!(retrieved.title, "Updated Note");

        // Delete
        store
            .delete("test/note.md")
            .await
            .expect("Failed to delete");
        let deleted = store.get("test/note.md").await.expect("Failed to get");
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_note_store_list() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        // Create multiple notes
        for i in 0..3 {
            let note = NoteRecord::new(format!("note{}.md", i), BlockHash::zero())
                .with_title(format!("Note {}", i));
            store.upsert(note).await.expect("Failed to upsert");
        }

        // List all
        let notes = store.list().await.expect("Failed to list");
        assert_eq!(notes.len(), 3);
    }

    #[tokio::test]
    async fn test_note_store_get_by_hash() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        // Create a hash
        let hash = BlockHash::new([1u8; 32]);
        let note = NoteRecord::new("test.md", hash).with_title("Test");
        store.upsert(note).await.expect("Failed to upsert");

        // Find by hash
        let found = store
            .get_by_hash(&hash)
            .await
            .expect("Failed to get by hash")
            .expect("Note should exist");
        assert_eq!(found.path, "test.md");

        // Non-existent hash
        let not_found = store
            .get_by_hash(&BlockHash::new([2u8; 32]))
            .await
            .expect("Failed to get by hash");
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_note_store_search() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        // Create notes with embeddings
        let note1 = NoteRecord::new("note1.md", BlockHash::zero())
            .with_title("Rust Programming")
            .with_tags(vec!["rust".to_string()])
            .with_embedding(vec![1.0, 0.0, 0.0]);

        let note2 = NoteRecord::new("note2.md", BlockHash::zero())
            .with_title("Python Programming")
            .with_tags(vec!["python".to_string()])
            .with_embedding(vec![0.0, 1.0, 0.0]);

        let note3 = NoteRecord::new("note3.md", BlockHash::zero())
            .with_title("JavaScript")
            .with_embedding(vec![0.5, 0.5, 0.0]);

        store.upsert(note1).await.expect("Failed to upsert");
        store.upsert(note2).await.expect("Failed to upsert");
        store.upsert(note3).await.expect("Failed to upsert");

        // Search similar to note1
        let query = vec![1.0, 0.0, 0.0];
        let results = store
            .search(&query, 3, None)
            .await
            .expect("Failed to search");

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].note.path, "note1.md"); // Most similar
        assert!((results[0].score - 1.0).abs() < 1e-6); // Exact match

        // Search with filter
        let filter = Filter::Tag("rust".to_string());
        let filtered = store
            .search(&query, 10, Some(filter))
            .await
            .expect("Failed to search");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].note.path, "note1.md");
    }

    #[tokio::test]
    async fn test_note_store_search_with_path_filter() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        let note1 = NoteRecord::new("projects/rust/note.md", BlockHash::zero())
            .with_embedding(vec![1.0, 0.0, 0.0]);

        let note2 = NoteRecord::new("projects/python/note.md", BlockHash::zero())
            .with_embedding(vec![0.0, 1.0, 0.0]);

        let note3 = NoteRecord::new("personal/note.md", BlockHash::zero())
            .with_embedding(vec![0.0, 0.0, 1.0]);

        store.upsert(note1).await.expect("Failed to upsert");
        store.upsert(note2).await.expect("Failed to upsert");
        store.upsert(note3).await.expect("Failed to upsert");

        // Filter by path prefix
        let filter = Filter::Path("projects/".to_string());
        let query = vec![1.0, 0.0, 0.0];
        let results = store
            .search(&query, 10, Some(filter))
            .await
            .expect("Failed to search");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.note.path.starts_with("projects/")));
    }

    #[tokio::test]
    async fn test_note_store_with_properties() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        let mut props = HashMap::new();
        props.insert("status".to_string(), Value::String("published".to_string()));
        props.insert(
            "priority".to_string(),
            Value::Number(serde_json::Number::from(1)),
        );

        let note = NoteRecord::new("test.md", BlockHash::zero()).with_properties(props);

        store.upsert(note).await.expect("Failed to upsert");

        let retrieved = store
            .get("test.md")
            .await
            .expect("Failed to get")
            .expect("Note should exist");

        assert_eq!(
            retrieved.properties.get("status"),
            Some(&Value::String("published".to_string()))
        );
        assert_eq!(
            retrieved.properties.get("priority"),
            Some(&Value::Number(serde_json::Number::from(1)))
        );
    }

    #[tokio::test]
    async fn test_note_links_junction_table() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool.clone())
            .await
            .expect("Failed to create store");

        // Create notes with links
        let note_a = NoteRecord::new("a.md", BlockHash::zero())
            .with_title("Note A")
            .with_links(vec!["b.md".to_string(), "c.md".to_string()]);
        let note_b = NoteRecord::new("b.md", BlockHash::zero())
            .with_title("Note B")
            .with_links(vec!["c.md".to_string()]);
        let note_c = NoteRecord::new("c.md", BlockHash::zero()).with_title("Note C");

        store.upsert(note_a).await.expect("Failed to upsert A");
        store.upsert(note_b).await.expect("Failed to upsert B");
        store.upsert(note_c).await.expect("Failed to upsert C");

        // Query the junction table directly to verify it's populated
        let inlinks_to_c: Vec<String> = pool
            .with_connection(|conn| {
                let mut stmt = conn
                    .prepare("SELECT source_path FROM note_links WHERE target_path = ?1")
                    .unwrap();
                let rows = stmt.query_map(["c.md"], |row| row.get(0)).unwrap();
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .expect("Failed to query");

        // Both a.md and b.md link to c.md
        assert_eq!(inlinks_to_c.len(), 2);
        assert!(inlinks_to_c.contains(&"a.md".to_string()));
        assert!(inlinks_to_c.contains(&"b.md".to_string()));

        // Update note_a to remove link to c.md
        let updated_a = NoteRecord::new("a.md", BlockHash::zero())
            .with_title("Note A Updated")
            .with_links(vec!["b.md".to_string()]); // No longer links to c.md
        store.upsert(updated_a).await.expect("Failed to update A");

        // Verify junction table was updated
        let inlinks_to_c: Vec<String> = pool
            .with_connection(|conn| {
                let mut stmt = conn
                    .prepare("SELECT source_path FROM note_links WHERE target_path = ?1")
                    .unwrap();
                let rows = stmt.query_map(["c.md"], |row| row.get(0)).unwrap();
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .expect("Failed to query");

        // Only b.md links to c.md now
        assert_eq!(inlinks_to_c.len(), 1);
        assert!(inlinks_to_c.contains(&"b.md".to_string()));
    }
}
