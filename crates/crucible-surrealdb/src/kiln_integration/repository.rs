//! KnowledgeRepository trait implementation
//!
//! Implements the KnowledgeRepository trait for SurrealClient.

use crate::SurrealClient;
use async_trait::async_trait;
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_core::types::{ParsedNote, SearchResult};
use crucible_core::{CrucibleError, Result as CoreResult};
use serde_json::json;

use super::document_storage::{convert_record_to_parsed_document, retrieve_parsed_document};
use super::relations::find_entity_id_by_title;
use super::utils::record_ref_to_string;

/// Get note by name internal implementation
async fn get_note_by_name_internal(
    client: &SurrealClient,
    name: &str,
) -> anyhow::Result<Option<ParsedNote>> {
    // Try to find by exact ID first
    if let Ok(doc) = retrieve_parsed_document(client, name).await {
        return Ok(Some(doc));
    }

    // Try to find by title
    if let Some(entity_id) = find_entity_id_by_title(client, name).await? {
        let id_str = format!("{}:{}", entity_id.table, entity_id.id);
        if let Ok(doc) = retrieve_parsed_document(client, &id_str).await {
            return Ok(Some(doc));
        }
    }

    // Try to find by filename (path)
    let sql = r#"
        SELECT * FROM entities
        WHERE content_category = 'note'
            AND string::ends_with(path, $name)
        LIMIT 1
    "#;
    let result = client.query(sql, &[json!({ "name": name })]).await?;
    if let Some(record) = result.records.first() {
        return Ok(Some(convert_record_to_parsed_document(record).await?));
    }

    Ok(None)
}

/// List notes internal implementation
async fn list_notes_internal(
    client: &SurrealClient,
    path_filter: Option<&str>,
) -> anyhow::Result<Vec<NoteInfo>> {
    let sql = if let Some(_path) = path_filter {
        r#"
            SELECT * FROM entities
            WHERE content_category = 'note'
                AND string::starts_with(path, $path)
        "#
    } else {
        r#"
            SELECT * FROM entities
            WHERE content_category = 'note'
        "#
    };

    let params = if let Some(path) = path_filter {
        vec![json!({ "path": path })]
    } else {
        vec![]
    };

    let result = client.query(sql, &params).await?;
    let mut notes = Vec::new();

    for record in result.records {
        let doc = convert_record_to_parsed_document(&record).await?;
        notes.push(NoteInfo {
            name: doc
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            path: doc.path.to_string_lossy().to_string(),
            title: Some(doc.title()),
            tags: doc.tags.iter().map(|t| t.name.clone()).collect(),
            created_at: Some(doc.parsed_at),
            updated_at: Some(doc.parsed_at),
        });
    }

    Ok(notes)
}

/// Search vectors internal implementation
async fn search_vectors_internal(
    client: &SurrealClient,
    vector: Vec<f32>,
) -> anyhow::Result<Vec<SearchResult>> {
    let sql = r#"
        SELECT
            entity_id,
            vector::similarity::cosine(embedding, $vector) AS score
        FROM embeddings
        ORDER BY score DESC
        LIMIT 20
    "#;

    let result = client.query(sql, &[json!({ "vector": vector })]).await?;
    let mut search_results = Vec::new();

    for record in result.records {
        let score = record
            .data
            .get("score")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        if score < 0.5 {
            continue;
        }

        if let Some(entity_id_val) = record.data.get("entity_id") {
            let entity_id_str = record_ref_to_string(entity_id_val).unwrap_or_default();
            if entity_id_str.is_empty() {
                continue;
            }

            let snippet = record
                .data
                .get("content_used")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            search_results.push(SearchResult {
                document_id: crucible_core::database::DocumentId(entity_id_str),
                score,
                highlights: None,
                snippet,
            });
        }
    }

    Ok(search_results)
}

#[async_trait]
impl KnowledgeRepository for SurrealClient {
    async fn get_note_by_name(&self, name: &str) -> CoreResult<Option<ParsedNote>> {
        get_note_by_name_internal(self, name)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))
    }

    async fn list_notes(&self, path_filter: Option<&str>) -> CoreResult<Vec<NoteInfo>> {
        list_notes_internal(self, path_filter)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))
    }

    async fn search_vectors(&self, vector: Vec<f32>) -> CoreResult<Vec<SearchResult>> {
        search_vectors_internal(self, vector)
            .await
            .map_err(|e| CrucibleError::DatabaseError(e.to_string()))
    }
}
