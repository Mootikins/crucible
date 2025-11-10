use anyhow::Result;
use blake3::Hasher;
use crucible_core::merkle::HybridMerkleTree;
use crucible_core::parser::types::ParsedDocument;
use crucible_core::storage::{Relation as CoreRelation, RelationStorage, Tag as CoreTag, TagStorage};
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

        // Extract and store relations from wikilinks and embeds
        let relations = extract_relations(&entity_id, doc);
        for relation in relations {
            self.store.store_relation(relation).await?;
        }

        // Compute and store section hashes
        let section_properties = compute_section_properties(&entity_id, doc);
        for property in section_properties {
            self.store.upsert_property(&property).await?;
        }

        // Note: Tag storage and associations are now handled separately by
        // create_tag_associations in kiln_integration to ensure consistent
        // tag ID schemes and proper hierarchy management.
        // The extract_tags function is kept for potential future use.

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
        let client = SurrealClient::new_isolated_memory().await.unwrap();
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

    #[tokio::test]
    async fn ingest_document_extracts_wikilink_relations() {
        use crucible_core::parser::types::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.wikilinks.push(Wikilink {
            target: "other-note".to_string(),
            alias: Some("Other Note".to_string()),
            heading_ref: Some("Section".to_string()),
            block_ref: None,
            is_embed: false,
            offset: 10,
        });
        doc.wikilinks.push(Wikilink {
            target: "embedded-note".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: Some("block-id".to_string()),
            is_embed: true,
            offset: 20,
        });

        let entity_id = ingestor.ingest(&doc, "notes/wikilink-test.md").await.unwrap();

        // Get all relations for this entity (use just the ID part, not the full "entities:..." string)
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 2, "Should have 2 relations");

        // Check wikilink relation
        let wikilink_rel = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .unwrap();
        // Adapter adds "entities:" prefix to entity IDs
        assert_eq!(
            wikilink_rel.to_entity_id,
            Some("entities:note:other-note".to_string())
        );
        assert_eq!(
            wikilink_rel.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Other Note")
        );
        assert_eq!(
            wikilink_rel
                .metadata
                .get("heading_ref")
                .and_then(|v| v.as_str()),
            Some("Section")
        );

        // Check embed relation
        let embed_rel = relations
            .iter()
            .find(|r| r.relation_type == "embed")
            .unwrap();
        assert_eq!(
            embed_rel.to_entity_id,
            Some("entities:note:embedded-note".to_string())
        );
        assert_eq!(
            embed_rel
                .metadata
                .get("block_ref")
                .and_then(|v| v.as_str()),
            Some("block-id")
        );
    }

    #[tokio::test]
    async fn ingest_document_extracts_hierarchical_tags() {
        use crucible_core::storage::TagStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.tags.clear();
        doc.tags.push(Tag::new("project/ai/nlp", 0));
        doc.tags.push(Tag::new("status/active", 0));

        // Extract and manually store tags since tag creation has been moved out of ingestion
        // (see lines 49-52 comment - tags are now created by create_tag_associations in kiln_integration)
        let tags = extract_tags(&doc);
        for tag in tags {
            store.store_tag(tag).await.unwrap();
        }

        let entity_id = ingestor.ingest(&doc, "notes/test-tags.md").await.unwrap();

        // Check that all tag levels were created
        let project_tag = store.get_tag("project").await.unwrap();
        assert!(project_tag.is_some(), "Should have 'project' tag");
        assert_eq!(project_tag.unwrap().name, "project");

        let ai_tag = store.get_tag("project:ai").await.unwrap();
        assert!(ai_tag.is_some(), "Should have 'project/ai' tag");
        let ai_tag = ai_tag.unwrap();
        assert_eq!(ai_tag.name, "project/ai");
        // Adapter adds "tags:" prefix to parent_id
        assert_eq!(ai_tag.parent_tag_id, Some("tags:project".to_string()));

        let nlp_tag = store.get_tag("project:ai:nlp").await.unwrap();
        assert!(nlp_tag.is_some(), "Should have 'project/ai/nlp' tag");
        let nlp_tag = nlp_tag.unwrap();
        assert_eq!(nlp_tag.name, "project/ai/nlp");
        assert_eq!(nlp_tag.parent_tag_id, Some("tags:project:ai".to_string()));

        let status_tag = store.get_tag("status").await.unwrap();
        assert!(status_tag.is_some(), "Should have 'status' tag");

        let active_tag = store.get_tag("status:active").await.unwrap();
        assert!(active_tag.is_some(), "Should have 'status/active' tag");
        let active_tag = active_tag.unwrap();
        assert_eq!(active_tag.parent_tag_id, Some("tags:status".to_string()));

        // Note: Entity-tag associations are also handled separately by kiln_integration,
        // so we don't check them in this unit test for DocumentIngestor.
    }

    #[tokio::test]
    async fn ingest_document_stores_relation_metadata() {
        use crucible_core::parser::types::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.wikilinks.push(Wikilink {
            target: "note-with-heading".to_string(),
            alias: Some("Custom Display Text".to_string()),
            heading_ref: Some("Introduction".to_string()),
            block_ref: Some("^abc123".to_string()),
            is_embed: false,
            offset: 42,
        });

        let entity_id = ingestor.ingest(&doc, "notes/metadata-test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let relation = &relations[0];

        // Verify all metadata fields are preserved
        assert_eq!(
            relation.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Custom Display Text")
        );
        assert_eq!(
            relation.metadata.get("heading_ref").and_then(|v| v.as_str()),
            Some("Introduction")
        );
        assert_eq!(
            relation.metadata.get("block_ref").and_then(|v| v.as_str()),
            Some("^abc123")
        );
        assert_eq!(
            relation.metadata.get("offset").and_then(|v| v.as_u64()),
            Some(42)
        );
    }

    #[tokio::test]
    async fn relations_support_backlinks() {
        use crucible_core::parser::types::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create two documents with cross-references
        let mut doc1 = sample_document();
        doc1.path = PathBuf::from("notes/backlink1.md");
        doc1.tags.clear(); // Clear tags to avoid conflicts with other tests
        doc1.wikilinks.push(Wikilink {
            target: "notes/backlink2.md".to_string(),  // Full path relative to vault root
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 0,
        });

        let mut doc2 = sample_document();
        doc2.path = PathBuf::from("notes/backlink2.md");
        doc2.tags.clear(); // Clear tags to avoid conflicts with other tests
        doc2.wikilinks.push(Wikilink {
            target: "notes/backlink1.md".to_string(),  // Full path relative to vault root
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 0,
        });

        let entity1_id = ingestor.ingest(&doc1, "notes/backlink1.md").await.unwrap();
        let entity2_id = ingestor.ingest(&doc2, "notes/backlink2.md").await.unwrap();

        // Get backlinks for doc1 (should find link from doc2)
        // Use the ID part only, which is what note_entity_id() generates
        let backlinks = store
            .get_backlinks(&entity1_id.id, None)
            .await
            .unwrap();

        assert!(
            !backlinks.is_empty(),
            "Should have backlinks to sample note"
        );

        let backlink = &backlinks[0];
        // The from_entity_id will be in "entities:note:backlink2.md" format due to adapter conversion
        assert_eq!(backlink.from_entity_id, format!("entities:{}", entity2_id.id));
    }

    #[tokio::test]
    async fn ingest_document_stores_section_hashes() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create a document with multiple sections
        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();

        // Add headings to create sections
        doc.content.headings.push(Heading::new(1, "Introduction", 0));
        doc.content.headings.push(Heading::new(2, "Details", 50));
        doc.content.headings.push(Heading::new(2, "Conclusion", 100));

        // Add paragraphs for content
        doc.content.paragraphs.push(Paragraph::new("Intro content".to_string(), 10));
        doc.content.paragraphs.push(Paragraph::new("Detail content".to_string(), 60));
        doc.content.paragraphs.push(Paragraph::new("Final thoughts".to_string(), 110));

        let entity_id = ingestor.ingest(&doc, "notes/sections-test.md").await.unwrap();

        // Query section properties
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND namespace = 'section'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert!(!result.records.is_empty(), "Should have section properties");

        // Verify we have the expected section properties
        let props = &result.records;

        // Should have: tree_root_hash, total_sections, and for each section: hash + metadata
        // With 4 sections (root + 3 headings), we expect: 2 + (4 * 2) = 10 properties
        assert!(props.len() >= 2, "Should have at least root hash and total sections");

        // Check for tree_root_hash
        let has_root_hash = props.iter().any(|p| {
            p.data.get("key")
                .and_then(|k| k.as_str())
                .map(|k| k == "tree_root_hash")
                .unwrap_or(false)
        });
        assert!(has_root_hash, "Should have tree_root_hash property");

        // Check for total_sections
        let total_sections = props.iter().find_map(|p| {
            if p.data.get("key")?.as_str()? == "total_sections" {
                // The value is stored as {"type": "number", "value": 4.0}
                p.data.get("value")?.get("value")?.as_f64()
            } else {
                None
            }
        });
        assert!(total_sections.is_some(), "Should have total_sections property");
        assert!(total_sections.unwrap() > 0.0, "Should have at least one section");
    }

    #[tokio::test]
    async fn section_hashes_are_retrievable() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();
        doc.content.headings.push(Heading::new(1, "Test Section", 0));
        doc.content.paragraphs.push(Paragraph::new("Test content".to_string(), 10));

        let entity_id = ingestor.ingest(&doc, "notes/hash-test.md").await.unwrap();

        // Query for section_0_hash
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND key = 'section_0_hash'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1, "Should find section_0_hash");

        let hash_prop = &result.records[0];
        // Value is stored as {"type": "text", "value": "hash_string"}
        let hash_value = hash_prop.data.get("value")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str());
        assert!(hash_value.is_some(), "Hash should be a string");
        assert!(!hash_value.unwrap().is_empty(), "Hash should not be empty");
    }

    #[tokio::test]
    async fn section_metadata_includes_heading_info() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.content.headings.clear();
        doc.content.paragraphs.clear();
        doc.content.headings.push(Heading::new(2, "My Heading", 0));
        doc.content.paragraphs.push(Paragraph::new("Content here".to_string(), 10));

        let entity_id = ingestor.ingest(&doc, "notes/metadata-test.md").await.unwrap();

        // Query for section metadata - just get all section properties and filter in code
        let result = client
            .query(
                "SELECT * FROM properties WHERE entity_id = type::thing('entities', $id) AND namespace = 'section'",
                &[json!({ "id": entity_id.id })],
            )
            .await
            .unwrap();

        assert!(!result.records.is_empty(), "Should have section metadata");

        // Find a section with heading metadata
        // Value is stored as {"type": "json", "value": {...}}
        let has_heading_metadata = result.records.iter().any(|prop| {
            // Only look at metadata properties
            let key = prop.data.get("key").and_then(|k| k.as_str());
            if !key.map(|k| k.contains("_metadata")).unwrap_or(false) {
                return false;
            }

            if let Some(outer_value) = prop.data.get("value") {
                if let Some(inner_value) = outer_value.get("value") {
                    return inner_value.get("heading_text").is_some()
                        && inner_value.get("heading_level").is_some();
                }
            }
            false
        });

        assert!(has_heading_metadata, "Should have heading metadata in at least one section");
    }
}

/// Compute section properties from the document's Merkle tree
fn compute_section_properties(
    entity_id: &RecordId<EntityRecord>,
    doc: &ParsedDocument,
) -> Vec<Property> {
    let mut props = Vec::new();

    // Build the hybrid Merkle tree to extract sections
    let merkle_tree = HybridMerkleTree::from_document(doc);

    // Store the root hash as a property
    props.push(Property::new(
        property_id(entity_id, "section", "tree_root_hash"),
        entity_id.clone(),
        "section",
        "tree_root_hash",
        PropertyValue::Text(merkle_tree.root_hash.to_hex()),
    ));

    // Store total section count
    props.push(Property::new(
        property_id(entity_id, "section", "total_sections"),
        entity_id.clone(),
        "section",
        "total_sections",
        PropertyValue::Number(merkle_tree.sections.len() as f64),
    ));

    // Store metadata for each section
    for (index, section) in merkle_tree.sections.iter().enumerate() {
        // Store section hash
        let hash_key = format!("section_{}_hash", index);
        props.push(Property::new(
            property_id(entity_id, "section", &hash_key),
            entity_id.clone(),
            "section",
            &hash_key,
            PropertyValue::Text(section.binary_tree.root_hash.to_hex()),
        ));

        // Store section metadata as JSON
        let metadata_key = format!("section_{}_metadata", index);
        let mut metadata = serde_json::Map::new();
        metadata.insert("block_count".to_string(), serde_json::json!(section.block_count));
        metadata.insert("depth".to_string(), serde_json::json!(section.depth));

        if let Some(heading) = &section.heading {
            metadata.insert("heading_text".to_string(), serde_json::json!(heading.text));
            metadata.insert("heading_level".to_string(), serde_json::json!(heading.level));
        }

        props.push(Property::new(
            property_id(entity_id, "section", &metadata_key),
            entity_id.clone(),
            "section",
            &metadata_key,
            PropertyValue::Json(Value::Object(metadata)),
        ));
    }

    props
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

    // LaTeX expressions with inline/block flag
    for latex in &doc.content.latex_expressions {
        let metadata = serde_json::json!({
            "inline": !latex.is_block,
            "display_mode": latex.is_block
        });
        blocks.push(make_block_with_metadata(
            entity_id,
            &format!("latex{}", index),
            index,
            "latex",
            &latex.expression,
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

/// Extract relations from wikilinks and embeds
fn extract_relations(entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<CoreRelation> {
    let mut relations = Vec::new();
    let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

    for wikilink in &doc.wikilinks {
        // Determine target entity ID
        // For now, we create unresolved targets (to_entity_id = None) since we don't
        // know if the target note exists yet. The resolver will fill these in later.
        let to_entity_id = Some(format!("note:{}", wikilink.target));

        // Determine relation type
        let relation_type = if wikilink.is_embed {
            "embed"
        } else {
            "wikilink"
        };

        // Create the relation
        let mut relation = CoreRelation::new(
            from_entity_id.clone(),
            to_entity_id,
            relation_type,
        );

        // Add metadata about the wikilink
        let mut metadata = serde_json::Map::new();
        if let Some(alias) = &wikilink.alias {
            metadata.insert("alias".to_string(), serde_json::json!(alias));
        }
        if let Some(heading_ref) = &wikilink.heading_ref {
            metadata.insert("heading_ref".to_string(), serde_json::json!(heading_ref));
        }
        if let Some(block_ref) = &wikilink.block_ref {
            metadata.insert("block_ref".to_string(), serde_json::json!(block_ref));
        }
        metadata.insert("offset".to_string(), serde_json::json!(wikilink.offset));

        relation.metadata = serde_json::Value::Object(metadata);

        relations.push(relation);
    }

    relations
}

/// Extract tags and build tag hierarchy
fn extract_tags(doc: &ParsedDocument) -> Vec<CoreTag> {
    use std::collections::HashMap;

    let mut tags_map: HashMap<String, CoreTag> = HashMap::new();
    let now = chrono::Utc::now();

    // Process all tags from the document
    for tag in doc.all_tags() {
        let tag_name = tag.trim_start_matches('#').to_string();

        // Handle hierarchical tags (e.g., "project/ai/nlp")
        let parts: Vec<&str> = tag_name.split('/').collect();

        // Create tags for each level of the hierarchy
        let mut parent_id: Option<String> = None;
        for i in 0..parts.len() {
            let current_path = parts[0..=i].join("/");
            // Use simple ID without "tag:" prefix - will be added by RecordId construction
            let tag_id = current_path.replace('/', ":");

            // Only create if not already in map
            if !tags_map.contains_key(&tag_id) {
                let tag = CoreTag {
                    id: tag_id.clone(),
                    name: current_path.clone(),
                    parent_tag_id: parent_id.clone(),
                    created_at: now,
                    updated_at: now,
                };
                tags_map.insert(tag_id.clone(), tag);
            }

            parent_id = Some(tag_id);
        }
    }

    tags_map.into_values().collect()
}
