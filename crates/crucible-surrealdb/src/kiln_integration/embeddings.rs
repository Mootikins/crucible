//! Embedding storage and retrieval
//!
//! Functions for storing and retrieving vector embeddings.

use crate::types::{DatabaseStats, DocumentEmbedding, Record};
use crate::SurrealClient;
use anyhow::{anyhow, Result};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tracing::{debug, info, warn};

use super::types::{CachedEmbedding, EmbeddingData, EmbeddingIndexMetadata};
use super::utils::{
    chunk_namespace, chunk_record_body, escape_record_id, normalize_document_id, record_body,
};

/// Track whether the MTREE index has been ensured in this session
pub(crate) static MTREE_INDEX_ENSURED: AtomicBool = AtomicBool::new(false);
/// Track the dimensions used for the current index
pub(crate) static MTREE_INDEX_DIMENSIONS: AtomicUsize = AtomicUsize::new(0);

/// Retry configuration for transaction conflicts
const MAX_RETRIES: u32 = 5;
const INITIAL_BACKOFF_MS: u64 = 10;

/// Check if an error is a retryable transaction conflict
fn is_retryable_error(error_msg: &str) -> bool {
    error_msg.contains("read or write conflict") || error_msg.contains("transaction can be retried")
}

/// Upsert embedding record with retry logic
#[allow(clippy::too_many_arguments)]
async fn upsert_embedding_record(
    client: &SurrealClient,
    chunk_id: &str,
    normalized_entity_id: &str,
    vector: &[f32],
    dims: usize,
    stored_model: &str,
    model_version: &str,
    content_used: &str,
) -> Result<()> {
    let chunk_body = chunk_record_body(chunk_id);
    let escaped_chunk = escape_record_id(chunk_body);
    let escaped_entity = escape_record_id(record_body(normalized_entity_id));
    let params = json!({
        "embedding": vector,
        "dimensions": dims as i64,
        "model": stored_model,
        "model_version": model_version,
        "content_used": content_used,
    });

    let upsert_sql = format!(
        "
        UPSERT embeddings:⟨{chunk}⟩
        SET
            entity_id = entities:⟨{entity}⟩,
            embedding = $embedding,
            dimensions = $dimensions,
            model = $model,
            model_version = $model_version,
            content_used = $content_used
        RETURN NONE;
    ",
        chunk = escaped_chunk,
        entity = escaped_entity
    );

    let mut last_error = None;
    for attempt in 0..MAX_RETRIES {
        match client
            .query(&upsert_sql, std::slice::from_ref(&params))
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) => {
                let error_msg = e.to_string();
                if is_retryable_error(&error_msg) && attempt < MAX_RETRIES - 1 {
                    let backoff = INITIAL_BACKOFF_MS * (1 << attempt);
                    debug!(
                        "Transaction conflict on embedding upsert (attempt {}), retrying in {}ms",
                        attempt + 1,
                        backoff
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                    last_error = Some(e);
                } else {
                    return Err(anyhow::anyhow!("Failed to upsert embedding: {}", e));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to upsert embedding after {} retries: {}",
        MAX_RETRIES,
        last_error.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Relate embedding record with retry logic
async fn relate_embedding_record(
    client: &SurrealClient,
    normalized_entity_id: &str,
    chunk_id: &str,
) -> Result<()> {
    let escaped_entity = escape_record_id(record_body(normalized_entity_id));
    let escaped_chunk = escape_record_id(chunk_record_body(chunk_id));
    let sql = format!(
        "
        RELATE entities:⟨{entity}⟩
            -> has_embedding ->
        embeddings:⟨{chunk}⟩;
    ",
        entity = escaped_entity,
        chunk = escaped_chunk
    );

    let mut last_error = None;
    for attempt in 0..MAX_RETRIES {
        match client.query(&sql, &[]).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                let error_msg = e.to_string();
                if is_retryable_error(&error_msg) && attempt < MAX_RETRIES - 1 {
                    let backoff = INITIAL_BACKOFF_MS * (1 << attempt);
                    debug!(
                        "Transaction conflict on relate (attempt {}), retrying in {}ms",
                        attempt + 1,
                        backoff
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(backoff)).await;
                    last_error = Some(e);
                } else {
                    return Err(anyhow::anyhow!("Failed to create graph relation: {}", e));
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to create graph relation after {} retries: {}",
        MAX_RETRIES,
        last_error.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Remove all stored embeddings and associated edges.
pub async fn clear_all_embeddings(client: &SurrealClient) -> Result<()> {
    client
        .query("DELETE has_embedding", &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to clear embedding relations: {}", e))?;

    client
        .query("DELETE FROM embeddings", &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to clear embeddings table: {}", e))?;

    Ok(())
}

/// Fetch the metadata for stored embeddings, if any exist.
pub async fn get_embedding_index_metadata(
    client: &SurrealClient,
) -> Result<Option<EmbeddingIndexMetadata>> {
    let sql = "SELECT model_version, dimensions FROM embeddings LIMIT 1";
    let result = client
        .query(sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to inspect embedding metadata: {}", e))?;

    if let Some(record) = result.records.first() {
        let model = record
            .data
            .get("model_version")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let dimensions = record
            .data
            .get("dimensions")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        Ok(Some(EmbeddingIndexMetadata { model, dimensions }))
    } else {
        Ok(None)
    }
}

/// Ensure the MTREE vector index exists with the correct dimensions.
pub async fn ensure_embedding_index(client: &SurrealClient, dimensions: usize) -> Result<()> {
    debug!("Ensuring embedding index with {} dimensions", dimensions);

    let _ = client
        .query("REMOVE INDEX embedding_vector_idx ON TABLE embeddings", &[])
        .await;

    let sql = format!(
        "DEFINE INDEX embedding_vector_idx ON TABLE embeddings COLUMNS embedding MTREE DIMENSION {} DIST COSINE",
        dimensions
    );
    client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create embedding index: {}", e))?;

    MTREE_INDEX_ENSURED.store(true, Ordering::Relaxed);
    MTREE_INDEX_DIMENSIONS.store(dimensions, Ordering::Relaxed);

    info!(
        "Created embedding vector index with {} dimensions",
        dimensions
    );
    Ok(())
}

/// Ensure the MTREE index exists based on existing embeddings in the database.
pub async fn ensure_embedding_index_from_existing(client: &SurrealClient) -> Result<bool> {
    if MTREE_INDEX_ENSURED.load(Ordering::Relaxed) {
        return Ok(true);
    }

    if let Some(metadata) = get_embedding_index_metadata(client).await? {
        if let Some(dims) = metadata.dimensions {
            debug!(
                "Found existing embeddings with {} dimensions, ensuring MTREE index",
                dims
            );
            ensure_embedding_index(client, dims).await?;
            Ok(true)
        } else {
            debug!("Embeddings exist but dimensions unknown, skipping MTREE index");
            Ok(false)
        }
    } else {
        debug!("No existing embeddings found, MTREE index will be created on first embedding");
        Ok(false)
    }
}

/// Clear all embeddings and recreate the index with new dimensions.
pub async fn clear_all_embeddings_and_recreate_index(
    client: &SurrealClient,
    new_dimensions: usize,
) -> Result<()> {
    info!(
        "Clearing all embeddings and recreating index with {} dimensions",
        new_dimensions
    );

    client
        .query("DELETE has_embedding", &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to clear embedding relations: {}", e))?;

    client
        .query("DELETE FROM embeddings", &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to clear embeddings table: {}", e))?;

    ensure_embedding_index(client, new_dimensions).await?;

    Ok(())
}

/// Store note embedding in database
pub async fn store_document_embedding(
    client: &SurrealClient,
    embedding: &DocumentEmbedding,
) -> Result<()> {
    let note_id = normalize_document_id(&embedding.document_id);
    let chunk_position = embedding.chunk_position.unwrap_or(0);

    store_embedding_with_chunk_id(
        client,
        &note_id,
        embedding.vector.clone(),
        &embedding.embedding_model,
        embedding.chunk_size,
        chunk_position,
        embedding.chunk_id.as_deref(),
        None,
    )
    .await?;

    debug!(
        "Stored embedding for note {} via graph relations",
        embedding.document_id
    );

    Ok(())
}

/// Store embedding with graph relations
#[allow(clippy::too_many_arguments)]
pub async fn store_embedding(
    client: &SurrealClient,
    note_id: &str,
    vector: Vec<f32>,
    embedding_model: &str,
    chunk_size: usize,
    chunk_position: usize,
    dimensions: Option<usize>,
    chunk_content: Option<&str>,
) -> Result<String> {
    let _ = chunk_size;
    let normalized_id = normalize_document_id(note_id);
    let chunk_scope = chunk_namespace(&normalized_id);
    let chunk_id = format!("embeddings:{}_chunk_{}", chunk_scope, chunk_position);
    let dims = dimensions.unwrap_or(vector.len());
    let content_used = chunk_content.unwrap_or("");

    let chunk_body = chunk_record_body(&chunk_id).to_string();
    let stored_model = format!("{}::{}", embedding_model, chunk_body);
    upsert_embedding_record(
        client,
        &chunk_id,
        &normalized_id,
        &vector,
        dims,
        &stored_model,
        embedding_model,
        content_used,
    )
    .await?;

    relate_embedding_record(client, &normalized_id, &chunk_id).await?;

    debug!(
        "Stored embedding {} with graph relation from {}",
        chunk_id, normalized_id
    );

    Ok(chunk_id)
}

/// Store embedding with graph relations and optional chunk_id field
#[allow(clippy::too_many_arguments)]
pub async fn store_embedding_with_chunk_id(
    client: &SurrealClient,
    note_id: &str,
    vector: Vec<f32>,
    embedding_model: &str,
    chunk_size: usize,
    chunk_position: usize,
    logical_chunk_id: Option<&str>,
    dimensions: Option<usize>,
) -> Result<String> {
    let _ = chunk_size;
    let normalized_id = normalize_document_id(note_id);
    let chunk_scope = chunk_namespace(&normalized_id);
    let chunk_id = if let Some(logical_id) = logical_chunk_id {
        let safe_logical_id = logical_id.replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
        format!("embeddings:{}_{}", chunk_scope, safe_logical_id)
    } else {
        format!("embeddings:{}_chunk_{}", chunk_scope, chunk_position)
    };
    let dims = dimensions.unwrap_or(vector.len());

    let chunk_body = chunk_record_body(&chunk_id).to_string();
    let stored_model = format!("{}::{}", embedding_model, chunk_body);
    upsert_embedding_record(
        client,
        &chunk_id,
        &normalized_id,
        &vector,
        dims,
        &stored_model,
        embedding_model,
        "",
    )
    .await?;

    relate_embedding_record(client, &normalized_id, &chunk_id).await?;

    debug!(
        "Stored embedding {} with graph relation from {} (logical_chunk_id: {:?})",
        chunk_id, normalized_id, logical_chunk_id
    );

    Ok(chunk_id)
}

/// Store multiple embeddings for a single note in one batch operation
pub async fn store_embeddings_batch(
    client: &SurrealClient,
    note_id: &str,
    embeddings: &[EmbeddingData],
) -> Result<Vec<String>> {
    if embeddings.is_empty() {
        return Ok(vec![]);
    }

    let normalized_id = normalize_document_id(note_id);
    let chunk_scope = chunk_namespace(&normalized_id);
    let mut chunk_ids = Vec::with_capacity(embeddings.len());

    for embedding in embeddings {
        let safe_block_id = embedding
            .block_id
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "_");
        let chunk_id = format!("embeddings:{}_{}", chunk_scope, safe_block_id);
        let escaped_chunk = escape_record_id(chunk_record_body(&chunk_id));
        let dims = embedding.dimensions;
        let stored_model = format!("{}::{}", embedding.model, escaped_chunk);

        upsert_embedding_record(
            client,
            &chunk_id,
            &normalized_id,
            &embedding.vector,
            dims,
            &stored_model,
            &embedding.model,
            "",
        )
        .await?;

        relate_embedding_record(client, &normalized_id, &chunk_id).await?;

        chunk_ids.push(chunk_id);
    }

    debug!(
        "Stored {} embeddings for {}",
        embeddings.len(),
        normalized_id
    );

    if let Some(first_embedding) = embeddings.first() {
        let dims = first_embedding.dimensions;
        let current_dims = MTREE_INDEX_DIMENSIONS.load(Ordering::Relaxed);

        if !MTREE_INDEX_ENSURED.load(Ordering::Relaxed) || current_dims != dims {
            if let Err(e) = ensure_embedding_index(client, dims).await {
                warn!(
                    "Failed to create MTREE index (search will use fallback): {}",
                    e
                );
            } else {
                MTREE_INDEX_ENSURED.store(true, Ordering::Relaxed);
                MTREE_INDEX_DIMENSIONS.store(dims, Ordering::Relaxed);
                info!("Created MTREE vector index with {} dimensions", dims);
            }
        }
    }

    Ok(chunk_ids)
}

/// Get note embeddings from database
pub async fn get_document_embeddings(
    client: &SurrealClient,
    document_id: &str,
) -> Result<Vec<DocumentEmbedding>> {
    let normalized_id = normalize_document_id(document_id);
    let entity_body = escape_record_id(record_body(&normalized_id));
    let sql = format!(
        "SELECT out FROM entities:⟨{entity}⟩->has_embedding",
        entity = entity_body
    );

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query note embeddings via graph: {}", e))?;

    let mut embeddings = Vec::new();

    for record in result.records {
        if let Some(out_value) = record.data.get("out") {
            let embedding_id = out_value.as_str().ok_or_else(|| {
                anyhow::anyhow!("Expected string for embedding ID, got: {:?}", out_value)
            })?;

            let emb_sql = format!("SELECT * FROM {}", embedding_id);
            let emb_result = client
                .query(&emb_sql, &[])
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch embedding record: {}", e))?;

            if let Some(emb_record) = emb_result.records.first() {
                let embedding =
                    convert_record_to_document_embedding_with_id(emb_record, document_id)?;
                embeddings.push(embedding);
            }
        }
    }

    Ok(embeddings)
}

/// Look up a cached embedding by content hash and model
pub async fn get_embedding_by_content_hash(
    client: &SurrealClient,
    content_hash: &str,
    model: &str,
    model_version: &str,
) -> Result<Option<CachedEmbedding>> {
    let params = json!({
        "content_hash": content_hash,
        "model": model,
        "model_version": model_version,
    });

    let sql = r#"
        SELECT embedding, model, model_version, content_used, dimensions
        FROM embeddings
        WHERE content_used = $content_hash
          AND model_version = $model_version
          AND model CONTAINS $model
        LIMIT 1
    "#;

    let result = client
        .query(sql, &[params])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query embedding by content hash: {}", e))?;

    if let Some(record) = result.records.first() {
        let vector = record
            .data
            .get("embedding")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect::<Vec<f32>>()
            })
            .unwrap_or_default();

        if vector.is_empty() {
            return Ok(None);
        }

        let model_str = record
            .data
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let model_version_str = record
            .data
            .get("model_version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let content_hash_str = record
            .data
            .get("content_used")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let dimensions = record
            .data
            .get("dimensions")
            .and_then(|v| v.as_i64())
            .unwrap_or(vector.len() as i64) as usize;

        debug!(
            "Found cached embedding for content_hash={}, model={}",
            content_hash, model_str
        );

        Ok(Some(CachedEmbedding {
            vector,
            model: model_str,
            model_version: model_version_str,
            content_hash: content_hash_str,
            dimensions,
        }))
    } else {
        Ok(None)
    }
}

/// Clear all embeddings for a note
pub async fn clear_document_embeddings(client: &SurrealClient, document_id: &str) -> Result<()> {
    let note_id = if document_id.starts_with("notes:") {
        document_id.to_string()
    } else {
        format!("notes:{}", document_id)
    };

    let query_sql = "SELECT id FROM (SELECT ->has_embedding->id AS emb_ids FROM type::thing($note_id))[0].emb_ids";
    let params = vec![serde_json::json!({ "note_id": note_id })];
    let query_result = client.query(query_sql, &params).await?;

    let mut embedding_ids = Vec::new();
    if let Some(record) = query_result.records.first() {
        if let Some(emb_ids_value) = record.data.get("emb_ids") {
            if let Some(ids_array) = emb_ids_value.as_array() {
                for id_val in ids_array {
                    if let Some(id_str) = id_val.as_str() {
                        embedding_ids.push(id_str.to_string());
                    }
                }
            }
        }
    }

    let delete_edges_sql = "DELETE type::thing($note_id) -> has_embedding";
    let params = vec![serde_json::json!({ "note_id": note_id })];
    client
        .query(delete_edges_sql, &params)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to delete has_embedding edges: {}", e))?;

    for embedding_id in &embedding_ids {
        let delete_sql = "DELETE type::thing($emb_id)";
        let params = vec![serde_json::json!({ "emb_id": embedding_id })];
        client.query(delete_sql, &params).await?;
    }

    debug!(
        "Cleared {} embeddings and edges for note {}",
        embedding_ids.len(),
        document_id
    );
    Ok(())
}

/// Get existing chunk hashes for a note
pub async fn get_document_chunk_hashes(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<HashMap<usize, String>> {
    debug!("Getting chunk hashes for note: {}", doc_id);

    let normalized = normalize_document_id(doc_id);
    let scope = chunk_namespace(&normalized);

    let sql = "SELECT chunk_position, chunk_hash FROM embeddings WHERE id >= type::thing('embeddings', $scope_start) AND id < type::thing('embeddings', $scope_end)";
    let params = vec![serde_json::json!({
        "scope_start": scope,
        "scope_end": format!("{}~", scope)
    })];

    let result = client
        .query(sql, &params)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query chunk hashes: {}", e))?;

    let mut chunk_hashes = HashMap::new();

    for record in result.records {
        if let (Some(pos_value), Some(hash_value)) = (
            record.data.get("chunk_position"),
            record.data.get("chunk_hash"),
        ) {
            if let (Some(pos), Some(hash)) =
                (pos_value.as_u64().map(|p| p as usize), hash_value.as_str())
            {
                chunk_hashes.insert(pos, hash.to_string());
            }
        }
    }

    debug!(
        "Found {} chunk hashes for note {}",
        chunk_hashes.len(),
        doc_id
    );

    Ok(chunk_hashes)
}

/// Delete specific chunks by position for a note
pub async fn delete_document_chunks(
    client: &SurrealClient,
    doc_id: &str,
    chunk_positions: &[usize],
) -> Result<usize> {
    if chunk_positions.is_empty() {
        return Ok(0);
    }

    debug!(
        "Deleting {} chunks for note: {}",
        chunk_positions.len(),
        doc_id
    );

    let normalized = normalize_document_id(doc_id);
    let scope = chunk_namespace(&normalized);

    let mut total_deleted = 0;

    for &pos in chunk_positions {
        let chunk_id = format!("embeddings:{}_chunk_{}", scope, pos);
        let sql = format!("DELETE {}", chunk_id);

        let result = client
            .query(&sql, &[])
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete chunk {}: {}", chunk_id, e))?;

        if !result.records.is_empty() {
            total_deleted += 1;
        }
    }

    debug!("Deleted {} chunks for note {}", total_deleted, doc_id);

    Ok(total_deleted)
}

/// Get database statistics for embeddings
pub async fn get_database_stats(client: &SurrealClient) -> Result<DatabaseStats> {
    let documents_sql =
        r#"SELECT count() AS total FROM entities WHERE entity_type = "note" GROUP ALL"#;
    let embeddings_sql = "SELECT count() AS total FROM embeddings GROUP ALL";

    let documents_result = client
        .query(documents_sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query note count: {}", e))?;
    let embeddings_result = client
        .query(embeddings_sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query embeddings count: {}", e))?;

    let total_documents = documents_result
        .records
        .first()
        .and_then(|r| r.data.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    let total_embeddings = embeddings_result
        .records
        .first()
        .and_then(|r| r.data.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    Ok(DatabaseStats {
        total_documents,
        total_embeddings,
        storage_size_bytes: 0,
        last_updated: chrono::Utc::now(),
    })
}

/// Retrieve all note embeddings from the database
pub async fn get_all_document_embeddings(client: &SurrealClient) -> Result<Vec<DocumentEmbedding>> {
    let sql = "SELECT * FROM embeddings";

    let result = client
        .query(sql, &[])
        .await
        .map_err(|e| anyhow!("Failed to retrieve note embeddings: {}", e))?;

    let mut embeddings = Vec::new();
    for record in result.records {
        match convert_record_to_document_embedding(&record) {
            Ok(embedding) => embeddings.push(embedding),
            Err(e) => {
                warn!(
                    "Failed to convert database record to DocumentEmbedding: {}",
                    e
                );
                continue;
            }
        }
    }

    Ok(embeddings)
}

/// Convert database record to DocumentEmbedding
fn convert_record_to_document_embedding(record: &Record) -> Result<DocumentEmbedding> {
    let embedding_id = record
        .id
        .as_ref()
        .ok_or_else(|| anyhow!("Missing id in embedding record"))?
        .to_string();

    let note_id = embedding_id
        .strip_prefix("embeddings:")
        .ok_or_else(|| anyhow!("Invalid embedding ID format: {}", embedding_id))?
        .rsplit_once("_chunk_")
        .map(|(doc_part, _)| format!("notes:{}", doc_part))
        .ok_or_else(|| anyhow!("Cannot extract note_id from embedding ID: {}", embedding_id))?;

    convert_record_to_document_embedding_with_id(record, &note_id)
}

/// Convert record to DocumentEmbedding with explicit document_id
fn convert_record_to_document_embedding_with_id(
    record: &Record,
    document_id: &str,
) -> Result<DocumentEmbedding> {
    let document_id = document_id.to_string();

    let chunk_id = record
        .data
        .get("chunk_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let vector = record
        .data
        .get("vector")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("Missing or invalid vector in embedding record"))?
        .iter()
        .filter_map(|v| v.as_f64())
        .map(|v| v as f32)
        .collect::<Vec<f32>>();

    let embedding_model = record
        .data
        .get("embedding_model")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing embedding_model in embedding record"))?
        .to_string();

    let chunk_size = record
        .data
        .get("chunk_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;

    let chunk_position = record
        .data
        .get("chunk_position")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let created_at = record
        .data
        .get("created_at")
        .and_then(|v| v.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);

    let mut embedding = DocumentEmbedding::new(document_id, vector, embedding_model);
    embedding.chunk_id = chunk_id;
    embedding.chunk_size = chunk_size;
    embedding.chunk_position = chunk_position;
    embedding.created_at = created_at;

    Ok(embedding)
}
