//! EAV+Graph Storage Traits
//!
//! This module defines trait abstractions for Entity-Attribute-Value + Graph storage
//! following the Interface Segregation Principle (ISP). Each trait focuses on a single
//! responsibility, enabling flexible composition and comprehensive testing.
//!
//! ## Architecture
//!
//! The EAV+Graph pattern separates entity storage into five focused traits:
//!
//! - **EntityStorage**: Core entity CRUD operations
//! - **PropertyStorage**: Entity properties with namespaces and batch operations
//! - **RelationStorage**: Wikilinks, references, and graph edges
//! - **BlockStorage**: Hierarchical content blocks
//! - **TagStorage**: Hierarchical tag taxonomy
//!
//! ## Design Principles
//!
//! 1. **Interface Segregation**: Small, focused traits instead of one large interface
//! 2. **Database Agnostic**: Traits work with any storage backend (SurrealDB, memory, file)
//! 3. **Testable**: Mock implementations enable comprehensive unit testing
//! 4. **Type Safe**: Strongly typed entities and properties
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! use crucible_core::storage::{EntityStorage, StorageResult};
//!
//! async fn store_note<S: EntityStorage>(storage: &S, note_id: &str) -> StorageResult<()> {
//!     let entity = Entity::new(
//!         RecordId::new("entities", note_id),
//!         EntityType::Note,
//!     );
//!     storage.store_entity(entity).await?;
//!     Ok(())
//! }
//! ```

use crate::storage::StorageResult;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::borrow::Cow;

// ============================================================================
// Core Entity Types
// ============================================================================

/// Entity types supported by the EAV+Graph schema
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Note,
    Block,
    Tag,
    Section,
    Media,
    Person,
}

/// Core entity representation (database-agnostic)
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: String,
    pub entity_type: EntityType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub version: i32,
    pub content_hash: Option<String>,
    pub created_by: Option<String>,
    pub vault_id: Option<String>,
    pub data: Option<Value>,
    pub search_text: Option<String>,
}

impl Entity {
    /// Create a new entity with the given ID and type
    pub fn new(id: String, entity_type: EntityType) -> Self {
        let now = Utc::now();
        Self {
            id,
            entity_type,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            version: 1,
            content_hash: None,
            created_by: None,
            vault_id: None,
            data: None,
            search_text: None,
        }
    }

    /// Set the content hash for this entity
    pub fn with_content_hash(mut self, hash: impl Into<String>) -> Self {
        self.content_hash = Some(hash.into());
        self
    }

    /// Set the search text for this entity
    pub fn with_search_text(mut self, text: impl Into<String>) -> Self {
        self.search_text = Some(text.into());
        self
    }

    /// Set the vault ID for this entity
    pub fn with_vault_id(mut self, vault_id: impl Into<String>) -> Self {
        self.vault_id = Some(vault_id.into());
        self
    }
}

/// Property namespace for organization
///
/// Uses `Cow<'static, str>` to avoid allocations for common namespaces like
/// "core" and "frontmatter", while supporting dynamic plugin namespaces.
///
/// # Performance
///
/// For 10,000 notes with frontmatter, this saves 10,000 string allocations
/// by using static string slices instead of heap-allocated strings.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyNamespace(pub Cow<'static, str>);

impl PropertyNamespace {
    /// Core system namespace (zero-allocation)
    pub fn core() -> Self {
        Self(Cow::Borrowed("core"))
    }

    /// Frontmatter namespace (zero-allocation) for YAML/TOML properties
    pub fn frontmatter() -> Self {
        Self(Cow::Borrowed("frontmatter"))
    }

    /// Plugin namespace (allocates only for the formatted string)
    pub fn plugin(name: impl Into<String>) -> Self {
        Self(Cow::Owned(format!("plugin:{}", name.into())))
    }

    /// Access the namespace string as a reference
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

/// Property value types
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub entity_id: String,
    pub namespace: PropertyNamespace,
    pub key: String,
    pub value: PropertyValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Strongly-typed property values with tagged serialization
///
/// This enum uses tagged serialization to produce self-describing JSON that is
/// easier to extend and debug. Each variant serializes as:
/// ```json
/// {"type": "text", "value": "hello"}
/// {"type": "number", "value": 42.5}
/// {"type": "bool", "value": true}
/// {"type": "date", "value": "2024-11-08"}
/// {"type": "json", "value": {...}}
/// ```
///
/// This format makes it explicit what type each property is, which helps with:
/// - Type validation and debugging
/// - Schema evolution and migration
/// - Cross-language interoperability
/// - Self-documenting APIs
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "snake_case")]
pub enum PropertyValue {
    Text(String),
    Number(f64),
    Bool(bool),
    Date(NaiveDate),
    Json(Value),
}

// ============================================================================
// EntityStorage Trait
// ============================================================================

/// Core entity CRUD operations
///
/// This trait defines the fundamental operations for storing and retrieving entities.
/// It is intentionally minimal to follow the Interface Segregation Principle.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{EntityStorage, Entity, EntityType};
///
/// async fn create_note<S: EntityStorage>(storage: &S) -> StorageResult<String> {
///     let entity = Entity::new("note:example".to_string(), EntityType::Note)
///         .with_content_hash("abc123")
///         .with_search_text("Example note content");
///
///     storage.store_entity(entity).await
/// }
/// ```
#[async_trait]
pub trait EntityStorage: Send + Sync {
    /// Store a new entity or update an existing one
    ///
    /// # Arguments
    ///
    /// * `entity` - The entity to store
    ///
    /// # Returns
    ///
    /// Returns the entity ID on success
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if the storage operation fails
    async fn store_entity(&self, entity: Entity) -> StorageResult<String>;

    /// Retrieve an entity by ID
    ///
    /// # Arguments
    ///
    /// * `id` - The entity ID to retrieve
    ///
    /// # Returns
    ///
    /// Returns `Some(Entity)` if found, `None` if not found
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if the storage operation fails
    async fn get_entity(&self, id: &str) -> StorageResult<Option<Entity>>;

    /// Update an existing entity
    ///
    /// # Arguments
    ///
    /// * `id` - The entity ID to update
    /// * `entity` - The updated entity data
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if the storage operation fails
    /// Returns `StorageError::InvalidOperation` if the entity doesn't exist
    async fn update_entity(&self, id: &str, entity: Entity) -> StorageResult<()>;

    /// Delete an entity (soft delete by setting deleted_at timestamp)
    ///
    /// # Arguments
    ///
    /// * `id` - The entity ID to delete
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if the storage operation fails
    async fn delete_entity(&self, id: &str) -> StorageResult<()>;

    /// Check if an entity exists
    ///
    /// # Arguments
    ///
    /// * `id` - The entity ID to check
    ///
    /// # Returns
    ///
    /// Returns `true` if the entity exists, `false` otherwise
    async fn entity_exists(&self, id: &str) -> StorageResult<bool>;
}

// ============================================================================
// PropertyStorage Trait
// ============================================================================

/// Property storage with namespace support and batch operations
///
/// This trait defines operations for storing and querying entity properties.
/// Properties are organized by namespace to prevent key collisions between
/// different systems (core, frontmatter, plugins).
///
/// # Batch Operations
///
/// The trait emphasizes batch operations for performance. Storing 100 properties
/// individually would require 100 database round-trips. Batch operations reduce
/// this to a single operation.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{PropertyStorage, Property, PropertyValue, PropertyNamespace};
///
/// async fn store_frontmatter<S: PropertyStorage>(
///     storage: &S,
///     entity_id: &str,
/// ) -> StorageResult<()> {
///     let properties = vec![
///         Property {
///             entity_id: entity_id.to_string(),
///             namespace: PropertyNamespace::frontmatter(),
///             key: "author".to_string(),
///             value: PropertyValue::Text("John Doe".to_string()),
///             created_at: Utc::now(),
///             updated_at: Utc::now(),
///         },
///         Property {
///             entity_id: entity_id.to_string(),
///             namespace: PropertyNamespace::frontmatter(),
///             key: "priority".to_string(),
///             value: PropertyValue::Number(5.0),
///             created_at: Utc::now(),
///             updated_at: Utc::now(),
///         },
///     ];
///
///     storage.batch_upsert_properties(properties).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait PropertyStorage: Send + Sync {
    /// Store or update multiple properties in a single operation
    ///
    /// This is the primary method for storing properties. It uses upsert semantics:
    /// if a property with the same (entity_id, namespace, key) exists, it will be
    /// updated; otherwise, a new property will be created.
    ///
    /// # Arguments
    ///
    /// * `properties` - Vector of properties to store
    ///
    /// # Returns
    ///
    /// Returns the number of properties stored
    ///
    /// # Performance Target
    ///
    /// Should complete in <100ms for 100 properties
    ///
    /// # Errors
    ///
    /// Returns `StorageError::Backend` if the storage operation fails
    async fn batch_upsert_properties(&self, properties: Vec<Property>) -> StorageResult<usize>;

    /// Retrieve all properties for an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity ID to query
    ///
    /// # Returns
    ///
    /// Returns a vector of all properties associated with the entity
    async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>>;

    /// Retrieve properties for an entity filtered by namespace
    ///
    /// This is useful for retrieving only frontmatter properties, or only
    /// plugin properties, etc.
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity ID to query
    /// * `namespace` - The namespace to filter by
    ///
    /// # Returns
    ///
    /// Returns a vector of properties in the specified namespace
    async fn get_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<Vec<Property>>;

    /// Retrieve a single property by key
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity ID to query
    /// * `namespace` - The property namespace
    /// * `key` - The property key
    ///
    /// # Returns
    ///
    /// Returns `Some(Property)` if found, `None` if not found
    async fn get_property(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
        key: &str,
    ) -> StorageResult<Option<Property>>;

    /// Delete all properties for an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity ID whose properties to delete
    ///
    /// # Returns
    ///
    /// Returns the number of properties deleted
    async fn delete_properties(&self, entity_id: &str) -> StorageResult<usize>;

    /// Delete properties in a specific namespace
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity ID whose properties to delete
    /// * `namespace` - The namespace to delete
    ///
    /// # Returns
    ///
    /// Returns the number of properties deleted
    async fn delete_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<usize>;
}

// ============================================================================
// Relation Types
// ============================================================================

/// Relation types in the knowledge graph
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    /// Wikilink reference (e.g., [[target]])
    Wikilink,
    /// Embed reference (e.g., ![[target]])
    Embed,
    /// Block reference (e.g., [[note#^block-id]])
    BlockReference,
    /// Tag reference (implicit relation through tags)
    Tag,
    /// Custom relation type
    Custom,
}

/// A relation between two entities
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    pub source_entity_id: String,
    pub target_entity_id: String,
    pub relation_type: RelationType,
    pub created_at: DateTime<Utc>,
    pub metadata: Option<Value>,
}

// ============================================================================
// Block Types
// ============================================================================

/// A hierarchical content block
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: String,
    pub entity_id: String,
    pub parent_block_id: Option<String>,
    pub content: String,
    pub block_type: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub content_hash: Option<String>,
}

// ============================================================================
// Tag Types
// ============================================================================

/// A hierarchical tag
#[derive(Debug, Clone, PartialEq)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub parent_tag_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Association between an entity and a tag
#[derive(Debug, Clone, PartialEq)]
pub struct EntityTag {
    pub entity_id: String,
    pub tag_id: String,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// RelationStorage Trait
// ============================================================================

/// Storage for entity relations (wikilinks, embeds, block references)
///
/// This trait manages the graph connections between entities. Relations capture
/// the links between notes, embedded content, and block references that form
/// the knowledge graph structure.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{RelationStorage, Relation, RelationType};
///
/// async fn create_wikilink<S: RelationStorage>(
///     storage: &S,
///     from: &str,
///     to: &str,
/// ) -> StorageResult<()> {
///     let relation = Relation {
///         source_entity_id: from.to_string(),
///         target_entity_id: to.to_string(),
///         relation_type: RelationType::Wikilink,
///         created_at: Utc::now(),
///         metadata: None,
///     };
///
///     storage.store_relation(relation).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait RelationStorage: Send + Sync {
    /// Store a new relation
    async fn store_relation(&self, relation: Relation) -> StorageResult<()>;

    /// Get all outgoing relations from an entity
    async fn get_outgoing_relations(&self, entity_id: &str) -> StorageResult<Vec<Relation>>;

    /// Get all incoming relations to an entity
    async fn get_incoming_relations(&self, entity_id: &str) -> StorageResult<Vec<Relation>>;

    /// Get relations filtered by type
    async fn get_relations_by_type(
        &self,
        entity_id: &str,
        relation_type: RelationType,
    ) -> StorageResult<Vec<Relation>>;

    /// Delete all relations for an entity
    async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize>;
}

// ============================================================================
// BlockStorage Trait
// ============================================================================

/// Storage for hierarchical content blocks
///
/// Blocks represent the hierarchical structure of document content. Each block
/// has a parent (or is a root block) and can have children, forming a tree
/// structure that mirrors the document organization.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{BlockStorage, Block};
///
/// async fn store_heading<S: BlockStorage>(
///     storage: &S,
///     entity_id: &str,
/// ) -> StorageResult<String> {
///     let block = Block {
///         id: "block:heading1".to_string(),
///         entity_id: entity_id.to_string(),
///         parent_block_id: None,
///         content: "# Introduction".to_string(),
///         block_type: "heading".to_string(),
///         position: 0,
///         created_at: Utc::now(),
///         updated_at: Utc::now(),
///         content_hash: None,
///     };
///
///     storage.store_block(block).await
/// }
/// ```
#[async_trait]
pub trait BlockStorage: Send + Sync {
    /// Store a new block
    async fn store_block(&self, block: Block) -> StorageResult<String>;

    /// Get a block by ID
    async fn get_block(&self, id: &str) -> StorageResult<Option<Block>>;

    /// Get all blocks for an entity
    async fn get_blocks(&self, entity_id: &str) -> StorageResult<Vec<Block>>;

    /// Get child blocks of a parent block
    async fn get_child_blocks(&self, parent_block_id: &str) -> StorageResult<Vec<Block>>;

    /// Update a block
    async fn update_block(&self, id: &str, block: Block) -> StorageResult<()>;

    /// Delete a block and optionally its children
    async fn delete_block(&self, id: &str, recursive: bool) -> StorageResult<usize>;

    /// Delete all blocks for an entity
    async fn delete_blocks(&self, entity_id: &str) -> StorageResult<usize>;
}

// ============================================================================
// TagStorage Trait
// ============================================================================

/// Storage for hierarchical tags and entity-tag associations
///
/// Tags can be hierarchical (e.g., #project/work/backend) and multiple entities
/// can share the same tag. This trait manages both the tag taxonomy and the
/// associations between entities and tags.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{TagStorage, Tag, EntityTag};
///
/// async fn tag_note<S: TagStorage>(
///     storage: &S,
///     entity_id: &str,
///     tag_name: &str,
/// ) -> StorageResult<()> {
///     // Create or get tag
///     let tag = Tag {
///         id: format!("tag:{}", tag_name),
///         name: tag_name.to_string(),
///         parent_tag_id: None,
///         created_at: Utc::now(),
///         updated_at: Utc::now(),
///     };
///     storage.store_tag(tag).await?;
///
///     // Associate entity with tag
///     let entity_tag = EntityTag {
///         entity_id: entity_id.to_string(),
///         tag_id: format!("tag:{}", tag_name),
///         created_at: Utc::now(),
///     };
///     storage.associate_tag(entity_tag).await?;
///
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait TagStorage: Send + Sync {
    /// Store a new tag
    async fn store_tag(&self, tag: Tag) -> StorageResult<String>;

    /// Get a tag by ID
    async fn get_tag(&self, id: &str) -> StorageResult<Option<Tag>>;

    /// Get child tags of a parent tag
    async fn get_child_tags(&self, parent_tag_id: &str) -> StorageResult<Vec<Tag>>;

    /// Associate a tag with an entity
    async fn associate_tag(&self, entity_tag: EntityTag) -> StorageResult<()>;

    /// Get all tags for an entity
    async fn get_entity_tags(&self, entity_id: &str) -> StorageResult<Vec<Tag>>;

    /// Get all entities with a specific tag
    async fn get_entities_by_tag(&self, tag_id: &str) -> StorageResult<Vec<String>>;

    /// Remove tag association from an entity
    async fn dissociate_tag(&self, entity_id: &str, tag_id: &str) -> StorageResult<()>;

    /// Delete a tag and optionally its associations
    async fn delete_tag(&self, id: &str, delete_associations: bool) -> StorageResult<usize>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StorageError;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Mock implementation of EntityStorage for testing
    struct MockEntityStorage {
        entities: Arc<Mutex<HashMap<String, Entity>>>,
    }

    impl MockEntityStorage {
        fn new() -> Self {
            Self {
                entities: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    #[async_trait]
    impl EntityStorage for MockEntityStorage {
        async fn store_entity(&self, entity: Entity) -> StorageResult<String> {
            let id = entity.id.clone();
            self.entities.lock().unwrap().insert(id.clone(), entity);
            Ok(id)
        }

        async fn get_entity(&self, id: &str) -> StorageResult<Option<Entity>> {
            Ok(self.entities.lock().unwrap().get(id).cloned())
        }

        async fn update_entity(&self, id: &str, entity: Entity) -> StorageResult<()> {
            let mut entities = self.entities.lock().unwrap();
            if !entities.contains_key(id) {
                return Err(StorageError::InvalidOperation(format!(
                    "Entity {} does not exist",
                    id
                )));
            }
            entities.insert(id.to_string(), entity);
            Ok(())
        }

        async fn delete_entity(&self, id: &str) -> StorageResult<()> {
            let mut entities = self.entities.lock().unwrap();
            if let Some(entity) = entities.get_mut(id) {
                entity.deleted_at = Some(Utc::now());
            }
            Ok(())
        }

        async fn entity_exists(&self, id: &str) -> StorageResult<bool> {
            Ok(self.entities.lock().unwrap().contains_key(id))
        }
    }

    #[tokio::test]
    async fn test_entity_storage_trait_compiles() {
        // RED: This test should compile and pass, verifying the trait definition
        let storage = MockEntityStorage::new();

        // Test store_entity
        let entity = Entity::new("note:test".to_string(), EntityType::Note)
            .with_content_hash("abc123")
            .with_search_text("Test note");

        let id = storage.store_entity(entity.clone()).await.unwrap();
        assert_eq!(id, "note:test");

        // Test get_entity
        let retrieved = storage.get_entity("note:test").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "note:test");
        assert_eq!(retrieved.entity_type, EntityType::Note);
        assert_eq!(retrieved.content_hash, Some("abc123".to_string()));

        // Test entity_exists
        let exists = storage.entity_exists("note:test").await.unwrap();
        assert!(exists);

        let not_exists = storage.entity_exists("note:nonexistent").await.unwrap();
        assert!(!not_exists);

        // Test update_entity
        let mut updated_entity = retrieved.clone();
        updated_entity.search_text = Some("Updated text".to_string());
        storage
            .update_entity("note:test", updated_entity)
            .await
            .unwrap();

        let updated = storage.get_entity("note:test").await.unwrap().unwrap();
        assert_eq!(updated.search_text, Some("Updated text".to_string()));

        // Test delete_entity
        storage.delete_entity("note:test").await.unwrap();
        let deleted = storage.get_entity("note:test").await.unwrap().unwrap();
        assert!(deleted.deleted_at.is_some());
    }

    #[tokio::test]
    async fn test_entity_builder_pattern() {
        let entity = Entity::new("note:builder".to_string(), EntityType::Note)
            .with_content_hash("hash123")
            .with_search_text("Searchable content")
            .with_vault_id("vault:main");

        assert_eq!(entity.id, "note:builder");
        assert_eq!(entity.entity_type, EntityType::Note);
        assert_eq!(entity.content_hash, Some("hash123".to_string()));
        assert_eq!(entity.search_text, Some("Searchable content".to_string()));
        assert_eq!(entity.vault_id, Some("vault:main".to_string()));
        assert_eq!(entity.version, 1);
    }

    #[tokio::test]
    async fn test_property_namespace_creation() {
        let core_ns = PropertyNamespace::core();
        assert_eq!(core_ns.0.as_ref(), "core");
        assert_eq!(core_ns.as_str(), "core");

        let frontmatter_ns = PropertyNamespace::frontmatter();
        assert_eq!(frontmatter_ns.0.as_ref(), "frontmatter");
        assert_eq!(frontmatter_ns.as_str(), "frontmatter");

        let plugin_ns = PropertyNamespace::plugin("my_plugin");
        assert_eq!(plugin_ns.0.as_ref(), "plugin:my_plugin");
        assert_eq!(plugin_ns.as_str(), "plugin:my_plugin");
    }

    #[tokio::test]
    async fn test_update_nonexistent_entity_fails() {
        let storage = MockEntityStorage::new();
        let entity = Entity::new("note:missing".to_string(), EntityType::Note);

        let result = storage.update_entity("note:missing", entity).await;
        assert!(result.is_err());
        match result {
            Err(StorageError::InvalidOperation(msg)) => {
                assert!(msg.contains("does not exist"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

    // ========================================================================
    // PropertyStorage Tests
    // ========================================================================

    /// Mock implementation of PropertyStorage for testing
    struct MockPropertyStorage {
        // Store properties indexed by (entity_id, namespace, key)
        properties: Arc<Mutex<HashMap<(String, String, String), Property>>>,
    }

    impl MockPropertyStorage {
        fn new() -> Self {
            Self {
                properties: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn make_key(entity_id: &str, namespace: &PropertyNamespace, key: &str) -> (String, String, String) {
            (entity_id.to_string(), namespace.0.to_string(), key.to_string())
        }
    }

    #[async_trait]
    impl PropertyStorage for MockPropertyStorage {
        async fn batch_upsert_properties(&self, properties: Vec<Property>) -> StorageResult<usize> {
            let mut store = self.properties.lock().unwrap();
            let count = properties.len();

            for prop in properties {
                let key = Self::make_key(&prop.entity_id, &prop.namespace, &prop.key);
                store.insert(key, prop);
            }

            Ok(count)
        }

        async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>> {
            let store = self.properties.lock().unwrap();
            let results: Vec<Property> = store
                .iter()
                .filter(|((eid, _, _), _)| eid == entity_id)
                .map(|(_, prop)| prop.clone())
                .collect();
            Ok(results)
        }

        async fn get_properties_by_namespace(
            &self,
            entity_id: &str,
            namespace: &PropertyNamespace,
        ) -> StorageResult<Vec<Property>> {
            let store = self.properties.lock().unwrap();
            let namespace_str = namespace.0.as_ref();
            let results: Vec<Property> = store
                .iter()
                .filter(|((eid, ns, _), _)| eid == entity_id && ns == namespace_str)
                .map(|(_, prop)| prop.clone())
                .collect();
            Ok(results)
        }

        async fn get_property(
            &self,
            entity_id: &str,
            namespace: &PropertyNamespace,
            key: &str,
        ) -> StorageResult<Option<Property>> {
            let store = self.properties.lock().unwrap();
            let lookup_key = Self::make_key(entity_id, namespace, key);
            Ok(store.get(&lookup_key).cloned())
        }

        async fn delete_properties(&self, entity_id: &str) -> StorageResult<usize> {
            let mut store = self.properties.lock().unwrap();
            let to_remove: Vec<(String, String, String)> = store
                .keys()
                .filter(|(eid, _, _)| eid == entity_id)
                .cloned()
                .collect();

            let count = to_remove.len();
            for key in to_remove {
                store.remove(&key);
            }

            Ok(count)
        }

        async fn delete_properties_by_namespace(
            &self,
            entity_id: &str,
            namespace: &PropertyNamespace,
        ) -> StorageResult<usize> {
            let mut store = self.properties.lock().unwrap();
            let namespace_str = namespace.0.as_ref();
            let to_remove: Vec<(String, String, String)> = store
                .keys()
                .filter(|(eid, ns, _)| eid == entity_id && ns == namespace_str)
                .cloned()
                .collect();

            let count = to_remove.len();
            for key in to_remove {
                store.remove(&key);
            }

            Ok(count)
        }
    }

    #[tokio::test]
    async fn test_property_storage_batch_upsert() {
        let storage = MockPropertyStorage::new();
        let now = Utc::now();

        // Create multiple properties for batch upsert
        let properties = vec![
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: PropertyValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "priority".to_string(),
                value: PropertyValue::Number(5.0),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "published".to_string(),
                value: PropertyValue::Bool(true),
                created_at: now,
                updated_at: now,
            },
        ];

        // Test batch upsert
        let count = storage.batch_upsert_properties(properties.clone()).await.unwrap();
        assert_eq!(count, 3);

        // Verify properties were stored
        let retrieved = storage.get_properties("note:test").await.unwrap();
        assert_eq!(retrieved.len(), 3);

        // Test upsert semantics - update existing property
        let updated_property = Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "author".to_string(),
            value: PropertyValue::Text("Jane Smith".to_string()),
            created_at: now,
            updated_at: Utc::now(),
        };

        storage.batch_upsert_properties(vec![updated_property]).await.unwrap();

        // Should still have 3 properties (upsert, not insert)
        let retrieved = storage.get_properties("note:test").await.unwrap();
        assert_eq!(retrieved.len(), 3);

        // Verify the update
        let author = storage
            .get_property("note:test", &PropertyNamespace::frontmatter(), "author")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(author.value, PropertyValue::Text("Jane Smith".to_string()));
    }

    #[tokio::test]
    async fn test_property_storage_namespace_filtering() {
        let storage = MockPropertyStorage::new();
        let now = Utc::now();

        // Store properties in different namespaces
        let properties = vec![
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: PropertyValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::core(),
                key: "version".to_string(),
                value: PropertyValue::Number(2.0),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::plugin("my_plugin"),
                key: "custom_field".to_string(),
                value: PropertyValue::Text("custom value".to_string()),
                created_at: now,
                updated_at: now,
            },
        ];

        storage.batch_upsert_properties(properties).await.unwrap();

        // Test namespace filtering
        let frontmatter_props = storage
            .get_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(frontmatter_props.len(), 1);
        assert_eq!(frontmatter_props[0].key, "author");

        let core_props = storage
            .get_properties_by_namespace("note:test", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(core_props.len(), 1);
        assert_eq!(core_props[0].key, "version");

        let plugin_props = storage
            .get_properties_by_namespace("note:test", &PropertyNamespace::plugin("my_plugin"))
            .await
            .unwrap();
        assert_eq!(plugin_props.len(), 1);
        assert_eq!(plugin_props[0].key, "custom_field");

        // All properties together
        let all_props = storage.get_properties("note:test").await.unwrap();
        assert_eq!(all_props.len(), 3);
    }

    #[tokio::test]
    async fn test_property_storage_deletion() {
        let storage = MockPropertyStorage::new();
        let now = Utc::now();

        // Store properties in different namespaces
        let properties = vec![
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "author".to_string(),
                value: PropertyValue::Text("John Doe".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "tags".to_string(),
                value: PropertyValue::Json(serde_json::json!(["test", "example"])),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:test".to_string(),
                namespace: PropertyNamespace::core(),
                key: "version".to_string(),
                value: PropertyValue::Number(1.0),
                created_at: now,
                updated_at: now,
            },
        ];

        storage.batch_upsert_properties(properties).await.unwrap();

        // Test delete by namespace
        let deleted_count = storage
            .delete_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(deleted_count, 2);

        // Verify frontmatter properties are gone
        let frontmatter_props = storage
            .get_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(frontmatter_props.len(), 0);

        // Verify core properties still exist
        let core_props = storage
            .get_properties_by_namespace("note:test", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(core_props.len(), 1);

        // Test delete all properties
        let deleted_all = storage.delete_properties("note:test").await.unwrap();
        assert_eq!(deleted_all, 1); // Only core property remaining

        // Verify all properties are gone
        let all_props = storage.get_properties("note:test").await.unwrap();
        assert_eq!(all_props.len(), 0);
    }

    #[tokio::test]
    async fn test_property_value_types() {
        // Test all property value types
        let text_val = PropertyValue::Text("hello".to_string());
        let num_val = PropertyValue::Number(42.5);
        let bool_val = PropertyValue::Bool(true);
        let _date_val = PropertyValue::Date(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
        let _json_val = PropertyValue::Json(serde_json::json!({"key": "value"}));

        // Verify equality comparisons work
        assert_eq!(text_val, PropertyValue::Text("hello".to_string()));
        assert_eq!(num_val, PropertyValue::Number(42.5));
        assert_eq!(bool_val, PropertyValue::Bool(true));
        assert_ne!(text_val, PropertyValue::Text("world".to_string()));
    }

    #[tokio::test]
    async fn test_property_value_tagged_serialization() {
        // Test that PropertyValue serializes with tagged format

        // Text variant
        let text_val = PropertyValue::Text("hello".to_string());
        let text_json = serde_json::to_value(&text_val).unwrap();
        assert_eq!(text_json["type"], "text");
        assert_eq!(text_json["value"], "hello");

        // Number variant
        let num_val = PropertyValue::Number(42.5);
        let num_json = serde_json::to_value(&num_val).unwrap();
        assert_eq!(num_json["type"], "number");
        assert_eq!(num_json["value"], 42.5);

        // Bool variant
        let bool_val = PropertyValue::Bool(true);
        let bool_json = serde_json::to_value(&bool_val).unwrap();
        assert_eq!(bool_json["type"], "bool");
        assert_eq!(bool_json["value"], true);

        // Date variant
        let date_val = PropertyValue::Date(NaiveDate::from_ymd_opt(2024, 11, 8).unwrap());
        let date_json = serde_json::to_value(&date_val).unwrap();
        assert_eq!(date_json["type"], "date");
        assert_eq!(date_json["value"], "2024-11-08");

        // Json variant
        let json_val = PropertyValue::Json(serde_json::json!({"key": "value"}));
        let json_json = serde_json::to_value(&json_val).unwrap();
        assert_eq!(json_json["type"], "json");
        assert_eq!(json_json["value"]["key"], "value");
    }

    #[tokio::test]
    async fn test_property_value_tagged_deserialization() {
        // Test that PropertyValue deserializes from tagged format

        // Text variant
        let text_json = serde_json::json!({"type": "text", "value": "hello"});
        let text_val: PropertyValue = serde_json::from_value(text_json).unwrap();
        assert_eq!(text_val, PropertyValue::Text("hello".to_string()));

        // Number variant
        let num_json = serde_json::json!({"type": "number", "value": 42.5});
        let num_val: PropertyValue = serde_json::from_value(num_json).unwrap();
        assert_eq!(num_val, PropertyValue::Number(42.5));

        // Bool variant
        let bool_json = serde_json::json!({"type": "bool", "value": true});
        let bool_val: PropertyValue = serde_json::from_value(bool_json).unwrap();
        assert_eq!(bool_val, PropertyValue::Bool(true));

        // Date variant
        let date_json = serde_json::json!({"type": "date", "value": "2024-11-08"});
        let date_val: PropertyValue = serde_json::from_value(date_json).unwrap();
        assert_eq!(date_val, PropertyValue::Date(NaiveDate::from_ymd_opt(2024, 11, 8).unwrap()));

        // Json variant
        let json_json = serde_json::json!({"type": "json", "value": {"key": "value"}});
        let json_val: PropertyValue = serde_json::from_value(json_json).unwrap();
        assert_eq!(json_val, PropertyValue::Json(serde_json::json!({"key": "value"})));
    }

    #[tokio::test]
    async fn test_batch_upsert_performance_target() {
        // This test verifies the trait compiles with the performance requirements
        // documented in the trait definition (100 properties in <100ms)
        //
        // The actual performance testing will be done in integration tests with
        // real database backends, but this verifies the API supports batch operations
        let storage = MockPropertyStorage::new();
        let now = Utc::now();

        // Create 100 properties
        let properties: Vec<Property> = (0..100)
            .map(|i| Property {
                entity_id: "note:perf_test".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: format!("key_{}", i),
                value: PropertyValue::Number(i as f64),
                created_at: now,
                updated_at: now,
            })
            .collect();

        // Batch upsert should handle 100 properties
        let count = storage.batch_upsert_properties(properties).await.unwrap();
        assert_eq!(count, 100);

        // Verify all properties were stored
        let retrieved = storage.get_properties("note:perf_test").await.unwrap();
        assert_eq!(retrieved.len(), 100);
    }
}
