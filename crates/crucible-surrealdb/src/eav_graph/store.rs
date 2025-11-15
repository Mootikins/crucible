use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::{QueryResult, SurrealClient};

use super::types::{
    BlockNode, EmbeddingVector, Entity, EntityRecord, EntityTag as SurrealEntityTag, Property,
    PropertyRecord, RecordId, Relation as SurrealRelation, RelationRecord, Tag as SurrealTag,
    TagRecord,
};
use surrealdb::sql::Thing;

/// High-level helper for writing entities, properties, and blocks into the EAV+Graph schema.
#[derive(Clone)]
pub struct EAVGraphStore {
    pub(crate) client: SurrealClient,
}

impl EAVGraphStore {
    pub fn new(client: SurrealClient) -> Self {
        Self { client }
    }

    /// Helper method to deserialize query results into Property structs
    ///
    /// Handles the conversion from SurrealDB's internal representation to our
    /// Property type with proper error handling and context.
    fn deserialize_properties(&self, result: QueryResult) -> StorageResult<Vec<Property>> {
        result
            .records
            .iter()
            .enumerate()
            .map(|(idx, record)| {
                serde_json::to_value(&record.data)
                    .map_err(|e| {
                        StorageError::Backend(format!(
                            "Failed to serialize property at index {}: {}",
                            idx, e
                        ))
                    })
                    .and_then(|v| {
                        serde_json::from_value(v).map_err(|e| {
                            StorageError::Backend(format!(
                                "Failed to deserialize property at index {}: {}",
                                idx, e
                            ))
                        })
                    })
            })
            .collect::<Result<Vec<_>, _>>()
    }

    /// Upsert an entity record.
    pub async fn upsert_entity(&self, entity: &Entity) -> Result<RecordId<EntityRecord>> {
        let id = entity
            .id
            .as_ref()
            .ok_or_else(|| anyhow!("entity id must be provided"))?;

        let content = json!({
            "type": entity.entity_type.as_str(),
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

        // Serialize PropertyValue as JSON object
        let value_json = serde_json::to_value(&property.value)?;

        let params = json!({
            "table": id.table,
            "id": id.id,
            "entity_table": entity_id.table,
            "entity_id": entity_id.id,
            "namespace": &property.namespace.0,
            "key": property.key,
            "value": value_json,
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
                    type: "note",
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
    ///
    /// This function implements a race-condition-safe upsert pattern using UPDATE-then-CREATE.
    /// Since SurrealDB doesn't have native UPSERT for records with custom IDs, we use a
    /// two-phase approach that safely handles concurrent tag creation from multiple threads.
    ///
    /// # Race Condition Handling:
    /// 1. Try UPDATE - works if tag already exists (including if another thread just created it)
    /// 2. If UPDATE fails (empty result), CREATE the tag
    /// 3. If CREATE fails with "already exists", ignore it - another thread created the tag
    ///
    /// This ensures that concurrent operations never fail due to race conditions and that
    /// the desired end state (tag exists with correct properties) is always achieved.
    pub async fn upsert_tag(&self, tag: &SurrealTag) -> Result<RecordId<TagRecord>> {
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

        // RACE CONDITION HANDLING: UPDATE-first approach
        //
        // SurrealDB doesn't have native UPSERT for records with custom IDs,
        // so we use a two-phase approach to handle concurrent tag creation:
        //
        // Phase 1: Try UPDATE first - this works if the tag already exists
        // If another thread created the tag between our check and now,
        // UPDATE will successfully update it in place.
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
                    icon = $icon;
                "#,
                &[params.clone()],
            )
            .await?;

        // Phase 2: Check if UPDATE succeeded
        if result.records.is_empty() {
            // UPDATE returned empty results, meaning the tag doesn't exist yet.
            // Now we need to CREATE it.
            //
            // RACE CONDITION SCENARIO:
            // Thread A: UPDATE fails (tag doesn't exist)
            // Thread B: UPDATE fails (tag doesn't exist)
            // Thread A: CREATE succeeds (creates tag)
            // Thread B: CREATE fails with "already exists" (because Thread A created it)
            // This is EXPECTED behavior - Thread B should not fail the operation
            let create_result = self.client
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
                        icon = $icon;
                    "#,
                    &[params],
                )
                .await;

            // Handle CREATE result with race condition awareness
            if let Err(e) = create_result {
                let err_msg = e.to_string();
                if !err_msg.contains("already exists") {
                    // Unexpected error - propagate it
                    return Err(anyhow::Error::from(e));
                }
                // SUCCESS CASE: "already exists" error means another thread
                // created the tag between our UPDATE and CREATE operations.
                // This is exactly what we want - the tag exists now!
            }
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
    ///
    /// This function implements a race-condition-safe upsert pattern using UPDATE-then-CREATE.
    /// Since SurrealDB doesn't have native UPSERT for records with custom IDs, we use a
    /// two-phase approach that safely handles concurrent entity-tag association creation.
    ///
    /// # Race Condition Handling:
    /// 1. Try UPDATE - works if mapping already exists (including if another thread just created it)
    /// 2. If UPDATE fails (empty result), CREATE the mapping
    /// 3. If CREATE fails with "already exists", ignore it - another thread created the mapping
    ///
    /// This ensures that concurrent operations never fail due to race conditions and that
    /// the desired end state (entity-tag mapping exists) is always achieved.
    pub async fn upsert_entity_tag(&self, mapping: &SurrealEntityTag) -> Result<()> {
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

        // RACE CONDITION HANDLING: UPDATE-first approach for entity-tag mapping
        //
        // Phase 1: Try UPDATE first - this works if the mapping already exists
        // If another thread created the mapping between our check and now,
        // UPDATE will successfully update it in place.
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

        // Phase 2: Check if UPDATE succeeded
        if result.records.is_empty() {
            // UPDATE returned empty results, meaning the mapping doesn't exist yet.
            // Now we need to CREATE it.
            //
            // RACE CONDITION SCENARIO:
            // Thread A: UPDATE fails (mapping doesn't exist)
            // Thread B: UPDATE fails (mapping doesn't exist)
            // Thread A: CREATE succeeds (creates mapping)
            // Thread B: CREATE fails with "already exists" (because Thread A created it)
            // This is EXPECTED behavior - Thread B should not fail the operation
            let create_result = self
                .client
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
                .await;

            // Handle CREATE result with race condition awareness
            if let Err(e) = create_result {
                let err_msg = e.to_string();
                if !err_msg.contains("already exists") {
                    // Unexpected error - propagate it
                    return Err(anyhow::Error::from(e));
                }
                // SUCCESS CASE: "already exists" error means another thread
                // created the mapping between our UPDATE and CREATE operations.
                // This is exactly what we want - the mapping exists now!
            }
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
    pub async fn upsert_relation(
        &self,
        relation: &SurrealRelation,
    ) -> Result<RecordId<RelationRecord>> {
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
            "content_category": relation.content_category,
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
                    metadata = $metadata,
                    content_category = $content_category
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
                        metadata = $metadata,
                        content_category = $content_category
                    RETURN NONE;
                    "#,
                    &[params],
                )
                .await?;
        }

        Ok(id.clone())
    }
}

// ============================================================================
// PropertyStorage Trait Implementation
// ============================================================================

use async_trait::async_trait;
use crucible_core::storage::{PropertyStorage as CorePropertyStorage, StorageError, StorageResult};

use super::adapter::{core_properties_to_surreal, string_to_entity_id, surreal_properties_to_core};

#[async_trait]
impl CorePropertyStorage for EAVGraphStore {
    /// Batch upsert properties to SurrealDB using parameterized queries
    ///
    /// Stores multiple properties for an entity, upserting on conflict by
    /// (entity_id, namespace, key). Uses parameterized queries to prevent SQL injection.
    ///
    /// # Arguments
    ///
    /// * `properties` - Vector of core properties to store
    ///
    /// # Returns
    ///
    /// Number of properties successfully stored
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if database operation fails or
    /// if property serialization/deserialization fails
    async fn batch_upsert_properties(
        &self,
        properties: Vec<crucible_core::storage::Property>,
    ) -> StorageResult<usize> {
        if properties.is_empty() {
            return Ok(0);
        }

        let count = properties.len();

        // Convert core properties to SurrealDB properties
        let surreal_props = core_properties_to_surreal(properties);

        // Build array of property objects for batch insert
        let props_array: Vec<serde_json::Value> = surreal_props
            .iter()
            .map(|prop| {
                // INVARIANT: core_properties_to_surreal() always generates IDs for all properties
                // See adapter.rs:117-129 where RecordId::new() is called for every property
                let prop_id = prop
                    .id
                    .as_ref()
                    .expect("Property must have ID from conversion");

                // INVARIANT: PropertyValue is a simple enum with #[derive(Serialize)]
                // Serialization can only fail for types with custom serializers, not simple enums
                // See crucible-core/src/storage/mod.rs:PropertyValue definition
                let value_json = serde_json::to_value(&prop.value)
                    .expect("PropertyValue should always serialize");

                json!({
                    "prop_table": prop_id.table,
                    "prop_id": prop_id.id,
                    "entity_table": prop.entity_id.table,
                    "entity_id": prop.entity_id.id,
                    "namespace": prop.namespace.0,
                    "key": prop.key,
                    "value": value_json,
                    "source": prop.source,
                    "confidence": prop.confidence,
                })
            })
            .collect();

        // Single batch upsert query using FOR loop
        self.client
            .query(
                r#"
                FOR $prop IN $properties {
                    LET $existing = (SELECT * FROM properties WHERE
                        entity_id = type::thing($prop.entity_table, $prop.entity_id)
                        AND namespace = $prop.namespace
                        AND key = $prop.key
                        LIMIT 1);

                    IF array::len($existing) > 0 THEN
                        UPDATE $existing[0].id
                        SET
                            value = $prop.value,
                            source = $prop.source,
                            confidence = $prop.confidence,
                            updated_at = time::now()
                        RETURN NONE
                    ELSE
                        CREATE type::thing($prop.prop_table, $prop.prop_id)
                        SET
                            entity_id = type::thing($prop.entity_table, $prop.entity_id),
                            namespace = $prop.namespace,
                            key = $prop.key,
                            value = $prop.value,
                            source = $prop.source,
                            confidence = $prop.confidence,
                            created_at = time::now(),
                            updated_at = time::now()
                        RETURN NONE
                    END;
                };
                "#,
                &[json!({
                    "properties": props_array
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(count)
    }

    /// Retrieve all properties for an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - Entity identifier (e.g., "note:123")
    ///
    /// # Returns
    ///
    /// Vector of all properties associated with the entity
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if query fails
    async fn get_properties(
        &self,
        entity_id: &str,
    ) -> StorageResult<Vec<crucible_core::storage::Property>> {
        let record_id = string_to_entity_id(entity_id);

        let result = self
            .client
            .query(
                r#"SELECT * FROM properties WHERE entity_id = type::thing($table, $id)"#,
                &[json!({
                    "table": "entities",
                    "id": record_id.id
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let surreal_props = self.deserialize_properties(result)?;
        Ok(surreal_properties_to_core(surreal_props))
    }

    /// Retrieve properties for an entity filtered by namespace
    ///
    /// # Arguments
    ///
    /// * `entity_id` - Entity identifier (e.g., "note:123")
    /// * `namespace` - Property namespace to filter by (e.g., "frontmatter", "core")
    ///
    /// # Returns
    ///
    /// Vector of properties in the specified namespace
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if query fails
    async fn get_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &crucible_core::storage::PropertyNamespace,
    ) -> StorageResult<Vec<crucible_core::storage::Property>> {
        let record_id = string_to_entity_id(entity_id);

        let result = self
            .client
            .query(
                r#"SELECT * FROM properties WHERE entity_id = type::thing($table, $id) AND namespace = $namespace"#,
                &[json!({
                    "table": "entities",
                    "id": record_id.id,
                    "namespace": namespace.0.as_ref()
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let surreal_props = self.deserialize_properties(result)?;
        Ok(surreal_properties_to_core(surreal_props))
    }

    /// Retrieve a single property by entity, namespace, and key
    ///
    /// # Arguments
    ///
    /// * `entity_id` - Entity identifier (e.g., "note:123")
    /// * `namespace` - Property namespace (e.g., "frontmatter")
    /// * `key` - Property key (e.g., "title")
    ///
    /// # Returns
    ///
    /// `Some(Property)` if found, `None` otherwise
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if query fails
    async fn get_property(
        &self,
        entity_id: &str,
        namespace: &crucible_core::storage::PropertyNamespace,
        key: &str,
    ) -> StorageResult<Option<crucible_core::storage::Property>> {
        let record_id = string_to_entity_id(entity_id);

        let result = self
            .client
            .query(
                r#"SELECT * FROM properties WHERE entity_id = type::thing($table, $id) AND namespace = $namespace AND key = $key LIMIT 1"#,
                &[json!({
                    "table": "entities",
                    "id": record_id.id,
                    "namespace": namespace.0.as_ref(),
                    "key": key
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let surreal_props = self.deserialize_properties(result)?;
        Ok(surreal_props
            .into_iter()
            .next()
            .map(super::adapter::surreal_property_to_core))
    }

    /// Delete all properties for an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - Entity identifier (e.g., "note:123")
    ///
    /// # Returns
    ///
    /// Number of properties deleted
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if delete operation fails
    async fn delete_properties(&self, entity_id: &str) -> StorageResult<usize> {
        let record_id = string_to_entity_id(entity_id);

        let result = self
            .client
            .query(
                r#"DELETE FROM properties WHERE entity_id = type::thing($table, $id) RETURN BEFORE"#,
                &[json!({
                    "table": "entities",
                    "id": record_id.id
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // SurrealDB DELETE returns the deleted records
        Ok(result.records.len())
    }

    /// Delete all properties for an entity in a specific namespace
    ///
    /// # Arguments
    ///
    /// * `entity_id` - Entity identifier (e.g., "note:123")
    /// * `namespace` - Namespace to delete properties from (e.g., "frontmatter")
    ///
    /// # Returns
    ///
    /// Number of properties deleted
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if delete operation fails
    async fn delete_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &crucible_core::storage::PropertyNamespace,
    ) -> StorageResult<usize> {
        let record_id = string_to_entity_id(entity_id);

        let result = self
            .client
            .query(
                r#"DELETE FROM properties WHERE entity_id = type::thing($table, $id) AND namespace = $namespace RETURN BEFORE"#,
                &[json!({
                    "table": "entities",
                    "id": record_id.id,
                    "namespace": namespace.0.as_ref()
                })],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // SurrealDB DELETE returns the deleted records
        Ok(result.records.len())
    }
}

// ============================================================================
// RelationStorage Implementation
// ============================================================================

use super::adapter::{core_relation_to_surreal, surreal_relation_to_core};
use crucible_core::storage::RelationStorage as CoreRelationStorage;

#[async_trait]
impl CoreRelationStorage for EAVGraphStore {
    async fn store_relation(
        &self,
        relation: crucible_core::storage::Relation,
    ) -> StorageResult<String> {
        let surreal_relation = core_relation_to_surreal(relation);

        // Generate ID if not provided
        let id = surreal_relation
            .id
            .clone()
            .unwrap_or_else(|| RecordId::new("relations", format!("rel:{}", uuid::Uuid::new_v4())));

        let params = json!({
            "table": id.table,
            "id": id.id,
            "from_table": surreal_relation.from_id.table,
            "from_id_value": surreal_relation.from_id.id,
            "to_table": surreal_relation.to_id.table,
            "to_id_value": surreal_relation.to_id.id,
            "relation_type": surreal_relation.relation_type,
            "weight": surreal_relation.weight,
            "directed": surreal_relation.directed,
            "confidence": surreal_relation.confidence,
            "source": surreal_relation.source,
            "position": surreal_relation.position,
            "metadata": surreal_relation.metadata,
            "content_category": surreal_relation.content_category,
            "created_at": surreal_relation.created_at.to_rfc3339(),
        });

        self.client
            .query(
                r#"
                CREATE type::thing($table, $id) SET
                    in = type::thing($from_table, $from_id_value),
                    out = type::thing($to_table, $to_id_value),
                    relation_type = $relation_type,
                    weight = $weight,
                    directed = $directed,
                    confidence = $confidence,
                    source = $source,
                    position = $position,
                    metadata = $metadata,
                    content_category = $content_category,
                    created_at = <datetime> $created_at
                "#,
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(id.id)
    }

    async fn batch_store_relations(
        &self,
        relations: &[crucible_core::storage::Relation],
    ) -> StorageResult<()> {
        if relations.is_empty() {
            return Ok(());
        }

        // Convert core relations to SurrealDB relations
        let surreal_rels: Vec<SurrealRelation> = relations
            .iter()
            .map(|r| core_relation_to_surreal(r.clone()))
            .collect();

        // Build array of relation objects for batch insert
        let rels_array: Vec<serde_json::Value> = surreal_rels
            .iter()
            .map(|rel| {
                // INVARIANT: core_relation_to_surreal() may not generate ID if relation.id is empty
                // In that case, use a new UUID for the relation ID
                let rel_id = rel
                    .id
                    .as_ref()
                    .map(|id| id.id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                json!({
                    "rel_table": "relations",
                    "rel_id": rel_id,
                    "from_table": rel.from_id.table,
                    "from_id": rel.from_id.id,
                    "to_table": rel.to_id.table,
                    "to_id": rel.to_id.id,
                    "relation_type": rel.relation_type,
                    "weight": rel.weight,
                    "directed": rel.directed,
                    "confidence": rel.confidence,
                    "source": rel.source,
                    "position": rel.position,
                    "metadata": rel.metadata,
                    "created_at": rel.created_at.to_rfc3339(),
                })
            })
            .collect();

        // Execute batch INSERT using FOR loop
        self.client
            .query(
                r#"
                FOR $rel IN $relations {
                    CREATE type::thing($rel.rel_table, $rel.rel_id) SET
                        in = type::thing($rel.from_table, $rel.from_id),
                        out = type::thing($rel.to_table, $rel.to_id),
                        relation_type = $rel.relation_type,
                        weight = $rel.weight,
                        directed = $rel.directed,
                        confidence = $rel.confidence,
                        source = $rel.source,
                        position = $rel.position,
                        metadata = $rel.metadata,
                        created_at = <datetime> $rel.created_at
                };
                "#,
                &[json!({"relations": rels_array})],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn get_relation(
        &self,
        id: &str,
    ) -> StorageResult<Option<crucible_core::storage::Relation>> {
        let params = json!({"id": id});

        let result = self
            .client
            .query(
                "SELECT * FROM relations WHERE id = type::thing('relations', $id)",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let surreal_relation: SurrealRelation = serde_json::from_value(
            serde_json::to_value(&result.records[0].data)
                .map_err(|e| StorageError::Backend(e.to_string()))?,
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(Some(surreal_relation_to_core(surreal_relation)))
    }

    async fn get_relations(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<crucible_core::storage::Relation>> {
        // Strip the 'entities:' prefix if present to get just the ID part
        let clean_entity_id = entity_id.strip_prefix("entities:").unwrap_or(entity_id);

        // Use 'in' field (graph edge source) to find relations originating FROM this entity
        let (query, params) = if let Some(rel_type) = relation_type {
            (
                "SELECT * FROM relations WHERE in = type::thing('entities', $entity_id) AND relation_type = $relation_type",
                json!({
                    "entity_id": clean_entity_id,
                    "relation_type": rel_type,
                })
            )
        } else {
            (
                "SELECT * FROM relations WHERE in = type::thing('entities', $entity_id)",
                json!({
                    "entity_id": clean_entity_id,
                }),
            )
        };

        let result = self
            .client
            .query(query, &[params])
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        result
            .records
            .iter()
            .map(|record| {
                let surreal_relation: SurrealRelation = serde_json::from_value(
                    serde_json::to_value(&record.data)
                        .map_err(|e| StorageError::Backend(e.to_string()))?,
                )
                .map_err(|e| StorageError::Backend(e.to_string()))?;
                Ok(surreal_relation_to_core(surreal_relation))
            })
            .collect()
    }

    async fn get_backlinks(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<crucible_core::storage::Relation>> {
        // Strip the 'entities:' prefix if present to get just the ID part
        let clean_entity_id = entity_id.strip_prefix("entities:").unwrap_or(entity_id);

        // Use 'out' field (graph edge target) to find relations pointing TO this entity
        let (query, params) = if let Some(rel_type) = relation_type {
            (
                "SELECT * FROM relations WHERE out = type::thing('entities', $entity_id) AND relation_type = $relation_type",
                json!({
                    "entity_id": clean_entity_id,
                    "relation_type": rel_type,
                })
            )
        } else {
            (
                "SELECT * FROM relations WHERE out = type::thing('entities', $entity_id)",
                json!({
                    "entity_id": clean_entity_id,
                }),
            )
        };

        let result = self
            .client
            .query(query, &[params])
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        result
            .records
            .iter()
            .map(|record| {
                let surreal_relation: SurrealRelation = serde_json::from_value(
                    serde_json::to_value(&record.data)
                        .map_err(|e| StorageError::Backend(e.to_string()))?,
                )
                .map_err(|e| StorageError::Backend(e.to_string()))?;
                Ok(surreal_relation_to_core(surreal_relation))
            })
            .collect()
    }

    async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize> {
        let params = json!({"entity_id": entity_id});

        // Use 'in' field for source entity
        let result = self
            .client
            .query(
                "DELETE FROM relations WHERE in = type::thing('entities', $entity_id)",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(result.records.len())
    }

    async fn delete_relation(&self, id: &str) -> StorageResult<()> {
        let params = json!({"id": id});

        self.client
            .query(
                "DELETE FROM relations WHERE id = type::thing('relations', $id)",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn find_block_by_hash(
        &self,
        entity_id: &str,
        hash: &[u8; 32],
    ) -> StorageResult<Option<String>> {
        let hash_hex = hex::encode(hash);
        let params = json!({
            "entity_id": entity_id,
            "hash_hex": hash_hex,
        });

        let result = self
            .client
            .query(
                r#"
                SELECT * FROM relations
                WHERE to_id = type::thing('entities', $entity_id)
                AND metadata.block_hash = $hash_hex
                LIMIT 1
                "#,
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let surreal_relation: SurrealRelation = serde_json::from_value(
            serde_json::to_value(&result.records[0].data)
                .map_err(|e| StorageError::Backend(e.to_string()))?,
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;

        // Construct block ID from entity + offset
        let block_offset = surreal_relation.metadata["block_offset"]
            .as_u64()
            .unwrap_or(0) as u32;
        Ok(Some(format!("{}#block_{}", entity_id, block_offset)))
    }
}

fn thing_value<T>(id: &RecordId<T>) -> serde_json::Value {
    let thing = Thing::from((id.table.as_str(), id.id.as_str()));
    serde_json::to_value(thing).unwrap_or_else(|e| {
        tracing::error!(
            error = %e,
            table = %id.table,
            id = %id.id,
            "Thing serialization failed, using fallback"
        );
        json!({"tb": id.table, "id": id.id})
    })
}

// ============================================================================
// TagStorage Implementation
// ============================================================================

use super::adapter::{core_entity_tag_to_surreal, core_tag_to_surreal, surreal_tag_to_core};
use crucible_core::storage::TagStorage as CoreTagStorage;

impl EAVGraphStore {
    /// Recursively collect all descendant tag names (including the tag itself)
    ///
    /// Uses an iterative breadth-first approach to avoid stack overflow on deep hierarchies.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - The root tag name to start from
    ///
    /// # Returns
    ///
    /// Vector of all tag names in the subtree (including the root)
    ///
    /// # Example
    ///
    /// Given hierarchy:
    /// - project
    ///   - project/ai
    ///     - project/ai/nlp
    ///     - project/ai/ml
    ///   - project/web
    ///
    /// `collect_descendant_tag_names("project")` returns:
    /// ["project", "project/ai", "project/ai/nlp", "project/ai/ml", "project/web"]
    async fn collect_descendant_tag_names(&self, tag_id: &str) -> StorageResult<Vec<String>> {
        let mut all_tag_ids = Vec::new();
        let mut queue = std::collections::VecDeque::new();

        // Start with the requested tag
        queue.push_back(tag_id.to_string());
        all_tag_ids.push(tag_id.to_string());

        eprintln!("[DEBUG] Starting hierarchy collection for tag: {}", tag_id);

        // Breadth-first traversal to collect all descendants
        while let Some(current_tag_id) = queue.pop_front() {
            eprintln!("[DEBUG] Processing tag: {}", current_tag_id);

            // Get direct children of current tag
            let children = self.get_child_tags(&current_tag_id).await?;
            eprintln!(
                "[DEBUG] Found {} children for tag: {}",
                children.len(),
                current_tag_id
            );

            for child in children {
                eprintln!(
                    "[DEBUG] Child: id={}, name={}, parent={:?}",
                    child.id, child.name, child.parent_tag_id
                );
                // Add child to results and queue for processing
                all_tag_ids.push(child.name.clone());
                queue.push_back(child.name);
            }
        }

        eprintln!("[DEBUG] Final tag ID list: {:?}", all_tag_ids);
        Ok(all_tag_ids)
    }
}

#[async_trait]
impl CoreTagStorage for EAVGraphStore {
    async fn store_tag(&self, tag: crucible_core::storage::Tag) -> StorageResult<String> {
        // Use the tag ID as-is (with slashes) - we'll use backticks in the query
        let sanitized_id = tag.id.clone();

        // If tag has a parent, look it up by name to get the actual record ID
        let parent_record_id = if let Some(parent_tag_id) = &tag.parent_tag_id {
            // Strip "tags:" prefix if present
            let parent_name = if parent_tag_id.starts_with("tags:") {
                parent_tag_id.strip_prefix("tags:").unwrap_or(parent_tag_id)
            } else {
                parent_tag_id
            };

            // Look up parent tag to get its actual record ID
            let parent_tag = self.get_tag(parent_name).await?;
            parent_tag.map(|t| RecordId::new("tags", t.id))
        } else {
            None
        };

        // Create SurrealTag - we need to keep the parent record ID as a RecordId
        let mut surreal_tag = core_tag_to_surreal(tag, None);
        surreal_tag.parent_id = parent_record_id.clone();

        // Build parent_id clause for the query
        let (parent_clause, parent_params) = if let Some(parent) = &parent_record_id {
            (
                "parent_id = type::thing($parent_table, $parent_id),".to_string(),
                json!({
                    "parent_table": parent.table,
                    "parent_id": parent.id,
                }),
            )
        } else {
            ("parent_id = NONE,".to_string(), json!({}))
        };

        // Use raw SurrealQL UPSERT query with explicit record ID
        // Use backticks around the ID to allow slashes
        let query = format!(
            r#"
            UPSERT tags:`{}`
            SET
                name = $name,
                {}
                path = $path,
                depth = $depth,
                description = $description,
                color = $color,
                icon = $icon
            RETURN AFTER;
            "#,
            sanitized_id, parent_clause
        );

        let mut params = json!({
            "name": surreal_tag.name,
            "path": surreal_tag.path,
            "depth": surreal_tag.depth,
            "description": surreal_tag.description,
            "color": surreal_tag.color,
            "icon": surreal_tag.icon,
        });

        // Merge parent params if present
        if let Some(obj) = params.as_object_mut() {
            if let Some(parent_obj) = parent_params.as_object() {
                for (k, v) in parent_obj {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }

        self.client
            .query(&query, &[params])
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        // Return the tag ID (which is what's actually stored in the database)
        Ok(sanitized_id)
    }

    async fn get_tag(&self, name: &str) -> StorageResult<Option<crucible_core::storage::Tag>> {
        let params = json!({"name": name});
        let result = self
            .client
            .query(
                "SELECT *, meta::id(id) as record_id_str FROM tags WHERE name = $name",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if result.records.is_empty() {
            return Ok(None);
        }

        let mut data = result.records[0].data.clone();

        // Extract record_id_str and convert it to a RecordId for the id field
        if let Some(record_id_str) = data.get("record_id_str").and_then(|v| v.as_str()) {
            // Parse "project" or "tags:project" format
            let id_part = if record_id_str.contains(':') {
                record_id_str.split(':').last().unwrap_or(record_id_str)
            } else {
                record_id_str
            };

            // Create a proper RecordId JSON representation
            data.insert(
                "id".to_string(),
                json!({
                    "table": "tags",
                    "id": id_part
                }),
            );
        }

        // Convert HashMap to serde_json::Value
        let data_value =
            serde_json::to_value(&data).map_err(|e| StorageError::Backend(e.to_string()))?;

        let surreal_tag: SurrealTag =
            serde_json::from_value(data_value).map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(Some(surreal_tag_to_core(surreal_tag)))
    }

    async fn get_child_tags(
        &self,
        parent_tag_name: &str,
    ) -> StorageResult<Vec<crucible_core::storage::Tag>> {
        // First, get the parent tag to find its record ID
        let parent_tag = self.get_tag(parent_tag_name).await?;

        if parent_tag.is_none() {
            return Ok(Vec::new());
        }

        let parent_id = parent_tag.unwrap().id;

        let params = json!({"parent_id": parent_id});
        let result = self
            .client
            .query(
                "SELECT *, meta::id(id) as record_id_str FROM tags WHERE parent_id = type::thing('tags', $parent_id)",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let tags: Vec<SurrealTag> = result
            .records
            .iter()
            .map(|record| {
                let mut data = record.data.clone();

                // Extract record_id_str and convert it to a RecordId for the id field
                if let Some(record_id_str) = data.get("record_id_str").and_then(|v| v.as_str()) {
                    let id_part = if record_id_str.contains(':') {
                        record_id_str.split(':').last().unwrap_or(record_id_str)
                    } else {
                        record_id_str
                    };

                    data.insert(
                        "id".to_string(),
                        json!({
                            "table": "tags",
                            "id": id_part
                        }),
                    );
                }

                let data_value = serde_json::to_value(&data)
                    .map_err(|e| StorageError::Backend(e.to_string()))?;

                serde_json::from_value(data_value).map_err(|e| StorageError::Backend(e.to_string()))
            })
            .collect::<StorageResult<Vec<_>>>()?;

        Ok(tags.into_iter().map(surreal_tag_to_core).collect())
    }

    async fn associate_tag(
        &self,
        entity_tag: crucible_core::storage::EntityTag,
    ) -> StorageResult<()> {
        // Generate RecordId for the entity_tag
        let entity_tag_id = RecordId::new(
            "entity_tags",
            format!("{}:{}", entity_tag.entity_id, entity_tag.tag_id),
        );
        let surreal_entity_tag =
            core_entity_tag_to_surreal(entity_tag, Some(entity_tag_id.clone()));

        let params = json!({
            "table": entity_tag_id.table,
            "id": entity_tag_id.id,
            "entity_table": surreal_entity_tag.entity_id.table,
            "entity_id_value": surreal_entity_tag.entity_id.id,
            "tag_table": surreal_entity_tag.tag_id.table,
            "tag_id_value": surreal_entity_tag.tag_id.id,
            "source": surreal_entity_tag.source,
            "confidence": surreal_entity_tag.confidence,
        });

        self.client
            .query(
                r#"
                CREATE type::thing($table, $id) SET
                    entity_id = type::thing($entity_table, $entity_id_value),
                    tag_id = type::thing($tag_table, $tag_id_value),
                    source = $source,
                    confidence = $confidence
                "#,
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn get_entity_tags(
        &self,
        entity_id: &str,
    ) -> StorageResult<Vec<crucible_core::storage::Tag>> {
        // Strip the 'entities:' prefix if present to get just the ID part
        let clean_entity_id = entity_id.strip_prefix("entities:").unwrap_or(entity_id);

        let params = json!({"entity_id": clean_entity_id});

        // Query using type::thing() to properly match the record<entities> type
        let result = self
            .client
            .query(
                r#"
                SELECT tag_id FROM entity_tags
                WHERE entity_id = type::thing('entities', $entity_id)
                "#,
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if result.records.is_empty() {
            return Ok(Vec::new());
        }

        let mut tag_ids = Vec::new();
        for record in &result.records {
            if let Some(tag_id_value) = record.data.get("tag_id") {
                // tag_id can be stored as a string (e.g., "tags:project:ai:nlp")
                // or as an object/Thing. Handle both cases.
                if let Some(tag_id_str) = tag_id_value.as_str() {
                    tag_ids.push(tag_id_str.to_string());
                } else if let Some(tag_id_obj) = tag_id_value.as_object() {
                    // Handle {"table": "tags", "id": "project:ai:nlp"} format
                    if let (Some(table), Some(id)) = (tag_id_obj.get("table"), tag_id_obj.get("id"))
                    {
                        let table_str = table.as_str().unwrap_or("tags");
                        let id_str = id.as_str().unwrap_or("");
                        tag_ids.push(format!("{}:{}", table_str, id_str));
                    }
                    // Handle {"tb": "tags", "id": "project:ai:nlp"} format
                    else if let (Some(table), Some(id)) =
                        (tag_id_obj.get("tb"), tag_id_obj.get("id"))
                    {
                        let table_str = table.as_str().unwrap_or("tags");
                        // Handle nested ID formats like {"String": "project:ai:nlp"}
                        let id_str = if let Some(id_str) = id.as_str() {
                            id_str
                        } else if let Some(id_obj) = id.as_object() {
                            id_obj.get("String").and_then(|v| v.as_str()).unwrap_or("")
                        } else {
                            ""
                        };
                        tag_ids.push(format!("{}:{}", table_str, id_str));
                    }
                }
            }
        }

        if tag_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Now fetch all tags individually (SurrealDB doesn't support IN queries with type::thing)
        // Tag IDs are in format "tags:project:ai:nlp", need to query using just "project:ai:nlp"
        let tag_query_ids: Vec<String> = tag_ids
            .iter()
            .map(|id| {
                if id.starts_with("tags:") {
                    id.strip_prefix("tags:").unwrap_or(id).to_string()
                } else {
                    id.clone()
                }
            })
            .collect();

        // Fetch each tag individually
        let mut all_tags = Vec::new();
        for tag_id in tag_query_ids {
            if let Some(tag) = self.get_tag(&tag_id).await? {
                all_tags.push(tag);
            }
        }

        Ok(all_tags)
    }

    /// Get all entities tagged with the specified tag OR any of its descendant tags
    ///
    /// This implements hierarchical tag search: searching for "project" will also
    /// find entities tagged with "project/ai", "project/ai/nlp", etc.
    ///
    /// # Arguments
    ///
    /// * `tag_id` - The tag ID to search for (e.g., "project", "project/ai")
    ///
    /// # Returns
    ///
    /// Vector of entity IDs that have the tag or any descendant tags
    ///
    /// # Example
    ///
    /// Given hierarchy: project -> project/ai -> project/ai/nlp
    /// - `get_entities_by_tag("project")` returns entities with project, project/ai, or project/ai/nlp
    /// - `get_entities_by_tag("project/ai")` returns entities with project/ai or project/ai/nlp
    /// - `get_entities_by_tag("project/ai/nlp")` returns only entities with project/ai/nlp
    async fn get_entities_by_tag(&self, tag_id: &str) -> StorageResult<Vec<String>> {
        // Collect all descendant tag names (including the tag itself)
        let all_tag_names = self.collect_descendant_tag_names(tag_id).await?;

        if all_tag_names.is_empty() {
            return Ok(Vec::new());
        }

        // Query for entities with ANY of the collected tag names
        // Use separate queries for each tag to work around SurrealDB's type::thing limitations with arrays
        let mut all_entity_ids = std::collections::HashSet::new();

        for tag_name_to_query in &all_tag_names {
            // Use the tag name as-is (with slashes)
            let params = json!({"tag_id": tag_name_to_query});
            let result = self
                .client
                .query(
                    r#"
                    SELECT entity_id FROM entity_tags
                    WHERE tag_id = type::thing('tags', $tag_id)
                    "#,
                    &[params],
                )
                .await
                .map_err(|e| StorageError::Backend(e.to_string()))?;

            for record in &result.records {
                if let Some(entity_id) = record
                    .data
                    .get("entity_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                {
                    all_entity_ids.insert(entity_id);
                }
            }
        }

        Ok(all_entity_ids.into_iter().collect())
    }

    async fn dissociate_tag(&self, entity_id: &str, tag_id: &str) -> StorageResult<()> {
        let params = json!({
            "entity_id": entity_id,
            "tag_id": tag_id,
        });

        self.client
            .query(
                r#"
                DELETE FROM entity_tags
                WHERE entity_id = type::thing('entities', $entity_id)
                AND tag_id = type::thing('tags', $tag_id)
                "#,
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(())
    }

    async fn delete_tag(&self, id: &str, delete_associations: bool) -> StorageResult<usize> {
        let params = json!({"id": id});

        if delete_associations {
            // First delete all entity_tag associations
            self.client
                .query(
                    "DELETE FROM entity_tags WHERE tag_id = type::thing('tags', $id)",
                    &[params.clone()],
                )
                .await
                .map_err(|e| StorageError::Backend(e.to_string()))?;
        }

        // Then delete the tag itself
        let result = self
            .client
            .query(
                "DELETE FROM tags WHERE id = type::thing('tags', $id)",
                &[params],
            )
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(result.records.len())
    }
}

// -- Tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eav_graph::apply_eav_graph_schema;
    use crate::eav_graph::types::{EntityType, PropertyValue};
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
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        let entity = sample_entity();
        store.upsert_entity(&entity).await.unwrap();

        let property = Property::new(
            property_id("title"),
            entity_id(),
            "core",
            "title",
            PropertyValue::Text("Sample".to_string()),
        );

        store.upsert_property(&property).await.unwrap();

        let result = client
            .query("SELECT * FROM properties WHERE key = 'title'", &[])
            .await
            .unwrap();

        assert_eq!(result.records.len(), 1);
        let record = &result.records[0];

        // Verify the value is stored as JSON with the PropertyValue structure (tagged enum)
        let value = record.data.get("value").unwrap();
        assert!(value.is_object());
        // PropertyValue uses tagged enum serialization: {"type": "text", "value": "Sample"}
        assert_eq!(value.get("type").unwrap().as_str(), Some("text"));
        assert_eq!(value.get("value").unwrap().as_str(), Some("Sample"));
    }

    #[tokio::test]
    async fn replace_blocks_writes_rows() {
        let client = SurrealClient::new_isolated_memory().await.unwrap();
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
        let client = SurrealClient::new_isolated_memory().await.unwrap();
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

    // ============================================================================
    // Hierarchical Tag Search Tests
    // ============================================================================

    use crucible_core::storage::{EntityTag, Tag};

    /// Helper to create a tag with proper structure
    fn create_tag(id: &str, name: &str, parent_id: Option<&str>) -> Tag {
        Tag {
            id: id.to_string(),
            name: name.to_string(),
            parent_tag_id: parent_id.map(|p| format!("tags:{}", p)),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Helper to associate an entity with a tag
    fn create_entity_tag(entity_id: &str, tag_id: &str) -> EntityTag {
        EntityTag {
            entity_id: entity_id.to_string(),
            tag_id: tag_id.to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_root_tag() {
        // Test: Searching for root tag returns all descendants
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create tag hierarchy: project -> project/ai -> project/ai/nlp
        let tag_project = create_tag("project", "project", None);
        let tag_project_ai = create_tag("project/ai", "project/ai", Some("project"));
        let tag_project_ai_nlp = create_tag("project/ai/nlp", "project/ai/nlp", Some("project/ai"));

        store.store_tag(tag_project).await.unwrap();
        store.store_tag(tag_project_ai).await.unwrap();
        store.store_tag(tag_project_ai_nlp).await.unwrap();

        // Create entities and tag them at different levels
        let entity1_id = RecordId::new("entities", "note:entity1");
        let entity2_id = RecordId::new("entities", "note:entity2");
        let entity3_id = RecordId::new("entities", "note:entity3");

        let entity1 = Entity::new(entity1_id.clone(), EntityType::Note);
        let entity2 = Entity::new(entity2_id.clone(), EntityType::Note);
        let entity3 = Entity::new(entity3_id.clone(), EntityType::Note);

        store.upsert_entity(&entity1).await.unwrap();
        store.upsert_entity(&entity2).await.unwrap();
        store.upsert_entity(&entity3).await.unwrap();

        // Entity1 tagged with "project"
        store
            .associate_tag(create_entity_tag("note:entity1", "project"))
            .await
            .unwrap();
        // Entity2 tagged with "project/ai"
        store
            .associate_tag(create_entity_tag("note:entity2", "project/ai"))
            .await
            .unwrap();
        // Entity3 tagged with "project/ai/nlp"
        store
            .associate_tag(create_entity_tag("note:entity3", "project/ai/nlp"))
            .await
            .unwrap();

        // Search for root tag "project" should return all 3 entities
        let entities = store.get_entities_by_tag("project").await.unwrap();
        assert_eq!(
            entities.len(),
            3,
            "Searching for 'project' should return all entities in the hierarchy"
        );

        // Verify all entities are present (order doesn't matter)
        assert!(entities.contains(&"entities:note:entity1".to_string()));
        assert!(entities.contains(&"entities:note:entity2".to_string()));
        assert!(entities.contains(&"entities:note:entity3".to_string()));
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_mid_level_tag() {
        // Test: Searching for mid-level tag returns its subtree
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create tag hierarchy
        let tag_project = create_tag("project", "project", None);
        let tag_project_ai = create_tag("project/ai", "project/ai", Some("project"));
        let tag_project_ai_nlp = create_tag("project/ai/nlp", "project/ai/nlp", Some("project/ai"));
        let tag_project_web = create_tag("project/web", "project/web", Some("project"));

        store.store_tag(tag_project).await.unwrap();
        store.store_tag(tag_project_ai).await.unwrap();
        store.store_tag(tag_project_ai_nlp).await.unwrap();
        store.store_tag(tag_project_web).await.unwrap();

        // Create entities
        let entity1_id = RecordId::new("entities", "note:entity1");
        let entity2_id = RecordId::new("entities", "note:entity2");
        let entity3_id = RecordId::new("entities", "note:entity3");
        let entity4_id = RecordId::new("entities", "note:entity4");

        store
            .upsert_entity(&Entity::new(entity1_id.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(entity2_id.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(entity3_id.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(entity4_id.clone(), EntityType::Note))
            .await
            .unwrap();

        // Tag entities
        store
            .associate_tag(create_entity_tag("note:entity1", "project"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:entity2", "project/ai"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:entity3", "project/ai/nlp"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:entity4", "project/web"))
            .await
            .unwrap();

        // Search for "project/ai" should return entity2 and entity3, but NOT entity1 or entity4
        let entities = store.get_entities_by_tag("project/ai").await.unwrap();
        assert_eq!(
            entities.len(),
            2,
            "Searching for 'project/ai' should return only AI subtree"
        );

        assert!(entities.contains(&"entities:note:entity2".to_string()));
        assert!(entities.contains(&"entities:note:entity3".to_string()));
        assert!(!entities.contains(&"entities:note:entity1".to_string()));
        assert!(!entities.contains(&"entities:note:entity4".to_string()));
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_leaf_tag() {
        // Test: Searching for leaf tag returns only exact matches
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create tag hierarchy
        let tag_project = create_tag("project", "project", None);
        let tag_project_ai = create_tag("project/ai", "project/ai", Some("project"));
        let tag_project_ai_nlp = create_tag("project/ai/nlp", "project/ai/nlp", Some("project/ai"));

        store.store_tag(tag_project).await.unwrap();
        store.store_tag(tag_project_ai).await.unwrap();
        store.store_tag(tag_project_ai_nlp).await.unwrap();

        // Create entities
        let entity1_id = RecordId::new("entities", "note:entity1");
        let entity2_id = RecordId::new("entities", "note:entity2");
        let entity3_id = RecordId::new("entities", "note:entity3");

        store
            .upsert_entity(&Entity::new(entity1_id.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(entity2_id.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(entity3_id.clone(), EntityType::Note))
            .await
            .unwrap();

        // Tag entities
        store
            .associate_tag(create_entity_tag("note:entity1", "project"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:entity2", "project/ai"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:entity3", "project/ai/nlp"))
            .await
            .unwrap();

        // Search for leaf tag "project/ai/nlp" should return only entity3
        let entities = store.get_entities_by_tag("project/ai/nlp").await.unwrap();
        assert_eq!(
            entities.len(),
            1,
            "Searching for leaf tag should return only exact matches"
        );
        assert!(entities.contains(&"entities:note:entity3".to_string()));
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_deep_hierarchy() {
        // Test: Deep hierarchies (3+ levels) work correctly
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create 4-level deep hierarchy
        let tag1 = create_tag("a", "a", None);
        let tag2 = create_tag("a/b", "a/b", Some("a"));
        let tag3 = create_tag("a/b/c", "a/b/c", Some("a/b"));
        let tag4 = create_tag("a/b/c/d", "a/b/c/d", Some("a/b/c"));

        store.store_tag(tag1).await.unwrap();
        store.store_tag(tag2).await.unwrap();
        store.store_tag(tag3).await.unwrap();
        store.store_tag(tag4).await.unwrap();

        // Create entities at each level
        let e1 = RecordId::new("entities", "note:e1");
        let e2 = RecordId::new("entities", "note:e2");
        let e3 = RecordId::new("entities", "note:e3");
        let e4 = RecordId::new("entities", "note:e4");

        store
            .upsert_entity(&Entity::new(e1.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(e2.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(e3.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(e4.clone(), EntityType::Note))
            .await
            .unwrap();

        store
            .associate_tag(create_entity_tag("note:e1", "a"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e2", "a/b"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e3", "a/b/c"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e4", "a/b/c/d"))
            .await
            .unwrap();

        // Search from root should return all 4
        let entities = store.get_entities_by_tag("a").await.unwrap();
        assert_eq!(entities.len(), 4, "Root search should return all 4 levels");

        // Search from level 2 should return 3 entities (b, c, d)
        let entities = store.get_entities_by_tag("a/b").await.unwrap();
        assert_eq!(entities.len(), 3, "Level 2 search should return 3 entities");

        // Search from level 3 should return 2 entities (c, d)
        let entities = store.get_entities_by_tag("a/b/c").await.unwrap();
        assert_eq!(entities.len(), 2, "Level 3 search should return 2 entities");

        // Search from level 4 should return 1 entity (d)
        let entities = store.get_entities_by_tag("a/b/c/d").await.unwrap();
        assert_eq!(entities.len(), 1, "Leaf search should return 1 entity");
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_multiple_entities_same_tag() {
        // Test: Multiple entities with the same tag are all returned
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create simple hierarchy
        let tag_project = create_tag("project", "project", None);
        let tag_project_ai = create_tag("project/ai", "project/ai", Some("project"));

        store.store_tag(tag_project).await.unwrap();
        store.store_tag(tag_project_ai).await.unwrap();

        // Create multiple entities with same tag
        let e1 = RecordId::new("entities", "note:e1");
        let e2 = RecordId::new("entities", "note:e2");
        let e3 = RecordId::new("entities", "note:e3");

        store
            .upsert_entity(&Entity::new(e1.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(e2.clone(), EntityType::Note))
            .await
            .unwrap();
        store
            .upsert_entity(&Entity::new(e3.clone(), EntityType::Note))
            .await
            .unwrap();

        // All tagged with same nested tag
        store
            .associate_tag(create_entity_tag("note:e1", "project/ai"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e2", "project/ai"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e3", "project/ai"))
            .await
            .unwrap();

        // Search for parent should return all 3
        let entities = store.get_entities_by_tag("project").await.unwrap();
        assert_eq!(
            entities.len(),
            3,
            "Should return all entities tagged with descendant"
        );

        // Search for exact tag should also return all 3
        let entities = store.get_entities_by_tag("project/ai").await.unwrap();
        assert_eq!(
            entities.len(),
            3,
            "Should return all entities with exact tag match"
        );
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_nonexistent_tag() {
        // Test: Searching for non-existent tag returns empty vector
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        let entities = store.get_entities_by_tag("nonexistent").await.unwrap();
        assert_eq!(
            entities.len(),
            0,
            "Non-existent tag should return empty results"
        );
    }

    #[tokio::test]
    async fn test_hierarchical_tag_search_with_branching() {
        // Test: Complex branching hierarchy
        // Structure:
        //   project
        //   ├── project/ai
        //   │   ├── project/ai/nlp
        //   │   └── project/ai/ml
        //   └── project/web
        //       └── project/web/frontend
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());

        // Create tag hierarchy
        store
            .store_tag(create_tag("project", "project", None))
            .await
            .unwrap();
        store
            .store_tag(create_tag("project/ai", "project/ai", Some("project")))
            .await
            .unwrap();
        store
            .store_tag(create_tag(
                "project/ai/nlp",
                "project/ai/nlp",
                Some("project/ai"),
            ))
            .await
            .unwrap();
        store
            .store_tag(create_tag(
                "project/ai/ml",
                "project/ai/ml",
                Some("project/ai"),
            ))
            .await
            .unwrap();
        store
            .store_tag(create_tag("project/web", "project/web", Some("project")))
            .await
            .unwrap();
        store
            .store_tag(create_tag(
                "project/web/frontend",
                "project/web/frontend",
                Some("project/web"),
            ))
            .await
            .unwrap();

        // Create entities
        let e1 = Entity::new(RecordId::new("entities", "note:e1"), EntityType::Note);
        let e2 = Entity::new(RecordId::new("entities", "note:e2"), EntityType::Note);
        let e3 = Entity::new(RecordId::new("entities", "note:e3"), EntityType::Note);
        let e4 = Entity::new(RecordId::new("entities", "note:e4"), EntityType::Note);

        store.upsert_entity(&e1).await.unwrap();
        store.upsert_entity(&e2).await.unwrap();
        store.upsert_entity(&e3).await.unwrap();
        store.upsert_entity(&e4).await.unwrap();

        // Tag entities in different branches
        store
            .associate_tag(create_entity_tag("note:e1", "project/ai/nlp"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e2", "project/ai/ml"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e3", "project/web"))
            .await
            .unwrap();
        store
            .associate_tag(create_entity_tag("note:e4", "project/web/frontend"))
            .await
            .unwrap();

        // Search from root should return all 4
        let entities = store.get_entities_by_tag("project").await.unwrap();
        assert_eq!(entities.len(), 4, "Root search should return all branches");

        // Search for "project/ai" should return only AI branch (e1, e2)
        let entities = store.get_entities_by_tag("project/ai").await.unwrap();
        assert_eq!(entities.len(), 2, "AI branch should return 2 entities");
        assert!(entities.contains(&"entities:note:e1".to_string()));
        assert!(entities.contains(&"entities:note:e2".to_string()));

        // Search for "project/web" should return only web branch (e3, e4)
        let entities = store.get_entities_by_tag("project/web").await.unwrap();
        assert_eq!(entities.len(), 2, "Web branch should return 2 entities");
        assert!(entities.contains(&"entities:note:e3".to_string()));
        assert!(entities.contains(&"entities:note:e4".to_string()));
    }
}
