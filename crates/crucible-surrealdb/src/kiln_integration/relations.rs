//! Relation management for wikilinks and embeds
//!
//! Functions for creating and querying wikilink and embed relationships.

use crate::eav_graph::{
    EAVGraphStore, EntityRecord as EAVGraphEntityRecord, RecordId as EAVGraphRecordId, Relation,
    RelationRecord,
};
use crate::SurrealClient;
use anyhow::{anyhow, Result};
use crucible_core::parser::Wikilink;
use crucible_core::types::ParsedNote;
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing::debug;

use super::document_storage::fetch_document_by_id;
use super::types::{EmbedMetadata, EmbedRelation, LinkRelation};
use super::utils::{
    clean_relative_path, normalize_document_id, record_ref_to_string, resolve_relative_path,
};

/// Parse entity record ID from document ID
pub(crate) fn parse_entity_record_id(
    doc_id: &str,
) -> Result<EAVGraphRecordId<EAVGraphEntityRecord>> {
    let normalized = normalize_document_id(doc_id);
    let (_, id) = normalized
        .split_once(':')
        .ok_or_else(|| anyhow!("invalid note id '{}'", doc_id))?;
    Ok(EAVGraphRecordId::new("entities", id))
}

/// Generate relation record ID
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

/// Resolve wikilink target to relative path
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

/// Create backlink relation
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

/// Determine the type of embed based on the wikilink properties
fn determine_embed_type(wikilink: &Wikilink) -> String {
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

/// Query relation documents helper
pub(crate) async fn query_relation_documents(
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

/// Get documents linked via wikilinks
pub async fn get_linked_documents(client: &SurrealClient, doc_id: &str) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "wikilink").await
}

/// Get documents embedded by a note
pub async fn get_embedded_documents(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "embed").await
}

/// Get documents by tag
pub async fn get_documents_by_tag(client: &SurrealClient, tag: &str) -> Result<Vec<ParsedNote>> {
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

fn embed_type_from_metadata(value: &serde_json::Value) -> String {
    value
        .as_object()
        .and_then(|obj| obj.get("embed_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("simple")
        .to_string()
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

/// Find entity ID by title
pub(crate) async fn find_entity_id_by_title(
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

/// Query embedding sources for entity
pub(crate) async fn query_embedding_sources_for_entity(
    client: &SurrealClient,
    entity_id: &EAVGraphRecordId<EAVGraphEntityRecord>,
) -> Result<Vec<ParsedNote>> {
    let pairs = fetch_embed_relation_pairs(client).await?;
    let target_key = entity_id.id.clone();
    let mut documents = Vec::new();

    for (source, target) in pairs {
        if super::utils::record_body(&target) == target_key {
            if let Some(doc) = fetch_document_by_id(client, &source).await? {
                documents.push(doc);
            }
        }
    }

    Ok(documents)
}

/// Query embedding sources by title
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

/// Fetch embed relation pairs
pub(crate) async fn fetch_embed_relation_pairs(
    client: &SurrealClient,
) -> Result<Vec<(String, String)>> {
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

/// Parse reference target to extract heading and block references
fn parse_reference_target(reference_target: Option<String>) -> (Option<String>, Option<String>) {
    if let Some(target) = reference_target {
        if target.starts_with("#^") {
            let block_ref = target.strip_prefix("#^").map(|s| s.to_string());
            (None, block_ref)
        } else if target.starts_with('#') {
            let heading_ref = target.strip_prefix('#').map(|s| s.to_string());
            (heading_ref, None)
        } else {
            (Some(target.clone()), None)
        }
    } else {
        (None, None)
    }
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

/// Get documents linked via wikilinks (separate from embeds)
pub async fn get_wikilinked_documents(
    client: &SurrealClient,
    doc_id: &str,
) -> Result<Vec<ParsedNote>> {
    query_relation_documents(client, doc_id, "wikilink").await
}
