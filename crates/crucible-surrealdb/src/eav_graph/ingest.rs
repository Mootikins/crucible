use anyhow::Result;
use blake3::Hasher;
use crucible_core::merkle::HybridMerkleTree;
use crucible_core::parser::ParsedDocument;
use crucible_core::storage::{Relation as CoreRelation, RelationStorage};
use serde_json::{Map, Value};

use super::store::EAVGraphStore;
use super::types::{
    BlockNode, Entity, EntityRecord, EntityTag, EntityTagRecord, EntityType, Property,
    PropertyRecord, PropertyValue, RecordId, TagRecord,
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
        let entity_id = self.note_entity_id(relative_path);

        let mut entity = Entity::new(entity_id.clone(), EntityType::Note)
            .with_content_hash(doc.content_hash.clone())
            .with_search_text(doc.content.plain_text.clone());
        entity.data = Some(self.entity_payload(doc, relative_path));

        self.store.upsert_entity(&entity).await?;

        for property in self.core_properties(&entity_id, doc, relative_path) {
            self.store.upsert_property(&property).await?;
        }

        let blocks = self.build_blocks(&entity_id, doc);
        self.store.replace_blocks(&entity_id, &blocks).await?;

        // Extract and store relations from wikilinks and embeds with resolution
        let relations = self.extract_relations_with_resolution(&entity_id, doc).await?;
        for relation in relations {
            self.store.store_relation(relation).await?;
        }

        // Extract and store inline link relations
        let inline_link_relations = self.extract_inline_link_relations(&entity_id, doc);
        for relation in inline_link_relations {
            self.store.store_relation(relation).await?;
        }

        // Extract and store footnote relations
        let footnote_relations = self.extract_footnote_relations(&entity_id, doc);
        for relation in footnote_relations {
            self.store.store_relation(relation).await?;
        }

        // Compute and store section hashes
        let section_properties = self.compute_section_properties(&entity_id, doc);
        for property in section_properties {
            self.store.upsert_property(&property).await?;
        }

        // Store tags and create tag associations
        self.store_tags(doc, &entity_id).await?;

        Ok(entity_id)
    }

    /// Resolve a wikilink target to entity IDs by searching the vault
    ///
    /// Searches for files matching the wikilink target. Returns:
    /// - Ok((Some(entity_id), vec![])) - Exactly one match found
    /// - Ok((None, candidates)) - Multiple ambiguous matches (stores candidates)
    /// - Ok((None, vec![])) - No matches found
    ///
    /// For ambiguous links, candidates are returned separately for metadata storage.
    async fn resolve_wikilink_target(
        &self,
        target: &str,
        _heading_ref: Option<&str>,
        _block_ref: Option<&str>,
    ) -> Result<(Option<RecordId<EntityRecord>>, Vec<String>)> {
        // Query database for all note entities and filter in Rust
        // SurrealDB returns the id in the record's id field, not in the data
        let query = "SELECT * FROM entities WHERE type = 'note' LIMIT 100";

        let result = self.store.client.query(query, &[]).await
            .map_err(|e| anyhow::anyhow!("Failed to query entities: {}", e))?;

        // Extract candidate paths from results - filter by target match
        let target_lower = target.to_lowercase();
        let mut candidates: Vec<(String, String)> = Vec::new();

        for record in &result.records {
            // Extract the relative_path from data field
            if let Some(data_obj) = record.data.get("data").and_then(|v| v.as_object()) {
                if let Some(path) = data_obj.get("relative_path").and_then(|v| v.as_str()) {
                    // Check if path contains or ends with target (case-insensitive)
                    let path_lower = path.to_lowercase();
                    if path_lower.contains(&target_lower) || path_lower.ends_with(&target_lower) {
                        // The ID is in the record's id field from SurrealDB
                        // For our query, we can derive it from the path since we know the pattern
                        // The entity ID format is "entities:note:path"
                        let entity_id_str = format!("note:{}", path.replace('\\', "/").replace(':', "_"));

                        candidates.push((path.to_string(), entity_id_str));
                    }
                }
            }
        }

        match candidates.len() {
            0 => {
                // No matches - emit warning
                tracing::warn!(
                    "Unresolved wikilink '{}' - no matching files found",
                    target
                );
                Ok((None, vec![]))
            }
            1 => {
                // Exact match - return entity ID
                let (path, id_str) = &candidates[0];
                tracing::debug!("Resolved wikilink '{}' to entity '{}'", target, path);
                let entity_id = RecordId::new("entities", id_str.clone());
                Ok((Some(entity_id), vec![]))
            }
            _ => {
                // Ambiguous - return candidates for metadata
                let candidate_paths: Vec<String> = candidates.iter()
                    .map(|(path, _)| path.clone())
                    .collect();

                tracing::warn!(
                    "Ambiguous wikilink '{}' - found {} candidates: {}",
                    target,
                    candidates.len(),
                    candidate_paths.join(", ")
                );
                Ok((None, candidate_paths))
            }
        }
    }

    /// Extract relations from wikilinks with target resolution
    async fn extract_relations_with_resolution(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedDocument,
    ) -> Result<Vec<CoreRelation>> {
        let mut relations = Vec::with_capacity(doc.wikilinks.len());
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for wikilink in &doc.wikilinks {
            // Resolve the target
            let (resolved_target, candidates) = self.resolve_wikilink_target(
                &wikilink.target,
                wikilink.heading_ref.as_deref(),
                wikilink.block_ref.as_deref(),
            ).await?;

            let relation_type = if wikilink.is_embed { "embed" } else { "wikilink" };

            let mut relation = CoreRelation::new(
                from_entity_id.clone(),
                resolved_target.map(|id| format!("{}:{}", id.table, id.id)),
                relation_type,
            );

            // Store metadata
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

            // Store candidates if ambiguous
            if !candidates.is_empty() {
                metadata.insert("candidates".to_string(), serde_json::json!(candidates));
                metadata.insert("ambiguous".to_string(), serde_json::json!(true));
            }

            relation.metadata = serde_json::Value::Object(metadata);
            relations.push(relation);
        }

        Ok(relations)
    }

    /// Store tags and create tag associations for a document
    ///
    /// Extracts tags from ParsedDocument, ensures hierarchical structure exists,
    /// and creates entity_tag associations.
    async fn store_tags(
        &self,
        doc: &ParsedDocument,
        entity_id: &RecordId<EntityRecord>,
    ) -> Result<()> {
        // Collect unique tags
        let unique_tags: std::collections::HashSet<String> = doc
            .all_tags()
            .into_iter()
            .filter_map(|tag| {
                let trimmed = tag.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();

        if unique_tags.is_empty() {
            return Ok(());
        }

        // Delete existing tag associations for this entity
        self.store.delete_entity_tags(entity_id).await?;

        // Create tags and associations
        for tag in unique_tags {
            if let Some(tag_id) = self.ensure_tag_hierarchy(&tag).await? {
                let mapping = EntityTag {
                    id: Some(self.entity_tag_record_id(entity_id, &tag_id)),
                    entity_id: entity_id.clone(),
                    tag_id,
                    source: "parser".into(),
                    confidence: 1.0,
                };
                self.store.upsert_entity_tag(&mapping).await?;
            }
        }

        Ok(())
    }

    /// Ensure tag hierarchy exists in database
    ///
    /// Creates parent tags if they don't exist (e.g., #project/ai/nlp creates project, project/ai, project/ai/nlp)
    async fn ensure_tag_hierarchy(
        &self,
        tag_path: &str,
    ) -> Result<Option<RecordId<TagRecord>>> {
        let segments: Vec<String> = tag_path
            .trim()
            .trim_start_matches('#')
            .split('/')
            .filter_map(|seg| {
                let trimmed = seg.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect();

        if segments.is_empty() {
            return Ok(None);
        }

        let mut current_path = String::new();
        let mut parent: Option<RecordId<TagRecord>> = None;

        for (depth, segment) in segments.iter().enumerate() {
            if !current_path.is_empty() {
                current_path.push('/');
            }
            current_path.push_str(segment);

            let tag_id = RecordId::new("tags", current_path.clone());
            let tag = super::types::Tag {
                id: Some(tag_id.clone()),
                name: current_path.clone(),
                parent_id: parent.clone(),
                path: current_path.clone(),
                depth: depth as i32,
                description: None,
                color: None,
                icon: None,
            };

            self.store.upsert_tag(&tag).await?;
            parent = Some(tag_id);
        }

        Ok(parent)
    }

    /// Generate entity_tag record ID from entity and tag IDs
    fn entity_tag_record_id(
        &self,
        entity_id: &RecordId<EntityRecord>,
        tag_id: &RecordId<TagRecord>,
    ) -> RecordId<EntityTagRecord> {
        let entity_part = entity_id.id.replace(':', "_");
        let tag_part = tag_id.id.replace(':', "_");
        RecordId::new("entity_tags", format!("{}:{}", entity_part, tag_part))
    }

    /// Generate a property record ID from entity ID, namespace, and key
    fn property_id(
        &self,
        entity_id: &RecordId<EntityRecord>,
        namespace: &str,
        key: &str,
    ) -> RecordId<PropertyRecord> {
        RecordId::new(
            "properties",
            format!("{}:{}:{}", entity_id.id, namespace, key),
        )
    }

    /// Compute section properties from the document's Merkle tree
    fn compute_section_properties(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedDocument,
    ) -> Vec<Property> {
        // Build the hybrid Merkle tree to extract sections
        let merkle_tree = HybridMerkleTree::from_document(doc);

        // Pre-calculate capacity: 2 base properties + 2 per section
        // Base: tree_root_hash, total_sections
        // Per section: section_{n}_hash, section_{n}_metadata
        let capacity = 2 + (merkle_tree.sections.len() * 2);
        let mut props = Vec::with_capacity(capacity);

        // Store the root hash as a property
        props.push(Property::new(
            self.property_id(entity_id, "section", "tree_root_hash"),
            entity_id.clone(),
            "section",
            "tree_root_hash",
            PropertyValue::Text(merkle_tree.root_hash.to_hex()),
        ));

        // Store total section count
        props.push(Property::new(
            self.property_id(entity_id, "section", "total_sections"),
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
                self.property_id(entity_id, "section", &hash_key),
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
                self.property_id(entity_id, "section", &metadata_key),
                entity_id.clone(),
                "section",
                &metadata_key,
                PropertyValue::Json(Value::Object(metadata)),
            ));
        }

        props
    }

    /// Compute core document properties
    fn core_properties(
        &self,
        entity_id: &RecordId<EntityRecord>,
        doc: &ParsedDocument,
        relative_path: &str,
    ) -> Vec<Property> {
        // Pre-calculate capacity: 4 base properties + 1 if frontmatter exists
        // Base: path, relative_path, title, tags
        // Optional: frontmatter
        let capacity = if doc.frontmatter.is_some() { 5 } else { 4 };
        let mut props = Vec::with_capacity(capacity);

        props.push(Property::new(
            self.property_id(entity_id, "core", "path"),
            entity_id.clone(),
            "core",
            "path",
            PropertyValue::Text(doc.path.to_string_lossy().to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "relative_path"),
            entity_id.clone(),
            "core",
            "relative_path",
            PropertyValue::Text(relative_path.to_string()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "title"),
            entity_id.clone(),
            "core",
            "title",
            PropertyValue::Text(doc.title()),
        ));

        props.push(Property::new(
            self.property_id(entity_id, "core", "tags"),
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
                self.property_id(entity_id, "core", "frontmatter"),
                entity_id.clone(),
                "core",
                "frontmatter",
                PropertyValue::Json(fm_value),
            ));
        }

        props
    }

    /// Generate entity payload data from parsed document
    fn entity_payload(&self, doc: &ParsedDocument, relative_path: &str) -> Value {
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

    /// Generate entity ID from relative path
    fn note_entity_id(&self, relative_path: &str) -> RecordId<EntityRecord> {
        let normalized = relative_path
            .trim_start_matches(std::path::MAIN_SEPARATOR)
            .replace('\\', "/")
            .replace(':', "_");
        RecordId::new("entities", format!("note:{}", normalized))
    }

    /// Extract inline link relations from the document
    fn extract_inline_link_relations(&self, entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<CoreRelation> {
        let capacity = doc.inline_links.len();
        let mut relations = Vec::with_capacity(capacity);
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for inline_link in &doc.inline_links {
            // Create relation for inline link
            // For external URLs, to_entity_id is None (no entity to link to)
            // For relative links, we could potentially resolve them to entities
            let to_entity_id = if inline_link.is_relative() {
                // Could parse relative path to entity ID
                // For now, just store the URL in metadata
                None
            } else {
                None // External URLs don't have entity IDs
            };

            let mut relation = CoreRelation::new(
                from_entity_id.clone(),
                to_entity_id,
                "link",
            );

            // Add metadata about the inline link
            let mut metadata = serde_json::Map::new();
            metadata.insert("url".to_string(), serde_json::json!(inline_link.url));
            metadata.insert("text".to_string(), serde_json::json!(inline_link.text));
            if let Some(title) = &inline_link.title {
                metadata.insert("title".to_string(), serde_json::json!(title));
            }
            metadata.insert("offset".to_string(), serde_json::json!(inline_link.offset));
            metadata.insert("is_external".to_string(), serde_json::json!(inline_link.is_external()));

            relation.metadata = serde_json::Value::Object(metadata);

            relations.push(relation);
        }

        relations
    }

    /// Extract footnote relations from the document
    fn extract_footnote_relations(&self, entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<CoreRelation> {
        let capacity = doc.footnotes.references.len();
        let mut relations = Vec::with_capacity(capacity);
        let from_entity_id = format!("{}:{}", entity_id.table, entity_id.id);

        for footnote_ref in &doc.footnotes.references {
            // Only create relations for references that have definitions
            if let Some(definition) = doc.footnotes.definitions.get(&footnote_ref.identifier) {
                let mut relation = CoreRelation::new(
                    from_entity_id.clone(),
                    None, // Footnotes are self-referential (within the same document)
                    "footnote",
                );

                // Add metadata about the footnote
                let mut metadata = serde_json::Map::new();
                metadata.insert("label".to_string(), serde_json::json!(footnote_ref.identifier));
                metadata.insert("content".to_string(), serde_json::json!(definition.content));
                metadata.insert("ref_offset".to_string(), serde_json::json!(footnote_ref.offset));
                metadata.insert("def_offset".to_string(), serde_json::json!(definition.offset));
                if let Some(order) = footnote_ref.order_number {
                    metadata.insert("order".to_string(), serde_json::json!(order));
                }

                relation.metadata = serde_json::Value::Object(metadata);

                relations.push(relation);
            }
        }

        relations
    }

    /// Create block with metadata
    fn make_block_with_metadata(
        &self,
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

    /// Build blocks from document content
    fn build_blocks(&self, entity_id: &RecordId<EntityRecord>, doc: &ParsedDocument) -> Vec<BlockNode> {
        // Pre-calculate capacity: sum of all content types
        // Headings + code_blocks + lists + callouts + latex + paragraphs (upper bound)
        // Note: Actual paragraph count may be lower due to filtering empty ones
        let capacity = doc.content.headings.len()
            + doc.content.paragraphs.len()
            + doc.content.code_blocks.len()
            + doc.content.lists.len()
            + doc.callouts.len()
            + doc.content.latex_expressions.len();
        let mut blocks = Vec::with_capacity(capacity);
        let mut index = 0;

        // Headings with metadata (level + text)
        for heading in &doc.content.headings {
            let metadata = serde_json::json!({
                "level": heading.level,
                "text": heading.text.clone()
            });
            blocks.push(self.make_block_with_metadata(
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
            blocks.push(self.make_block_with_metadata(
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
            blocks.push(self.make_block_with_metadata(
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
                    crucible_core::parser::ListType::Ordered => "ordered",
                    crucible_core::parser::ListType::Unordered => "unordered",
                },
                "item_count": list.items.len()
            });

            // Serialize list as text (simple approach for now)
            let list_text = list.items.iter()
                .map(|item| {
                    if let Some(task_status) = &item.task_status {
                        let check = match task_status {
                            crucible_core::parser::TaskStatus::Pending => " ",
                            crucible_core::parser::TaskStatus::Completed => "x",
                        };
                        format!("- [{}] {}", check, item.content)
                    } else {
                        format!("- {}", item.content)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");

            blocks.push(self.make_block_with_metadata(
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
            blocks.push(self.make_block_with_metadata(
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
            blocks.push(self.make_block_with_metadata(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;
    use crucible_core::parser::{
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
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create target notes first so wikilinks can be resolved
        let mut target_doc1 = sample_document();
        target_doc1.path = PathBuf::from("other-note.md");
        target_doc1.tags.clear();
        ingestor.ingest(&target_doc1, "other-note.md").await.unwrap();

        let mut target_doc2 = sample_document();
        target_doc2.path = PathBuf::from("embedded-note.md");
        target_doc2.tags.clear();
        ingestor.ingest(&target_doc2, "embedded-note.md").await.unwrap();

        // Now create document with wikilinks
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

        // Check wikilink relation - now should be resolved
        let wikilink_rel = relations
            .iter()
            .find(|r| r.relation_type == "wikilink")
            .unwrap();
        // Should be resolved to the actual target entity
        assert!(wikilink_rel.to_entity_id.is_some(), "Wikilink should be resolved");
        assert!(
            wikilink_rel.to_entity_id.as_ref().unwrap().contains("other-note"),
            "Should link to other-note"
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

        // Check embed relation - now should be resolved
        let embed_rel = relations
            .iter()
            .find(|r| r.relation_type == "embed")
            .unwrap();
        assert!(embed_rel.to_entity_id.is_some(), "Embed should be resolved");
        assert!(
            embed_rel.to_entity_id.as_ref().unwrap().contains("embedded-note"),
            "Should link to embedded-note"
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

        // Tags are now automatically stored during ingestion
        let entity_id = ingestor.ingest(&doc, "notes/test-tags.md").await.unwrap();

        // Check that all tag levels were created
        let project_tag = store.get_tag("project").await.unwrap();
        assert!(project_tag.is_some(), "Should have 'project' tag");
        assert_eq!(project_tag.unwrap().name, "project");

        let ai_tag = store.get_tag("project/ai").await.unwrap();
        assert!(ai_tag.is_some(), "Should have 'project/ai' tag");
        let ai_tag = ai_tag.unwrap();
        assert_eq!(ai_tag.name, "project/ai");
        // Adapter adds "tags:" prefix to parent_id
        assert_eq!(ai_tag.parent_tag_id, Some("tags:project".to_string()));

        let nlp_tag = store.get_tag("project/ai/nlp").await.unwrap();
        assert!(nlp_tag.is_some(), "Should have 'project/ai/nlp' tag");
        let nlp_tag = nlp_tag.unwrap();
        assert_eq!(nlp_tag.name, "project/ai/nlp");
        assert_eq!(nlp_tag.parent_tag_id, Some("tags:project/ai".to_string()));

        let status_tag = store.get_tag("status").await.unwrap();
        assert!(status_tag.is_some(), "Should have 'status' tag");

        let active_tag = store.get_tag("status/active").await.unwrap();
        assert!(active_tag.is_some(), "Should have 'status/active' tag");
        let active_tag = active_tag.unwrap();
        assert_eq!(active_tag.parent_tag_id, Some("tags:status".to_string()));

        // Entity-tag associations are also automatically created during ingestion
    }

    #[tokio::test]
    async fn ingest_document_stores_relation_metadata() {
        use crucible_core::parser::Wikilink;
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
        use crucible_core::parser::Wikilink;
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
    async fn test_inline_link_extraction() {
        use crucible_core::parser::InlineLink;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();
        doc.inline_links.push(InlineLink::new(
            "Rust".to_string(),
            "https://rust-lang.org".to_string(),
            10,
        ));
        doc.inline_links.push(InlineLink::with_title(
            "GitHub".to_string(),
            "https://github.com".to_string(),
            "Code hosting".to_string(),
            50,
        ));
        doc.inline_links.push(InlineLink::new(
            "other note".to_string(),
            "./notes/other.md".to_string(),
            80,
        ));

        let entity_id = ingestor.ingest(&doc, "notes/inline-links-test.md").await.unwrap();

        // Get all relations for this entity
        let relations = store.get_relations(&entity_id.id, Some("link")).await.unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 inline link relations");

        // Check first link (external)
        let rust_link = relations
            .iter()
            .find(|r| r.metadata.get("text").and_then(|v| v.as_str()) == Some("Rust"))
            .unwrap();
        assert_eq!(rust_link.relation_type, "link");
        assert_eq!(
            rust_link.metadata.get("url").and_then(|v| v.as_str()),
            Some("https://rust-lang.org")
        );
        assert_eq!(
            rust_link.metadata.get("is_external").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(rust_link.metadata.get("title"), None);

        // Check second link (with title)
        let github_link = relations
            .iter()
            .find(|r| r.metadata.get("text").and_then(|v| v.as_str()) == Some("GitHub"))
            .unwrap();
        assert_eq!(
            github_link.metadata.get("title").and_then(|v| v.as_str()),
            Some("Code hosting")
        );

        // Check third link (relative)
        let relative_link = relations
            .iter()
            .find(|r| r.metadata.get("url").and_then(|v| v.as_str()) == Some("./notes/other.md"))
            .unwrap();
        assert_eq!(
            relative_link.metadata.get("is_external").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[tokio::test]
    async fn test_footnote_extraction() {
        use crucible_core::parser::{FootnoteDefinition, FootnoteReference};

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc = sample_document();

        // Add footnote references
        doc.footnotes.add_reference(FootnoteReference::with_order("1".to_string(), 20, 1));
        doc.footnotes.add_reference(FootnoteReference::with_order("note".to_string(), 50, 2));

        // Add footnote definitions
        doc.footnotes.add_definition(
            "1".to_string(),
            FootnoteDefinition::new("1".to_string(), "First footnote definition".to_string(), 100, 5)
        );
        doc.footnotes.add_definition(
            "note".to_string(),
            FootnoteDefinition::new("note".to_string(), "Second footnote with custom label".to_string(), 150, 6)
        );

        let entity_id = ingestor.ingest(&doc, "notes/footnote-test.md").await.unwrap();

        // Get all footnote relations
        let relations = store.get_relations(&entity_id.id, Some("footnote")).await.unwrap();
        assert_eq!(relations.len(), 2, "Should have 2 footnote relations");

        // Check first footnote
        let footnote1 = relations
            .iter()
            .find(|r| r.metadata.get("label").and_then(|v| v.as_str()) == Some("1"))
            .unwrap();
        assert_eq!(footnote1.relation_type, "footnote");
        assert_eq!(
            footnote1.metadata.get("content").and_then(|v| v.as_str()),
            Some("First footnote definition")
        );
        assert_eq!(
            footnote1.metadata.get("order").and_then(|v| v.as_u64()),
            Some(1)
        );
        assert_eq!(
            footnote1.metadata.get("ref_offset").and_then(|v| v.as_u64()),
            Some(20)
        );
        assert_eq!(
            footnote1.metadata.get("def_offset").and_then(|v| v.as_u64()),
            Some(100)
        );

        // Check second footnote
        let footnote2 = relations
            .iter()
            .find(|r| r.metadata.get("label").and_then(|v| v.as_str()) == Some("note"))
            .unwrap();
        assert_eq!(
            footnote2.metadata.get("content").and_then(|v| v.as_str()),
            Some("Second footnote with custom label")
        );
        assert_eq!(
            footnote2.metadata.get("order").and_then(|v| v.as_u64()),
            Some(2)
        );
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

    #[tokio::test]
    async fn test_ambiguous_wikilink_resolution() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create two notes with same name in different folders
        let mut doc1 = sample_document();
        doc1.path = PathBuf::from("Project A/Note.md");
        doc1.tags.clear();
        let mut doc2 = sample_document();
        doc2.path = PathBuf::from("Project B/Note.md");
        doc2.tags.clear();

        ingestor.ingest(&doc1, "Project A/Note.md").await.unwrap();
        ingestor.ingest(&doc2, "Project B/Note.md").await.unwrap();

        // Create a note with ambiguous wikilink
        let mut doc3 = sample_document();
        doc3.path = PathBuf::from("Index.md");
        doc3.tags.clear();
        doc3.wikilinks.push(Wikilink {
            target: "Note".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let entity_id = ingestor.ingest(&doc3, "Index.md").await.unwrap();

        // Verify relation created with candidates in metadata
        let relations = store.get_relations(&entity_id.id, Some("wikilink")).await.unwrap();

        assert_eq!(relations.len(), 1, "Should have one wikilink relation");
        assert!(relations[0].to_entity_id.is_none(), "Ambiguous link should have no single target");

        let metadata = relations[0].metadata.as_object().unwrap();
        assert_eq!(
            metadata.get("ambiguous").and_then(|v| v.as_bool()),
            Some(true),
            "Should be marked as ambiguous. Metadata: {:?}",
            metadata
        );

        let candidates = metadata.get("candidates").unwrap().as_array().unwrap();
        assert_eq!(candidates.len(), 2, "Should have 2 candidates");
        assert!(
            candidates.iter().any(|c| c.as_str().unwrap().contains("Project A")),
            "Should have Project A candidate"
        );
        assert!(
            candidates.iter().any(|c| c.as_str().unwrap().contains("Project B")),
            "Should have Project B candidate"
        );
    }

    #[tokio::test]
    async fn test_unresolved_wikilink() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create note with wikilink to non-existent target
        let mut doc = sample_document();
        doc.path = PathBuf::from("Index.md");
        doc.tags.clear();
        doc.wikilinks.push(Wikilink {
            target: "NonExistent".to_string(),
            alias: None,
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let entity_id = ingestor.ingest(&doc, "Index.md").await.unwrap();

        // Verify relation created with no target and no candidates
        let relations = store.get_relations(&entity_id.id, Some("wikilink")).await.unwrap();
        assert_eq!(relations.len(), 1, "Should have one wikilink relation");
        assert!(relations[0].to_entity_id.is_none(), "Unresolved link should have no target");

        let metadata = relations[0].metadata.as_object().unwrap();
        // Check that candidates is either missing or empty
        let has_no_candidates = metadata.get("candidates").is_none()
            || metadata.get("candidates").unwrap().as_array().unwrap().is_empty();
        assert!(has_no_candidates, "Should have no candidates");
    }

    #[tokio::test]
    async fn test_resolved_wikilink() {
        use crucible_core::parser::Wikilink;
        use crucible_core::storage::RelationStorage;

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create target note
        let mut target_doc = sample_document();
        target_doc.path = PathBuf::from("Target.md");
        target_doc.tags.clear();
        let target_entity_id = ingestor.ingest(&target_doc, "Target.md").await.unwrap();

        // Create source note with wikilink to target
        let mut source_doc = sample_document();
        source_doc.path = PathBuf::from("Source.md");
        source_doc.tags.clear();
        source_doc.wikilinks.push(Wikilink {
            target: "Target".to_string(),
            alias: Some("My Target".to_string()),
            heading_ref: None,
            block_ref: None,
            is_embed: false,
            offset: 10,
        });

        let source_entity_id = ingestor.ingest(&source_doc, "Source.md").await.unwrap();

        // Verify relation created with resolved target
        let relations = store.get_relations(&source_entity_id.id, Some("wikilink")).await.unwrap();
        assert_eq!(relations.len(), 1, "Should have one wikilink relation");

        // The adapter prefixes entity IDs with "entities:"
        let expected_target = format!("entities:{}", target_entity_id.id);
        assert_eq!(
            relations[0].to_entity_id,
            Some(expected_target),
            "Should have resolved target"
        );

        let metadata = relations[0].metadata.as_object().unwrap();
        assert_eq!(
            metadata.get("alias").and_then(|v| v.as_str()),
            Some("My Target"),
            "Should preserve alias"
        );
        // Should not be marked as ambiguous
        assert_eq!(
            metadata.get("ambiguous"),
            None,
            "Should not be marked as ambiguous"
        );
    }
}



