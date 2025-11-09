//! Adapter between crucible-core and SurrealDB EAV+Graph types
//!
//! This module provides conversion functions between the database-agnostic
//! core types and the SurrealDB-specific types that use RecordId<T>.

use crucible_core::storage as core;

use super::types::{
    EntityRecord, Property as SurrealProperty, PropertyNamespace as SurrealPropertyNamespace,
    PropertyRecord, RecordId,
};

// ============================================================================
// ID Conversion
// ============================================================================

/// Convert a String ID to a RecordId<EntityRecord>
///
/// # Arguments
///
/// * `id` - String ID (e.g., "note:test123" or "entities:note:test123")
///
/// # Returns
///
/// RecordId with "entities" table and the provided ID
///
/// # Examples
///
/// - "note:test123" → RecordId { table: "entities", id: "note:test123" }
/// - "entities:note:test123" → RecordId { table: "entities", id: "note:test123" }
pub fn string_to_entity_id(id: &str) -> RecordId<EntityRecord> {
    // If ID already has "entities:" prefix, extract just the ID part
    let id_part = if id.starts_with("entities:") {
        id.strip_prefix("entities:").unwrap_or(id)
    } else {
        id
    };

    RecordId::new("entities", id_part)
}

/// Convert a RecordId<EntityRecord> to a String ID
///
/// # Arguments
///
/// * `record_id` - The RecordId to convert
///
/// # Returns
///
/// String in format "table:id"
pub fn entity_id_to_string<T>(record_id: &RecordId<T>) -> String {
    format!("{}:{}", record_id.table, record_id.id)
}

// ============================================================================
// Property Conversion
// ============================================================================

/// Convert core Property to SurrealDB Property
///
/// # Arguments
///
/// * `core_prop` - Property from crucible-core
/// * `property_id` - Optional RecordId for the property (if None, generates one)
///
/// # Returns
///
/// SurrealDB Property with RecordId entity_id
pub fn core_property_to_surreal(
    core_prop: core::Property,
    property_id: Option<RecordId<PropertyRecord>>,
) -> SurrealProperty {
    let entity_id = string_to_entity_id(&core_prop.entity_id);

    SurrealProperty {
        id: property_id,
        entity_id,
        namespace: SurrealPropertyNamespace(core_prop.namespace.0.into()),
        key: core_prop.key,
        value: core_prop.value,
        source: "parser".to_string(),
        confidence: 1.0,
        created_at: core_prop.created_at,
        updated_at: core_prop.updated_at,
    }
}

/// Convert SurrealDB Property to core Property
///
/// # Arguments
///
/// * `surreal_prop` - Property from SurrealDB
///
/// # Returns
///
/// Core Property with String entity_id
pub fn surreal_property_to_core(surreal_prop: SurrealProperty) -> core::Property {
    core::Property {
        entity_id: entity_id_to_string(&surreal_prop.entity_id),
        namespace: core::PropertyNamespace(surreal_prop.namespace.0.into()),
        key: surreal_prop.key,
        value: surreal_prop.value,
        created_at: surreal_prop.created_at,
        updated_at: surreal_prop.updated_at,
    }
}

/// Batch convert core Properties to SurrealDB Properties
///
/// # Arguments
///
/// * `core_props` - Vector of properties from crucible-core
///
/// # Returns
///
/// Vector of SurrealDB Properties with generated IDs
pub fn core_properties_to_surreal(core_props: Vec<core::Property>) -> Vec<SurrealProperty> {
    core_props
        .into_iter()
        .map(|prop| {
            // Generate a property ID: entities:note:test:frontmatter:title
            let prop_id = RecordId::new(
                "properties",
                format!("{}:{}:{}", prop.entity_id, prop.namespace.0.as_ref(), prop.key),
            );
            core_property_to_surreal(prop, Some(prop_id))
        })
        .collect()
}

/// Batch convert SurrealDB Properties to core Properties
///
/// # Arguments
///
/// * `surreal_props` - Vector of properties from SurrealDB
///
/// # Returns
///
/// Vector of core Properties
pub fn surreal_properties_to_core(surreal_props: Vec<SurrealProperty>) -> Vec<core::Property> {
    surreal_props
        .into_iter()
        .map(surreal_property_to_core)
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_string_to_entity_id_simple() {
        let id = string_to_entity_id("note:test123");
        assert_eq!(id.table, "entities");
        assert_eq!(id.id, "note:test123");
    }

    #[test]
    fn test_string_to_entity_id_without_prefix() {
        let id = string_to_entity_id("test123");
        assert_eq!(id.table, "entities");
        assert_eq!(id.id, "test123");
    }

    #[test]
    fn test_entity_id_to_string() {
        let record_id = RecordId::<EntityRecord>::new("entities", "note:test123");
        let string_id = entity_id_to_string(&record_id);
        assert_eq!(string_id, "entities:note:test123");
    }

    #[test]
    fn test_round_trip_entity_id() {
        let original = "note:test123";
        let record_id = string_to_entity_id(original);
        let result = entity_id_to_string(&record_id);
        assert_eq!(result, "entities:note:test123");
    }

    #[test]
    fn test_core_property_to_surreal() {
        let now = Utc::now();
        let core_prop = core::Property {
            entity_id: "note:test".to_string(),
            namespace: core::PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: core::PropertyValue::Text("Test Note".to_string()),
            created_at: now,
            updated_at: now,
        };

        let surreal_prop = core_property_to_surreal(core_prop.clone(), None);

        assert_eq!(surreal_prop.entity_id.table, "entities");
        assert_eq!(surreal_prop.entity_id.id, "note:test");
        assert_eq!(surreal_prop.namespace.0.as_str(), "frontmatter");
        assert_eq!(surreal_prop.key, "title");
        assert_eq!(
            surreal_prop.value,
            core::PropertyValue::Text("Test Note".to_string())
        );
    }

    #[test]
    fn test_surreal_property_to_core() {
        let now = Utc::now();
        let surreal_prop = SurrealProperty {
            id: Some(RecordId::new("properties", "prop1")),
            entity_id: RecordId::new("entities", "note:test"),
            namespace: SurrealPropertyNamespace("frontmatter".to_string()),
            key: "author".to_string(),
            value: core::PropertyValue::Text("John Doe".to_string()),
            source: "parser".to_string(),
            confidence: 1.0,
            created_at: now,
            updated_at: now,
        };

        let core_prop = surreal_property_to_core(surreal_prop);

        assert_eq!(core_prop.entity_id, "entities:note:test");
        assert_eq!(core_prop.namespace.0.as_ref(), "frontmatter");
        assert_eq!(core_prop.key, "author");
        assert_eq!(
            core_prop.value,
            core::PropertyValue::Text("John Doe".to_string())
        );
    }

    #[test]
    fn test_round_trip_property() {
        let now = Utc::now();
        let original = core::Property {
            entity_id: "note:test".to_string(),
            namespace: core::PropertyNamespace::frontmatter(),
            key: "count".to_string(),
            value: core::PropertyValue::Number(42.0),
            created_at: now,
            updated_at: now,
        };

        let surreal = core_property_to_surreal(original.clone(), None);
        let result = surreal_property_to_core(surreal);

        assert_eq!(result.entity_id, "entities:note:test");
        assert_eq!(result.namespace.0.as_ref(), original.namespace.0.as_ref());
        assert_eq!(result.key, original.key);
        assert_eq!(result.value, original.value);
    }

    #[test]
    fn test_batch_conversion() {
        let now = Utc::now();
        let core_props = vec![
            core::Property {
                entity_id: "note:test".to_string(),
                namespace: core::PropertyNamespace::frontmatter(),
                key: "title".to_string(),
                value: core::PropertyValue::Text("Test".to_string()),
                created_at: now,
                updated_at: now,
            },
            core::Property {
                entity_id: "note:test".to_string(),
                namespace: core::PropertyNamespace::frontmatter(),
                key: "count".to_string(),
                value: core::PropertyValue::Number(42.0),
                created_at: now,
                updated_at: now,
            },
        ];

        let surreal_props = core_properties_to_surreal(core_props.clone());
        assert_eq!(surreal_props.len(), 2);

        let result_props = surreal_properties_to_core(surreal_props);
        assert_eq!(result_props.len(), 2);

        // Check first property
        assert_eq!(result_props[0].key, "title");
        assert_eq!(
            result_props[0].value,
            core::PropertyValue::Text("Test".to_string())
        );

        // Check second property
        assert_eq!(result_props[1].key, "count");
        assert_eq!(result_props[1].value, core::PropertyValue::Number(42.0));
    }
}

// ============================================================================
// Relation Conversion
// ============================================================================

use super::types::{
    EntityTag as SurrealEntityTag, EntityTagRecord, Relation as SurrealRelation,
    Tag as SurrealTag, TagRecord,
};

/// Convert core Relation to SurrealDB Relation
///
/// Maps the database-agnostic core Relation type to the SurrealDB-specific
/// type with RecordId fields. Block link fields (offset, hash, occurrence)
/// are stored in metadata.
pub fn core_relation_to_surreal(relation: core::Relation) -> SurrealRelation {
    // Store block link fields in metadata
    let mut metadata = relation.metadata.clone();
    if let Some(offset) = relation.block_offset {
        metadata["block_offset"] = serde_json::json!(offset);
    }
    if let Some(hash) = relation.block_hash {
        // Store hash as hex string in metadata
        metadata["block_hash"] = serde_json::json!(hex::encode(hash));
    }
    if let Some(occurrence) = relation.heading_occurrence {
        metadata["heading_occurrence"] = serde_json::json!(occurrence);
    }
    if let Some(context) = relation.context {
        metadata["context"] = serde_json::json!(context);
    }

    SurrealRelation {
        id: if !relation.id.is_empty() {
            Some(RecordId::new("relations", relation.id))
        } else {
            None
        },
        from_id: string_to_entity_id(&relation.from_entity_id),
        to_id: relation
            .to_entity_id
            .as_ref()
            .map(|id| string_to_entity_id(id))
            .unwrap_or_else(|| RecordId::new("entities", "unresolved")),
        relation_type: relation.relation_type,
        weight: 1.0,
        directed: true,
        confidence: 1.0,
        source: "parser".to_string(),
        position: None,
        metadata,
        created_at: relation.created_at,
    }
}

/// Convert SurrealDB Relation to core Relation
///
/// Maps the SurrealDB-specific Relation type back to the database-agnostic
/// core type. Extracts block link fields from metadata.
pub fn surreal_relation_to_core(surreal: SurrealRelation) -> core::Relation {
    // Extract block link fields from metadata
    let block_offset = surreal.metadata["block_offset"].as_u64().map(|v| v as u32);
    let block_hash = surreal.metadata["block_hash"]
        .as_str()
        .and_then(|hex_str| {
            hex::decode(hex_str)
                .ok()
                .and_then(|bytes| {
                    if bytes.len() == 32 {
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&bytes);
                        Some(hash)
                    } else {
                        None
                    }
                })
        });
    let heading_occurrence = surreal.metadata["heading_occurrence"]
        .as_u64()
        .map(|v| v as u32);
    let context = surreal.metadata["context"]
        .as_str()
        .map(|s| s.to_string());

    core::Relation {
        id: surreal
            .id
            .map(|id| id.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        from_entity_id: entity_id_to_string(&surreal.from_id),
        to_entity_id: if surreal.to_id.id == "unresolved" {
            None
        } else {
            Some(entity_id_to_string(&surreal.to_id))
        },
        relation_type: surreal.relation_type,
        metadata: surreal.metadata,
        context,
        block_offset,
        block_hash,
        heading_occurrence,
        created_at: surreal.created_at,
    }
}

#[cfg(test)]
mod relation_conversion_tests {
    use super::*;

    #[test]
    fn test_core_relation_to_surreal_basic() {
        let relation = core::Relation::wikilink("note:source", "note:target");

        let surreal = core_relation_to_surreal(relation);

        assert_eq!(surreal.from_id.id, "note:source");
        assert_eq!(surreal.to_id.id, "note:target");
        assert_eq!(surreal.relation_type, "wikilink");
    }

    #[test]
    fn test_core_relation_to_surreal_with_block_link() {
        let hash = [42u8; 32];
        let relation = core::Relation::wikilink("note:source", "note:target")
            .with_block_link(5, hash, Some(2))
            .with_context("Block 5 context");

        let surreal = core_relation_to_surreal(relation);

        assert_eq!(surreal.metadata["block_offset"], 5);
        assert_eq!(
            surreal.metadata["block_hash"].as_str().unwrap(),
            hex::encode(hash)
        );
        assert_eq!(surreal.metadata["heading_occurrence"], 2);
        assert_eq!(surreal.metadata["context"], "Block 5 context");
    }

    #[test]
    fn test_surreal_relation_to_core() {
        let surreal = SurrealRelation {
            id: Some(RecordId::new("relations", "rel:123")),
            from_id: RecordId::new("entities", "note:source"),
            to_id: RecordId::new("entities", "note:target"),
            relation_type: "embed".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "parser".to_string(),
            position: None,
            metadata: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
        };

        let core_rel = surreal_relation_to_core(surreal);

        assert_eq!(core_rel.from_entity_id, "entities:note:source");
        assert_eq!(core_rel.to_entity_id, Some("entities:note:target".to_string()));
        assert_eq!(core_rel.relation_type, "embed");
    }

    #[test]
    fn test_round_trip_relation_with_block_link() {
        let hash = [99u8; 32];
        let original = core::Relation::wikilink("note:a", "note:b")
            .with_block_link(3, hash, Some(1))
            .with_context("Context text");

        let surreal = core_relation_to_surreal(original.clone());
        let result = surreal_relation_to_core(surreal);

        // IDs will have "entities:" prefix after round-trip
        assert_eq!(result.from_entity_id, "entities:note:a");
        assert_eq!(result.to_entity_id, Some("entities:note:b".to_string()));
        assert_eq!(result.relation_type, original.relation_type);
        assert_eq!(result.block_offset, Some(3));
        assert_eq!(result.block_hash, Some(hash));
        assert_eq!(result.heading_occurrence, Some(1));
        assert_eq!(result.context, Some("Context text".to_string()));
    }

    #[test]
    fn test_unresolved_target() {
        let relation = core::Relation::new("note:source", None, "wikilink");

        let surreal = core_relation_to_surreal(relation);
        assert_eq!(surreal.to_id.id, "unresolved");

        let core_rel = surreal_relation_to_core(surreal);
        assert_eq!(core_rel.to_entity_id, None);
    }
}

// ============================================================================
// Tag Conversion
// ============================================================================

/// Convert core Tag to SurrealDB Tag
///
/// Maps the database-agnostic core Tag type to the SurrealDB-specific
/// type with RecordId fields and additional metadata (path, depth).
pub fn core_tag_to_surreal(
    tag: core::Tag,
    tag_id: Option<RecordId<TagRecord>>,
) -> SurrealTag {
    // Parse the tag name to build hierarchical path
    let parts: Vec<&str> = tag.name.split('/').collect();
    let depth = parts.len() as i32;
    let path = tag.name.clone();

    SurrealTag {
        id: tag_id,
        name: tag.name,
        parent_id: tag.parent_tag_id.as_ref().map(|id| {
            // Extract just the ID part if it has "tags:" prefix
            let id_part = if id.starts_with("tags:") {
                id.strip_prefix("tags:").unwrap_or(id)
            } else {
                id
            };
            RecordId::new("tags", id_part)
        }),
        path,
        depth,
        description: None,
        color: None,
        icon: None,
    }
}

/// Convert SurrealDB Tag to core Tag
///
/// Maps the SurrealDB-specific Tag type back to the database-agnostic
/// core type. Additional metadata fields (description, color, icon) are lost.
pub fn surreal_tag_to_core(surreal: SurrealTag) -> core::Tag {
    core::Tag {
        id: surreal
            .id
            .map(|id| id.id)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        name: surreal.name,
        parent_tag_id: surreal.parent_id.map(|id| format!("{}:{}", id.table, id.id)),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Convert core EntityTag to SurrealDB EntityTag
pub fn core_entity_tag_to_surreal(
    entity_tag: core::EntityTag,
    entity_tag_id: Option<RecordId<EntityTagRecord>>,
) -> SurrealEntityTag {
    SurrealEntityTag {
        id: entity_tag_id,
        entity_id: string_to_entity_id(&entity_tag.entity_id),
        tag_id: {
            let id_part = if entity_tag.tag_id.starts_with("tags:") {
                entity_tag.tag_id.strip_prefix("tags:").unwrap_or(&entity_tag.tag_id)
            } else {
                &entity_tag.tag_id
            };
            RecordId::new("tags", id_part)
        },
        source: "parser".to_string(),
        confidence: 1.0,
    }
}

/// Convert SurrealDB EntityTag to core EntityTag
pub fn surreal_entity_tag_to_core(surreal: SurrealEntityTag) -> core::EntityTag {
    core::EntityTag {
        entity_id: entity_id_to_string(&surreal.entity_id),
        tag_id: format!("{}:{}", surreal.tag_id.table, surreal.tag_id.id),
        created_at: chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tag_conversion_tests {
    use super::*;

    #[test]
    fn test_core_tag_to_surreal_simple() {
        let now = chrono::Utc::now();
        let tag = core::Tag {
            id: "tag1".to_string(),
            name: "project".to_string(),
            parent_tag_id: None,
            created_at: now,
            updated_at: now,
        };

        let surreal = core_tag_to_surreal(tag.clone(), None);

        assert_eq!(surreal.name, "project");
        assert_eq!(surreal.path, "project");
        assert_eq!(surreal.depth, 1);
        assert!(surreal.parent_id.is_none());
    }

    #[test]
    fn test_core_tag_to_surreal_hierarchical() {
        let now = chrono::Utc::now();
        let tag = core::Tag {
            id: "tag2".to_string(),
            name: "project/ai/nlp".to_string(),
            parent_tag_id: Some("tag1".to_string()),
            created_at: now,
            updated_at: now,
        };

        let surreal = core_tag_to_surreal(tag.clone(), None);

        assert_eq!(surreal.name, "project/ai/nlp");
        assert_eq!(surreal.path, "project/ai/nlp");
        assert_eq!(surreal.depth, 3);
        assert_eq!(surreal.parent_id.unwrap().id, "tag1");
    }

    #[test]
    fn test_surreal_tag_to_core() {
        let surreal = SurrealTag {
            id: Some(RecordId::new("tags", "tag1")),
            name: "research".to_string(),
            parent_id: None,
            path: "research".to_string(),
            depth: 1,
            description: Some("Research notes".to_string()),
            color: Some("#ff0000".to_string()),
            icon: Some("📚".to_string()),
        };

        let core_tag = surreal_tag_to_core(surreal);

        assert_eq!(core_tag.name, "research");
        assert!(core_tag.parent_tag_id.is_none());
    }

    #[test]
    fn test_round_trip_tag() {
        let now = chrono::Utc::now();
        let original = core::Tag {
            id: "tag1".to_string(),
            name: "work/meetings".to_string(),
            parent_tag_id: Some("parent1".to_string()),
            created_at: now,
            updated_at: now,
        };

        let surreal = core_tag_to_surreal(original.clone(), Some(RecordId::new("tags", "tag1")));
        let result = surreal_tag_to_core(surreal);

        assert_eq!(result.name, original.name);
        assert_eq!(result.parent_tag_id, Some("tags:parent1".to_string()));
    }

    #[test]
    fn test_core_entity_tag_to_surreal() {
        let now = chrono::Utc::now();
        let entity_tag = core::EntityTag {
            entity_id: "note:test".to_string(),
            tag_id: "tag1".to_string(),
            created_at: now,
        };

        let surreal = core_entity_tag_to_surreal(entity_tag, None);

        assert_eq!(surreal.entity_id.table, "entities");
        assert_eq!(surreal.entity_id.id, "note:test");
        assert_eq!(surreal.tag_id.table, "tags");
        assert_eq!(surreal.tag_id.id, "tag1");
        assert_eq!(surreal.source, "parser");
        assert_eq!(surreal.confidence, 1.0);
    }

    #[test]
    fn test_surreal_entity_tag_to_core() {
        let surreal = SurrealEntityTag {
            id: Some(RecordId::new("entity_tags", "et1")),
            entity_id: RecordId::new("entities", "note:test"),
            tag_id: RecordId::new("tags", "tag1"),
            source: "parser".to_string(),
            confidence: 1.0,
        };

        let core_entity_tag = surreal_entity_tag_to_core(surreal);

        assert_eq!(core_entity_tag.entity_id, "entities:note:test");
        assert_eq!(core_entity_tag.tag_id, "tags:tag1");
    }

    #[test]
    fn test_round_trip_entity_tag() {
        let now = chrono::Utc::now();
        let original = core::EntityTag {
            entity_id: "note:test".to_string(),
            tag_id: "tag1".to_string(),
            created_at: now,
        };

        let surreal = core_entity_tag_to_surreal(
            original.clone(),
            Some(RecordId::new("entity_tags", "et1")),
        );
        let result = surreal_entity_tag_to_core(surreal);

        // IDs will have table prefix after round-trip
        assert_eq!(result.entity_id, "entities:note:test");
        assert_eq!(result.tag_id, "tags:tag1");
    }
}
