use anyhow::Result;
use blake3::Hasher;
use crucible_core::parser::types::ParsedDocument;
use serde_json::Value;

use super::store::EprStore;
use super::types::{
    BlockNode, Entity, EntityRecord, EntityType, Property, PropertyRecord, PropertyValue,
    RecordId,
};

/// High-level helper for writing parsed documents into the EPR schema.
pub struct DocumentIngestor<'a> {
    store: &'a EprStore,
}

impl<'a> DocumentIngestor<'a> {
    pub fn new(store: &'a EprStore) -> Self {
        Self { store }
    }

    pub async fn ingest(
        &self,
        doc: &ParsedDocument,
        relative_path: &str,
    ) -> Result<RecordId<EntityRecord>> {
        let entity_id = note_entity_id(relative_path);

        let entity = Entity::new(entity_id.clone(), EntityType::Note)
            .with_content_hash(doc.content_hash.clone())
            .with_search_text(doc.content.plain_text.clone());

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
    use crate::epr::apply_epr_schema;
    use crate::SurrealClient;
    use crucible_core::parser::types::{DocumentContent, Frontmatter, FrontmatterFormat, Heading, Paragraph, Tag};
    use std::path::PathBuf;
    use serde_json::json;

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
        apply_epr_schema(&client).await.unwrap();
        let store = EprStore::new(client.clone());
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
        PropertyValue::text(doc.path.to_string_lossy()),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "relative_path"),
        entity_id.clone(),
        "core",
        "relative_path",
        PropertyValue::text(relative_path),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "title"),
        entity_id.clone(),
        "core",
        "title",
        PropertyValue::text(doc.title()),
    ));

    props.push(Property::new(
        property_id(entity_id, "core", "tags"),
        entity_id.clone(),
        "core",
        "tags",
        PropertyValue::json(Value::Array(
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
            PropertyValue::json(fm_value),
        ));
    }

    props
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

fn build_blocks(
    entity_id: &RecordId<EntityRecord>,
    doc: &ParsedDocument,
) -> Vec<BlockNode> {
    let mut blocks = Vec::new();
    let mut index = 0;

    for heading in &doc.content.headings {
        blocks.push(make_block(
            entity_id,
            &format!("h{}", index),
            index,
            "heading",
            &heading.text,
        ));
        index += 1;
    }

    for paragraph in &doc.content.paragraphs {
        if paragraph.content.trim().is_empty() {
            continue;
        }
        blocks.push(make_block(
            entity_id,
            &format!("p{}", index),
            index,
            "paragraph",
            &paragraph.content,
        ));
        index += 1;
    }

    blocks
}

fn make_block(
    entity_id: &RecordId<EntityRecord>,
    suffix: &str,
    block_index: i32,
    block_type: &str,
    content: &str,
) -> BlockNode {
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize().to_hex().to_string();

    BlockNode::new(
        RecordId::new("blocks", format!("{}:{}", entity_id.id, suffix)),
        entity_id.clone(),
        block_index,
        block_type,
        content,
        hash,
    )
}
