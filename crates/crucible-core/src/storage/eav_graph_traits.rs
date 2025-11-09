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
// RelationStorage Trait
// ============================================================================

/// A relation between two entities (wikilink, embed, inline link, etc.)
///
/// Relations represent connections in the knowledge graph including:
/// - Wikilinks: `[[Note]]`, `[[Note|Alias]]`, `[[Note#Heading]]`
/// - Block links: `[[Note#Heading^5#hash]]` with content-addressed validation
/// - Embeds: `![[Note]]` (reversed semantics - content inclusion)
/// - Inline links: `[text](url)`
///
/// # Block Link Fields
///
/// Block links use three fields for precise targeting:
/// - `block_offset`: Position of block under heading (1-indexed, None for non-block links)
/// - `block_hash`: BLAKE3 hash of normalized block content (whitespace trimmed, markdown preserved)
/// - `heading_occurrence`: Which occurrence of duplicate heading (1-indexed, None if unique)
///
/// # Hash Storage
///
/// Hashes are stored as raw bytes `[u8; 32]` in-memory but converted to hex strings
/// for database storage. The adapter layer handles transparent conversion.
///
/// # Examples
///
/// ```rust,ignore
/// // Wikilink: [[Related Note]]
/// let wikilink = Relation {
///     from_entity_id: "note:source".into(),
///     to_entity_id: Some("note:related".into()),
///     relation_type: "wikilink".into(),
///     metadata: json!({ "link_text": "Related Note" }),
///     ..Default::default()
/// };
///
/// // Block link: [[Note#Heading^5#abc123]]
/// let block_link = Relation {
///     from_entity_id: "note:source".into(),
///     to_entity_id: Some("note:target".into()),
///     relation_type: "wikilink".into(),
///     metadata: json!({ "heading": "Heading", "link_text": "Note" }),
///     block_offset: Some(5),
///     block_hash: Some([0xab, 0xc1, 0x23, ...]), // BLAKE3 hash
///     ..Default::default()
/// };
///
/// // Embed: ![[Summary#Overview^2#def456]]
/// let embed = Relation {
///     from_entity_id: "note:source".into(),
///     to_entity_id: Some("note:summary".into()),
///     relation_type: "embed".into(), // Reversed semantics
///     metadata: json!({ "heading": "Overview", "embedded_content": "Summary" }),
///     block_offset: Some(2),
///     block_hash: Some([0xde, 0xf4, 0x56, ...]),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Relation {
    /// Unique identifier (auto-generated on store)
    pub id: String,

    /// Source entity ID (where the link appears)
    pub from_entity_id: String,

    /// Target entity ID (None if unresolved/ambiguous)
    pub to_entity_id: Option<String>,

    /// Relation type: "wikilink", "embed", "link", "footnote"
    pub relation_type: String,

    /// Type-specific metadata (alias, heading, URL, etc.)
    pub metadata: Value,

    /// Surrounding text context (for breadcrumbs/backlinks)
    pub context: Option<String>,

    /// Block offset for block links (1-indexed, None for regular links)
    pub block_offset: Option<u32>,

    /// BLAKE3 hash of block content (32 bytes, None for non-block links)
    pub block_hash: Option<[u8; 32]>,

    /// Which occurrence of duplicate heading (1-indexed, None if unique)
    pub heading_occurrence: Option<u32>,

    /// When the relation was created
    pub created_at: DateTime<Utc>,
}

impl Default for Relation {
    fn default() -> Self {
        Self {
            id: String::new(),
            from_entity_id: String::new(),
            to_entity_id: None,
            relation_type: String::new(),
            metadata: Value::Null,
            context: None,
            block_offset: None,
            block_hash: None,
            heading_occurrence: None,
            created_at: Utc::now(),
        }
    }
}

impl Relation {
    /// Create a new relation
    pub fn new(from: impl Into<String>, to: Option<String>, relation_type: impl Into<String>) -> Self {
        Self {
            from_entity_id: from.into(),
            to_entity_id: to,
            relation_type: relation_type.into(),
            created_at: Utc::now(),
            ..Default::default()
        }
    }

    /// Create a wikilink relation
    pub fn wikilink(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::new(from, Some(to.into()), "wikilink")
    }

    /// Create an embed relation (reversed semantics - content inclusion)
    pub fn embed(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::new(from, Some(to.into()), "embed")
    }

    /// Create an inline link relation (external URL)
    pub fn link(from: impl Into<String>, url: impl Into<String>) -> Self {
        Self::new(from, Some(url.into()), "link")
    }

    /// Add block link fields (offset, hash, heading occurrence)
    pub fn with_block_link(mut self, offset: u32, hash: [u8; 32], heading_occurrence: Option<u32>) -> Self {
        self.block_offset = Some(offset);
        self.block_hash = Some(hash);
        self.heading_occurrence = heading_occurrence;
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add context (surrounding text)
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Storage for relations between entities (wikilinks, embeds, inline links)
///
/// This trait manages the graph structure of the knowledge base, storing
/// connections between notes, blocks, and external resources.
///
/// # Relation Types
///
/// - **wikilink**: `[[Note]]` - reference to another note
/// - **embed**: `![[Note]]` - content inclusion (reversed semantics)
/// - **link**: `[text](url)` - external or internal URL
/// - **footnote**: `[^1]` - footnote reference
///
/// # Block Links
///
/// Block links (`[[Note#Heading^5#hash]]`) enable content-addressed linking
/// to specific blocks. The hash provides validation and supports CAS lookup
/// if the block moves.
///
/// # Examples
///
/// ```rust,ignore
/// use crucible_core::storage::{RelationStorage, Relation};
///
/// async fn link_notes<S: RelationStorage>(
///     storage: &S,
///     from: &str,
///     to: &str,
/// ) -> StorageResult<()> {
///     let relation = Relation::wikilink(from, to);
///     storage.store_relation(relation).await?;
///     Ok(())
/// }
/// ```
#[async_trait]
pub trait RelationStorage: Send + Sync {
    /// Store a new relation
    ///
    /// # Arguments
    ///
    /// * `relation` - The relation to store
    ///
    /// # Returns
    ///
    /// The generated relation ID
    async fn store_relation(&self, relation: Relation) -> StorageResult<String>;

    /// Store multiple relations in a batch (optimized)
    ///
    /// # Arguments
    ///
    /// * `relations` - Vector of relations to store
    ///
    /// # Performance
    ///
    /// Uses a single database transaction for all relations
    async fn batch_store_relations(&self, relations: &[Relation]) -> StorageResult<()>;

    /// Get a relation by ID
    async fn get_relation(&self, id: &str) -> StorageResult<Option<Relation>>;

    /// Get all relations for an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The source entity ID
    /// * `relation_type` - Optional filter by relation type
    async fn get_relations(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>>;

    /// Get backlinks (incoming relations) to an entity
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The target entity ID
    /// * `relation_type` - Optional filter by relation type
    async fn get_backlinks(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>>;

    /// Delete all relations for an entity
    async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize>;

    /// Delete a specific relation
    async fn delete_relation(&self, id: &str) -> StorageResult<()>;

    /// Find a block by content hash (for block link CAS lookup)
    ///
    /// # Arguments
    ///
    /// * `entity_id` - The entity to search within
    /// * `hash` - BLAKE3 hash of the block content
    ///
    /// # Returns
    ///
    /// Block ID if found, None otherwise
    async fn find_block_by_hash(
        &self,
        entity_id: &str,
        hash: &[u8; 32],
    ) -> StorageResult<Option<String>>;
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

    // ========================================================================
    // RelationStorage Tests
    // ========================================================================

    /// Mock implementation of RelationStorage for testing
    struct MockRelationStorage {
        relations: Arc<Mutex<HashMap<String, Relation>>>,
    }

    impl MockRelationStorage {
        fn new() -> Self {
            Self {
                relations: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn generate_id() -> String {
            format!("rel:{}", uuid::Uuid::new_v4())
        }
    }

    #[async_trait]
    impl RelationStorage for MockRelationStorage {
        async fn store_relation(&self, mut relation: Relation) -> StorageResult<String> {
            let mut store = self.relations.lock().unwrap();

            // Generate ID if not provided
            if relation.id.is_empty() {
                relation.id = Self::generate_id();
            }

            let id = relation.id.clone();
            store.insert(id.clone(), relation);
            Ok(id)
        }

        async fn batch_store_relations(&self, relations: &[Relation]) -> StorageResult<()> {
            let mut store = self.relations.lock().unwrap();

            for rel in relations {
                let mut relation = rel.clone();
                if relation.id.is_empty() {
                    relation.id = Self::generate_id();
                }
                store.insert(relation.id.clone(), relation);
            }

            Ok(())
        }

        async fn get_relation(&self, id: &str) -> StorageResult<Option<Relation>> {
            let store = self.relations.lock().unwrap();
            Ok(store.get(id).cloned())
        }

        async fn get_relations(
            &self,
            entity_id: &str,
            relation_type: Option<&str>,
        ) -> StorageResult<Vec<Relation>> {
            let store = self.relations.lock().unwrap();

            Ok(store
                .values()
                .filter(|r| {
                    r.from_entity_id == entity_id
                        && relation_type.map_or(true, |rt| r.relation_type == rt)
                })
                .cloned()
                .collect())
        }

        async fn get_backlinks(
            &self,
            entity_id: &str,
            relation_type: Option<&str>,
        ) -> StorageResult<Vec<Relation>> {
            let store = self.relations.lock().unwrap();

            Ok(store
                .values()
                .filter(|r| {
                    r.to_entity_id.as_ref() == Some(&entity_id.to_string())
                        && relation_type.map_or(true, |rt| r.relation_type == rt)
                })
                .cloned()
                .collect())
        }

        async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize> {
            let mut store = self.relations.lock().unwrap();
            let before_count = store.len();

            store.retain(|_, r| r.from_entity_id != entity_id);

            let after_count = store.len();
            Ok(before_count - after_count)
        }

        async fn delete_relation(&self, id: &str) -> StorageResult<()> {
            let mut store = self.relations.lock().unwrap();
            store.remove(id);
            Ok(())
        }

        async fn find_block_by_hash(
            &self,
            entity_id: &str,
            hash: &[u8; 32],
        ) -> StorageResult<Option<String>> {
            let store = self.relations.lock().unwrap();

            // Find any relation pointing to this entity with a matching block hash
            Ok(store
                .values()
                .find(|r| {
                    r.to_entity_id.as_ref() == Some(&entity_id.to_string())
                        && r.block_hash.as_ref() == Some(hash)
                })
                .and_then(|r| {
                    // Return a block ID constructed from entity + offset
                    r.block_offset.map(|offset| format!("{}#block_{}", entity_id, offset))
                }))
        }
    }

    #[tokio::test]
    async fn test_relation_storage_basic_operations() {
        let storage = MockRelationStorage::new();
        let now = Utc::now();

        // Test store_relation with builder pattern
        let relation = Relation::wikilink("note:source", "note:target")
            .with_context("See [[Target Note]] for details");

        let id = storage.store_relation(relation).await.unwrap();
        assert!(!id.is_empty());

        // Test get_relation
        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.from_entity_id, "note:source");
        assert_eq!(retrieved.to_entity_id, Some("note:target".to_string()));
        assert_eq!(retrieved.relation_type, "wikilink");
        assert_eq!(retrieved.context, Some("See [[Target Note]] for details".to_string()));

        // Test get_relations
        let relations = storage
            .get_relations("note:source", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].id, id);

        // Test delete_relation
        storage.delete_relation(&id).await.unwrap();
        let deleted = storage.get_relation(&id).await.unwrap();
        assert!(deleted.is_none());
    }

    #[tokio::test]
    async fn test_relation_storage_block_links() {
        let storage = MockRelationStorage::new();

        // Create a block link with hash and offset
        let block_hash = [42u8; 32];
        let relation = Relation::wikilink("note:source", "note:target")
            .with_block_link(5, block_hash, None)
            .with_context("Block 5 under heading");

        let id = storage.store_relation(relation).await.unwrap();

        // Verify block link fields
        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.block_offset, Some(5));
        assert_eq!(retrieved.block_hash, Some(block_hash));
        assert_eq!(retrieved.heading_occurrence, None);

        // Test find_block_by_hash
        let block_id = storage
            .find_block_by_hash("note:target", &block_hash)
            .await
            .unwrap();
        assert_eq!(block_id, Some("note:target#block_5".to_string()));

        // Test with non-existent hash
        let missing_hash = [99u8; 32];
        let missing_block = storage
            .find_block_by_hash("note:target", &missing_hash)
            .await
            .unwrap();
        assert_eq!(missing_block, None);
    }

    #[tokio::test]
    async fn test_relation_storage_batch_operations() {
        let storage = MockRelationStorage::new();

        // Create multiple relations
        let relations = vec![
            Relation::wikilink("note:source", "note:target1"),
            Relation::wikilink("note:source", "note:target2"),
            Relation::embed("note:source", "note:embedded"),
            Relation::link("note:source", "https://example.com"),
        ];

        storage.batch_store_relations(&relations).await.unwrap();

        // Test retrieval with type filtering
        let wikilinks = storage
            .get_relations("note:source", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(wikilinks.len(), 2);

        let embeds = storage
            .get_relations("note:source", Some("embed"))
            .await
            .unwrap();
        assert_eq!(embeds.len(), 1);

        let links = storage
            .get_relations("note:source", Some("link"))
            .await
            .unwrap();
        assert_eq!(links.len(), 1);

        // Test get all relations (no type filter)
        let all_relations = storage.get_relations("note:source", None).await.unwrap();
        assert_eq!(all_relations.len(), 4);
    }

    #[tokio::test]
    async fn test_relation_storage_backlinks() {
        let storage = MockRelationStorage::new();

        // Create relations from multiple sources to same target
        let relations = vec![
            Relation::wikilink("note:source1", "note:target"),
            Relation::wikilink("note:source2", "note:target"),
            Relation::embed("note:source3", "note:target"),
        ];

        storage.batch_store_relations(&relations).await.unwrap();

        // Test get_backlinks with type filter
        let wikilink_backlinks = storage
            .get_backlinks("note:target", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(wikilink_backlinks.len(), 2);

        // Test get all backlinks (no type filter)
        let all_backlinks = storage.get_backlinks("note:target", None).await.unwrap();
        assert_eq!(all_backlinks.len(), 3);

        // Verify backlink sources
        let sources: Vec<_> = all_backlinks
            .iter()
            .map(|r| r.from_entity_id.as_str())
            .collect();
        assert!(sources.contains(&"note:source1"));
        assert!(sources.contains(&"note:source2"));
        assert!(sources.contains(&"note:source3"));
    }

    #[tokio::test]
    async fn test_relation_storage_deletion() {
        let storage = MockRelationStorage::new();

        // Create relations from multiple entities
        let relations = vec![
            Relation::wikilink("note:to_delete", "note:target1"),
            Relation::wikilink("note:to_delete", "note:target2"),
            Relation::wikilink("note:to_keep", "note:target3"),
        ];

        storage.batch_store_relations(&relations).await.unwrap();

        // Test delete_relations for specific entity
        let deleted_count = storage.delete_relations("note:to_delete").await.unwrap();
        assert_eq!(deleted_count, 2);

        // Verify relations are deleted
        let remaining = storage.get_relations("note:to_delete", None).await.unwrap();
        assert_eq!(remaining.len(), 0);

        // Verify other relations remain
        let kept = storage.get_relations("note:to_keep", None).await.unwrap();
        assert_eq!(kept.len(), 1);
    }

    #[tokio::test]
    async fn test_relation_builder_patterns() {
        // Test all builder methods
        let wikilink = Relation::wikilink("note:a", "note:b");
        assert_eq!(wikilink.relation_type, "wikilink");
        assert_eq!(wikilink.to_entity_id, Some("note:b".to_string()));

        let embed = Relation::embed("note:a", "note:b");
        assert_eq!(embed.relation_type, "embed");

        let link = Relation::link("note:a", "https://example.com");
        assert_eq!(link.relation_type, "link");
        assert_eq!(link.to_entity_id, Some("https://example.com".to_string()));

        // Test chaining with metadata and context
        let block_hash = [1u8; 32];
        let complex = Relation::new("note:source", Some("note:target".to_string()), "custom")
            .with_block_link(3, block_hash, Some(2))
            .with_metadata(serde_json::json!({"key": "value"}))
            .with_context("Some context text");

        assert_eq!(complex.relation_type, "custom");
        assert_eq!(complex.block_offset, Some(3));
        assert_eq!(complex.block_hash, Some(block_hash));
        assert_eq!(complex.heading_occurrence, Some(2));
        assert_eq!(complex.metadata, serde_json::json!({"key": "value"}));
        assert_eq!(complex.context, Some("Some context text".to_string()));
    }

    #[tokio::test]
    async fn test_relation_with_heading_occurrence() {
        let storage = MockRelationStorage::new();

        // Test handling duplicate headings
        let hash = [7u8; 32];
        let relation = Relation::wikilink("note:source", "note:target")
            .with_block_link(2, hash, Some(3)) // 3rd occurrence of heading, 2nd block under it
            .with_context("Third 'Overview' heading, second block");

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.heading_occurrence, Some(3));
        assert_eq!(retrieved.block_offset, Some(2));
    }

    #[tokio::test]
    async fn test_relation_with_unresolved_target() {
        let storage = MockRelationStorage::new();

        // Test relation with None target (unresolved/ambiguous)
        let relation = Relation::new("note:source", None, "wikilink")
            .with_context("[[Ambiguous Note]] - multiple matches");

        let id = storage.store_relation(relation).await.unwrap();

        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.to_entity_id, None);
        assert!(retrieved.context.as_ref().unwrap().contains("Ambiguous"));
    }
}
