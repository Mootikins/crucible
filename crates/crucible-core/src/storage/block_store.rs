//! Block-granularity vector storage.
//!
//! A note's blocks are stored one row each, so retrieval can name the passage
//! that answered a query rather than the whole file it sat in.
//!
//! # Identity and reuse are different keys
//!
//! `(note_path, span_start)` is identity. Two top-level blocks of a note
//! cannot begin at the same byte, so this separates repeated headings and
//! repeated quotes that no content hash can.
//!
//! `content_hash` is reuse. Identical blocks hash alike on purpose: one
//! embedding serves every copy, in this note and in every other. Looking a
//! hash up before calling the provider is what makes re-indexing cheap, and
//! it is why an edit at the top of a file does not re-embed the whole file —
//! every span below shifts, so every row is rewritten, but no vector is
//! recomputed.

use crate::parser::BlockHash;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::error::StorageResult;

/// One stored block of a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockRecord {
    /// Kiln-relative path of the note this block belongs to.
    pub note_path: String,

    /// Byte offset where the block starts, relative to the note body.
    /// With `note_path`, this is the row's identity.
    pub span_start: usize,

    /// Byte offset one past the block's last byte.
    pub span_end: usize,

    /// The block's kind, as `BlockKind::as_str` names it.
    pub kind: String,

    /// BLAKE3 of the block's source bytes. The reuse key.
    pub content_hash: BlockHash,

    /// The text that was embedded, heading trail included.
    ///
    /// Stored so a hit can be quoted without re-reading and re-parsing the
    /// file, which is what lets retrieval return a passage.
    pub text: String,

    /// The block's vector. `None` when the block fell under the word floor.
    pub embedding: Option<Vec<f32>>,

    /// Model that produced `embedding`.
    pub embedding_model: Option<String>,

    /// Dimensions of `embedding`.
    pub embedding_dimensions: Option<u32>,
}

/// One block returned by a vector search.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockHit {
    /// The block that matched.
    pub block: BlockRecord,

    /// Cosine similarity against the query.
    pub score: f32,
}

/// A vector already paid for, keyed by content hash and model.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedVector {
    /// The vector.
    pub embedding: Vec<f32>,
    /// Its dimensions.
    pub dimensions: u32,
}

/// Block-granularity storage.
#[async_trait]
pub trait BlockStore: Send + Sync {
    /// Replace every block of one note.
    ///
    /// Delete-then-insert in one transaction, the same shape the resolved-link
    /// index uses, so a note that loses blocks leaves no stale rows behind.
    async fn replace_note_blocks(
        &self,
        note_path: &str,
        blocks: Vec<BlockRecord>,
    ) -> StorageResult<()>;

    /// Look up vectors already computed for these content hashes under `model`.
    ///
    /// The caller embeds only what comes back missing.
    async fn cached_vectors(
        &self,
        hashes: &[BlockHash],
        model: &str,
    ) -> StorageResult<Vec<(BlockHash, CachedVector)>>;

    /// Every block of one note, in document order.
    async fn blocks_for_note(&self, note_path: &str) -> StorageResult<Vec<BlockRecord>>;

    /// The `limit` blocks closest to `vector`, best first.
    async fn search_blocks(&self, vector: &[f32], limit: usize) -> StorageResult<Vec<BlockHit>>;
}
