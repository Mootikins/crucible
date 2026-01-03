//! EAV Storage Contract Tests
//!
//! These tests verify that storage backends conform to the EavGraphStorage trait contract.
//! Any backend implementing the crucible-core EAV traits must pass these tests.
//!
//! # Running Contract Tests
//!
//! ```bash
//! # Test SQLite backend
//! cargo test -p crucible-storage-tests --features sqlite
//!
//! # Test SurrealDB backend
//! cargo test -p crucible-storage-tests --features surrealdb
//! ```
//!
//! # Contract Requirements
//!
//! Each test documents the behavioral contract that all implementations must follow.
//! These are not just "does it work" tests, but "does it behave correctly" tests.

#![cfg(any(feature = "sqlite", feature = "surrealdb"))]

use chrono::Utc;
use crucible_core::storage::eav_graph_traits::{
    AttributeValue, Block, BlockStorage, Entity, EntityStorage, EntityTag, EntityType, Property,
    PropertyNamespace, PropertyStorage, Relation, RelationStorage, Tag, TagStorage,
};

// ============================================================================
// Backend Factories
// ============================================================================

async fn create_entity_storage() -> impl EntityStorage {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        crucible_sqlite::eav::SqliteEntityStorage::new(pool)
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        apply_eav_graph_schema(&client)
            .await
            .expect("Failed to apply schema");
        EAVGraphStore::new(client)
    }
}

async fn create_property_storage() -> (impl PropertyStorage, impl EntityStorage) {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        let entity = crucible_sqlite::eav::SqliteEntityStorage::new(pool.clone());
        let property = crucible_sqlite::eav::SqlitePropertyStorage::new(pool);
        (property, entity)
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        apply_eav_graph_schema(&client)
            .await
            .expect("Failed to apply schema");
        let store = EAVGraphStore::new(client);
        (store.clone(), store)
    }
}

async fn create_relation_storage() -> (impl RelationStorage, impl EntityStorage) {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        let entity = crucible_sqlite::eav::SqliteEntityStorage::new(pool.clone());
        let relation = crucible_sqlite::eav::SqliteRelationStorage::new(pool);
        (relation, entity)
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        apply_eav_graph_schema(&client)
            .await
            .expect("Failed to apply schema");
        let store = EAVGraphStore::new(client);
        (store.clone(), store)
    }
}

async fn create_block_storage() -> (impl BlockStorage, impl EntityStorage) {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        let entity = crucible_sqlite::eav::SqliteEntityStorage::new(pool.clone());
        let block = crucible_sqlite::eav::SqliteBlockStorage::new(pool);
        (block, entity)
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        apply_eav_graph_schema(&client)
            .await
            .expect("Failed to apply schema");
        let store = EAVGraphStore::new(client);
        (store.clone(), store)
    }
}

async fn create_tag_storage() -> (impl TagStorage, impl EntityStorage) {
    #[cfg(feature = "sqlite")]
    {
        let pool = crucible_sqlite::SqlitePool::memory().expect("Failed to create SQLite pool");
        let entity = crucible_sqlite::eav::SqliteEntityStorage::new(pool.clone());
        let tag = crucible_sqlite::eav::SqliteTagStorage::new(pool);
        (tag, entity)
    }

    #[cfg(feature = "surrealdb")]
    {
        use crucible_surrealdb::test_utils::{apply_eav_graph_schema, EAVGraphStore, SurrealClient};

        let client = SurrealClient::new_memory()
            .await
            .expect("Failed to create SurrealDB client");
        apply_eav_graph_schema(&client)
            .await
            .expect("Failed to apply schema");
        let store = EAVGraphStore::new(client);
        (store.clone(), store)
    }
}

// ============================================================================
// EntityStorage Contract Tests
// ============================================================================

mod entity_contract {
    use super::*;

    /// CONTRACT: store_entity returns the entity ID
    #[tokio::test]
    async fn store_returns_entity_id() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:test".to_string(), EntityType::Note);

        let id = storage.store_entity(entity).await.unwrap();

        assert_eq!(id, "note:test");
    }

    /// CONTRACT: get_entity returns None for non-existent entity
    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let storage = create_entity_storage().await;

        let result = storage.get_entity("does:not:exist").await.unwrap();

        assert!(result.is_none());
    }

    /// CONTRACT: get_entity returns stored entity with all fields preserved
    #[tokio::test]
    async fn get_returns_stored_entity() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:roundtrip".to_string(), EntityType::Note)
            .with_content_hash("sha256:abc123")
            .with_vault_id("vault:main");

        storage.store_entity(entity).await.unwrap();
        let retrieved = storage.get_entity("note:roundtrip").await.unwrap().unwrap();

        assert_eq!(retrieved.id, "note:roundtrip");
        assert_eq!(retrieved.entity_type, EntityType::Note);
        assert_eq!(retrieved.content_hash, Some("sha256:abc123".to_string()));
        assert_eq!(retrieved.vault_id, Some("vault:main".to_string()));
        assert_eq!(retrieved.version, 1);
        assert!(retrieved.deleted_at.is_none());
    }

    /// CONTRACT: entity_exists returns true for existing entity
    #[tokio::test]
    async fn exists_returns_true_for_existing() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:exists".to_string(), EntityType::Note);

        storage.store_entity(entity).await.unwrap();

        assert!(storage.entity_exists("note:exists").await.unwrap());
    }

    /// CONTRACT: entity_exists returns false for non-existent entity
    #[tokio::test]
    async fn exists_returns_false_for_nonexistent() {
        let storage = create_entity_storage().await;

        assert!(!storage.entity_exists("note:nope").await.unwrap());
    }

    /// CONTRACT: update_entity fails for non-existent entity
    #[tokio::test]
    async fn update_nonexistent_fails() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:ghost".to_string(), EntityType::Note);

        let result = storage.update_entity("note:ghost", entity).await;

        assert!(result.is_err());
    }

    /// CONTRACT: update_entity modifies existing entity
    #[tokio::test]
    async fn update_modifies_entity() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:update".to_string(), EntityType::Note);
        storage.store_entity(entity).await.unwrap();

        let mut updated = storage.get_entity("note:update").await.unwrap().unwrap();
        updated.content_hash = Some("new_hash".to_string());
        updated.version = 2;
        storage.update_entity("note:update", updated).await.unwrap();

        let after = storage.get_entity("note:update").await.unwrap().unwrap();
        assert_eq!(after.content_hash, Some("new_hash".to_string()));
        assert_eq!(after.version, 2);
    }

    /// CONTRACT: delete_entity sets deleted_at timestamp (soft delete)
    #[tokio::test]
    async fn delete_is_soft_delete() {
        let storage = create_entity_storage().await;
        let entity = Entity::new("note:delete".to_string(), EntityType::Note);
        storage.store_entity(entity).await.unwrap();

        storage.delete_entity("note:delete").await.unwrap();

        let deleted = storage.get_entity("note:delete").await.unwrap().unwrap();
        assert!(deleted.deleted_at.is_some());
    }

    /// CONTRACT: delete_entity on non-existent is a no-op (no error)
    #[tokio::test]
    async fn delete_nonexistent_is_noop() {
        let storage = create_entity_storage().await;

        // Should not error
        let result = storage.delete_entity("note:never:existed").await;
        assert!(result.is_ok());
    }

    /// CONTRACT: store_entity with same ID updates (upsert semantics)
    #[tokio::test]
    async fn store_same_id_upserts() {
        let storage = create_entity_storage().await;

        let entity1 = Entity::new("note:upsert".to_string(), EntityType::Note)
            .with_content_hash("hash1");
        storage.store_entity(entity1).await.unwrap();

        let entity2 = Entity::new("note:upsert".to_string(), EntityType::Note)
            .with_content_hash("hash2");
        storage.store_entity(entity2).await.unwrap();

        let retrieved = storage.get_entity("note:upsert").await.unwrap().unwrap();
        assert_eq!(retrieved.content_hash, Some("hash2".to_string()));
    }

    /// CONTRACT: all EntityType variants are stored and retrieved correctly
    #[tokio::test]
    async fn all_entity_types_preserved() {
        let storage = create_entity_storage().await;

        let types = vec![
            (EntityType::Note, "note:1"),
            (EntityType::Block, "block:1"),
            (EntityType::Tag, "tag:1"),
            (EntityType::Section, "section:1"),
            (EntityType::Media, "media:1"),
            (EntityType::Person, "person:1"),
        ];

        for (entity_type, id) in types {
            let entity = Entity::new(id.to_string(), entity_type);
            storage.store_entity(entity).await.unwrap();

            let retrieved = storage.get_entity(id).await.unwrap().unwrap();
            assert_eq!(
                retrieved.entity_type, entity_type,
                "EntityType mismatch for {}",
                id
            );
        }
    }

    /// CONTRACT: unicode content is preserved exactly
    #[tokio::test]
    async fn unicode_preserved() {
        let storage = create_entity_storage().await;

        let entity = Entity::new("note:unicode".to_string(), EntityType::Note)
            .with_content_hash("🔥火事📝مرحبا日本語");
        storage.store_entity(entity).await.unwrap();

        let retrieved = storage.get_entity("note:unicode").await.unwrap().unwrap();
        assert_eq!(
            retrieved.content_hash,
            Some("🔥火事📝مرحبا日本語".to_string())
        );
    }
}

// ============================================================================
// PropertyStorage Contract Tests
// ============================================================================

mod property_contract {
    use super::*;

    /// CONTRACT: batch_upsert_properties returns count of properties stored
    #[tokio::test]
    async fn batch_upsert_returns_count() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:props".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let props = vec![
            Property {
                entity_id: "note:props".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "key1".to_string(),
                value: AttributeValue::Text("val1".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:props".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "key2".to_string(),
                value: AttributeValue::Number(42.0),
                created_at: now,
                updated_at: now,
            },
        ];

        let count = storage.batch_upsert_properties(props).await.unwrap();

        assert_eq!(count, 2);
    }

    /// CONTRACT: get_property returns None for non-existent property
    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:empty".to_string(), EntityType::Note))
            .await
            .unwrap();

        let result = storage
            .get_property("note:empty", &PropertyNamespace::core(), "nope")
            .await
            .unwrap();

        assert!(result.is_none());
    }

    /// CONTRACT: all AttributeValue types are preserved
    #[tokio::test]
    async fn all_value_types_preserved() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:types".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let date = chrono::NaiveDate::from_ymd_opt(2024, 12, 25).unwrap();
        let props = vec![
            Property {
                entity_id: "note:types".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "text".to_string(),
                value: AttributeValue::Text("hello".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:types".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "number".to_string(),
                value: AttributeValue::Number(3.14),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:types".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "bool".to_string(),
                value: AttributeValue::Bool(true),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:types".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "date".to_string(),
                value: AttributeValue::Date(date),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:types".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "json".to_string(),
                value: AttributeValue::Json(serde_json::json!({"key": "value"})),
                created_at: now,
                updated_at: now,
            },
        ];

        storage.batch_upsert_properties(props).await.unwrap();

        // Verify each type
        let text = storage
            .get_property("note:types", &PropertyNamespace::frontmatter(), "text")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(text.value, AttributeValue::Text("hello".to_string()));

        let number = storage
            .get_property("note:types", &PropertyNamespace::frontmatter(), "number")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(number.value, AttributeValue::Number(3.14));

        let bool_val = storage
            .get_property("note:types", &PropertyNamespace::frontmatter(), "bool")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bool_val.value, AttributeValue::Bool(true));

        let date_val = storage
            .get_property("note:types", &PropertyNamespace::frontmatter(), "date")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(date_val.value, AttributeValue::Date(date));
    }

    /// CONTRACT: upsert with same key updates value
    #[tokio::test]
    async fn upsert_updates_existing() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:upsert".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let prop1 = Property {
            entity_id: "note:upsert".to_string(),
            namespace: PropertyNamespace::core(),
            key: "version".to_string(),
            value: AttributeValue::Number(1.0),
            created_at: now,
            updated_at: now,
        };
        storage.batch_upsert_properties(vec![prop1]).await.unwrap();

        let prop2 = Property {
            entity_id: "note:upsert".to_string(),
            namespace: PropertyNamespace::core(),
            key: "version".to_string(),
            value: AttributeValue::Number(2.0),
            created_at: now,
            updated_at: Utc::now(),
        };
        storage.batch_upsert_properties(vec![prop2]).await.unwrap();

        let props = storage
            .get_properties_by_namespace("note:upsert", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].value, AttributeValue::Number(2.0));
    }

    /// CONTRACT: get_properties_by_namespace only returns matching namespace
    #[tokio::test]
    async fn namespace_filtering_works() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:ns".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let props = vec![
            Property {
                entity_id: "note:ns".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "fm".to_string(),
                value: AttributeValue::Text("frontmatter".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:ns".to_string(),
                namespace: PropertyNamespace::core(),
                key: "core".to_string(),
                value: AttributeValue::Text("core".to_string()),
                created_at: now,
                updated_at: now,
            },
        ];
        storage.batch_upsert_properties(props).await.unwrap();

        let fm_props = storage
            .get_properties_by_namespace("note:ns", &PropertyNamespace::frontmatter())
            .await
            .unwrap();
        assert_eq!(fm_props.len(), 1);
        assert_eq!(fm_props[0].key, "fm");

        let core_props = storage
            .get_properties_by_namespace("note:ns", &PropertyNamespace::core())
            .await
            .unwrap();
        assert_eq!(core_props.len(), 1);
        assert_eq!(core_props[0].key, "core");
    }

    /// CONTRACT: delete_properties removes all properties for entity
    #[tokio::test]
    async fn delete_removes_all() {
        let (storage, entity_storage) = create_property_storage().await;
        entity_storage
            .store_entity(Entity::new("note:del".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let props = vec![
            Property {
                entity_id: "note:del".to_string(),
                namespace: PropertyNamespace::frontmatter(),
                key: "k1".to_string(),
                value: AttributeValue::Text("v1".to_string()),
                created_at: now,
                updated_at: now,
            },
            Property {
                entity_id: "note:del".to_string(),
                namespace: PropertyNamespace::core(),
                key: "k2".to_string(),
                value: AttributeValue::Text("v2".to_string()),
                created_at: now,
                updated_at: now,
            },
        ];
        storage.batch_upsert_properties(props).await.unwrap();

        let deleted = storage.delete_properties("note:del").await.unwrap();
        assert_eq!(deleted, 2);

        let remaining = storage.get_properties("note:del").await.unwrap();
        assert!(remaining.is_empty());
    }
}

// ============================================================================
// RelationStorage Contract Tests
// ============================================================================

mod relation_contract {
    use super::*;

    /// CONTRACT: store_relation returns generated relation ID
    #[tokio::test]
    async fn store_returns_id() {
        let (storage, entity_storage) = create_relation_storage().await;
        entity_storage
            .store_entity(Entity::new("note:from".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:to".to_string(), EntityType::Note))
            .await
            .unwrap();

        let relation = Relation::wikilink("note:from", "note:to");
        let id = storage.store_relation(relation).await.unwrap();

        assert!(!id.is_empty());
        assert!(id.starts_with("rel:"));
    }

    /// CONTRACT: get_relation returns None for non-existent
    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (storage, _) = create_relation_storage().await;

        let result = storage.get_relation("rel:ghost").await.unwrap();

        assert!(result.is_none());
    }

    /// CONTRACT: relations with block links preserve hash and offset
    #[tokio::test]
    async fn block_link_fields_preserved() {
        let (storage, entity_storage) = create_relation_storage().await;
        entity_storage
            .store_entity(Entity::new("note:src".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:tgt".to_string(), EntityType::Note))
            .await
            .unwrap();

        let hash = [42u8; 32];
        let relation = Relation::wikilink("note:src", "note:tgt").with_block_link(5, hash, Some(2));

        let id = storage.store_relation(relation).await.unwrap();
        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();

        assert_eq!(retrieved.block_offset, Some(5));
        assert_eq!(retrieved.block_hash, Some(hash));
        assert_eq!(retrieved.heading_occurrence, Some(2));
    }

    /// CONTRACT: get_backlinks returns all incoming relations
    #[tokio::test]
    async fn backlinks_returns_incoming() {
        let (storage, entity_storage) = create_relation_storage().await;
        entity_storage
            .store_entity(Entity::new("note:a".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:b".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:c".to_string(), EntityType::Note))
            .await
            .unwrap();

        storage
            .batch_store_relations(&[
                Relation::wikilink("note:a", "note:c"),
                Relation::wikilink("note:b", "note:c"),
            ])
            .await
            .unwrap();

        let backlinks = storage.get_backlinks("note:c", None).await.unwrap();

        assert_eq!(backlinks.len(), 2);
    }

    /// CONTRACT: get_relations with type filter only returns matching type
    #[tokio::test]
    async fn type_filter_works() {
        let (storage, entity_storage) = create_relation_storage().await;
        entity_storage
            .store_entity(Entity::new("note:src".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:t1".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:t2".to_string(), EntityType::Note))
            .await
            .unwrap();

        storage
            .batch_store_relations(&[
                Relation::wikilink("note:src", "note:t1"),
                Relation::embed("note:src", "note:t2"),
            ])
            .await
            .unwrap();

        let wikilinks = storage
            .get_relations("note:src", Some("wikilink"))
            .await
            .unwrap();
        assert_eq!(wikilinks.len(), 1);
        assert_eq!(wikilinks[0].relation_type, "wikilink");

        let embeds = storage
            .get_relations("note:src", Some("embed"))
            .await
            .unwrap();
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0].relation_type, "embed");
    }

    /// CONTRACT: relations with None target (unresolved) are stored correctly
    #[tokio::test]
    async fn unresolved_target_preserved() {
        let (storage, entity_storage) = create_relation_storage().await;
        entity_storage
            .store_entity(Entity::new("note:src".to_string(), EntityType::Note))
            .await
            .unwrap();

        let relation = Relation::new("note:src", None, "wikilink")
            .with_context("[[Ambiguous]] - unresolved");

        let id = storage.store_relation(relation).await.unwrap();
        let retrieved = storage.get_relation(&id).await.unwrap().unwrap();

        assert!(retrieved.to_entity_id.is_none());
    }

    /// CONTRACT: empty batch_store_relations succeeds
    #[tokio::test]
    async fn empty_batch_succeeds() {
        let (storage, _) = create_relation_storage().await;

        let result = storage.batch_store_relations(&[]).await;

        assert!(result.is_ok());
    }
}

// ============================================================================
// BlockStorage Contract Tests
// ============================================================================

mod block_contract {
    use super::*;

    /// CONTRACT: store_block returns block ID
    #[tokio::test]
    async fn store_returns_id() {
        let (storage, entity_storage) = create_block_storage().await;
        entity_storage
            .store_entity(Entity::new("note:1".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        let block = Block {
            id: "block:test".to_string(),
            entity_id: "note:1".to_string(),
            parent_block_id: None,
            content: "# Heading".to_string(),
            block_type: "heading".to_string(),
            position: 0,
            created_at: now,
            updated_at: now,
            content_hash: Some("hash123".to_string()),
        };

        let id = storage.store_block(block).await.unwrap();

        assert_eq!(id, "block:test");
    }

    /// CONTRACT: get_child_blocks returns only direct children
    #[tokio::test]
    async fn child_blocks_returns_direct_children() {
        let (storage, entity_storage) = create_block_storage().await;
        entity_storage
            .store_entity(Entity::new("note:1".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();

        // Parent
        storage
            .store_block(Block {
                id: "block:parent".to_string(),
                entity_id: "note:1".to_string(),
                parent_block_id: None,
                content: "Parent".to_string(),
                block_type: "section".to_string(),
                position: 0,
                created_at: now,
                updated_at: now,
                content_hash: None,
            })
            .await
            .unwrap();

        // Children
        for i in 0..3 {
            storage
                .store_block(Block {
                    id: format!("block:child{}", i),
                    entity_id: "note:1".to_string(),
                    parent_block_id: Some("block:parent".to_string()),
                    content: format!("Child {}", i),
                    block_type: "paragraph".to_string(),
                    position: i,
                    created_at: now,
                    updated_at: now,
                    content_hash: None,
                })
                .await
                .unwrap();
        }

        let children = storage.get_child_blocks("block:parent").await.unwrap();

        assert_eq!(children.len(), 3);
    }

    /// CONTRACT: get_blocks returns all blocks for entity
    #[tokio::test]
    async fn get_blocks_returns_all_for_entity() {
        let (storage, entity_storage) = create_block_storage().await;
        entity_storage
            .store_entity(Entity::new("note:1".to_string(), EntityType::Note))
            .await
            .unwrap();
        entity_storage
            .store_entity(Entity::new("note:2".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();

        // Blocks for note:1
        for i in 0..3 {
            storage
                .store_block(Block {
                    id: format!("block:1-{}", i),
                    entity_id: "note:1".to_string(),
                    parent_block_id: None,
                    content: format!("Block {}", i),
                    block_type: "paragraph".to_string(),
                    position: i,
                    created_at: now,
                    updated_at: now,
                    content_hash: None,
                })
                .await
                .unwrap();
        }

        // Block for note:2
        storage
            .store_block(Block {
                id: "block:2-0".to_string(),
                entity_id: "note:2".to_string(),
                parent_block_id: None,
                content: "Other note".to_string(),
                block_type: "paragraph".to_string(),
                position: 0,
                created_at: now,
                updated_at: now,
                content_hash: None,
            })
            .await
            .unwrap();

        let blocks = storage.get_blocks("note:1").await.unwrap();

        assert_eq!(blocks.len(), 3);
    }

    /// CONTRACT: delete_blocks returns count of deleted blocks
    #[tokio::test]
    async fn delete_returns_count() {
        let (storage, entity_storage) = create_block_storage().await;
        entity_storage
            .store_entity(Entity::new("note:del".to_string(), EntityType::Note))
            .await
            .unwrap();

        let now = Utc::now();
        for i in 0..5 {
            storage
                .store_block(Block {
                    id: format!("block:del-{}", i),
                    entity_id: "note:del".to_string(),
                    parent_block_id: None,
                    content: format!("Block {}", i),
                    block_type: "paragraph".to_string(),
                    position: i,
                    created_at: now,
                    updated_at: now,
                    content_hash: None,
                })
                .await
                .unwrap();
        }

        let deleted = storage.delete_blocks("note:del").await.unwrap();

        assert_eq!(deleted, 5);
    }
}

// ============================================================================
// TagStorage Contract Tests
// ============================================================================

mod tag_contract {
    use super::*;

    /// CONTRACT: store_tag returns tag name
    #[tokio::test]
    async fn store_returns_name() {
        let (storage, _) = create_tag_storage().await;

        let now = Utc::now();
        let tag = Tag {
            id: "project".to_string(),
            name: "project".to_string(),
            parent_tag_id: None,
            created_at: now,
            updated_at: now,
        };

        let name = storage.store_tag(tag).await.unwrap();

        assert_eq!(name, "project");
    }

    /// CONTRACT: get_child_tags returns direct children
    #[tokio::test]
    async fn child_tags_returns_direct_children() {
        let (storage, _) = create_tag_storage().await;
        let now = Utc::now();

        // Parent tag
        storage
            .store_tag(Tag {
                id: "project".to_string(),
                name: "project".to_string(),
                parent_tag_id: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        // Child tags
        for child in ["project/ai", "project/web", "project/cli"] {
            storage
                .store_tag(Tag {
                    id: child.to_string(),
                    name: child.to_string(),
                    parent_tag_id: Some("project".to_string()),
                    created_at: now,
                    updated_at: now,
                })
                .await
                .unwrap();
        }

        let children = storage.get_child_tags("project").await.unwrap();

        assert_eq!(children.len(), 3);
    }

    /// CONTRACT: associate_tag links entity to tag
    #[tokio::test]
    async fn association_works() {
        let (storage, entity_storage) = create_tag_storage().await;
        let now = Utc::now();

        entity_storage
            .store_entity(Entity::new("note:tagged".to_string(), EntityType::Note))
            .await
            .unwrap();

        storage
            .store_tag(Tag {
                id: "status/active".to_string(),
                name: "status/active".to_string(),
                parent_tag_id: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        storage
            .associate_tag(EntityTag {
                entity_id: "note:tagged".to_string(),
                tag_id: "status/active".to_string(),
                created_at: now,
            })
            .await
            .unwrap();

        let entity_tags = storage.get_entity_tags("note:tagged").await.unwrap();

        assert_eq!(entity_tags.len(), 1);
        assert_eq!(entity_tags[0].name, "status/active");
    }

    /// CONTRACT: dissociate_tag removes entity-tag link
    #[tokio::test]
    async fn dissociate_removes_link() {
        let (storage, entity_storage) = create_tag_storage().await;
        let now = Utc::now();

        entity_storage
            .store_entity(Entity::new("note:untagged".to_string(), EntityType::Note))
            .await
            .unwrap();

        storage
            .store_tag(Tag {
                id: "temp".to_string(),
                name: "temp".to_string(),
                parent_tag_id: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();

        storage
            .associate_tag(EntityTag {
                entity_id: "note:untagged".to_string(),
                tag_id: "temp".to_string(),
                created_at: now,
            })
            .await
            .unwrap();

        storage
            .dissociate_tag("note:untagged", "temp")
            .await
            .unwrap();

        let tags = storage.get_entity_tags("note:untagged").await.unwrap();

        assert!(tags.is_empty());
    }
}
