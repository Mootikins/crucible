//! EAV+Graph Document Representation
//!
//! This module provides an intermediate representation for converting parsed markdown
//! documents into the Entity-Attribute-Value + Graph storage model. It acts as a bridge
//! between the parser output and the storage traits.
//!
//! ## Purpose
//!
//! The `EAVDocument` type solves the impedance mismatch between:
//! - **ParsedDocument**: Markdown-focused structure (headings, blocks, wikilinks)
//! - **Storage Traits**: EAV+Graph focused (entities, properties, relations)
//!
//! ## Architecture
//!
//! ```text
//! ParsedDocument → EAVDocument → Storage Traits
//!     (parser)    (intermediate)    (database)
//! ```
//!
//! ## Design Principles
//!
//! 1. **Database Agnostic**: No SurrealDB dependencies (uses storage traits)
//! 2. **Type Safe**: Strongly typed entities, properties, relations
//! 3. **Builder Pattern**: Fluent API for constructing documents
//! 4. **Validation**: Ensures data integrity before storage
//!
//! See `EAVDocument::builder()` for usage.

use crate::storage::{
    Block, Entity, EntityTag, EntityType, Property, PropertyNamespace, Relation, StorageError, Tag,
};
use chrono::Utc;
use std::collections::HashMap;

// ============================================================================
// EAVDocument - Intermediate Representation
// ============================================================================

/// Intermediate representation of a parsed document in EAV+Graph format
///
/// This type bridges the gap between ParsedDocument (markdown structure) and
/// the storage traits (EAV+Graph structure). It provides:
///
/// - Type-safe entity representation
/// - Collection of properties grouped by namespace
/// - Relations (wikilinks, embeds, block references)
/// - Hierarchical blocks
/// - Tag associations
///
/// # Validation
///
/// Documents can be validated before storage using the `validate()` method.
/// This ensures:
/// - Entity has a valid ID and type
/// - All properties reference the same entity ID
/// - All relations have valid source and target IDs
/// - All blocks belong to the entity
///
/// # Database Agnostic
///
/// This type uses only the storage trait types, with NO SurrealDB dependencies.
/// It can be stored using any backend that implements the storage traits.
#[derive(Debug, Clone)]
pub struct EAVDocument {
    /// The core entity (note, block, section, etc.)
    pub entity: Entity,

    /// Properties grouped by namespace
    pub properties: HashMap<PropertyNamespace, Vec<Property>>,

    /// Outgoing relations (wikilinks, embeds, block refs)
    pub relations: Vec<Relation>,

    /// Content blocks (hierarchical structure)
    pub blocks: Vec<Block>,

    /// Tags associated with this entity
    pub tags: Vec<Tag>,

    /// Entity-tag associations
    pub entity_tags: Vec<EntityTag>,
}

impl EAVDocument {
    /// Create a new document builder
    pub fn builder() -> EAVDocumentBuilder {
        EAVDocumentBuilder::new()
    }

    /// Validate the document structure
    ///
    /// Ensures:
    /// - Entity has a valid ID
    /// - All properties reference the entity
    /// - All relations have valid source/target
    /// - All blocks belong to the entity
    /// - All entity_tags reference the entity
    ///
    /// # Errors
    ///
    /// Returns `ValidationError` if any validation check fails
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Validate entity ID is not empty
        if self.entity.id.is_empty() {
            return Err(ValidationError::EmptyEntityId);
        }

        // Validate all properties reference the same entity
        for (namespace, props) in &self.properties {
            for prop in props {
                if prop.entity_id != self.entity.id {
                    return Err(ValidationError::PropertyEntityMismatch {
                        property_entity: prop.entity_id.clone(),
                        document_entity: self.entity.id.clone(),
                        namespace: namespace.0.to_string(),
                        key: prop.key.clone(),
                    });
                }
            }
        }

        // Validate all relations have the entity as source
        for relation in &self.relations {
            if relation.from_entity_id != self.entity.id {
                return Err(ValidationError::RelationSourceMismatch {
                    relation_source: relation.from_entity_id.clone(),
                    document_entity: self.entity.id.clone(),
                });
            }
        }

        // Validate all blocks belong to the entity
        for block in &self.blocks {
            if block.entity_id != self.entity.id {
                return Err(ValidationError::BlockEntityMismatch {
                    block_entity: block.entity_id.clone(),
                    document_entity: self.entity.id.clone(),
                    block_id: block.id.clone(),
                });
            }
        }

        // Validate all entity_tags reference the entity
        for entity_tag in &self.entity_tags {
            if entity_tag.entity_id != self.entity.id {
                return Err(ValidationError::EntityTagMismatch {
                    tag_entity: entity_tag.entity_id.clone(),
                    document_entity: self.entity.id.clone(),
                    tag_id: entity_tag.tag_id.clone(),
                });
            }
        }

        Ok(())
    }

    /// Get all properties (across all namespaces)
    pub fn all_properties(&self) -> Vec<&Property> {
        self.properties.values().flatten().collect()
    }

    /// Get properties in a specific namespace
    pub fn properties_in_namespace(&self, namespace: &PropertyNamespace) -> Option<&Vec<Property>> {
        self.properties.get(namespace)
    }

    /// Count total properties
    pub fn property_count(&self) -> usize {
        self.properties.values().map(|v| v.len()).sum()
    }
}

// ============================================================================
// EAVDocumentBuilder - Fluent Construction API
// ============================================================================

/// Builder for constructing EAVDocument instances
///
/// Provides a fluent API for building documents with validation.
///
/// See builder methods for usage.
#[derive(Debug, Default)]
pub struct EAVDocumentBuilder {
    entity_id: Option<String>,
    entity_type: Option<EntityType>,
    content_hash: Option<String>,
    search_text: Option<String>,
    vault_id: Option<String>,
    properties: HashMap<PropertyNamespace, Vec<Property>>,
    relations: Vec<Relation>,
    blocks: Vec<Block>,
    tags: Vec<Tag>,
    entity_tags: Vec<EntityTag>,
}

impl EAVDocumentBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the entity ID
    pub fn entity_id(mut self, id: impl Into<String>) -> Self {
        self.entity_id = Some(id.into());
        self
    }

    /// Set the entity type
    pub fn entity_type(mut self, entity_type: EntityType) -> Self {
        self.entity_type = Some(entity_type);
        self
    }

    /// Set the content hash
    pub fn content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    /// Set the search text
    pub fn search_text(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }

    /// Set the vault ID
    pub fn vault_id(mut self, vault_id: impl Into<String>) -> Self {
        self.vault_id = Some(vault_id.into());
        self
    }

    /// Add a property
    pub fn add_property(mut self, property: Property) -> Self {
        self.properties
            .entry(property.namespace.clone())
            .or_insert_with(Vec::new)
            .push(property);
        self
    }

    /// Add multiple properties
    pub fn add_properties(mut self, properties: Vec<Property>) -> Self {
        for property in properties {
            self = self.add_property(property);
        }
        self
    }

    /// Add a relation
    pub fn add_relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }

    /// Add multiple relations
    pub fn add_relations(mut self, relations: Vec<Relation>) -> Self {
        self.relations.extend(relations);
        self
    }

    /// Add a block
    pub fn add_block(mut self, block: Block) -> Self {
        self.blocks.push(block);
        self
    }

    /// Add multiple blocks
    pub fn add_blocks(mut self, blocks: Vec<Block>) -> Self {
        self.blocks.extend(blocks);
        self
    }

    /// Add a tag
    pub fn add_tag(mut self, tag: Tag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Add an entity-tag association
    pub fn add_entity_tag(mut self, entity_tag: EntityTag) -> Self {
        self.entity_tags.push(entity_tag);
        self
    }

    /// Build the EAVDocument
    ///
    /// # Errors
    ///
    /// Returns `ValidationError::MissingField` if required fields are missing
    pub fn build(self) -> Result<EAVDocument, ValidationError> {
        let entity_id = self
            .entity_id
            .ok_or(ValidationError::MissingField("entity_id"))?;

        let entity_type = self
            .entity_type
            .ok_or(ValidationError::MissingField("entity_type"))?;

        let entity = Entity {
            id: entity_id,
            entity_type,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            version: 1,
            content_hash: self.content_hash,
            created_by: None,
            vault_id: self.vault_id,
            data: None,
            search_text: self.search_text,
        };

        let doc = EAVDocument {
            entity,
            properties: self.properties,
            relations: self.relations,
            blocks: self.blocks,
            tags: self.tags,
            entity_tags: self.entity_tags,
        };

        // Validate before returning
        doc.validate()?;

        Ok(doc)
    }
}

// ============================================================================
// ValidationError - Type-Safe Validation Errors
// ============================================================================

/// Validation errors for EAVDocument construction
#[derive(Debug, Clone, thiserror::Error)]
pub enum ValidationError {
    /// Entity ID is empty
    #[error("Entity ID cannot be empty")]
    EmptyEntityId,

    /// Property references different entity than document
    #[error("Property entity mismatch: property references '{property_entity}' but document entity is '{document_entity}' (namespace: {namespace}, key: {key})")]
    PropertyEntityMismatch {
        property_entity: String,
        document_entity: String,
        namespace: String,
        key: String,
    },

    /// Relation source doesn't match document entity
    #[error("Relation source mismatch: relation source is '{relation_source}' but document entity is '{document_entity}'")]
    RelationSourceMismatch {
        relation_source: String,
        document_entity: String,
    },

    /// Block entity doesn't match document entity
    #[error("Block entity mismatch: block references '{block_entity}' but document entity is '{document_entity}' (block_id: {block_id})")]
    BlockEntityMismatch {
        block_entity: String,
        document_entity: String,
        block_id: String,
    },

    /// Entity tag doesn't match document entity
    #[error("Entity tag mismatch: tag references '{tag_entity}' but document entity is '{document_entity}' (tag_id: {tag_id})")]
    EntityTagMismatch {
        tag_entity: String,
        document_entity: String,
        tag_id: String,
    },

    /// Required field is missing
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
}

// Implement conversion from ValidationError to StorageError
impl From<ValidationError> for StorageError {
    fn from(err: ValidationError) -> Self {
        StorageError::InvalidOperation(err.to_string())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::PropertyValue;

    #[test]
    fn test_eav_document_builder_basic() {
        // RED: This test should compile and fail initially
        let doc = EAVDocument::builder()
            .entity_id("note:test")
            .entity_type(EntityType::Note)
            .content_hash("abc123")
            .search_text("Test content")
            .build();

        assert!(doc.is_ok());
        let doc = doc.unwrap();
        assert_eq!(doc.entity.id, "note:test");
        assert_eq!(doc.entity.entity_type, EntityType::Note);
        assert_eq!(doc.entity.content_hash, Some("abc123".to_string()));
        assert_eq!(doc.entity.search_text, Some("Test content".to_string()));
    }

    #[test]
    fn test_eav_document_builder_missing_fields() {
        // Missing entity_id
        let result = EAVDocument::builder()
            .entity_type(EntityType::Note)
            .build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::MissingField("entity_id")
        ));

        // Missing entity_type
        let result = EAVDocument::builder().entity_id("note:test").build();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ValidationError::MissingField("entity_type")
        ));
    }

    #[test]
    fn test_eav_document_with_properties() {
        let now = Utc::now();
        let doc = EAVDocument::builder()
            .entity_id("note:test")
            .entity_type(EntityType::Note)
            .add_property(Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: PropertyValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            })
            .add_property(Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::core(),
                key: "version".to_string(),
                value: PropertyValue::Number(1.0),
                created_at: now,
                updated_at: now,
            })
            .build()
            .unwrap();

        assert_eq!(doc.property_count(), 2);
        assert_eq!(
            doc.properties_in_namespace(&PropertyNamespace::frontmatter())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            doc.properties_in_namespace(&PropertyNamespace::core())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_eav_document_validation_property_mismatch() {
        let now = Utc::now();
        let result = EAVDocument::builder()
            .entity_id("note:test")
            .entity_type(EntityType::Note)
            .add_property(Property {
                entity_id: "note:WRONG".to_string(), // Wrong entity!
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: PropertyValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            })
            .build();

        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationError::PropertyEntityMismatch {
                property_entity,
                document_entity,
                ..
            } => {
                assert_eq!(property_entity, "note:WRONG");
                assert_eq!(document_entity, "note:test");
            }
            _ => panic!("Expected PropertyEntityMismatch error"),
        }
    }

    #[test]
    fn test_eav_document_with_relations() {
        let now = Utc::now();
        let doc = EAVDocument::builder()
            .entity_id("note:source")
            .entity_type(EntityType::Note)
            .add_relation(Relation {
                from_entity_id: "note:source".to_string(),
                to_entity_id: Some("note:target".to_string()),
                relation_type: "wikilink".to_string(),
                created_at: now,
                metadata: serde_json::Value::Null,
                ..Default::default()
            })
            .build()
            .unwrap();

        assert_eq!(doc.relations.len(), 1);
        assert_eq!(doc.relations[0].relation_type, "wikilink");
    }

    #[test]
    fn test_eav_document_validation_relation_mismatch() {
        let now = Utc::now();
        let result = EAVDocument::builder()
            .entity_id("note:test")
            .entity_type(EntityType::Note)
            .add_relation(Relation {
                from_entity_id: "note:WRONG".to_string(), // Wrong source!
                to_entity_id: Some("note:target".to_string()),
                relation_type: "wikilink".to_string(),
                created_at: now,
                metadata: serde_json::Value::Null,
                ..Default::default()
            })
            .build();

        assert!(result.is_err());
        match result.unwrap_err() {
            ValidationError::RelationSourceMismatch {
                relation_source,
                document_entity,
            } => {
                assert_eq!(relation_source, "note:WRONG");
                assert_eq!(document_entity, "note:test");
            }
            _ => panic!("Expected RelationSourceMismatch error"),
        }
    }

    #[test]
    fn test_eav_document_no_surrealdb_dependencies() {
        // This test verifies at compile time that we can construct an EAVDocument
        // without any SurrealDB imports
        let doc = EAVDocument::builder()
            .entity_id("note:test")
            .entity_type(EntityType::Note)
            .build()
            .unwrap();

        // If this compiles, we have no SurrealDB dependencies
        assert_eq!(doc.entity.id, "note:test");
    }
}
