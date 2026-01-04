//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements embedding storage and semantic search support.
//!
//! Note metadata storage is handled by NoteStore. Links and embeds are stored inline
//! in NoteRecord by NoteIngestor.

mod embeddings;
mod semantic_search;
mod types;
mod utils;

pub use embeddings::{
    clear_all_embeddings, clear_all_embeddings_and_recreate_index, clear_document_embeddings,
    delete_document_chunks, ensure_embedding_index, ensure_embedding_index_from_existing,
    get_all_document_embeddings, get_database_stats, get_document_chunk_hashes,
    get_document_embeddings, get_embedding_by_content_hash, get_embedding_index_metadata,
    store_document_embedding, store_embedding, store_embedding_with_chunk_id,
    store_embeddings_batch,
};

pub use semantic_search::{semantic_search, semantic_search_with_reranking};

pub use types::{
    CachedEmbedding, EmbedMetadata, EmbedRelation, EmbeddingData, EmbeddingIndexMetadata,
    LinkRelation,
};

pub use utils::{generate_document_id, normalize_document_id};

// Schema initialization
use crate::SurrealClient;
use anyhow::Result;
use tracing::info;

/// Initialize the kiln schema in the database
///
/// Use NoteStore for note metadata storage.
/// This function initializes embedding support.
pub async fn initialize_kiln_schema(client: &SurrealClient) -> Result<()> {
    // Apply notes schema from schema_notes.surql
    let schema = include_str!("../schema_notes.surql");
    client.db().query(schema).await?;
    info!("Kiln schema initialized using NoteStore definitions");

    // Note: MTREE index creation is deferred to first embedding storage
    Ok(())
}
