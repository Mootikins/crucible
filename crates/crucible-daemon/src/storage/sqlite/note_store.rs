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

use crate::storage::sqlite::connection::SqlitePool;
use crate::storage::sqlite::error_ext::SqliteResultExt;
use crucible_core::events::SessionEvent;
use crucible_core::parser::BlockHash;
use crucible_core::storage::{
    Filter, NoteRecord, NoteStore, Op, Scope, SearchResult, StorageError, StorageResult,
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
    let has_embedding_model = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('notes') WHERE name = 'embedding_model'",
            [],
            |_| Ok(()),
        )
        .is_ok();

    // Check if embedding_dimensions column exists
    let has_embedding_dimensions = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('notes') WHERE name = 'embedding_dimensions'",
            [],
            |_| Ok(()),
        )
        .is_ok();

    // Add embedding_model column if it doesn't exist
    if !has_embedding_model {
        conn.execute("ALTER TABLE notes ADD COLUMN embedding_model TEXT", [])?;
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

/// Build the WHERE-clause fragment that enforces `authority.can_read(note_scope)`
/// at the SQL layer.
///
/// The note's scope is stored in `properties.scope` as a JSON object:
///   `{"kind":"global"}`, `{"kind":"workspace","path":"/x"}`, `{"kind":"user","id":"alice"}`.
///
/// Legacy / unstamped notes (no `properties.scope` value) are treated as
/// "the kiln this row was inserted into". Because SQLite is per-kiln, any
/// `Workspace` authority querying this DB is implicitly querying the same
/// kiln — so legacy rows are visible. `User` authority does NOT get legacy
/// rows: a user-scoped session never owns kiln-shared metadata.
///
/// This matches the migration policy in the plan: "treat as workspace-
/// scoped derived from the kiln they live in".
fn scope_authority_to_sql(authority: &Scope, params: &mut Vec<Box<dyn ToSql + Send>>) -> String {
    match authority {
        Scope::Global => {
            // Elevated authority — everything is visible. Use a tautology so
            // the parameter count is predictable downstream.
            "1=1".to_string()
        }
        Scope::Workspace { path } => {
            // Visible:
            //   (a) note scope kind is 'global', OR
            //   (b) note scope kind is 'workspace' AND path matches, OR
            //   (c) legacy: no scope property at all (treated as "this kiln").
            params.push(Box::new(path.to_string_lossy().to_string()));
            "(json_extract(properties, '$.scope.kind') = 'global' \
             OR (json_extract(properties, '$.scope.kind') = 'workspace' \
                 AND json_extract(properties, '$.scope.path') = ?) \
             OR json_extract(properties, '$.scope') IS NULL)"
                .to_string()
        }
        Scope::User { id } => {
            // Visible: 'global' OR ('user' with same id). Legacy rows are
            // NOT visible to user authority — kiln data is not user data.
            params.push(Box::new(id.clone()));
            "(json_extract(properties, '$.scope.kind') = 'global' \
             OR (json_extract(properties, '$.scope.kind') = 'user' \
                 AND json_extract(properties, '$.scope.id') = ?))"
                .to_string()
        }
    }
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
        Filter::Scope(authority) => Ok(scope_authority_to_sql(authority, params)),
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
/// use crucible_daemon::storage::sqlite::{SqliteConfig, SqlitePool, note_store::SqliteNoteStore};
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

    /// Get a reference to the underlying connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Apply the notes table schema
    ///
    /// This should be called once when initializing the store.
    pub async fn apply_schema(&self) -> StorageResult<()> {
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                conn.execute_batch(NOTES_SCHEMA).sql()?;
                debug!("Notes schema applied successfully");

                // Apply idempotent migration for embedding metadata columns
                ensure_embedding_metadata_columns(conn).sql()?;
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
                .sql()?;

                // Update note_links junction table for fast inlinks queries
                conn.execute(
                    "DELETE FROM note_links WHERE source_path = ?1",
                    params![note.path],
                )
                .sql()?;

                // Insert new links
                if !note.links_to.is_empty() {
                    let mut stmt = conn
                        .prepare(
                        "INSERT OR IGNORE INTO note_links (source_path, target_path) VALUES (?1, ?2)",
                    )
                        .sql()?;
                    for target in &note.links_to {
                        stmt.execute(params![note.path, target])
                            .sql()?;
                    }
                }

                let event = if existed {
                    SessionEvent::internal(crucible_core::events::InternalSessionEvent::NoteModified {
                        path: note.path.clone().into(),
                        change_type: crucible_core::events::NoteChangeType::Content,
                    })
                } else {
                    SessionEvent::internal(crucible_core::events::InternalSessionEvent::NoteCreated {
                        path: note.path.clone().into(),
                        title: Some(note.title.clone()),
                    })
                };

                Ok(vec![event])
            })
        })
        .await??;

        Ok(result)
    }

    async fn get(&self, path: &str, authority: &Scope) -> StorageResult<Option<NoteRecord>> {
        let pool = self.pool.clone();
        let path = path.to_string();
        let authority = authority.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Compose path predicate AND scope predicate at SQL layer so a
                // record outside the caller's authority is indistinguishable
                // from a missing record (no side channel).
                let mut scope_params: Vec<Box<dyn ToSql + Send>> = Vec::new();
                let scope_clause = scope_authority_to_sql(&authority, &mut scope_params);
                let sql = format!(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    WHERE path = ?1 AND {}
                    "#,
                    scope_clause,
                );

                let mut stmt = conn.prepare(&sql).sql()?;

                let mut all_params: Vec<&dyn ToSql> = Vec::with_capacity(1 + scope_params.len());
                all_params.push(&path as &dyn ToSql);
                for p in &scope_params {
                    all_params.push(p.as_ref() as &dyn ToSql);
                }

                let note = stmt
                    .query_row(params_from_iter(all_params), row_to_note)
                    .optional()
                    .sql()?;
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
                    .sql()?;
                Ok(existed)
            })
        })
        .await??;

        // Return NoteDeleted event
        let event =
            SessionEvent::internal(crucible_core::events::InternalSessionEvent::NoteDeleted {
                path: path_for_event.into(),
                existed,
            });
        Ok(event)
    }

    async fn list(&self, authority: &Scope) -> StorageResult<Vec<NoteRecord>> {
        let pool = self.pool.clone();
        let authority = authority.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut scope_params: Vec<Box<dyn ToSql + Send>> = Vec::new();
                let scope_clause = scope_authority_to_sql(&authority, &mut scope_params);
                let sql = format!(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    WHERE {}
                    ORDER BY updated_at DESC
                    "#,
                    scope_clause,
                );

                let mut stmt = conn.prepare(&sql).sql()?;
                let param_refs: Vec<&dyn ToSql> =
                    scope_params.iter().map(|p| p.as_ref() as &dyn ToSql).collect();

                let notes = stmt
                    .query_map(params_from_iter(param_refs), row_to_note)
                    .sql()?
                    .collect::<Result<Vec<_>, _>>()
                    .sql()?;

                Ok(notes)
            })
        })
        .await?
    }

    async fn get_by_hash(
        &self,
        hash: &BlockHash,
        authority: &Scope,
    ) -> StorageResult<Option<NoteRecord>> {
        let pool = self.pool.clone();
        let hash_bytes = hash.as_bytes().to_vec();
        let authority = authority.clone();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut scope_params: Vec<Box<dyn ToSql + Send>> = Vec::new();
                let scope_clause = scope_authority_to_sql(&authority, &mut scope_params);
                let sql = format!(
                    r#"
                    SELECT path, content_hash, embedding, embedding_model, embedding_dimensions, title, tags, links_to, properties, updated_at
                    FROM notes
                    WHERE content_hash = ?1 AND {}
                    LIMIT 1
                    "#,
                    scope_clause,
                );

                let mut stmt = conn.prepare(&sql).sql()?;

                let mut all_params: Vec<&dyn ToSql> = Vec::with_capacity(1 + scope_params.len());
                all_params.push(&hash_bytes as &dyn ToSql);
                for p in &scope_params {
                    all_params.push(p.as_ref() as &dyn ToSql);
                }

                let note = stmt
                    .query_row(params_from_iter(all_params), row_to_note)
                    .optional()
                    .sql()?;
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
                    .sql()?;

                // Collect notes with their embeddings
                let mut results: Vec<(NoteRecord, f32)> = Vec::new();

                // Build params slice for query
                let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref() as &dyn ToSql).collect();

                let rows = stmt
                    .query_map(params_from_iter(param_refs), |row| {
                    let note = row_to_note(row)?;
                    Ok(note)
                })
                    .sql()?;

                for row_result in rows {
                    let note = row_result.sql()?;
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
/// use crucible_daemon::storage::sqlite::{SqliteConfig, SqlitePool, note_store::create_note_store};
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
    async fn schema_idempotency_apply_twice_no_error() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = SqliteNoteStore::new(pool);

        store
            .apply_schema()
            .await
            .expect("First apply_schema should succeed");
        store
            .apply_schema()
            .await
            .expect("Second apply_schema should also succeed");
    }

    #[tokio::test]
    async fn embedding_metadata_round_trip() {
        let pool = SqlitePool::memory().expect("Failed to create pool");
        let store = create_note_store(pool)
            .await
            .expect("Failed to create store");

        let note = NoteRecord::new("metadata.md", BlockHash::zero())
            .with_title("Metadata")
            .with_embedding(vec![0.1, 0.2, 0.3])
            .with_embedding_metadata("test-model".to_string(), 384);

        store.upsert(note).await.expect("Failed to upsert note");

        let retrieved = store
            .get("metadata.md", &Scope::Global)
            .await
            .expect("Failed to retrieve note")
            .expect("Expected note to exist");

        assert_eq!(retrieved.embedding_model.as_deref(), Some("test-model"));
        assert_eq!(retrieved.embedding_dimensions, Some(384));
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
            .get("test/note.md", &Scope::Global)
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
            .get("test/note.md", &Scope::Global)
            .await
            .expect("Failed to get")
            .expect("Note should exist");
        assert_eq!(retrieved.title, "Updated Note");

        // Delete
        store
            .delete("test/note.md")
            .await
            .expect("Failed to delete");
        let deleted = store
            .get("test/note.md", &Scope::Global)
            .await
            .expect("Failed to get");
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
        let notes = store.list(&Scope::Global).await.expect("Failed to list");
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
            .get_by_hash(&hash, &Scope::Global)
            .await
            .expect("Failed to get by hash")
            .expect("Note should exist");
        assert_eq!(found.path, "test.md");

        // Non-existent hash
        let not_found = store
            .get_by_hash(&BlockHash::new([2u8; 32]), &Scope::Global)
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
            .get("test.md", &Scope::Global)
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

    // =========================================================================
    // Memory Scoping — security tests
    // =========================================================================
    //
    // These tests pin the memory-scoping boundary at the SQLite layer. The
    // threat model lives in crucible-core/src/storage/scope.rs; these
    // verify the SQL-level enforcement matches that model.
    //
    // Each test seeds notes at known scopes via `with_scope`, then queries
    // through `NoteStore::{get,list,get_by_hash,search}` under a specific
    // request authority and asserts on visibility.

    use std::path::PathBuf;

    fn ws(p: &str) -> Scope {
        Scope::Workspace {
            path: PathBuf::from(p),
        }
    }

    async fn scoped_store_with_notes(notes: Vec<(&str, Scope)>) -> SqliteNoteStore {
        let pool = SqlitePool::memory().expect("pool");
        let store = create_note_store(pool).await.expect("store");
        for (path, scope) in notes {
            let n = NoteRecord::new(path, BlockHash::zero())
                .with_title(path)
                .with_embedding(vec![1.0, 0.0, 0.0])
                .with_scope(scope);
            store.upsert(n).await.expect("upsert");
        }
        store
    }

    #[tokio::test]
    async fn scope_workspace_a_cannot_read_workspace_b_notes() {
        let store = scoped_store_with_notes(vec![("a.md", ws("/a")), ("b.md", ws("/b"))]).await;

        // Authority is workspace A — only a.md visible.
        let notes = store.list(&ws("/a")).await.unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"a.md"), "got: {:?}", paths);
        assert!(!paths.contains(&"b.md"), "cross-scope leak: {:?}", paths);

        // get() directly also denies.
        let b_via_a = store.get("b.md", &ws("/a")).await.unwrap();
        assert!(
            b_via_a.is_none(),
            "get of b.md from workspace A must be None"
        );
    }

    #[tokio::test]
    async fn scope_user_alice_cannot_read_user_bob_notes() {
        let store = scoped_store_with_notes(vec![
            ("alice.md", Scope::user("alice")),
            ("bob.md", Scope::user("bob")),
        ])
        .await;

        let notes = store.list(&Scope::user("alice")).await.unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"alice.md"));
        assert!(!paths.contains(&"bob.md"), "user-scope leak: {:?}", paths);

        let bob_via_alice = store.get("bob.md", &Scope::user("alice")).await.unwrap();
        assert!(bob_via_alice.is_none());
    }

    #[tokio::test]
    async fn scope_global_visible_to_all_scopes() {
        let store = scoped_store_with_notes(vec![("global.md", Scope::Global)]).await;

        for auth in [Scope::Global, ws("/anywhere"), Scope::user("alice")] {
            let notes = store.list(&auth).await.unwrap();
            assert!(
                notes.iter().any(|n| n.path == "global.md"),
                "Global note must be visible to {:?}",
                auth
            );
        }
    }

    #[tokio::test]
    async fn scope_workspace_can_read_own_notes_plus_global() {
        let store = scoped_store_with_notes(vec![
            ("local.md", ws("/a")),
            ("global.md", Scope::Global),
            ("sibling.md", ws("/b")),
            ("private.md", Scope::user("alice")),
        ])
        .await;

        let notes = store.list(&ws("/a")).await.unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"local.md"));
        assert!(paths.contains(&"global.md"));
        assert!(!paths.contains(&"sibling.md"));
        assert!(!paths.contains(&"private.md"));
    }

    #[tokio::test]
    async fn user_only_sees_user_and_global() {
        // Per Wave 2 design: user scopes do NOT inherit workspace visibility.
        // The user authority strictly sees its own user notes + globals.
        let store = scoped_store_with_notes(vec![
            ("user_alice.md", Scope::user("alice")),
            ("user_bob.md", Scope::user("bob")),
            ("global.md", Scope::Global),
            ("workspace.md", ws("/a")),
        ])
        .await;

        let notes = store.list(&Scope::user("alice")).await.unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"user_alice.md"));
        assert!(paths.contains(&"global.md"));
        assert!(!paths.contains(&"user_bob.md"));
        assert!(
            !paths.contains(&"workspace.md"),
            "user authority must not see workspace-scoped notes"
        );
    }

    #[tokio::test]
    async fn get_by_hash_respects_scope() {
        let pool = SqlitePool::memory().expect("pool");
        let store = create_note_store(pool).await.expect("store");

        let h1 = BlockHash::new([1u8; 32]);
        let h2 = BlockHash::new([2u8; 32]);

        let mine = NoteRecord::new("mine.md", h1)
            .with_title("mine")
            .with_scope(ws("/a"));
        let theirs = NoteRecord::new("theirs.md", h2)
            .with_title("theirs")
            .with_scope(ws("/b"));
        store.upsert(mine).await.unwrap();
        store.upsert(theirs).await.unwrap();

        // Workspace A can hash-find mine but not theirs.
        assert!(store.get_by_hash(&h1, &ws("/a")).await.unwrap().is_some());
        assert!(
            store.get_by_hash(&h2, &ws("/a")).await.unwrap().is_none(),
            "cross-workspace hash lookup must be denied"
        );

        // Global authority sees both.
        assert!(store
            .get_by_hash(&h1, &Scope::Global)
            .await
            .unwrap()
            .is_some());
        assert!(store
            .get_by_hash(&h2, &Scope::Global)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn list_respects_scope() {
        let store = scoped_store_with_notes(vec![
            ("a.md", ws("/a")),
            ("b.md", ws("/b")),
            ("g.md", Scope::Global),
        ])
        .await;

        let global = store.list(&Scope::Global).await.unwrap();
        assert_eq!(global.len(), 3, "global authority sees all");

        let a_only = store.list(&ws("/a")).await.unwrap();
        assert_eq!(a_only.len(), 2, "workspace A sees own + global");
        assert!(a_only.iter().any(|n| n.path == "a.md"));
        assert!(a_only.iter().any(|n| n.path == "g.md"));
    }

    #[tokio::test]
    async fn search_respects_scope() {
        let store = scoped_store_with_notes(vec![
            ("a.md", ws("/a")),
            ("b.md", ws("/b")),
            ("g.md", Scope::Global),
        ])
        .await;

        let query = vec![1.0, 0.0, 0.0];
        // Search with explicit scope filter — only own + global visible.
        let filter = Some(crucible_core::storage::Filter::Scope(ws("/a")));
        let hits = store.search(&query, 10, filter).await.unwrap();
        let paths: Vec<_> = hits.iter().map(|h| h.note.path.as_str()).collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"g.md"));
        assert!(
            !paths.contains(&"b.md"),
            "search leaked cross-scope: {:?}",
            paths
        );
    }

    #[tokio::test]
    async fn unset_scope_defaults_to_workspace_derived_from_kiln() {
        // Notes seeded without a `scope` property are visible to ANY workspace
        // authority that's reading this DB. Per the migration policy in the
        // plan: legacy notes belong to "this kiln's workspace".
        let pool = SqlitePool::memory().expect("pool");
        let store = create_note_store(pool).await.expect("store");

        let legacy = NoteRecord::new("legacy.md", BlockHash::zero()).with_title("legacy");
        // No with_scope call — properties.scope is unset.
        store.upsert(legacy).await.unwrap();

        // Workspace authority sees the legacy note (DB is per-kiln, so this
        // is the "implicit workspace" the legacy note belonged to).
        let from_workspace = store.list(&ws("/some-kiln")).await.unwrap();
        assert_eq!(from_workspace.len(), 1);
        assert_eq!(from_workspace[0].path, "legacy.md");

        // User authority does NOT see legacy notes — kiln data isn't user data.
        let from_user = store.list(&Scope::user("alice")).await.unwrap();
        assert!(
            from_user.is_empty(),
            "user authority must not see legacy unstamped notes"
        );
    }

    #[tokio::test]
    async fn explicit_scope_frontmatter_overrides_derived() {
        // If a note is stamped with an explicit scope, the SQL filter
        // honors that — even if the value is narrower or broader than
        // what the kiln's binding would derive.
        let store = scoped_store_with_notes(vec![
            ("global.md", Scope::Global),
            ("user.md", Scope::user("alice")),
        ])
        .await;

        // A workspace-authority list should see global (explicit) but NOT
        // the user-scoped one even though both live in the same DB.
        let notes = store.list(&ws("/anywhere")).await.unwrap();
        let paths: Vec<_> = notes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"global.md"));
        assert!(
            !paths.contains(&"user.md"),
            "explicit user-scope must override the legacy-default-to-workspace fallback"
        );
    }
}
