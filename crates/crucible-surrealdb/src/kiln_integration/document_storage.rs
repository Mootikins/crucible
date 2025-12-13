//! Document storage and retrieval
//!
//! Functions for storing and retrieving ParsedNote documents.

use crate::eav_graph::{EAVGraphStore, NoteIngestor};
use crate::types::Record;
use crate::SurrealClient;
use anyhow::Result;
use crucible_core::parser::Frontmatter;
use crucible_core::types::{FrontmatterFormat, ParsedNote, Tag};
use serde_json::json;
use std::path::PathBuf;
use tracing::info;

use super::utils::{normalize_document_id, parse_timestamp, resolve_relative_path};

/// Store a ParsedNote in the database
///
/// # Arguments
/// * `client` - SurrealDB client
/// * `doc` - The parsed note to store
/// * `kiln_root` - Root path of the kiln (for generating relative IDs)
///
/// # Returns
/// The record ID (e.g., "entities:note:Projects/file.md")
pub async fn store_parsed_document(
    client: &SurrealClient,
    doc: &ParsedNote,
    kiln_root: &std::path::Path,
) -> Result<String> {
    let store = EAVGraphStore::new(client.clone());
    let ingestor = NoteIngestor::new(&store);
    let relative_path = resolve_relative_path(&doc.path, kiln_root);
    let entity_id = ingestor.ingest(doc, &relative_path).await?;
    let record_id = format!("entities:{}", entity_id.id);

    info!(
        "Stored note {} with {} wikilinks and {} tags (relations pending)",
        record_id,
        doc.wikilinks.len(),
        doc.tags.len()
    );

    Ok(record_id)
}

/// Retrieve a ParsedNote from the database by ID
pub async fn retrieve_parsed_document(client: &SurrealClient, id: &str) -> Result<ParsedNote> {
    let normalized = normalize_document_id(id);
    if let Some(doc) = fetch_document_by_id(client, &normalized).await? {
        Ok(doc)
    } else {
        Err(anyhow::anyhow!("Note not found: {}", normalized))
    }
}

/// Fetch a document by its ID
pub(crate) async fn fetch_document_by_id(
    client: &SurrealClient,
    record_id: &str,
) -> Result<Option<ParsedNote>> {
    let normalized = normalize_document_id(record_id);
    let (table, id) = match normalized.split_once(':') {
        Some((table, id)) => (table.to_string(), id.to_string()),
        None => return Ok(None),
    };
    let sql = r#"SELECT * FROM type::thing($table, $id)"#;
    let result = client
        .query(sql, &[json!({ "table": table, "id": id })])
        .await?;
    if let Some(first) = result.records.first() {
        let doc = convert_record_to_parsed_document(first).await?;
        Ok(Some(doc))
    } else {
        Ok(None)
    }
}

/// Convert a database record to ParsedNote
///
/// Note: Content is read from disk using the stored path, not from the database.
/// This avoids storing duplicate plain text in the database.
pub(crate) async fn convert_record_to_parsed_document(record: &Record) -> Result<ParsedNote> {
    let data_map = record.data.get("data").and_then(|value| value.as_object());

    let path = data_map
        .and_then(|obj| obj.get("path").and_then(|v| v.as_str()))
        .or_else(|| record.data.get("path").and_then(|v| v.as_str()))
        .unwrap_or("unknown.md");

    let mut doc = ParsedNote::new(PathBuf::from(path));

    // Read content from disk instead of database
    if let Ok(content) = tokio::fs::read_to_string(path).await {
        doc.content.plain_text = content;
    }

    doc.parsed_at = parse_timestamp(
        data_map.and_then(|obj| obj.get("parsed_at")),
        record.data.get("updated_at"),
        record.data.get("created_at"),
    );

    doc.content_hash = record
        .data
        .get("content_hash")
        .and_then(|v| v.as_str())
        .or_else(|| data_map.and_then(|obj| obj.get("content_hash").and_then(|v| v.as_str())))
        .unwrap_or("")
        .to_string();

    doc.file_size = data_map
        .and_then(|obj| obj.get("file_size").and_then(|v| v.as_u64()))
        .or_else(|| record.data.get("file_size").and_then(|v| v.as_u64()))
        .unwrap_or(0);

    if let Some(frontmatter) = data_map
        .and_then(|obj| obj.get("frontmatter"))
        .and_then(|v| v.as_object())
    {
        let yaml_str = serde_yaml::to_string(frontmatter)
            .map_err(|e| anyhow::anyhow!("Failed to serialize frontmatter: {}", e))?;
        doc.frontmatter = Some(Frontmatter::new(yaml_str, FrontmatterFormat::Yaml));
    } else if let Some(title) = data_map
        .and_then(|obj| obj.get("title").and_then(|v| v.as_str()))
        .or_else(|| record.data.get("title").and_then(|v| v.as_str()))
    {
        let mut metadata_map = serde_json::Map::new();
        metadata_map.insert(
            "title".to_string(),
            serde_json::Value::String(title.to_string()),
        );
        let yaml_str = serde_yaml::to_string(&metadata_map)
            .map_err(|e| anyhow::anyhow!("Failed to serialize title metadata: {}", e))?;
        doc.frontmatter = Some(Frontmatter::new(yaml_str, FrontmatterFormat::Yaml));
    } else if let Some(metadata) = record.data.get("metadata") {
        if let serde_json::Value::Object(map) = metadata.clone() {
            let yaml_str = serde_yaml::to_string(&map)
                .map_err(|e| anyhow::anyhow!("Failed to serialize metadata: {}", e))?;
            doc.frontmatter = Some(Frontmatter::new(yaml_str, FrontmatterFormat::Yaml));
        }
    }

    if let Some(tags) = data_map
        .and_then(|obj| obj.get("tags"))
        .and_then(|value| value.as_array())
    {
        doc.tags = tags
            .iter()
            .filter_map(|tag| tag.as_str())
            .map(|tag| Tag::new(tag, 0))
            .collect();
    } else if let Some(tags_array) = record.data.get("tags").and_then(|v| v.as_array()) {
        doc.tags = tags_array
            .iter()
            .filter_map(|tag| tag.as_str())
            .map(|tag| Tag::new(tag, 0))
            .collect();
    }

    Ok(doc)
}
