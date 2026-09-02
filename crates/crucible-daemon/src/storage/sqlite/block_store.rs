//! SQLite `note_blocks`: one row per block, so retrieval can name a passage.
//!
//! Vector search is an exact cosine scan over the raw `embedding` blobs, the
//! same shape `note_store` uses for notes. At kiln scale the scan is fast and
//! exact means recall is 100%.

use async_trait::async_trait;
use crucible_core::parser::BlockHash;
use crucible_core::storage::{
    BlockHit, BlockRecord, BlockStore, CachedVector, StorageError, StorageResult,
};

use super::connection::SqlitePool;
use crate::storage::sqlite::error_ext::SqliteResultExt;

/// `note_blocks` DDL. Executed by the migration ladder, which is the only DDL
/// owner for the kiln database; the constant lives here because this module
/// owns the table's shape.
///
/// `PRIMARY KEY (note_path, span_start)` is the identity: no two top-level
/// blocks of a note begin at the same byte. `WITHOUT ROWID` because that key
/// is the whole row's address and there is no second index into it.
///
/// `note_blocks_reuse_idx` answers "has this exact text already been embedded
/// by this model", which is what keeps a re-index from re-paying a provider.
pub(crate) const NOTE_BLOCKS_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS note_blocks (
    note_path TEXT NOT NULL REFERENCES notes(path) ON DELETE CASCADE,
    span_start INTEGER NOT NULL,
    span_end INTEGER NOT NULL,
    kind TEXT NOT NULL,
    content_hash BLOB NOT NULL,
    text TEXT NOT NULL,
    embedding BLOB,
    embedding_model TEXT,
    embedding_dimensions INTEGER,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (note_path, span_start)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS note_blocks_reuse_idx
    ON note_blocks(content_hash, embedding_model);
CREATE INDEX IF NOT EXISTS note_blocks_path_idx ON note_blocks(note_path);
"#;

fn serialize_embedding(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn deserialize_embedding(bytes: &[u8]) -> Vec<f32> {
    let (chunks, _partial) = bytes.as_chunks::<4>();
    chunks.iter().copied().map(f32::from_le_bytes).collect()
}

/// Cosine similarity against a raw blob, without materializing it first.
/// Returns 0.0 on dimension mismatch or zero magnitude.
fn cosine_similarity_blob(query: &[f32], blob: &[u8]) -> f32 {
    if query.is_empty() || blob.len() != query.len() * 4 {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_b_sq = 0.0f32;
    for (chunk, q) in blob.as_chunks::<4>().0.iter().zip(query) {
        let v = f32::from_le_bytes(*chunk);
        dot += q * v;
        norm_b_sq += v * v;
    }
    let norm_a: f32 = query.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = norm_b_sq.sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// SQLite-backed block storage.
pub struct SqliteBlockStore {
    pool: SqlitePool,
}

impl SqliteBlockStore {
    /// Create a store over an existing pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<BlockRecord> {
    let hash_bytes: Vec<u8> = row.get("content_hash")?;
    let mut hash = [0u8; 32];
    let len = hash_bytes.len().min(32);
    hash[..len].copy_from_slice(&hash_bytes[..len]);

    let embedding_bytes: Option<Vec<u8>> = row.get("embedding")?;
    let span_start: i64 = row.get("span_start")?;
    let span_end: i64 = row.get("span_end")?;
    let dimensions: Option<i64> = row.get("embedding_dimensions")?;

    Ok(BlockRecord {
        note_path: row.get("note_path")?,
        span_start: span_start.max(0) as usize,
        span_end: span_end.max(0) as usize,
        kind: row.get("kind")?,
        content_hash: BlockHash::new(hash),
        text: row.get("text")?,
        embedding: embedding_bytes.map(|b| deserialize_embedding(&b)),
        embedding_model: row.get("embedding_model")?,
        embedding_dimensions: dimensions.map(|d| d as u32),
    })
}

#[async_trait]
impl BlockStore for SqliteBlockStore {
    async fn replace_note_blocks(
        &self,
        note_path: &str,
        blocks: Vec<BlockRecord>,
    ) -> StorageResult<()> {
        let pool = self.pool.clone();
        let note_path = note_path.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_transaction(|conn| {
                // Delete-then-insert, so a note that lost blocks leaves none
                // behind. The same shape the resolved-link index uses.
                conn.execute("DELETE FROM note_blocks WHERE note_path = ?1", [&note_path])
                    .sql()?;

                let now = chrono::Utc::now().to_rfc3339();
                let mut stmt = conn
                    .prepare(
                        "INSERT INTO note_blocks (
                        note_path, span_start, span_end, kind, content_hash,
                        text, embedding, embedding_model, embedding_dimensions, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    )
                    .sql()?;

                for block in &blocks {
                    stmt.execute(rusqlite::params![
                        note_path,
                        block.span_start as i64,
                        block.span_end as i64,
                        block.kind,
                        block.content_hash.as_bytes(),
                        block.text,
                        block.embedding.as_deref().map(serialize_embedding),
                        block.embedding_model,
                        block.embedding_dimensions.map(|d| d as i64),
                        now,
                    ])
                    .sql()?;
                }
                Ok(())
            })
        })
        .await
        .map_err(|e| StorageError::Backend(format!("block write task: {e}")))?
        .map_err(|e| StorageError::Backend(format!("block write: {e}")))
    }

    async fn cached_vectors(
        &self,
        hashes: &[BlockHash],
        model: &str,
    ) -> StorageResult<Vec<(BlockHash, CachedVector)>> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.pool.clone();
        let model = model.to_string();
        let wanted: Vec<Vec<u8>> = hashes.iter().map(|h| h.as_bytes().to_vec()).collect();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut found: Vec<(BlockHash, CachedVector)> = Vec::new();
                let mut stmt = conn
                    .prepare(
                        "SELECT content_hash, embedding, embedding_dimensions
                     FROM note_blocks
                     WHERE content_hash = ?1 AND embedding_model = ?2 AND embedding IS NOT NULL
                     LIMIT 1",
                    )
                    .sql()?;

                for hash in &wanted {
                    let row = stmt
                        .query_row(rusqlite::params![hash, model], |row| {
                            let bytes: Vec<u8> = row.get(1)?;
                            let dims: Option<i64> = row.get(2)?;
                            Ok((bytes, dims))
                        })
                        .ok();

                    if let Some((bytes, dims)) = row {
                        let embedding = deserialize_embedding(&bytes);
                        let dimensions = dims.map(|d| d as u32).unwrap_or(embedding.len() as u32);
                        let mut key = [0u8; 32];
                        let len = hash.len().min(32);
                        key[..len].copy_from_slice(&hash[..len]);
                        found.push((
                            BlockHash::new(key),
                            CachedVector {
                                embedding,
                                dimensions,
                            },
                        ));
                    }
                }
                Ok(found)
            })
        })
        .await
        .map_err(|e| StorageError::Backend(format!("cache lookup task: {e}")))?
        .map_err(|e| StorageError::Backend(format!("cache lookup: {e}")))
    }

    async fn blocks_for_note(&self, note_path: &str) -> StorageResult<Vec<BlockRecord>> {
        let pool = self.pool.clone();
        let note_path = note_path.to_string();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                let mut stmt = conn
                    .prepare("SELECT * FROM note_blocks WHERE note_path = ?1 ORDER BY span_start")
                    .sql()?;
                let rows = stmt.query_map([&note_path], row_to_record).sql()?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row.sql()?);
                }
                Ok(out)
            })
        })
        .await
        .map_err(|e| StorageError::Backend(format!("block read task: {e}")))?
        .map_err(|e| StorageError::Backend(format!("block read: {e}")))
    }

    async fn search_blocks(&self, vector: &[f32], limit: usize) -> StorageResult<Vec<BlockHit>> {
        if vector.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let pool = self.pool.clone();
        let query = vector.to_vec();

        tokio::task::spawn_blocking(move || {
            pool.with_connection(|conn| {
                // Score every embedded row, then materialize only the winners.
                let mut stmt = conn
                    .prepare(
                        "SELECT note_path, span_start, embedding FROM note_blocks
                     WHERE embedding IS NOT NULL",
                    )
                    .sql()?;
                let mut scored: Vec<(f32, String, i64)> = Vec::new();
                let mut rows = stmt.query([]).sql()?;
                while let Some(row) = rows.next().sql()? {
                    let path: String = row.get(0).sql()?;
                    let span_start: i64 = row.get(1).sql()?;
                    let blob: Vec<u8> = row.get(2).sql()?;
                    scored.push((cosine_similarity_blob(&query, &blob), path, span_start));
                }

                // Score descending, then path and offset ascending, so ties
                // are ordered by document position rather than by scan order.
                scored.sort_by(|a, b| {
                    b.0.total_cmp(&a.0)
                        .then_with(|| a.1.cmp(&b.1))
                        .then_with(|| a.2.cmp(&b.2))
                });
                scored.truncate(limit);

                let mut fetch = conn
                    .prepare("SELECT * FROM note_blocks WHERE note_path = ?1 AND span_start = ?2")
                    .sql()?;
                let mut hits = Vec::with_capacity(scored.len());
                for (score, path, span_start) in scored {
                    let block = fetch
                        .query_row(rusqlite::params![path, span_start], row_to_record)
                        .sql()?;
                    hits.push(BlockHit { block, score });
                }
                Ok(hits)
            })
        })
        .await
        .map_err(|e| StorageError::Backend(format!("block search task: {e}")))?
        .map_err(|e| StorageError::Backend(format!("block search: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::storage::{NoteRecord, NoteStore};

    async fn store_with_note(path: &str) -> SqliteBlockStore {
        // `SqlitePool::memory` runs the migration ladder, v7 included.
        let pool = SqlitePool::memory().unwrap();
        // note_blocks references notes(path), so the parent must exist.
        let notes = super::super::SqliteNoteStore::new(pool.clone());
        notes
            .upsert(NoteRecord::new(
                path,
                crucible_core::parser::BlockHash::zero(),
            ))
            .await
            .unwrap();
        SqliteBlockStore::new(pool)
    }

    fn record(path: &str, start: usize, text: &str, vector: Option<Vec<f32>>) -> BlockRecord {
        BlockRecord {
            note_path: path.to_string(),
            span_start: start,
            span_end: start + text.len(),
            kind: "paragraph".to_string(),
            content_hash: BlockHash::new(*blake3::hash(text.as_bytes()).as_bytes()),
            text: text.to_string(),
            embedding_dimensions: vector.as_ref().map(|v| v.len() as u32),
            embedding_model: vector.as_ref().map(|_| "test-model".to_string()),
            embedding: vector,
        }
    }

    #[tokio::test]
    async fn blocks_round_trip_in_document_order() {
        let store = store_with_note("note.md").await;

        store
            .replace_note_blocks(
                "note.md",
                vec![
                    record("note.md", 100, "second", Some(vec![0.0, 1.0])),
                    record("note.md", 0, "first", Some(vec![1.0, 0.0])),
                ],
            )
            .await
            .unwrap();

        let blocks = store.blocks_for_note("note.md").await.unwrap();

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "first");
        assert_eq!(blocks[1].text, "second");
        assert_eq!(blocks[0].embedding, Some(vec![1.0, 0.0]));
        assert_eq!(blocks[0].embedding_model.as_deref(), Some("test-model"));
    }

    #[tokio::test]
    async fn two_identical_blocks_of_one_note_both_survive() {
        // The content hash collides for these two; the span is what keeps
        // them apart. A hash-keyed table would have merged them silently.
        let store = store_with_note("note.md").await;

        store
            .replace_note_blocks(
                "note.md",
                vec![
                    record("note.md", 0, "same words", Some(vec![1.0, 0.0])),
                    record("note.md", 50, "same words", Some(vec![1.0, 0.0])),
                ],
            )
            .await
            .unwrap();

        let blocks = store.blocks_for_note("note.md").await.unwrap();

        assert_eq!(blocks.len(), 2, "identical blocks must both be stored");
        assert_eq!(blocks[0].content_hash, blocks[1].content_hash);
        assert_ne!(blocks[0].span_start, blocks[1].span_start);
    }

    #[tokio::test]
    async fn a_note_that_shrinks_leaves_no_stale_rows() {
        let store = store_with_note("note.md").await;
        store
            .replace_note_blocks(
                "note.md",
                (0..5)
                    .map(|i| {
                        record(
                            "note.md",
                            i * 10,
                            &format!("block {i}"),
                            Some(vec![1.0, 0.0]),
                        )
                    })
                    .collect(),
            )
            .await
            .unwrap();

        store
            .replace_note_blocks(
                "note.md",
                vec![record("note.md", 0, "only one left", Some(vec![1.0, 0.0]))],
            )
            .await
            .unwrap();

        let blocks = store.blocks_for_note("note.md").await.unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "only one left");
    }

    #[tokio::test]
    async fn a_stored_vector_is_found_again_by_content_hash() {
        let store = store_with_note("note.md").await;
        let block = record("note.md", 0, "reusable text", Some(vec![0.5, 0.5]));
        let hash = block.content_hash;
        store
            .replace_note_blocks("note.md", vec![block])
            .await
            .unwrap();

        let hits = store.cached_vectors(&[hash], "test-model").await.unwrap();

        assert_eq!(
            hits.len(),
            1,
            "the reuse index must find the paid-for vector"
        );
        assert_eq!(hits[0].0, hash);
        assert_eq!(hits[0].1.embedding, vec![0.5, 0.5]);
    }

    #[tokio::test]
    async fn a_vector_from_another_model_is_not_reused() {
        let store = store_with_note("note.md").await;
        let block = record("note.md", 0, "reusable text", Some(vec![0.5, 0.5]));
        let hash = block.content_hash;
        store
            .replace_note_blocks("note.md", vec![block])
            .await
            .unwrap();

        let hits = store.cached_vectors(&[hash], "other-model").await.unwrap();

        assert!(
            hits.is_empty(),
            "a vector is only reusable under its own model"
        );
    }

    #[tokio::test]
    async fn search_returns_the_nearest_block_not_the_note() {
        let store = store_with_note("note.md").await;
        store
            .replace_note_blocks(
                "note.md",
                vec![
                    record("note.md", 0, "about cats", Some(vec![1.0, 0.0])),
                    record("note.md", 50, "about dogs", Some(vec![0.0, 1.0])),
                ],
            )
            .await
            .unwrap();

        let hits = store.search_blocks(&[0.0, 1.0], 1).await.unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].block.text, "about dogs");
        assert_eq!(hits[0].block.span_start, 50);
        assert!(hits[0].score > 0.99);
    }

    #[tokio::test]
    async fn a_block_with_no_vector_is_never_a_hit() {
        let store = store_with_note("note.md").await;
        store
            .replace_note_blocks(
                "note.md",
                vec![
                    record("note.md", 0, "too short", None),
                    record("note.md", 50, "has a vector", Some(vec![1.0, 0.0])),
                ],
            )
            .await
            .unwrap();

        let hits = store.search_blocks(&[1.0, 0.0], 10).await.unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].block.text, "has a vector");
    }
}
