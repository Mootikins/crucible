use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::SurrealClient;

use super::types::{
    BlockNode, EmbeddingVector, Entity, EntityRecord, EntityTag, EntityTagRecord, EntityType,
    Property, PropertyRecord, PropertyValue, RecordId, Relation, RelationRecord, Tag, TagRecord,
};
use surrealdb::sql::Thing;

/// High-level helper for writing entities, properties, and blocks into the EAV+Graph schema.
#[derive(Clone)]
pub struct EAVGraphStore {
    client: SurrealClient,
}

impl EAVGraphStore {
    pub fn new(client: SurrealClient) -> Self {
        Self { client }
    }

    /// Upsert an entity record.
    pub async fn upsert_entity(&self, entity: &Entity) -> Result<RecordId<EntityRecord>> {
        let id = entity
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("entity id must be provided"))?;

        let content = json!({
            "entity_type": entity.entity_type.as_str(),
            "deleted_at": entity.deleted_at,
            "version": entity.version,
            "content_hash": entity.content_hash,
            "created_by": entity.created_by,
            "vault_id": entity.vault_id,
            "data": entity.data,
            "search_text": entity.search_text,
        });

        let params = json!({
            "table": id.table,
            "id": id.id,
            "content": content,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                CONTENT $content
                RETURN AFTER;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    CONTENT $content
                    RETURN AFTER;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(id.clone())
    }

    /// Upsert a property row.
    pub async fn upsert_property(&self, property: &Property) -> Result<RecordId<PropertyRecord>> {
        let id = property
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("property id must be provided"))?;

        let entity_id = &property.entity_id;
        let value = &property.value;

        let params = json!({
            "table": id.table,
            "id": id.id,
            "entity_table": entity_id.table,
            "entity_id": entity_id.id,
            "namespace": property.namespace.0,
            "key": property.key,
            "value": value.as_json_string(),
            "value_type": value.value_type.as_str(),
            "value_text": value.value_text,
            "value_number": value.value_number,
            "value_bool": value.value_bool,
            "value_date": value.value_date.clone(),
            "source": property.source,
            "confidence": property.confidence,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                SET
                    entity_id = type::thing($entity_table, $entity_id),
                    namespace = $namespace,
                    key = $key,
                    value = $value,
                    value_type = $value_type,
                    value_text = $value_text,
                    value_number = $value_number,
                    value_bool = $value_bool,
                    value_date = $value_date,
                    source = $source,
                    confidence = $confidence
                RETURN AFTER;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        entity_id = type::thing($entity_table, $entity_id),
                        namespace = $namespace,
                        key = $key,
                        value = $value,
                        value_type = $value_type,
                        value_text = $value_text,
                        value_number = $value_number,
                        value_bool = $value_bool,
                        value_date = $value_date,
                        source = $source,
                        confidence = $confidence
                    RETURN AFTER;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(id.clone())
    }

    /// Replace all blocks associated with an entity with the provided list.
    pub async fn replace_blocks(
        &self,
        entity_id: &RecordId<EntityRecord>,
        blocks: &[BlockNode],
    ) -> Result<()> {
        self.client
            .query(
                r#"
                DELETE blocks WHERE entity_id = type::thing($entity_table, $entity_id);
                "#,
                &[json!({
                    "entity_table": entity_id.table,
                    "entity_id": entity_id.id,
                })],
            )
            .await?;

        for block in blocks {
            let block_id = block
                .id
                .as_ref()
                .ok_or_else(|| anyhow!("block id must be provided"))?;

            let parent_ref = block
                .parent_block_id
                .as_ref()
                .map(|parent| thing_value(parent))
                .unwrap_or(serde_json::Value::Null);

            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        entity_id = type::thing($entity_table, $entity_id),
                        block_index = $block_index,
                        block_type = $block_type,
                        content = $content,
                        content_hash = $content_hash,
                        start_offset = $start_offset,
                        end_offset = $end_offset,
                        start_line = $start_line,
                        end_line = $end_line,
                        parent_block_id = $parent_block_id,
                        depth = $depth,
                        metadata = $metadata
                    RETURN NONE;
                    "#,
                    &[json!({
                        "table": block_id.table,
                        "id": block_id.id,
                        "entity_table": entity_id.table,
                        "entity_id": entity_id.id,
                        "block_index": block.block_index,
                        "block_type": block.block_type,
                        "content": block.content,
                        "content_hash": block.content_hash,
                        "start_offset": block.start_offset,
                        "end_offset": block.end_offset,
                        "start_line": block.start_line,
                        "end_line": block.end_line,
                        "parent_block_id": parent_ref,
                        "depth": block.depth,
                        "metadata": block.metadata,
                    })],
                )
                .await?;
        }

        Ok(())
    }

    /// Upsert an embedding vector for an entity (optionally at block level).
    pub async fn upsert_embedding(&self, embedding: &EmbeddingVector) -> Result<()> {
        let id = embedding
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("embedding id must be provided"))?;

        let block_ref = embedding
            .block_id
            .as_ref()
            .map(|b| thing_value(b))
            .unwrap_or(serde_json::Value::Null);

        let params = json!({
            "table": id.table,
            "id": id.id,
            "entity_table": embedding.entity_id.table,
            "entity_id": embedding.entity_id.id,
            "block_id": block_ref,
            "embedding": embedding.embedding,
            "dimensions": embedding.dimensions,
            "model": embedding.model,
            "model_version": embedding.model_version,
            "content_used": embedding.content_used,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                SET
                    entity_id = type::thing($entity_table, $entity_id),
                    block_id = $block_id,
                    embedding = $embedding,
                    dimensions = $dimensions,
                    model = $model,
                    model_version = $model_version,
                    content_used = $content_used
                RETURN NONE;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        entity_id = type::thing($entity_table, $entity_id),
                        block_id = $block_id,
                        embedding = $embedding,
                        dimensions = $dimensions,
                        model = $model,
                        model_version = $model_version,
                        content_used = $content_used
                    RETURN NONE;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(())
    }

    /// Check whether an entity exists.
    pub async fn entity_exists(&self, entity_id: &RecordId<EntityRecord>) -> Result<bool> {
        let result = self
            .client
            .query(
                r#"
                SELECT id FROM type::thing($table, $id) LIMIT 1;
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                })],
            )
            .await?;

        Ok(!result.records.is_empty())
    }

    /// Create a placeholder note entity if one doesn't already exist.
    pub async fn ensure_note_entity(
        &self,
        entity_id: &RecordId<EntityRecord>,
        placeholder_title: &str,
    ) -> Result<()> {
        if self.entity_exists(entity_id).await? {
            return Ok(());
        }

        self.client
            .query(
                r#"
                CREATE type::thing($table, $id)
                CONTENT {
                    entity_type: "note",
                    created_at: time::now(),
                    updated_at: time::now(),
                    version: 1,
                    data: {
                        placeholder: true,
                        title: $title
                    }
                }
                RETURN NONE;
                "#,
                &[json!({
                    "table": entity_id.table,
                    "id": entity_id.id,
                    "title": placeholder_title,
                })],
            )
            .await?;

        Ok(())
    }

    /// Upsert (or create) a tag entry.
    pub async fn upsert_tag(&self, tag: &Tag) -> Result<RecordId<TagRecord>> {
        let id = tag
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("tag id must be provided"))?;

        let (parent_table, parent_id, has_parent) = if let Some(parent) = &tag.parent_id {
            (
                Value::String(parent.table.clone()),
                Value::String(parent.id.clone()),
                Value::Bool(true),
            )
        } else {
            (Value::Null, Value::Null, Value::Bool(false))
        };

        let params = json!({
            "table": id.table,
            "id": id.id,
            "name": tag.name,
            "parent_table": parent_table,
            "parent_id": parent_id,
            "has_parent": has_parent,
            "path": tag.path,
            "depth": tag.depth,
            "description": tag.description,
            "color": tag.color,
            "icon": tag.icon,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                SET
                    name = $name,
                    parent_id = if $has_parent THEN type::thing($parent_table, $parent_id) ELSE NONE END,
                    path = $path,
                    depth = $depth,
                    description = $description,
                    color = $color,
                    icon = $icon
                RETURN NONE;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        name = $name,
                        parent_id = if $has_parent THEN type::thing($parent_table, $parent_id) ELSE NONE END,
                        path = $path,
                        depth = $depth,
                        description = $description,
                        color = $color,
                        icon = $icon
                    RETURN NONE;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(id.clone())
    }

    /// Remove all tag associations for an entity.
    pub async fn delete_entity_tags(&self, entity_id: &RecordId<EntityRecord>) -> Result<()> {
        self.client
            .query(
                r#"
                DELETE entity_tags
                WHERE entity_id = type::thing($entity_table, $entity_id);
                "#,
                &[json!({
                    "entity_table": entity_id.table,
                    "entity_id": entity_id.id,
                })],
            )
            .await?;

        Ok(())
    }

    /// Upsert the mapping between an entity and a tag.
    pub async fn upsert_entity_tag(&self, mapping: &EntityTag) -> Result<()> {
        let id = mapping
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("entity_tag id must be provided"))?;

        let params = json!({
            "table": id.table,
            "id": id.id,
            "entity_table": mapping.entity_id.table,
            "entity_id": mapping.entity_id.id,
            "tag_table": mapping.tag_id.table,
            "tag_id": mapping.tag_id.id,
            "source": mapping.source,
            "confidence": mapping.confidence,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                SET
                    entity_id = type::thing($entity_table, $entity_id),
                    tag_id = type::thing($tag_table, $tag_id),
                    source = $source,
                    confidence = $confidence
                RETURN NONE;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        entity_id = type::thing($entity_table, $entity_id),
                        tag_id = type::thing($tag_table, $tag_id),
                        source = $source,
                        confidence = $confidence
                    RETURN NONE;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(())
    }

    /// Delete all relations of a given type for the provided entity.
    pub async fn delete_relations_from(
        &self,
        entity_id: &RecordId<EntityRecord>,
        relation_type: &str,
    ) -> Result<()> {
        self.client
            .query(
                r#"
                DELETE relations
                WHERE relation_type = $relation_type
                  AND in = type::thing($entity_table, $entity_id);
                "#,
                &[json!({
                    "relation_type": relation_type,
                    "entity_table": entity_id.table,
                    "entity_id": entity_id.id,
                })],
            )
            .await?;

        Ok(())
    }

    /// Upsert a relation record.
    pub async fn upsert_relation(&self, relation: &Relation) -> Result<RecordId<RelationRecord>> {
        let id = relation
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("relation id must be provided"))?;

        let params = json!({
            "table": id.table,
            "id": id.id,
            "from_table": relation.from_id.table,
            "from_id": relation.from_id.id,
            "to_table": relation.to_id.table,
            "to_id": relation.to_id.id,
            "relation_type": relation.relation_type,
            "weight": relation.weight,
            "directed": relation.directed,
            "confidence": relation.confidence,
            "source": relation.source,
            "position": relation.position,
            "metadata": relation.metadata,
        });

        let result = self
            .client
            .query(
                r#"
                UPDATE type::thing($table, $id)
                SET
                    in = type::thing($from_table, $from_id),
                    out = type::thing($to_table, $to_id),
                    relation_type = $relation_type,
                    weight = $weight,
                    directed = $directed,
                    confidence = $confidence,
                    source = $source,
                    position = $position,
                    metadata = $metadata
                RETURN NONE;
                "#,
                &[params.clone()],
            )
            .await?;

        if result.records.is_empty() {
            self.client
                .query(
                    r#"
                    CREATE type::thing($table, $id)
                    SET
                        in = type::thing($from_table, $from_id),
                        out = type::thing($to_table, $to_id),
                        relation_type = $relation_type,
                        weight = $weight,
                        directed = $directed,
                        confidence = $confidence,
                        source = $source,
                        position = $position,
                        metadata = $metadata
                    RETURN NONE;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(id.clone())
    }
}

fn thing_value<T>(id: &RecordId<T>) -> serde_json::Value {
    let thing = Thing::from((id.table.as_str(), id.id.as_str()));
    serde_json::to_value(thing).expect("Thing serialization")
}

// -- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::SurrealClient;

    fn entity_id() -> RecordId<EntityRecord> {
        RecordId::new("entities", "note:test")
    }

    fn sample_entity() -> Entity {
        Entity::new(entity_id(), EntityType::Note)
            .with_content_hash("abc123")
            .with_search_text("hello world")
    }

    fn property_id(key: &str) -> RecordId<PropertyRecord> {
        RecordId::new("properties", format!("note:test:{}", key))
    }

    #[tokio::test]
    async fn upsert_entity_and_property_flow() {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        let entity = sample_entity();
        store.upsert_entity(&entity).await.unwrap();

        let property = Property::new(
            property_id("title"),
            entity_id(),
            "core",
            "title",
            PropertyValue::text("Sample"),
        );

        store.upsert_property(&property).await.unwrap();

        let result = client
            .query("SELECT * FROM properties WHERE key = 'title'", &[])
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
        let record = &result.records[0];
        assert_eq!(
            record.data.get("value_text").unwrap().as_str(),
            Some("Sample")
        );
    }

    #[tokio::test]
    async fn replace_blocks_writes_rows() {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        store.upsert_entity(&sample_entity()).await.unwrap();

        let block = BlockNode::new(
            RecordId::new("blocks", "block:test:0"),
            entity_id(),
            0,
            "paragraph",
            "Hello",
            "hash0",
        );

        store.replace_blocks(&entity_id(), &[block]).await.unwrap();

        let result = client
            .query(
                "SELECT * FROM blocks WHERE entity_id = type::thing('entities', 'note:test')",
                &[],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
    }

    #[tokio::test]
    async fn upsert_embedding_stores_vector() {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        store.upsert_entity(&sample_entity()).await.unwrap();

        let vector: Vec<f32> = vec![0.5; 384];

        let embedding = EmbeddingVector::new(
            RecordId::new("embeddings", "embedding:test"),
            entity_id(),
            vector,
            384,
            "mini-lm",
            "v1",
            "sample",
        );

        store.upsert_embedding(&embedding).await.unwrap();

        let result = client
            .query(
                "SELECT * FROM embeddings WHERE id = type::thing('embeddings', 'embedding:test')",
                &[],
            )
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
        let record = &result.records[0];
        assert_eq!(record.data.get("dimensions").unwrap().as_i64(), Some(384));
    }
}
