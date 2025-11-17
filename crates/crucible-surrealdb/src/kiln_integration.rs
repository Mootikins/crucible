//! Kiln Integration Module
//!
//! This module provides the integration layer between the parser system and SurrealDB.
//! It implements the bridge between ParsedNote structures and the database schema.
//! Includes comprehensive vector embedding support for semantic search and processing.

use crate::eav_graph::{
    apply_eav_graph_schema, EAVGraphStore, EntityRecord as EAVGraphEntityRecord, NoteIngestor,
    RecordId as EAVGraphRecordId, Relation, RelationRecord,
};
// TODO: Update to use new enrichment architecture (EnrichedNote, NoteEnricher)
// These types were part of the old embedding_pool architecture that has been replaced
// use crucible_enrichment::{DocumentEmbedding, EmbeddingConfig, EmbeddingError, EmbeddingModel, EmbeddingProcessingResult, PrivacyMode, ThreadPoolMetrics};
use crate::types::{DatabaseStats, Record};
use crate::SurrealClient;
use anyhow::{anyhow, Result};
use crucible_core::parser::Wikilink;
use crucible_core::types::{FrontmatterFormat, ParsedNote, Tag};
use serde_json::json;
use std::path::{Component, Path, PathBuf};
use tracing::{debug, error, info, warn};

/// Initialize the kiln schema in the database
pub async fn initialize_kiln_schema(client: &SurrealClient) -> Result<()> {
    apply_eav_graph_schema(client).await?;
    info!("Kiln schema initialized using EAV+Graph definitions");
    Ok(())
}

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

fn resolve_relative_path(path: &std::path::Path, kiln_root: &std::path::Path) -> String {
    let relative = path.strip_prefix(kiln_root).unwrap_or(path).to_path_buf();

    let mut normalized = relative.to_string_lossy().replace('\\', "/");
    if normalized.starts_with("./") {
        normalized = normalized.trim_start_matches("./").to_string();
    }
    if normalized.starts_with('/') {
        normalized = normalized.trim_start_matches('/').to_string();
    }
    if normalized.is_empty() {
        return path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("note")
            .to_string();
    }
    normalized
}

pub fn normalize_document_id(doc_id: &str) -> String {
    if doc_id.starts_with("entities:") {
        return doc_id.to_string();
    }

    if doc_id.starts_with("note:") {
        return format!("entities:{}", doc_id);
    }

    if let Some(stripped) = doc_id.strip_prefix("notes:") {
        if stripped.starts_with("note:") {
            return format!("entities:{}", stripped);
        }
        return format!("entities:note:{}", stripped);
    }

    format!("entities:{}", doc_id)
}

fn chunk_namespace(normalized_doc_id: &str) -> String {
    let body = record_body(normalized_doc_id);
    let trimmed = body.trim_start_matches("note:");
    trimmed
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .replace(['\\', '/', ':'], "_")
}

const EMBEDDING_DIMENSION: usize = 384;

fn normalize_embedding_vector(mut vector: Vec<f32>) -> Vec<f32> {
    if vector.len() > EMBEDDING_DIMENSION {
        vector.truncate(EMBEDDING_DIMENSION);
        vector
    } else if vector.len() < EMBEDDING_DIMENSION {
        vector.resize(EMBEDDING_DIMENSION, 0.0);
        vector
    } else {
        vector
    }
}

fn escape_record_id(value: &str) -> String {
    value.replace('\'', "\\'")
}

fn chunk_record_body(chunk_id: &str) -> &str {
    chunk_id.strip_prefix("embeddings:").unwrap_or(chunk_id)
}

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

    let update_sql = format!(
        "
        UPDATE embeddings:⟨{chunk}⟩
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
    let result = client.query(&update_sql, &[params.clone()]).await?;

    if result.records.is_empty() {
        let create_sql = format!(
            "
            CREATE embeddings:⟨{chunk}⟩
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
        client
            .query(&create_sql, &[params])
            .await
            .map_err(|e| anyhow::anyhow!("Failed to insert embedding: {}", e))?;
    }

    Ok(())
}

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

    client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create graph relation: {}", e))?;

    Ok(())
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

/// Metadata describing the current embedding index.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingIndexMetadata {
    pub model: Option<String>,
    pub dimensions: Option<usize>,
}

/// Fetch the metadata for stored embeddings, if any exist.
pub async fn get_embedding_index_metadata(
    client: &SurrealClient,
) -> Result<Option<EmbeddingIndexMetadata>> {
    let sql = "SELECT embedding_model, vector_dimensions FROM embeddings LIMIT 1";
    let result = client
        .query(sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to inspect embedding metadata: {}", e))?;

    if let Some(record) = result.records.first() {
        let model = record
            .data
            .get("embedding_model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let dimensions = record
            .data
            .get("vector_dimensions")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        Ok(Some(EmbeddingIndexMetadata { model, dimensions }))
    } else {
        Ok(None)
    }
}

/// Create wikilink relationships for a note
pub async fn create_wikilink_edges(
    client: &SurrealClient,
    doc_id: &str,
    doc: &ParsedNote,
    kiln_root: &Path,
) -> Result<()> {
    if doc.wikilinks.is_empty() {
        debug!("no wikilinks detected for {}", doc.path.display());
        return Ok(());
    }

    let entity_id = parse_entity_record_id(doc_id)?;
    let store = EAVGraphStore::new(client.clone());
    store.delete_relations_from(&entity_id, "wikilink").await?;

    let mut created = 0usize;
    for (index, wikilink) in doc.wikilinks.iter().enumerate() {
        if wikilink.is_embed {
            continue;
        }

        let Some(relative_path) = resolve_wikilink_target(doc, kiln_root, wikilink) else {
            debug!(
                "Skipping wikilink '{}' from {} because target path could not be resolved",
                wikilink.target,
                doc.path.display()
            );
            continue;
        };

        let target_id = EAVGraphRecordId::new("entities", format!("note:{}", relative_path));
        store
            .ensure_note_entity(&target_id, wikilink.display())
            .await?;

        let relation = Relation {
            id: Some(relation_record_id(
                &entity_id, &target_id, "wikilink", index,
            )),
            from_id: entity_id.clone(),
            to_id: target_id.clone(),
            relation_type: "wikilink".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "parser".to_string(),
            position: Some(index as i32),
            metadata: json!({
                "alias": wikilink.alias,
                "heading_ref": wikilink.heading_ref,
                "block_ref": wikilink.block_ref,
            }),
            content_category: "note".to_string(),
            created_at: chrono::Utc::now(),
        };

        store.upsert_relation(&relation).await?;
        create_backlink_relation(
            &store,
            &target_id,
            &entity_id,
            "wikilink",
            index,
            json!({ "source_title": doc.title() }),
        )
        .await?;
        created += 1;
    }

    debug!(
        "created {} wikilink relations for {}",
        created,
        doc.path.display()
    );

    Ok(())
}

pub(crate) fn parse_entity_record_id(
    doc_id: &str,
) -> Result<EAVGraphRecordId<EAVGraphEntityRecord>> {
    let normalized = normalize_document_id(doc_id);
    let (_, id) = normalized
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid note id '{}'", doc_id))?;
    Ok(EAVGraphRecordId::new("entities", id))
}

fn relation_record_id(
    from_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
    to_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
    relation_type: &str,
    position: usize,
) -> EAVGraphRecordId<RelationRecord> {
    let from_part = from_id.id.replace(':', "_");
    let to_part = to_id.id.replace(':', "_");
    let rel_part = relation_type.replace(':', "_");
    EAVGraphRecordId::new(
        "relations",
        format!("rel:{}:{}:{}:{}", from_part, rel_part, to_part, position),
    )
}

fn resolve_wikilink_target(
    doc: &ParsedNote,
    kiln_root: &Path,
    wikilink: &Wikilink,
) -> Option<String> {
    let mut target = wikilink.target.trim().replace('\\', "/");
    if target.is_empty() {
        return None;
    }

    let mut is_absolute = false;
    if target.starts_with('/') {
        target = target.trim_start_matches('/').to_string();
        is_absolute = true;
    }

    let lowercase = target.to_ascii_lowercase();
    if !(lowercase.ends_with(".md") || lowercase.ends_with(".markdown")) {
        target.push_str(".md");
    }

    let mut candidate = PathBuf::from(target);
    if !is_absolute {
        let relative_doc = PathBuf::from(resolve_relative_path(&doc.path, kiln_root));
        let parent = relative_doc
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(PathBuf::new);
        candidate = parent.join(candidate);
    }

    let normalized = clean_relative_path(&candidate)?;
    Some(normalized.to_string_lossy().replace('\\', "/"))
}

fn clean_relative_path(path: &Path) -> Option<PathBuf> {
    let mut stack: Vec<PathBuf> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if stack.pop().is_none() {
                    return None;
                }
            }
            Component::Normal(part) => stack.push(PathBuf::from(part)),
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }

    let mut normalized = PathBuf::new();
    for part in stack {
        normalized.push(part);
    }

    Some(normalized)
}

async fn create_backlink_relation(
    store: &EAVGraphStore,
    from_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
    to_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
    edge_type: &str,
    position: usize,
    metadata: serde_json::Value,
) -> Result<()> {
    let relation_type = format!("{}_backlink", edge_type);
    let relation = Relation {
        id: Some(relation_record_id(from_id, to_id, &relation_type, position)),
        from_id: from_id.clone(),
        to_id: to_id.clone(),
        relation_type,
        weight: 1.0,
        directed: true,
        confidence: 1.0,
        source: "parser".to_string(),
        position: Some(position as i32),
        metadata,
        content_category: "note".to_string(),
        created_at: chrono::Utc::now(),
    };

    store.upsert_relation(&relation).await?;
    Ok(())
}

fn record_ref_to_string(value: &serde_json::Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }

    if let Some(obj) = value.as_object() {
        if let Some(thing) = obj.get("thing").and_then(|v| v.as_str()) {
            return Some(thing.to_string());
        }
        let table = obj.get("tb")?.as_str()?;
        let id = obj.get("id")?.as_str()?;
        return Some(format!("{}:{}", table, id));
    }

    None
}

fn embed_type_from_metadata(value: &serde_json::Value) -> String {
    value
        .as_object()
        .and_then(|obj| obj.get("embed_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("simple")
        .to_string()
}

async fn fetch_document_by_id(
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

async fn query_relation_documents(
    client: &SurrealClient,
    doc_id: &str,
    relation_type: &str,
) -> Result<Vec<ParsedNote>> {
    let entity = parse_entity_record_id(doc_id)?;
    let sql = r#"
        SELECT out AS target
        FROM relations
        WHERE relation_type = $relation_type
          AND in = type::thing($table, $id)
    "#;

    let result = client
        .query(
            sql,
            &[json!({
                "relation_type": relation_type,
                "table": entity.table,
                "id": entity.id,
            })],
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query {} relations: {}", relation_type, e))?;

    let mut documents = Vec::new();
    for record in result.records {
        if let Some(target_id) = record.data.get("target").and_then(record_ref_to_string) {
            if let Some(doc) = fetch_document_by_id(client, &target_id).await? {
                documents.push(doc);
            }
        }
    }

    Ok(documents)
}

async fn find_entity_id_by_title(
    client: &SurrealClient,
    title: &str,
) -> Result<Option<EAVGraphRecordId<EAVGraphEntityRecord>>> {
    let sql = r#"
        SELECT entity_id
        FROM properties
        WHERE namespace = "core"
          AND key = "title"
          AND value.type = "text"
          AND value.value = $title
        LIMIT 1
    "#;

    let result = client.query(sql, &[json!({ "title": title })]).await?;
    if let Some(record) = result.records.first() {
        if let Some(entity_str) = record.data.get("entity_id").and_then(record_ref_to_string) {
            if let Some((table, id)) = entity_str.split_once(':') {
                return Ok(Some(EAVGraphRecordId::new(table, id)));
            }
        }
    }
    Ok(None)
}

async fn query_embedding_sources_for_entity(
    client: &SurrealClient,
    entity_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
) -> Result<Vec<ParsedNote>> {
    let pairs = fetch_embed_relation_pairs(client).await?;
    let target_key = entity_id.id.clone();
    let mut documents = Vec::new();

    for (source, target) in pairs {
        if record_body(&target) == target_key {
            if let Some(doc) = fetch_document_by_id(client, &source).await? {
                documents.push(doc);
            }
        }
    }

    Ok(documents)
}

async fn query_embedding_sources_by_title(
    client: &SurrealClient,
    target_title: &str,
) -> Result<Vec<ParsedNote>> {
    let pairs = fetch_embed_relation_pairs(client).await?;
    let mut documents = Vec::new();

    for (source, target) in pairs {
        if let Some(target_doc) = fetch_document_by_id(client, &target).await? {
            if target_doc.title() == target_title {
                if let Some(doc) = fetch_document_by_id(client, &source).await? {
                    documents.push(doc);
                }
            }
        }
    }

    Ok(documents)
}

async fn fetch_embed_relation_pairs(client: &SurrealClient) -> Result<Vec<(String, String)>> {
    let sql = r#"
        SELECT in AS source, out AS target
        FROM relations
        WHERE relation_type = "embed"
    "#;

    let result = client.query(sql, &[]).await?;
    let mut pairs = Vec::new();

    for record in result.records {
        let Some(source_id) = record.data.get("source").and_then(record_ref_to_string) else {
            continue;
        };
        let Some(target_id) = record.data.get("target").and_then(record_ref_to_string) else {
            continue;
        };
        pairs.push((source_id, target_id));
    }

    Ok(pairs)
}

fn record_body(reference: &str) -> &str {
    if let Some((prefix, rest)) = reference.split_once(':') {
        if prefix == "entities" || prefix == "notes" {
            return rest;
        }
    }
    reference
}

/// Get documents linked via wikilinks
pub async fn get_linked_documents(client: &SurrealClient, doc_id: &str) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "wikilink").await
}

/// Get documents by tag
pub async fn get_documents_by_tag(client: &SurrealClient, tag: &str) -> Result<Vec<ParsedNote>> {
    // Tags are stored using the path directly (e.g., "project/ai/nlp")
    let tag_path = tag.trim().trim_start_matches('#');
    let sql = r#"
        SELECT entity_id
        FROM entity_tags
        WHERE tag_id = type::thing("tags", $tag_id)
    "#;

    let result = client
        .query(sql, &[json!({ "tag_id": tag_path })])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query documents by tag: {}", e))?;

    let mut documents = Vec::new();
    for record in &result.records {
        if let Some(source_id) = record.data.get("entity_id").and_then(record_ref_to_string) {
            if let Some(doc) = fetch_document_by_id(client, &source_id).await? {
                documents.push(doc);
            }
        }
    }

    Ok(documents)
}

// Helper functions

async fn convert_record_to_parsed_document(record: &Record) -> Result<ParsedNote> {
    let data_map = record.data.get("data").and_then(|value| value.as_object());

    let path = data_map
        .and_then(|obj| obj.get("path").and_then(|v| v.as_str()))
        .or_else(|| record.data.get("path").and_then(|v| v.as_str()))
        .unwrap_or("unknown.md");

    let mut doc = ParsedNote::new(PathBuf::from(path));

    doc.content.plain_text = data_map
        .and_then(|obj| obj.get("content").and_then(|v| v.as_str()))
        .or_else(|| record.data.get("content").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

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
        doc.frontmatter = Some(crucible_core::parser::Frontmatter::new(
            yaml_str,
            FrontmatterFormat::Yaml,
        ));
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
        doc.frontmatter = Some(crucible_core::parser::Frontmatter::new(
            yaml_str,
            FrontmatterFormat::Yaml,
        ));
    } else if let Some(metadata) = record.data.get("metadata") {
        if let serde_json::Value::Object(map) = metadata.clone() {
            let yaml_str = serde_yaml::to_string(&map)
                .map_err(|e| anyhow::anyhow!("Failed to serialize metadata: {}", e))?;
            doc.frontmatter = Some(crucible_core::parser::Frontmatter::new(
                yaml_str,
                FrontmatterFormat::Yaml,
            ));
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

fn parse_timestamp(
    primary: Option<&serde_json::Value>,
    fallback_one: Option<&serde_json::Value>,
    fallback_two: Option<&serde_json::Value>,
) -> chrono::DateTime<chrono::Utc> {
    let candidates = [
        primary.and_then(|v| v.as_str()),
        fallback_one.and_then(|v| v.as_str()),
        fallback_two.and_then(|v| v.as_str()),
    ];

    for candidate in candidates {
        if let Some(ts) = candidate {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(ts) {
                return parsed.with_timezone(&chrono::Utc);
            }
        }
    }

    chrono::Utc::now()
}

async fn ensure_tag_exists(client: &SurrealClient, tag_name: &str) -> Result<()> {
    let normalized_name = normalize_tag_name(tag_name);

    // Use UPDATE with RETURN NONE to create-or-update the tag without failing on duplicates
    // This is idempotent and won't error if the tag already exists
    let upsert_sql = format!(
        "UPDATE tag:{} SET name = '{}', created_at = created_at OR time::now() RETURN NONE",
        normalized_name,
        tag_name.replace('\'', "\\'") // Escape single quotes
    );

    client.query(&upsert_sql, &[]).await?;
    Ok(())
}

fn normalize_tag_name(tag: &str) -> String {
    tag.to_lowercase()
        .replace([' ', '-', '/'], "_")
        .replace(['#'], "")
}

// =============================================================================
// EMBED RELATIONSHIP FUNCTIONS
// =============================================================================

/// Create embed relationships for a note
pub async fn create_embed_relationships(
    client: &SurrealClient,
    doc_id: &str,
    doc: &ParsedNote,
    kiln_root: &Path,
) -> Result<()> {
    let embeds: Vec<(usize, &Wikilink)> = doc
        .wikilinks
        .iter()
        .enumerate()
        .filter(|(_, link)| link.is_embed)
        .collect();

    if embeds.is_empty() {
        debug!("no embeds detected for {}", doc.path.display());
        return Ok(());
    }

    let entity_id = parse_entity_record_id(doc_id)?;
    let store = EAVGraphStore::new(client.clone());
    store.delete_relations_from(&entity_id, "embed").await?;

    for (index, wikilink) in embeds {
        let Some(relative_path) = resolve_wikilink_target(doc, kiln_root, wikilink) else {
            debug!(
                "Skipping embed '{}' from {} because target path could not be resolved",
                wikilink.target,
                doc.path.display()
            );
            continue;
        };

        let target_id = EAVGraphRecordId::new("entities", format!("note:{}", relative_path));
        store
            .ensure_note_entity(&target_id, wikilink.display())
            .await?;

        let embed_type = determine_embed_type(wikilink);
        let relation = Relation {
            id: Some(relation_record_id(&entity_id, &target_id, "embed", index)),
            from_id: entity_id.clone(),
            to_id: target_id.clone(),
            relation_type: "embed".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "parser".to_string(),
            position: Some(index as i32),
            metadata: json!({
                "embed_type": embed_type,
                "alias": wikilink.alias,
                "heading_ref": wikilink.heading_ref,
                "block_ref": wikilink.block_ref,
            }),
            content_category: "note".to_string(),
            created_at: chrono::Utc::now(),
        };
        store.upsert_relation(&relation).await?;

        create_backlink_relation(
            &store,
            &target_id,
            &entity_id,
            "embed",
            index,
            json!({ "embed_type": embed_type }),
        )
        .await?;
    }

    Ok(())
}

/// Get documents embedded by a note
pub async fn get_embedded_documents(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "embed").await
}

/// Get embed metadata for a note
pub async fn get_embed_metadata(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<EmbedMetadata>> {
    let entity = parse_entity_record_id(doc_id)?;
    let sql = r#"
        SELECT out, metadata, position
        FROM relations
        WHERE relation_type = "embed"
          AND in = type::thing($table, $id)
    "#;

    let result = client
        .query(
            sql,
            &[json!({
                "table": entity.table,
                "id": entity.id,
            })],
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query embed metadata: {}", e))?;

    let mut metadata_list = Vec::new();
    for record in result.records {
        let target_id = match record.data.get("out").and_then(record_ref_to_string) {
            Some(id) => id,
            None => continue,
        };
        let target_title = fetch_document_by_id(client, &target_id)
            .await?
            .map(|doc| doc.title())
            .unwrap_or_else(|| "Unknown".to_string());

        let metadata = record
            .data
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let heading_ref = metadata
            .get("heading_ref")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let block_ref = metadata
            .get("block_ref")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let alias = metadata
            .get("alias")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        metadata_list.push(EmbedMetadata {
            target: target_title,
            is_embed: true,
            heading_ref,
            block_ref,
            alias,
            position: record
                .data
                .get("position")
                .and_then(|p| p.as_i64())
                .unwrap_or(0) as usize,
        });
    }

    Ok(metadata_list)
}

/// Get embedded documents filtered by embed type
pub async fn get_embedded_documents_by_type(
    client: &SurrealClient,
    doc_id: &str,
    embed_type: &str,
) -> Result<Vec<ParsedNote>> {
    let entity = parse_entity_record_id(doc_id)?;
    let sql = r#"
        SELECT out, metadata
        FROM relations
        WHERE relation_type = "embed"
          AND in = type::thing($table, $id)
    "#;

    let result = client
        .query(
            sql,
            &[json!({
                "table": entity.table,
                "id": entity.id,
            })],
        )
        .await?;

    let mut documents = Vec::new();
    for record in result.records.iter() {
        let relation_embed_type = record
            .data
            .get("metadata")
            .and_then(|m| m.get("embed_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("simple");
        if !relation_embed_type.eq_ignore_ascii_case(embed_type) {
            continue;
        }
        if let Some(target_id) = record.data.get("out").and_then(record_ref_to_string) {
            if let Some(doc) = fetch_document_by_id(client, &target_id).await? {
                documents.push(doc);
            }
        }
    }

    Ok(documents)
}

/// Get documents linked via wikilinks (separate from embeds)
pub async fn get_wikilinked_documents(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "wikilink").await
}

/// Get wikilink relations for a note
pub async fn get_wikilink_relations(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<LinkRelation>> {
    let entity = parse_entity_record_id(doc_id)?;
    let sql = r#"
        SELECT out, metadata
        FROM relations
        WHERE relation_type = "wikilink"
          AND in = type::thing($table, $id)
    "#;

    let result = client
        .query(
            sql,
            &[json!({
                "table": entity.table,
                "id": entity.id,
            })],
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query wikilink relations: {}", e))?;

    let mut relations = Vec::new();
    for record in result.records {
        let target_id = match record.data.get("out").and_then(record_ref_to_string) {
            Some(id) => id,
            None => continue,
        };
        let target_title = fetch_document_by_id(client, &target_id)
            .await?
            .map(|doc| doc.title())
            .unwrap_or_else(|| "Unknown".to_string());

        let metadata = record
            .data
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        relations.push(LinkRelation {
            relation_type: "wikilink".to_string(),
            is_embed: metadata
                .get("is_embed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            target: target_title,
        });
    }

    Ok(relations)
}

/// Get embed relations for a note
pub async fn get_embed_relations(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<EmbedRelation>> {
    let entity = parse_entity_record_id(doc_id)?;
    let sql = r#"
        SELECT out, metadata
        FROM relations
        WHERE relation_type = "embed"
          AND in = type::thing($table, $id)
    "#;

    let result = client
        .query(
            sql,
            &[json!({
                "table": entity.table,
                "id": entity.id,
            })],
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query embed relations: {}", e))?;

    let mut relations = Vec::new();
    for record in result.records {
        let target_id = record
            .data
            .get("out")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let target_title = fetch_document_by_id(client, target_id)
            .await?
            .map(|doc| doc.title())
            .unwrap_or_else(|| "Unknown".to_string());

        let embed_type = record
            .data
            .get("metadata")
            .map(embed_type_from_metadata)
            .unwrap_or_else(|| "simple".to_string());

        relations.push(EmbedRelation {
            relation_type: "embed".to_string(),
            is_embed: true,
            target: target_title,
            embed_type,
        });
    }

    Ok(relations)
}

/// Get documents that embed a specific target note
pub async fn get_embedding_documents(
    client: &SurrealClient,
    target_title: &str,
) -> Result<Vec<ParsedNote>> {
    if let Some(entity_id) = find_entity_id_by_title(client, target_title).await? {
        let docs = query_embedding_sources_for_entity(client, &entity_id).await?;
        if !docs.is_empty() {
            return Ok(docs);
        }
    }

    query_embedding_sources_by_title(client, target_title).await
}

/// Get specific embed with metadata
pub async fn get_embed_with_metadata(
    client: &SurrealClient,
    doc_id: &str,
    target_title: &str,
) -> Result<Option<EmbedMetadata>> {
    let sql = format!(
        "SELECT * FROM embeds WHERE from = {} AND to = (SELECT id FROM notes WHERE title = '{}')",
        doc_id, target_title
    );

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query embed with metadata: {}", e))?;

    if let Some(record) = result.records.first() {
        let _embed_type = record
            .data
            .get("embed_type")
            .and_then(|t| t.as_str())
            .unwrap_or("simple")
            .to_string();

        let reference_target = record
            .data
            .get("reference_target")
            .and_then(|r| r.as_str())
            .map(|s| s.to_string());

        let alias = record
            .data
            .get("display_alias")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string());

        let position = record
            .data
            .get("position")
            .and_then(|p| p.as_u64())
            .unwrap_or(0) as usize;

        let (heading_ref, block_ref) = parse_reference_target(reference_target);

        Ok(Some(EmbedMetadata {
            target: target_title.to_string(),
            is_embed: true,
            heading_ref,
            block_ref,
            alias,
            position,
        }))
    } else {
        Ok(None)
    }
}

// =============================================================================
// EMBED HELPER FUNCTIONS
// =============================================================================

/// Determine the type of embed based on the wikilink properties
fn determine_embed_type(wikilink: &crucible_core::parser::Wikilink) -> String {
    if wikilink.heading_ref.is_some() {
        "heading".to_string()
    } else if wikilink.block_ref.is_some() {
        "block".to_string()
    } else if wikilink.alias.is_some() {
        "aliased".to_string()
    } else {
        "simple".to_string()
    }
}

/// Parse reference target to extract heading and block references
fn parse_reference_target(reference_target: Option<String>) -> (Option<String>, Option<String>) {
    if let Some(target) = reference_target {
        if target.starts_with("#^") {
            // Block reference
            let block_ref = target.strip_prefix("#^").map(|s| s.to_string());
            (None, block_ref)
        } else if target.starts_with('#') {
            // Heading reference
            let heading_ref = target.strip_prefix('#').map(|s| s.to_string());
            (heading_ref, None)
        } else {
            // Simple reference
            (Some(target.clone()), None)
        }
    } else {
        (None, None)
    }
}

// =============================================================================
// EMBED TYPE DEFINITIONS
// =============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedMetadata {
    pub target: String,
    pub is_embed: bool,
    pub heading_ref: Option<String>,
    pub block_ref: Option<String>,
    pub alias: Option<String>,
    pub position: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkRelation {
    pub relation_type: String,
    pub is_embed: bool,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbedRelation {
    pub relation_type: String,
    pub is_embed: bool,
    pub target: String,
    pub embed_type: String,
}

// =============================================================================
// EMBEDDING INTEGRATION FUNCTIONS
// =============================================================================

/// Store note embedding in database
pub async fn store_document_embedding(
    client: &SurrealClient,
    embedding: &DocumentEmbedding,
) -> Result<()> {
    let note_id = normalize_document_id(&embedding.document_id);

    // Use store_embedding() with graph relations
    let chunk_position = embedding.chunk_position.unwrap_or(0);

    store_embedding_with_chunk_id(
        client,
        &note_id,
        embedding.vector.clone(),
        &embedding.embedding_model,
        embedding.chunk_size,
        chunk_position,
        embedding.chunk_id.as_deref(),
        None, // dimensions not available in legacy DocumentEmbedding
    )
    .await?;

    debug!(
        "Stored embedding for note {} via graph relations",
        embedding.document_id
    );

    Ok(())
}

/// Store embedding with graph relations (Phase 4)
///
/// Creates a deterministic embedding record and establishes a graph relation
/// from the note to the embedding using SurrealDB's RELATE statement.
///
/// # Arguments
/// * `client` - SurrealDB client
/// * `note_id` - Note record ID (legacy values are normalized automatically)
/// * `vector` - Embedding vector
/// * `embedding_model` - Model name used for embedding
/// * `chunk_size` - Size of the text chunk
/// * `chunk_position` - Position of this chunk in the note
/// * `dimensions` - Optional vector dimensions (for compatibility checking)
/// * `chunk_content` - Optional chunk content (for hash computation)
///
/// # Returns
/// The deterministic chunk ID (format: "embeddings:Projects_file_md_chunk_0")
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
    let adjusted_vector = normalize_embedding_vector(vector);
    let dims = dimensions.unwrap_or(adjusted_vector.len());
    let content_used = chunk_content.unwrap_or("");

    let chunk_body = chunk_record_body(&chunk_id).to_string();
    let stored_model = format!("{}::{}", embedding_model, chunk_body);
    upsert_embedding_record(
        client,
        &chunk_id,
        &normalized_id,
        &adjusted_vector,
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
/// Store embedding with graph relations and optional chunk_id field (for backward compatibility)
///
/// This is a wrapper around store_embedding() that also stores the logical chunk_id
/// as a database field for backward compatibility with old tests/APIs.
///
/// # Arguments
/// * `client` - SurrealDB client
/// * `note_id` - Note record ID (legacy `notes:` ids are normalized automatically)
/// * `vector` - Embedding vector
/// * `embedding_model` - Model name used for embedding
/// * `chunk_size` - Size of the text chunk
/// * `chunk_position` - Position of this chunk in the note
/// * `logical_chunk_id` - Optional logical chunk identifier (e.g., "chunk-0", "chunk-1")
/// * `dimensions` - Optional vector dimensions (for compatibility checking)
///
/// # Returns
/// The deterministic chunk ID (format: "embeddings:Projects_file_md_chunk_0")
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
    let adjusted_vector = normalize_embedding_vector(vector);
    let dims = dimensions.unwrap_or(adjusted_vector.len());

    let chunk_body = chunk_record_body(&chunk_id).to_string();
    let stored_model = format!("{}::{}", embedding_model, chunk_body);
    upsert_embedding_record(
        client,
        &chunk_id,
        &normalized_id,
        &adjusted_vector,
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

    // The graph traversal returns records with 'out' field containing embedding IDs
    for record in result.records {
        // Extract the 'out' field which contains the embedding record ID
        if let Some(out_value) = record.data.get("out") {
            // Fetch the actual embedding record
            let embedding_id = out_value.as_str().ok_or_else(|| {
                anyhow::anyhow!("Expected string for embedding ID, got: {:?}", out_value)
            })?;

            // Query the embedding record by ID
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

/// Clear all embeddings for a note (deletes graph edges and embedding records)
pub async fn clear_document_embeddings(client: &SurrealClient, document_id: &str) -> Result<()> {
    // Convert document_id to note_id format
    let note_id = if document_id.starts_with("notes:") {
        document_id.to_string()
    } else {
        format!("notes:{}", document_id)
    };

    // Step 1: Query to get embedding record IDs via graph traversal
    let query_sql = format!(
        "SELECT id FROM (SELECT ->has_embedding->id AS emb_ids FROM {})[0].emb_ids",
        note_id.replace("'", "\\'")
    );
    let query_result = client.query(&query_sql, &[]).await?;

    // Extract embedding IDs from the query result
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

    // Step 2: Delete all has_embedding edges from this note
    let delete_edges_sql = format!("DELETE {} -> has_embedding", note_id.replace("'", "\\'"));
    client
        .query(&delete_edges_sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to delete has_embedding edges: {}", e))?;

    // Step 3: Delete each embedding record by ID
    for embedding_id in &embedding_ids {
        let delete_sql = format!("DELETE {}", embedding_id.replace("'", "\\'"));
        client.query(&delete_sql, &[]).await?;
    }

    debug!(
        "Cleared {} embeddings and edges for note {}",
        embedding_ids.len(),
        document_id
    );
    Ok(())
}

/// Get existing chunk hashes for a note
///
/// Returns a HashMap mapping chunk_position -> chunk_hash for existing embeddings
pub async fn get_document_chunk_hashes(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<std::collections::HashMap<usize, String>> {
    debug!("Getting chunk hashes for note: {}", doc_id);

    let normalized = normalize_document_id(doc_id);
    let scope = chunk_namespace(&normalized);

    // Query embeddings to get chunk_position and chunk_hash
    let sql = format!(
        "SELECT chunk_position, chunk_hash FROM embeddings WHERE id >= 'embeddings:{}' AND id < 'embeddings:{}~'",
        scope.replace('\'', "\\'"),
        scope.replace('\'', "\\'")
    );

    let result = client
        .query(&sql, &[])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query chunk hashes: {}", e))?;

    let mut chunk_hashes = std::collections::HashMap::new();

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
///
/// This is used for incremental re-embedding to delete only changed chunks
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

    // Delete each chunk individually
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

/// Semantic search using vector similarity
pub async fn semantic_search(
    client: &SurrealClient,
    query: &str,
    limit: usize,
    embedding_provider: std::sync::Arc<dyn crucible_llm::embeddings::EmbeddingProvider>,
) -> Result<Vec<(String, f64)>> {
    debug!(
        "Performing semantic search for query: '{}', limit: {}",
        query, limit
    );

    // Handle empty queries
    if query.trim().is_empty() {
        warn!("Empty query provided for semantic search");
        return Ok(Vec::new());
    }

    // Generate real query embedding using the provided embedding provider
    let response = embedding_provider
        .embed(query)
        .await
        .map_err(|e| anyhow!("Failed to generate query embedding: {}", e))?;

    let query_embedding = response.embedding;

    debug!(
        "Generated query embedding with {} dimensions using provider: {}",
        query_embedding.len(),
        embedding_provider.provider_name()
    );

    // Retrieve all note embeddings from database
    let stored_embeddings = match get_all_document_embeddings(client).await {
        Ok(embeddings) => embeddings,
        Err(e) => {
            error!("Failed to retrieve note embeddings: {}", e);
            return Err(anyhow!("Failed to retrieve note embeddings: {}", e));
        }
    };

    if stored_embeddings.is_empty() {
        debug!("No note embeddings found in database");
        return Ok(Vec::new());
    }

    debug!(
        "Retrieved {} note embeddings for similarity calculation",
        stored_embeddings.len()
    );

    // Calculate cosine similarity between query and all note embeddings
    let mut similarity_results = Vec::new();
    for doc_embedding in stored_embeddings {
        // Only calculate similarity if dimensions match
        if doc_embedding.vector.len() == query_embedding.len() {
            let similarity = calculate_cosine_similarity(&query_embedding, &doc_embedding.vector);
            similarity_results.push((doc_embedding.document_id.clone(), similarity));
        } else {
            debug!(
                "Skipping note {} due to dimension mismatch (query: {}, doc: {})",
                doc_embedding.document_id,
                query_embedding.len(),
                doc_embedding.vector.len()
            );
        }
    }

    // Sort by similarity score (descending)
    similarity_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Apply similarity threshold and limit
    let similarity_threshold = 0.5; // Configurable threshold
    let mut filtered_results: Vec<(String, f64)> = similarity_results
        .iter()
        .filter(|(_, score)| *score >= similarity_threshold)
        .take(limit)
        .map(|(id, score)| (id.clone(), *score))
        .collect();

    debug!(
        "Returning {} results after filtering and limiting",
        filtered_results.len()
    );

    // If no results meet the threshold, return the top results regardless of threshold
    if filtered_results.is_empty() {
        debug!("No results met similarity threshold, returning top results without threshold");
        filtered_results = similarity_results
            .iter()
            .take(limit)
            .map(|(id, score)| (id.clone(), *score))
            .collect();
    }

    Ok(filtered_results)
}

/// Perform semantic search with optional reranking for improved relevance.
///
/// This function implements a two-stage search pipeline:
/// 1. Vector search: Retrieve `initial_limit` candidates using embedding similarity
/// 2. Reranking: Optionally rerank candidates using a cross-attention model
///
/// # Arguments
/// * `client` - Database client
/// * `query` - Search query text
/// * `initial_limit` - Number of candidates to retrieve in vector search stage
/// * `reranker` - Optional reranker to improve result quality
/// * `final_limit` - Final number of results to return after reranking
/// * `embedding_provider` - Embedding provider for generating query embeddings
///
/// # Returns
/// Vec of (document_id, score) tuples, sorted by relevance
pub async fn semantic_search_with_reranking(
    client: &SurrealClient,
    query: &str,
    initial_limit: usize,
    reranker: Option<std::sync::Arc<dyn crucible_llm::Reranker>>,
    final_limit: usize,
    embedding_provider: std::sync::Arc<dyn crucible_llm::embeddings::EmbeddingProvider>,
) -> Result<Vec<(String, f64)>> {
    eprintln!(
        "DEBUG RERANK: semantic_search_with_reranking called: query='{}', initial_limit={}, final_limit={}, reranker={}",
        query,
        initial_limit,
        final_limit,
        if reranker.is_some() { "Some" } else { "None" }
    );

    // Stage 1: Vector search - retrieve more candidates than needed
    let initial_results = semantic_search(client, query, initial_limit, embedding_provider).await?;

    eprintln!(
        "DEBUG RERANK: Stage 1 vector search returned {} results",
        initial_results.len()
    );

    if initial_results.is_empty() {
        warn!("Stage 1 vector search returned no results");
        return Ok(Vec::new());
    }

    // Stage 2: Reranking (if reranker provided)
    if let Some(reranker) = reranker {
        eprintln!(
            "DEBUG RERANK: Reranking {} initial results to top {} with model: {}",
            initial_results.len(),
            final_limit,
            reranker.model_info().name
        );

        // Fetch full note content for reranking
        // Optimized: Use indexed document_id field for O(1) lookups
        let mut documents = Vec::new();
        let mut failed_retrievals = 0;
        eprintln!(
            "DEBUG RERANK: Starting optimized note retrieval for {} results",
            initial_results.len()
        );

        for (document_id, vec_score) in &initial_results {
            eprintln!("DEBUG RERANK: Fetching document_id: {}", document_id);

            let normalized_id = normalize_document_id(document_id);
            match fetch_document_by_id(client, &normalized_id).await {
                Ok(Some(doc)) => {
                    let text = doc.content.plain_text.clone();
                    eprintln!(
                        "DEBUG RERANK: Retrieved note with {} chars of text",
                        text.len()
                    );
                    documents.push((normalized_id, text, *vec_score));
                }
                Ok(None) => {
                    eprintln!(
                        "DEBUG RERANK: Note not found for document_id: {}",
                        document_id
                    );
                    failed_retrievals += 1;
                }
                Err(e) => {
                    eprintln!("DEBUG RERANK: Failed to fetch note {}: {}", document_id, e);
                    failed_retrievals += 1;
                }
            }
        }

        eprintln!(
            "DEBUG RERANK: Retrieved {}/{} documents for reranking ({} failed)",
            documents.len(),
            initial_results.len(),
            failed_retrievals
        );

        if documents.is_empty() {
            eprintln!("DEBUG RERANK: No documents could be retrieved for reranking, returning empty results");
            return Ok(Vec::new());
        }

        // Rerank with original query
        let reranked = reranker
            .rerank(query, documents, Some(final_limit))
            .await
            .map_err(|e| anyhow!("Reranking failed: {}", e))?;

        debug!("Reranking complete, returning {} results", reranked.len());

        // Convert back to (id, score) format
        Ok(reranked
            .into_iter()
            .map(|r| (r.document_id, r.score))
            .collect())
    } else {
        // No reranking, just truncate to final_limit
        Ok(initial_results.into_iter().take(final_limit).collect())
    }
}

// =============================================================================
// EMBEDDING HELPER FUNCTIONS
// =============================================================================

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

/// Calculate cosine similarity between two vectors
fn calculate_cosine_similarity(vec_a: &[f32], vec_b: &[f32]) -> f64 {
    if vec_a.len() != vec_b.len() {
        return 0.0;
    }

    if vec_a.is_empty() || vec_b.is_empty() {
        return 0.0;
    }

    // Calculate dot product
    let dot_product: f64 = vec_a
        .iter()
        .zip(vec_b.iter())
        .map(|(a, b)| *a as f64 * *b as f64)
        .sum();

    // Calculate magnitudes
    let magnitude_a: f64 = vec_a
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();

    let magnitude_b: f64 = vec_b
        .iter()
        .map(|x| (*x as f64) * (*x as f64))
        .sum::<f64>()
        .sqrt();

    // Handle zero vectors
    if magnitude_a == 0.0 || magnitude_b == 0.0 {
        return 0.0;
    }

    // Calculate cosine similarity
    dot_product / (magnitude_a * magnitude_b)
}

/// Convert database record to DocumentEmbedding (extracts document_id from record ID)
fn convert_record_to_document_embedding(record: &Record) -> Result<DocumentEmbedding> {
    // Extract note_id from the embedding record ID
    // Embedding ID format: "embeddings:Projects_file_md_chunk_0"
    // We need to extract "Projects_file_md" and create "notes:Projects_file_md"
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

    // For backward compatibility, use note_id as document_id in DocumentEmbedding
    convert_record_to_document_embedding_with_id(record, &note_id)
}

/// Convert record to DocumentEmbedding with explicit document_id
fn convert_record_to_document_embedding_with_id(
    record: &Record,
    document_id: &str,
) -> Result<DocumentEmbedding> {
    // Use the provided document_id instead of extracting from record
    let document_id = document_id.to_string();

    // Extract chunk_id from the database field (not the record ID)
    // This is the logical chunk_id provided by the application
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

/// Calculate mock similarity score for testing
#[allow(dead_code)]
fn calculate_mock_similarity(query: &str, content: &str) -> f64 {
    let query_lower = query.to_lowercase();
    let content_lower = content.to_lowercase();

    // Simple word matching score
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    let content_words: Vec<&str> = content_lower.split_whitespace().collect();

    if query_words.is_empty() {
        return 0.0;
    }

    let mut matches = 0;
    for query_word in &query_words {
        if content_words.contains(query_word) {
            matches += 1;
        }
    }

    let base_score = matches as f64 / query_words.len() as f64;

    // Add some randomness to make it more realistic for testing
    let random_factor = 0.1 + (query.len() % 100) as f64 / 1000.0;

    (base_score + random_factor).min(1.0)
}

/// Generate mock semantic search results for testing
#[allow(dead_code)]
fn generate_mock_semantic_results(query: &str, _limit: usize) -> Vec<(String, f64)> {
    let _query_lower = query.to_lowercase();
    let mut results = Vec::new();

    // Mock documents that should be returned based on query content
    let mock_docs = vec![
        (
            "rust-doc",
            "Rust programming language systems programming memory safety",
        ),
        (
            "ai-doc",
            "Artificial intelligence machine learning neural networks",
        ),
        ("db-doc", "Database systems SQL NoSQL vector embeddings"),
        (
            "web-doc",
            "Web development HTML CSS JavaScript frontend backend",
        ),
        (
            "devops-doc",
            "DevOps CI/CD Docker Kubernetes deployment automation",
        ),
    ];

    for (doc_id, content) in mock_docs {
        let score = calculate_mock_similarity(query, content);
        if score > 0.1 {
            // Only include documents with some relevance
            results.push((format!("/notes/{}.md", doc_id), score));
        }
    }

    // If still no results, add a generic result
    if results.is_empty() {
        results.push(("/notes/welcome.md".to_string(), 0.5));
    }

    results
}

// =============================================================================
// SCHEMA MIGRATION FUNCTIONS
// =============================================================================

// DEPRECATED: document_id field has been removed from schema.
// Record IDs are now generated directly from relative paths (e.g., notes:Projects_file_md)
// See: Phase 2 of graph relations refactor
//
// The following migration functions have been removed:
// - migrate_add_document_id_field()
// - check_document_id_migration_needed()
//
// No migration is needed for new deployments. Existing databases can be recreated
// or continue to work (the document_id field will simply be ignored if present).

// Re-export logging macros for convenience

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;
    use crucible_core::parser::Wikilink;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_store_embedding_with_graph_relations() {
        // Create an in-memory client for testing (using explicit :memory: path)
        use crate::types::SurrealDbConfig;
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        // Initialize kiln schema (skip if tables already exist)
        let _ = initialize_kiln_schema(&client).await;

        // Create a test note
        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/Projects/test_file.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "Test content for embedding".to_string();
        doc.content_hash = "test_hash_123".to_string();

        // Store the note
        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        println!("Stored note with ID: {}", note_id);

        // Create a test embedding vector
        let vector: Vec<f32> = (0..768).map(|i| i as f32 / 768.0).collect();

        // Store embedding with graph relation
        let chunk_id = store_embedding(
            &client,
            &note_id,
            vector.clone(),
            "test-model",
            1000,
            0,
            None,
            None,
        )
        .await
        .unwrap();

        println!("Stored embedding with ID: {}", chunk_id);

        // Verify the chunk ID format
        assert!(chunk_id.starts_with("embeddings:"));
        assert!(chunk_id.contains("_chunk_0"));

        // Query embeddings using graph traversal
        // SurrealDB graph traversal: we need to get the out node of the edge
        // The ->has_embedding syntax returns the edge, we need to get 'out' to get the embedding
        let traversal_sql = format!(
            "SELECT out FROM entities:⟨{}⟩->has_embedding",
            note_id.strip_prefix("entities:").unwrap_or(&note_id)
        );
        println!("Executing traversal query: {}", traversal_sql);

        let result = client.query(&traversal_sql, &[]).await.unwrap();

        println!("Graph traversal returned {} records", result.records.len());

        // Verify we got the embedding back
        assert_eq!(
            result.records.len(),
            1,
            "Should retrieve one embedding via graph traversal"
        );

        // The result contains 'out' field pointing to the embedding record ID
        let embedding_record = &result.records[0];
        assert!(
            embedding_record.data.contains_key("out"),
            "Should have 'out' field with embedding ID"
        );

        println!("✓ Graph relations test passed!");
    }

    #[tokio::test]
    async fn test_multiple_chunks_with_graph_relations() {
        // Create an in-memory client for testing (using explicit :memory: path)
        use crate::types::SurrealDbConfig;
        let config = SurrealDbConfig {
            path: ":memory:".to_string(),
            ..Default::default()
        };
        let client = SurrealClient::new(config).await.unwrap();

        // Initialize kiln schema (skip if tables already exist)
        let _ = initialize_kiln_schema(&client).await;

        // Create a test note
        let kiln_root = PathBuf::from("/test/kiln");
        let doc_path = PathBuf::from("/test/kiln/Projects/large_file.md");
        let mut doc = ParsedNote::new(doc_path.clone());
        doc.content.plain_text = "Large test content".to_string();
        doc.content_hash = "test_hash_456".to_string();

        // Store the note
        let note_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        // Store multiple embedding chunks
        let vector: Vec<f32> = (0..768).map(|i| i as f32 / 768.0).collect();

        for chunk_pos in 0..3 {
            let chunk_id = store_embedding(
                &client,
                &note_id,
                vector.clone(),
                "test-model",
                1000,
                chunk_pos,
                None,
                None,
            )
            .await
            .unwrap();

            println!("Stored chunk {}: {}", chunk_pos, chunk_id);
        }

        // Query all embeddings using graph traversal
        // SurrealDB graph traversal: we need to get the out node of the edge
        let traversal_sql = format!(
            "SELECT out FROM entities:⟨{}⟩->has_embedding",
            note_id.strip_prefix("entities:").unwrap_or(&note_id)
        );
        let result = client.query(&traversal_sql, &[]).await.unwrap();

        println!(
            "Retrieved {} embeddings via graph traversal",
            result.records.len()
        );

        // Verify we got all 3 chunks
        assert_eq!(
            result.records.len(),
            3,
            "Should retrieve all three embedding chunks"
        );

        println!("✓ Multiple chunks graph relations test passed!");
    }

    #[tokio::test]
    async fn tag_associations_create_hierarchy() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();

        let kiln_root = PathBuf::from("/vault");
        let mut doc = ParsedNote::new(kiln_root.join("projects/sample.md"));
        doc.content_hash = "tag-hash-1".into();
        doc.tags.push(Tag::new("project/crucible", 0));
        doc.tags.push(Tag::new("design/ui", 0));

        let doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        // Tags are now automatically stored during note ingestion

        let tags = client.query("SELECT * FROM tags", &[]).await.unwrap();
        assert_eq!(tags.records.len(), 4);

        let entity_tags = client
            .query("SELECT * FROM entity_tags", &[])
            .await
            .unwrap();
        assert_eq!(entity_tags.records.len(), 2);

        let docs = get_documents_by_tag(&client, "project/crucible")
            .await
            .unwrap();
        assert_eq!(docs.len(), 1);
    }
    #[tokio::test]
    async fn wikilink_edges_create_relations_and_placeholders() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let kiln_root = PathBuf::from("/vault");

        let mut doc = ParsedNote::new(kiln_root.join("projects/source.md"));
        doc.content_hash = "wikihash".into();
        doc.content.plain_text = "Scenario with wikilinks".into();
        doc.wikilinks.push(Wikilink::new("TargetNote", 5));
        doc.wikilinks.push(Wikilink::new("../Shared/OtherDoc", 15));

        let mut target_doc = ParsedNote::new(kiln_root.join("projects/TargetNote.md"));
        target_doc.content_hash = "targethash".into();
        target_doc.content.plain_text = "Target note".into();
        store_parsed_document(&client, &target_doc, &kiln_root)
            .await
            .unwrap();

        let doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        create_wikilink_edges(&client, &doc_id, &doc, &kiln_root)
            .await
            .unwrap();

        let relations = client
            .query(
                "SELECT relation_type, out, in FROM relations ORDER BY relation_type",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(relations.records.len(), 4);

        let targets: Vec<String> = relations
            .records
            .iter()
            .filter_map(|record| record.data.get("out").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        assert!(targets.iter().any(|t| t.contains("projects/TargetNote.md")));
        assert!(targets.iter().any(|t| t.contains("Shared/OtherDoc.md")));

        let relation_types: Vec<String> = relations
            .records
            .iter()
            .filter_map(|record| record.data.get("relation_type").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        assert!(relation_types.iter().any(|t| t == "wikilink"));
        assert!(relation_types.iter().any(|t| t == "wikilink_backlink"));

        let linked = get_linked_documents(&client, &doc_id).await.unwrap();
        assert_eq!(linked.len(), 2);

        let relation_list = get_wikilink_relations(&client, &doc_id).await.unwrap();
        assert_eq!(relation_list.len(), 2);

        let placeholder = client
            .query(
                "SELECT data FROM type::thing('entities', 'note:Shared/OtherDoc.md')",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(placeholder.records.len(), 1);
    }

    #[tokio::test]
    async fn embed_relationships_create_relations_and_backlinks() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let kiln_root = PathBuf::from("/vault");

        let mut doc = ParsedNote::new(kiln_root.join("media/source.md"));
        doc.content_hash = "embedhash".into();
        doc.content.plain_text = "Doc with embeds".into();
        doc.wikilinks.push(Wikilink::embed("Assets/Diagram", 3));

        let mut target_doc = ParsedNote::new(kiln_root.join("media/Assets/Diagram.md"));
        target_doc.content_hash = "diagramhash".into();
        target_doc.content.plain_text = "Diagram content".into();
        store_parsed_document(&client, &target_doc, &kiln_root)
            .await
            .unwrap();

        let doc_id = store_parsed_document(&client, &doc, &kiln_root)
            .await
            .unwrap();

        create_embed_relationships(&client, &doc_id, &doc, &kiln_root)
            .await
            .unwrap();

        let relations = client
            .query("SELECT relation_type, out, in FROM relations", &[])
            .await
            .unwrap();
        assert_eq!(relations.records.len(), 2);
        let mut has_forward = false;
        let mut has_backlink = false;
        for record in &relations.records {
            match record.data.get("relation_type").and_then(|v| v.as_str()) {
                Some("embed") => {
                    has_forward = true;
                    assert!(record
                        .data
                        .get("out")
                        .and_then(record_ref_to_string)
                        .map(|s| s.contains("Assets/Diagram.md"))
                        .unwrap_or(false));
                }
                Some("embed_backlink") => {
                    has_backlink = true;
                    assert!(record
                        .data
                        .get("out")
                        .and_then(record_ref_to_string)
                        .map(|s| s.contains("media/source.md"))
                        .unwrap_or(false));
                }
                _ => {}
            }
        }
        assert!(has_forward);
        assert!(has_backlink);
        let embed_target_ids: Vec<String> = relations
            .records
            .iter()
            .filter(|record| {
                record.data.get("relation_type").and_then(|v| v.as_str()) == Some("embed")
            })
            .filter_map(|record| record.data.get("out").and_then(record_ref_to_string))
            .collect();
        assert!(embed_target_ids
            .iter()
            .any(|id| id.contains("Assets/Diagram.md")));

        let embed_relations = get_embed_relations(&client, &doc_id).await.unwrap();
        assert_eq!(embed_relations.len(), 1);

        let entity = super::find_entity_id_by_title(&client, "Diagram")
            .await
            .unwrap()
            .expect("entity for Diagram should exist");
        assert!(entity.id.contains("Assets/Diagram"));

        let embed_pairs = super::fetch_embed_relation_pairs(&client).await.unwrap();
        assert_eq!(embed_pairs.len(), 1);
        assert!(
            embed_pairs[0].1.contains("Assets/Diagram.md"),
            "pair target {}",
            embed_pairs[0].1
        );
        assert_eq!(
            super::record_body(&embed_pairs[0].1),
            entity.id,
            "normalized target {} expected {}",
            super::record_body(&embed_pairs[0].1),
            entity.id
        );

        let backlink_sources = super::query_embedding_sources_for_entity(&client, &entity)
            .await
            .unwrap();
        assert_eq!(backlink_sources.len(), 1);

        let filtered_docs =
            get_embedded_documents_by_type(&client, &doc_id, &embed_relations[0].embed_type)
                .await
                .unwrap();
        assert_eq!(filtered_docs.len(), 1);

        let embedded_docs = get_embedded_documents(&client, &doc_id).await.unwrap();
        assert_eq!(embedded_docs.len(), 1);

        let metadata = get_embed_metadata(&client, &doc_id).await.unwrap();
        assert_eq!(metadata.len(), 1);

        let embedding_docs = get_embedding_documents(&client, "Diagram").await.unwrap();
        assert_eq!(embedding_docs.len(), 1);
    }
}

/// Generate a note ID from path and kiln root
pub fn generate_document_id(
    document_path: &std::path::Path,
    kiln_root: &std::path::Path,
) -> String {
    let relative = resolve_relative_path(document_path, kiln_root);
    let normalized = relative
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .replace('\\', "/")
        .replace(':', "_");
    format!("entities:note:{}", normalized)
}
