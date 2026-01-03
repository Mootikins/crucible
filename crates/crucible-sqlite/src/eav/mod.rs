//! EAV+Graph storage implementations
//!
//! This module provides SQLite implementations of the core storage traits:
//! - `EntityStorage` - Entity CRUD operations
//! - `PropertyStorage` - Namespaced property storage
//! - `RelationStorage` - Graph edges (wikilinks, embeds)
//! - `BlockStorage` - Hierarchical content blocks
//! - `TagStorage` - Tag taxonomy management

mod block;
mod entity;
mod property;
mod relation;
mod tag;

pub use block::SqliteBlockStorage;
pub use entity::SqliteEntityStorage;
pub use property::SqlitePropertyStorage;
pub use relation::SqliteRelationStorage;
pub use tag::SqliteTagStorage;

use crate::connection::SqlitePool;
use async_trait::async_trait;
use crucible_core::storage::eav_graph_traits::{
    Block, BlockStorage, Entity, EntityStorage, EntityTag, Property, PropertyNamespace,
    PropertyStorage, Relation, RelationStorage, Tag, TagStorage,
};
use crucible_core::storage::StorageResult;

/// Combined EAV+Graph storage implementation
///
/// This struct provides a unified interface to all EAV storage traits,
/// sharing a single connection pool. It implements the `EavGraphStorage`
/// composite trait from crucible-core.
///
/// # Example
///
/// ```rust,ignore
/// use crucible_sqlite::{SqliteConfig, SqlitePool};
/// use crucible_sqlite::eav::EavGraphStore;
/// use crucible_core::storage::EavGraphStorage;
///
/// let pool = SqlitePool::new(SqliteConfig::memory())?;
/// let store = EavGraphStore::new(pool);
///
/// // Use as unified EavGraphStorage
/// let entity = store.get_entity("note:example").await?;
/// ```
#[derive(Clone)]
pub struct EavGraphStore {
    entity: SqliteEntityStorage,
    property: SqlitePropertyStorage,
    relation: SqliteRelationStorage,
    block: SqliteBlockStorage,
    tag: SqliteTagStorage,
}

impl EavGraphStore {
    /// Create a new EAV+Graph store with the given connection pool
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            entity: SqliteEntityStorage::new(pool.clone()),
            property: SqlitePropertyStorage::new(pool.clone()),
            relation: SqliteRelationStorage::new(pool.clone()),
            block: SqliteBlockStorage::new(pool.clone()),
            tag: SqliteTagStorage::new(pool),
        }
    }

    /// Get the entity storage component
    pub fn entity(&self) -> &SqliteEntityStorage {
        &self.entity
    }

    /// Get the property storage component
    pub fn property(&self) -> &SqlitePropertyStorage {
        &self.property
    }

    /// Get the relation storage component
    pub fn relation(&self) -> &SqliteRelationStorage {
        &self.relation
    }

    /// Get the block storage component
    pub fn block(&self) -> &SqliteBlockStorage {
        &self.block
    }

    /// Get the tag storage component
    pub fn tag(&self) -> &SqliteTagStorage {
        &self.tag
    }
}

// ============================================================================
// EntityStorage Implementation (delegates to SqliteEntityStorage)
// ============================================================================

#[async_trait]
impl EntityStorage for EavGraphStore {
    async fn store_entity(&self, entity: Entity) -> StorageResult<String> {
        self.entity.store_entity(entity).await
    }

    async fn get_entity(&self, id: &str) -> StorageResult<Option<Entity>> {
        self.entity.get_entity(id).await
    }

    async fn update_entity(&self, id: &str, entity: Entity) -> StorageResult<()> {
        self.entity.update_entity(id, entity).await
    }

    async fn delete_entity(&self, id: &str) -> StorageResult<()> {
        self.entity.delete_entity(id).await
    }

    async fn entity_exists(&self, id: &str) -> StorageResult<bool> {
        self.entity.entity_exists(id).await
    }
}

// ============================================================================
// PropertyStorage Implementation (delegates to SqlitePropertyStorage)
// ============================================================================

#[async_trait]
impl PropertyStorage for EavGraphStore {
    async fn batch_upsert_properties(&self, properties: Vec<Property>) -> StorageResult<usize> {
        self.property.batch_upsert_properties(properties).await
    }

    async fn get_properties(&self, entity_id: &str) -> StorageResult<Vec<Property>> {
        self.property.get_properties(entity_id).await
    }

    async fn get_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<Vec<Property>> {
        self.property
            .get_properties_by_namespace(entity_id, namespace)
            .await
    }

    async fn get_property(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
        key: &str,
    ) -> StorageResult<Option<Property>> {
        self.property.get_property(entity_id, namespace, key).await
    }

    async fn delete_properties(&self, entity_id: &str) -> StorageResult<usize> {
        self.property.delete_properties(entity_id).await
    }

    async fn delete_properties_by_namespace(
        &self,
        entity_id: &str,
        namespace: &PropertyNamespace,
    ) -> StorageResult<usize> {
        self.property
            .delete_properties_by_namespace(entity_id, namespace)
            .await
    }
}

// ============================================================================
// RelationStorage Implementation (delegates to SqliteRelationStorage)
// ============================================================================

#[async_trait]
impl RelationStorage for EavGraphStore {
    async fn store_relation(&self, relation: Relation) -> StorageResult<String> {
        self.relation.store_relation(relation).await
    }

    async fn batch_store_relations(&self, relations: &[Relation]) -> StorageResult<()> {
        self.relation.batch_store_relations(relations).await
    }

    async fn get_relation(&self, id: &str) -> StorageResult<Option<Relation>> {
        self.relation.get_relation(id).await
    }

    async fn get_relations(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>> {
        self.relation.get_relations(entity_id, relation_type).await
    }

    async fn get_backlinks(
        &self,
        entity_id: &str,
        relation_type: Option<&str>,
    ) -> StorageResult<Vec<Relation>> {
        self.relation.get_backlinks(entity_id, relation_type).await
    }

    async fn delete_relations(&self, entity_id: &str) -> StorageResult<usize> {
        self.relation.delete_relations(entity_id).await
    }

    async fn delete_relation(&self, id: &str) -> StorageResult<()> {
        self.relation.delete_relation(id).await
    }

    async fn find_block_by_hash(
        &self,
        entity_id: &str,
        hash: &[u8; 32],
    ) -> StorageResult<Option<String>> {
        self.relation.find_block_by_hash(entity_id, hash).await
    }
}

// ============================================================================
// BlockStorage Implementation (delegates to SqliteBlockStorage)
// ============================================================================

#[async_trait]
impl BlockStorage for EavGraphStore {
    async fn store_block(&self, block: Block) -> StorageResult<String> {
        self.block.store_block(block).await
    }

    async fn get_block(&self, id: &str) -> StorageResult<Option<Block>> {
        self.block.get_block(id).await
    }

    async fn get_blocks(&self, entity_id: &str) -> StorageResult<Vec<Block>> {
        self.block.get_blocks(entity_id).await
    }

    async fn get_child_blocks(&self, parent_block_id: &str) -> StorageResult<Vec<Block>> {
        self.block.get_child_blocks(parent_block_id).await
    }

    async fn update_block(&self, id: &str, block: Block) -> StorageResult<()> {
        self.block.update_block(id, block).await
    }

    async fn delete_block(&self, id: &str, recursive: bool) -> StorageResult<usize> {
        self.block.delete_block(id, recursive).await
    }

    async fn delete_blocks(&self, entity_id: &str) -> StorageResult<usize> {
        self.block.delete_blocks(entity_id).await
    }
}

// ============================================================================
// TagStorage Implementation (delegates to SqliteTagStorage)
// ============================================================================

#[async_trait]
impl TagStorage for EavGraphStore {
    async fn store_tag(&self, tag: Tag) -> StorageResult<String> {
        self.tag.store_tag(tag).await
    }

    async fn get_tag(&self, name: &str) -> StorageResult<Option<Tag>> {
        self.tag.get_tag(name).await
    }

    async fn get_child_tags(&self, parent_tag_name: &str) -> StorageResult<Vec<Tag>> {
        self.tag.get_child_tags(parent_tag_name).await
    }

    async fn associate_tag(&self, entity_tag: EntityTag) -> StorageResult<()> {
        self.tag.associate_tag(entity_tag).await
    }

    async fn get_entity_tags(&self, entity_id: &str) -> StorageResult<Vec<Tag>> {
        self.tag.get_entity_tags(entity_id).await
    }

    async fn get_entities_by_tag(&self, tag_id: &str) -> StorageResult<Vec<String>> {
        self.tag.get_entities_by_tag(tag_id).await
    }

    async fn dissociate_tag(&self, entity_id: &str, tag_id: &str) -> StorageResult<()> {
        self.tag.dissociate_tag(entity_id, tag_id).await
    }

    async fn delete_tag(&self, id: &str, delete_associations: bool) -> StorageResult<usize> {
        self.tag.delete_tag(id, delete_associations).await
    }
}

// EavGraphStore now automatically implements EavGraphStorage via blanket impl

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::storage::eav_graph_traits::{Entity, EntityType};

    #[tokio::test]
    async fn test_eav_graph_store_unified_access() {
        let pool = SqlitePool::memory().unwrap();
        let store = EavGraphStore::new(pool);

        // Test via EntityStorage trait
        let entity = Entity::new("note:test".to_string(), EntityType::Note);
        let id = store.store_entity(entity).await.unwrap();
        assert_eq!(id, "note:test");

        // Verify through unified interface
        let retrieved = store.get_entity("note:test").await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_eav_graph_storage_trait_object() {
        use crucible_core::storage::EavGraphStorage;

        let pool = SqlitePool::memory().unwrap();
        let store = EavGraphStore::new(pool);

        // Verify it can be used as trait object
        fn use_storage(_storage: &dyn EavGraphStorage) {
            // This compiles, proving EavGraphStore implements EavGraphStorage
        }
        use_storage(&store);
    }
}
