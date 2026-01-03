//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements embedding storage and semantic search support.
//!
//! ## Phase 4 Cleanup
//!
//! The EAV graph components (document_storage, relations) have been removed.
//! Use NoteStore for note metadata storage instead.
//!
//! The following functions are deprecated stubs that will be removed:
//! - `store_parsed_document` - Use NoteStore::upsert instead
//! - `retrieve_parsed_document` - Use NoteStore::get instead
//! - `create_wikilink_edges` - Links are now stored inline in NoteRecord
//! - `create_embed_relationships` - Embeds are now stored inline in NoteRecord

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
use crucible_core::types::ParsedNote;
use serde_json::json;
use std::path::Path;
use tracing::{info, warn};

/// Initialize the kiln schema in the database
///
/// NOTE: The EAV graph schema has been removed in Phase 4.
/// Use NoteStore for note metadata storage.
/// This function now only initializes embedding support.
pub async fn initialize_kiln_schema(client: &SurrealClient) -> Result<()> {
    // Apply notes schema from schema_notes.surql
    let schema = include_str!("../schema_notes.surql");
    client.db().query(schema).await?;
    info!("Kiln schema initialized using NoteStore definitions");

    // Note: MTREE index creation is deferred to first embedding storage
    Ok(())
}

// =============================================================================
// DEPRECATED STUB FUNCTIONS
// These functions are stubs to maintain API compatibility during migration.
// They will be removed once all callers are migrated to NoteStore.
// =============================================================================

/// DEPRECATED: Store a parsed document
///
/// This function is a stub that stores minimal data to the notes table.
/// Migrate to using NoteStore::upsert for full functionality.
#[deprecated(note = "Use NoteStore::upsert instead")]
pub async fn store_parsed_document(
    client: &SurrealClient,
    note: &ParsedNote,
    kiln_root: &Path,
) -> Result<String> {
    let relative_path = note
        .path
        .strip_prefix(kiln_root)
        .unwrap_or(&note.path)
        .to_string_lossy()
        .replace('\\', "/");

    let doc_id = format!("entities:note:{}", relative_path);

    // Store minimal data in notes table
    let sql = r#"
        UPSERT notes CONTENT {
            path: $path,
            title: $title,
            content_hash: $content_hash,
            content: $content,
            updated_at: time::now()
        }
    "#;

    // Extract title from frontmatter or use path stem
    let title = note
        .frontmatter
        .as_ref()
        .and_then(|fm| fm.properties().get("title"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            note.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string()
        });

    client
        .query(
            sql,
            &[json!({
                "path": relative_path,
                "title": title,
                "content_hash": note.content_hash,
                "content": note.content.plain_text,
            })],
        )
        .await?;

    Ok(doc_id)
}

/// DEPRECATED: Retrieve a parsed document
///
/// This function is a stub that returns None.
/// Migrate to using NoteStore::get for full functionality.
#[deprecated(note = "Use NoteStore::get instead")]
pub async fn retrieve_parsed_document(
    _client: &SurrealClient,
    _document_id: &str,
) -> Result<Option<ParsedNote>> {
    warn!("retrieve_parsed_document is deprecated - use NoteStore::get instead");
    Ok(None)
}

/// DEPRECATED: Create wikilink edges
///
/// This function is a no-op stub.
/// Links are now stored inline in NoteRecord.
#[deprecated(note = "Links are now stored inline in NoteRecord")]
pub async fn create_wikilink_edges(
    _client: &SurrealClient,
    _document_id: &str,
    _note: &ParsedNote,
    _kiln_root: &Path,
) -> Result<()> {
    // No-op: Links are now stored inline in NoteRecord
    Ok(())
}

/// DEPRECATED: Create embed relationships
///
/// This function is a no-op stub.
/// Embeds are now stored inline in NoteRecord.
#[deprecated(note = "Embeds are now stored inline in NoteRecord")]
pub async fn create_embed_relationships(
    _client: &SurrealClient,
    _document_id: &str,
    _note: &ParsedNote,
    _kiln_root: &Path,
) -> Result<()> {
    // No-op: Embeds are now stored inline in NoteRecord
    Ok(())
}

// Tests have been removed as they depend on deleted EAV graph functionality.
// New tests should use NoteStore for note metadata operations.
