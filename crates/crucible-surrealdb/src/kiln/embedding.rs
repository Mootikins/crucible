//! Embedding storage and retrieval
//!
//! This module re-exports embedding-related functions from kiln_integration.rs.
//! Future work: Move implementations here for better organization.

// Re-export from legacy kiln_integration module
pub use crate::kiln_integration::{
    clear_all_embeddings,
    clear_all_embeddings_and_recreate_index,
    clear_document_embeddings,
    delete_document_chunks,
    ensure_embedding_index,
    ensure_embedding_index_from_existing,
    get_all_document_embeddings,
    get_document_chunk_hashes,
    get_document_embeddings,
    get_embedding_by_content_hash,
    get_embedding_index_metadata,
    store_document_embedding,
    store_embedding,
    store_embedding_with_chunk_id,
    store_embeddings_batch,
    EmbeddingIndexMetadata,
};
