//! Vector storage abstraction for semantic search.
//!
//! Separated from [`NoteStore`](super::note_store::NoteStore) so the vector
//! index can live in a dedicated, vector-native backend (LanceDB) while note
//! metadata stays in a relational store (SQLite).
//!
//! # Design
//!
//! A `VectorStore` is a keyed embedding index. Keys are arbitrary strings
//! (typically note paths, but the trait doesn't constrain semantics — could
//! be block hashes, document ids, anything). Values are float vectors of a
//! fixed dimension chosen at store construction.
//!
//! The store owns *only* the embedding and the key. It does NOT store the
//! original text, metadata, tags, or any other note attributes. That data
//! lives in `NoteStore`. To hydrate a search result into a full note, look
//! the key up in `NoteStore` after.

use async_trait::async_trait;

use super::error::StorageResult;

/// One result from a similarity search: the matched key and a similarity
/// score where higher is better and 1.0 is identical.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorMatch {
    pub id: String,
    pub similarity: f32,
}

/// Keyed vector index for semantic search.
///
/// Implementations: [`crucible_daemon::storage::lance::LanceVectorIndex`].
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// Insert or replace the embedding for `id`. The vector dimension must
    /// match the store's configured dimension; mismatches return an error.
    async fn upsert(&self, id: &str, embedding: Vec<f32>) -> StorageResult<()>;

    /// Top-`limit` similarity matches for `query`, sorted by similarity
    /// descending. Returns an empty Vec if the index is empty.
    async fn search(&self, query: &[f32], limit: usize) -> StorageResult<Vec<VectorMatch>>;

    /// Remove the embedding for `id`. No-op if the id is absent.
    async fn delete(&self, id: &str) -> StorageResult<()>;

    /// Number of vectors currently in the index. Primarily for diagnostics
    /// and tests.
    async fn count(&self) -> StorageResult<usize>;

    /// Embedding dimension this store accepts. Implementations choose this
    /// at construction time; it cannot change for a given store instance.
    fn dimension(&self) -> usize;
}
