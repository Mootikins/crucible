use anyhow::Result;
use blake3::Hasher;
use crucible_core::parser::types::ParsedDocument;
use serde_json::{Map, Value};

use super::store::EAVGraphStore;
use super::types::{
    BlockNode, Entity, EntityRecord, EntityType, Property, PropertyRecord, PropertyValue, RecordId,
};

/// High-level helper for writing parsed documents into the EAV+Graph schema.
pub struct DocumentIngestor<'a> {
    store: &'a EAVGraphStore,
}

impl<'a> DocumentIngestor<'a> {
    pub fn new(store: &'a EAVGraphStore) -> Self {
        Self { store }
    }

    pub async fn ingest(
        &self,
        doc: &ParsedDocument,
        relative_path: &str,
    ) -> Result<RecordId<EntityRecord>> {
        let entity_id = note_entity_id(relative_path);

        let mut entity = Entity::new(entity_id.clone(), EntityType::Note)
            .with_content_hash(doc.content_hash.clone())
            .with_search_text(doc.content.plain_text.clone());
        entity.data = Some(entity_payload(doc, relative_path));

        self.store.upsert_entity(&entity).await?;

        for property in core_properties(&entity_id, doc, relative_path) {
            self.store.upsert_property(&property).await?;
        }

        let blocks = build_blocks(&entity_id, doc);
        self.store.replace_blocks(&entity_id, &blocks).await?;

        Ok(entity_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;
    use crucible_core::parser::types::{
        DocumentContent, Frontmatter, FrontmatterFormat, Heading, Paragraph, Tag,
    };
    use serde_json::json;
    use std::path::PathBuf;

    fn sample_document() -> ParsedDocument {
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("notes/sample.md");
        doc.content_hash = "abc123".into();
        doc.content = DocumentContent::default();
        doc.content.plain_text = "Hello world".into();
        doc.content
            .paragraphs
            .push(Paragraph::new("Hello world".into(), 0));
        doc.content.headings.push(Heading::new(1, "Intro", 0));
        doc.tags.push(Tag::new("project/crucible", 0));
        doc.frontmatter = Some(Frontmatter::new(
            "title: Sample Doc".to_string(),
            FrontmatterFormat::Yaml,
        ));
        doc
    }

    #[tokio::test]
    async fn ingest_document_writes_entities_properties_blocks() {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let doc = sample_document();
        let entity_id = ingestor.ingest(&doc, "notes/sample.md").await.unwrap();

        let result = client
            .query(
                "SELECT * FROM entities WHERE id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert_eq!(result.records.len(), 1);

        let blocks = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', $id)",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();
        assert!(!blocks.records.is_empty());
    }
}

fn note_entity_id(relative_path: &str) -> RecordId<EntityRecord> {
    let normalized = relative_path
        .trim_start_matches(std::path::MAIN_SEPARATOR)
        .replace('\\', "/")
        .replace(':', "_");
    RecordId::new("entities", format!("note:{}", normalized))
}

fn core_properties(
    entity_id: &RecordId<EntityRecord>,
    doc: &ParsedDocument,
    relative_path: &str,
) -> Vec<Property> {
    let mut props = Vec::new();

    props.push(Property::new(
        property_id(entity_id, "core", "path"),
        entity_id.clone(),
        "core",
        "path",
        PropertyValue::Text(doc.path.to_string_lossy().to_string()),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "relative_path"),
        entity_id.clone(),
        "core",
        "relative_path",
        PropertyValue::Text(relative_path.to_string()),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "title"),
        entity_id.clone(),
        "core",
        "title",
        PropertyValue::Text(doc.title()),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "tags"),
        entity_id.clone(),
        "core",
        "tags",
        PropertyValue::Json(Value::Array(
            doc.all_tags()
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>(),
        )),
    ));

    if let Some(frontmatter) = &doc.frontmatter {
        let fm_value = Value::Object(
            frontmatter
                .properties()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<serde_json::Map<_, _>>(),
        );
        props.push(Property::new(
            property_id(entity_id, "core", "frontmatter"),
            entity_id.clone(),
            "core",
            "frontmatter",
            PropertyValue::Json(fm_value),
        ));
    }

    props
}

fn entity_payload(doc: &ParsedDocument, relative_path: &str) -> Value {
    let tags = doc
        .all_tags()
        .into_iter()
        .map(Value::String)
        .collect::<Vec<_>>();

    let frontmatter_value = doc.frontmatter.as_ref().map(|fm| {
        let map = fm
            .properties()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Map<_, _>>();
        Value::Object(map)
    });

    let mut payload = Map::new();
    payload.insert(
        "path".to_string(),
        Value::String(doc.path.to_string_lossy().into_owned()),
    );
    payload.insert(
        "relative_path".to_string(),
        Value::String(relative_path.to_string()),
    );
    payload.insert("title".to_string(), Value::String(doc.title()));
    payload.insert(
        "content".to_string(),
        Value::String(doc.content.plain_text.clone()),
    );
    payload.insert("tags".to_string(), Value::Array(tags));
    if let Some(frontmatter) = frontmatter_value {
        payload.insert("frontmatter".to_string(), frontmatter);
    }
    payload.insert(
        "parsed_at".to_string(),
        Value::String(doc.parsed_at.to_rfc3339()),
    );
    payload.insert(
        "file_size".to_string(),
        Value::Number(serde_json::Number::from(doc.file_size)),
    );
    payload.insert(
        "content_hash".to_string(),
        Value::String(doc.content_hash.clone()),
    );
    payload.insert(
        "wikilink_count".to_string(),
        Value::Number(serde_json::Number::from(doc.wikilinks.len() as u64)),
    );
    payload.insert(
        "created_via".to_string(),
        Value::String("parser".to_string()),
    );

    Value::Object(payload)
}

fn property_id(
    entity_id: &RecordId<EntityRecord>,
    namespace: &str,
    key: &str,
) -> RecordId<PropertyRecord> {
    RecordId::new(
        "properties",
        format!("{}:{}:{}", entity_id.id, namespace, key),
    )
}

fn build_blocks(entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<BlockNode> {
    let mut blocks = Vec::new();
    let mut index = 0;

    // Headings with metadata (level + text)
    for heading in &doc.content.headings {
        let metadata = serde_json::json!({
            "level": heading.level,
            "text": heading.text.clone()
        });
        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("h{}", index),
            index,
            "heading",
            &heading.text,
            metadata,
        ));
        index += 1;
    }

    // Paragraphs (non-empty only)
    for paragraph in &doc.content.paragraphs {
        if paragraph.content.trim().is_empty() {
            continue;
        }
        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("p{}", index),
            index,
            "paragraph",
            &paragraph.content,
            serde_json::json!({}),
        ));
        index += 1;
    }

    // Code blocks with language + line count metadata
    for code_block in &doc.content.code_blocks {
        let metadata = serde_json::json!({
            "language": code_block.language.clone().unwrap_or_default(),
            "line_count": code_block.content.lines().count()
        });
        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("code{}", index),
            index,
            "code",
            &code_block.content,
            metadata,
        ));
        index += 1;
    }

    // Lists with type + item count metadata
    for list in &doc.content.lists {
        let metadata = serde_json::json!({
            "type": match list.list_type {
                crucible_core::parser::types::ListType::Ordered => "ordered",
                crucible_core::parser::types::ListType::Unordered => "unordered",
            },
            "item_count": list.items.len()
        });

        // Serialize list as text (simple approach for now)
        let list_text = list.items.iter()
            .map(|item| {
                if let Some(task_status) = &item.task_status {
                    let check = match task_status {
                        crucible_core::parser::types::TaskStatus::Pending => " ",
                        crucible_core::parser::types::TaskStatus::Completed => "x",
                    };
                    format!("- [{}] {}", check, item.content)
                } else {
                    format!("- {}", item.content)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("list{}", index),
            index,
            "list",
            &list_text,
            metadata,
        ));
        index += 1;
    }

    // Callouts with type + title metadata
    for callout in &doc.callouts {
        let metadata = serde_json::json!({
            "callout_type": callout.callout_type.clone(),
            "title": callout.title.clone().unwrap_or_default()
        });
        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("callout{}", index),
            index,
            "callout",
            &callout.content,
            metadata,
        ));
        index += 1;
    }

    blocks
}

fn make_block_with_metadata(
    entity_id: &RecordId<EntityRecord>,
    suffix: &str,
    block_index: i32,
    block_type: &str,
    content: &str,
    metadata: Value,
) -> BlockNode {
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize().to_hex().to_string();

    let mut block = BlockNode::new(
        RecordId::new("blocks", format!("{}:{}", entity_id.id, suffix)),
        entity_id.clone(),
        block_index,
        block_type,
        content,
        hash,
    );
    block.metadata = metadata;
    block
}
