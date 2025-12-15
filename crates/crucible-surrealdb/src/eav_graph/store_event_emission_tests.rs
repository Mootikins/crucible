//! Tests for EAVGraphStore direct event emission.
//!
//! These tests verify that EAVGraphStore correctly emits SessionEvents
//! when configured with an EventEmitter via `with_emitter()`.

#[cfg(test)]
mod tests {
    use crate::eav_graph::{apply_eav_graph_schema, EAVGraphStore};
    use crate::surreal_client::SurrealClient;
    use chrono::Utc;
    use crucible_core::events::{EntityType, EventEmitter, SessionEvent};
    use crucible_core::test_support::mocks::MockEventEmitter;
    use std::sync::Arc;

    use crate::eav_graph::types::{
        BlockNode, BlockRecord, Entity, EntityRecord, EntityType as SurrealEntityType, RecordId,
        Relation, RelationRecord,
    };

    /// Helper to create an in-memory SurrealDB client with schema applied.
    async fn setup_client() -> SurrealClient {
        let client = SurrealClient::new_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        client
    }

    /// Helper to create a test entity.
    fn make_test_entity(id: &str) -> Entity {
        Entity {
            id: Some(RecordId::new("entities", id)),
            entity_type: SurrealEntityType::Note,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            version: 1,
            content_hash: Some(format!("hash_{}", id)),
            created_by: Some("test".to_string()),
            vault_id: Some("test_vault".to_string()),
            data: Some(serde_json::json!({"title": "Test Note"})),
        }
    }

    /// Helper to create test blocks.
    fn make_test_blocks(entity_id: &str, count: usize) -> Vec<BlockNode> {
        (0..count)
            .map(|i| BlockNode {
                id: Some(RecordId::new("blocks", format!("{}_{}", entity_id, i))),
                entity_id: RecordId::new("entities", entity_id),
                block_index: i as i32,
                block_type: "paragraph".to_string(),
                content: format!("Test block content {}", i),
                content_hash: format!("block_hash_{}_{}", entity_id, i),
                start_offset: Some((i * 100) as i32),
                end_offset: Some(((i + 1) * 100) as i32),
                start_line: Some(i as i32),
                end_line: Some((i + 1) as i32),
                parent_block_id: None,
                depth: Some(0),
                metadata: serde_json::json!({}),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .collect()
    }

    // ========================================================================
    // EntityStored event tests
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_entity_emits_entity_stored_event() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Upsert an entity
        let entity = make_test_entity("test_note");
        store.upsert_entity(&entity).await.unwrap();

        // Verify an event was emitted
        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());

        // Verify the event is EntityStored with correct fields
        match &events[0] {
            SessionEvent::EntityStored {
                entity_id,
                entity_type,
            } => {
                assert!(
                    entity_id.contains("test_note"),
                    "entity_id should contain 'test_note', got: {}",
                    entity_id
                );
                assert_eq!(
                    *entity_type,
                    EntityType::Note,
                    "entity_type should be Note"
                );
            }
            other => panic!("Expected EntityStored event, got: {:?}", other),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_entity_no_event_without_emitter() {
        let client = setup_client().await;

        // Create store WITHOUT emitter
        let store = EAVGraphStore::new(client);

        // Upsert should succeed without errors even without emitter
        let entity = make_test_entity("test_note_no_emitter");
        let result = store.upsert_entity(&entity).await;

        assert!(result.is_ok(), "Upsert should succeed without emitter");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_entity_emits_correct_entity_type_for_block() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Upsert a block entity
        let entity = Entity {
            id: Some(RecordId::new("entities", "test_block")),
            entity_type: SurrealEntityType::Block,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            version: 1,
            content_hash: Some("block_hash".to_string()),
            created_by: None,
            vault_id: None,
            data: None,
        };
        store.upsert_entity(&entity).await.unwrap();

        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1);

        match &events[0] {
            SessionEvent::EntityStored { entity_type, .. } => {
                assert_eq!(
                    *entity_type,
                    EntityType::Block,
                    "entity_type should be Block"
                );
            }
            other => panic!("Expected EntityStored event, got: {:?}", other),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_entity_emits_correct_entity_type_for_tag() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Upsert a tag entity
        let entity = Entity {
            id: Some(RecordId::new("entities", "test_tag")),
            entity_type: SurrealEntityType::Tag,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
            version: 1,
            content_hash: None,
            created_by: None,
            vault_id: None,
            data: Some(serde_json::json!({"name": "test-tag"})),
        };
        store.upsert_entity(&entity).await.unwrap();

        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1);

        match &events[0] {
            SessionEvent::EntityStored { entity_type, .. } => {
                assert_eq!(*entity_type, EntityType::Tag, "entity_type should be Tag");
            }
            other => panic!("Expected EntityStored event, got: {:?}", other),
        }
    }

    // ========================================================================
    // BlocksUpdated event tests
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_replace_blocks_emits_blocks_updated_event() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // First create an entity to associate blocks with
        let entity = make_test_entity("note_with_blocks");
        store.upsert_entity(&entity).await.unwrap();

        // Clear emitter to only capture blocks event
        emitter.reset();

        // Replace blocks
        let entity_id: RecordId<EntityRecord> = RecordId::new("entities", "note_with_blocks");
        let blocks = make_test_blocks("note_with_blocks", 3);
        store.replace_blocks(&entity_id, &blocks).await.unwrap();

        // Verify BlocksUpdated event was emitted
        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());

        match &events[0] {
            SessionEvent::BlocksUpdated {
                entity_id,
                block_count,
            } => {
                assert!(
                    entity_id.contains("note_with_blocks"),
                    "entity_id should contain 'note_with_blocks', got: {}",
                    entity_id
                );
                assert_eq!(
                    *block_count, 3,
                    "block_count should be 3, got: {}",
                    block_count
                );
            }
            other => panic!("Expected BlocksUpdated event, got: {:?}", other),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_replace_blocks_with_empty_list_emits_event() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Create entity
        let entity = make_test_entity("note_empty_blocks");
        store.upsert_entity(&entity).await.unwrap();
        emitter.reset();

        // Replace with empty blocks
        let entity_id: RecordId<EntityRecord> = RecordId::new("entities", "note_empty_blocks");
        store.replace_blocks(&entity_id, &[]).await.unwrap();

        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1);

        match &events[0] {
            SessionEvent::BlocksUpdated { block_count, .. } => {
                assert_eq!(*block_count, 0, "block_count should be 0 for empty list");
            }
            other => panic!("Expected BlocksUpdated event, got: {:?}", other),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_replace_blocks_no_event_without_emitter() {
        let client = setup_client().await;

        // Create store WITHOUT emitter
        let store = EAVGraphStore::new(client.clone());

        // First create an entity
        let entity = make_test_entity("note_no_emitter_blocks");
        store.upsert_entity(&entity).await.unwrap();

        // Replace blocks should succeed without emitter
        let entity_id: RecordId<EntityRecord> = RecordId::new("entities", "note_no_emitter_blocks");
        let blocks = make_test_blocks("note_no_emitter_blocks", 2);
        let result = store.replace_blocks(&entity_id, &blocks).await;

        assert!(
            result.is_ok(),
            "replace_blocks should succeed without emitter"
        );
    }

    // ========================================================================
    // RelationStored event tests
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_relation_emits_relation_stored_event() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Create two entities to relate
        let entity1 = make_test_entity("source_note");
        let entity2 = make_test_entity("target_note");
        store.upsert_entity(&entity1).await.unwrap();
        store.upsert_entity(&entity2).await.unwrap();
        emitter.reset();

        // Create a relation
        let relation = Relation {
            id: Some(RecordId::new("relations", "test_relation")),
            from_id: RecordId::new("entities", "source_note"),
            to_id: RecordId::new("entities", "target_note"),
            relation_type: "wikilink".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "test".to_string(),
            position: Some(0),
            metadata: serde_json::json!({}),
            content_category: "note".to_string(),
            created_at: Utc::now(),
        };
        store.upsert_relation(&relation).await.unwrap();

        // Verify RelationStored event was emitted
        let events = emitter.emitted_events();
        assert_eq!(events.len(), 1, "Expected 1 event, got {}", events.len());

        match &events[0] {
            SessionEvent::RelationStored {
                from_id,
                to_id,
                relation_type,
            } => {
                assert!(
                    from_id.contains("source_note"),
                    "from_id should contain 'source_note', got: {}",
                    from_id
                );
                assert!(
                    to_id.contains("target_note"),
                    "to_id should contain 'target_note', got: {}",
                    to_id
                );
                assert_eq!(
                    relation_type, "wikilink",
                    "relation_type should be 'wikilink'"
                );
            }
            other => panic!("Expected RelationStored event, got: {:?}", other),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_upsert_relation_no_event_without_emitter() {
        let client = setup_client().await;

        // Create store WITHOUT emitter
        let store = EAVGraphStore::new(client);

        // Create entities
        let entity1 = make_test_entity("source_no_emit");
        let entity2 = make_test_entity("target_no_emit");
        store.upsert_entity(&entity1).await.unwrap();
        store.upsert_entity(&entity2).await.unwrap();

        // Create a relation
        let relation = Relation {
            id: Some(RecordId::new("relations", "test_relation_no_emit")),
            from_id: RecordId::new("entities", "source_no_emit"),
            to_id: RecordId::new("entities", "target_no_emit"),
            relation_type: "backlink".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "test".to_string(),
            position: None,
            metadata: serde_json::json!({}),
            content_category: "note".to_string(),
            created_at: Utc::now(),
        };
        let result = store.upsert_relation(&relation).await;

        assert!(
            result.is_ok(),
            "upsert_relation should succeed without emitter"
        );
    }

    // ========================================================================
    // Error handling tests
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_emitter_error_does_not_fail_upsert_entity() {
        use crucible_core::events::EventError;

        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        // Configure emitter to return an error
        emitter.set_error(Some(EventError::unavailable("Test emitter failure")));

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Upsert should still succeed despite emitter error (fire-and-forget semantics)
        let entity = make_test_entity("entity_despite_emitter_error");
        let result = store.upsert_entity(&entity).await;

        assert!(
            result.is_ok(),
            "Upsert should succeed despite emitter error"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_emitter_error_does_not_fail_replace_blocks() {
        use crucible_core::events::EventError;

        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // First create an entity
        let entity = make_test_entity("entity_for_blocks_error");
        store.upsert_entity(&entity).await.unwrap();

        // Now configure emitter to fail
        emitter.set_error(Some(EventError::unavailable("Test emitter failure")));

        // Replace blocks should still succeed
        let entity_id: RecordId<EntityRecord> = RecordId::new("entities", "entity_for_blocks_error");
        let blocks = make_test_blocks("entity_for_blocks_error", 2);
        let result = store.replace_blocks(&entity_id, &blocks).await;

        assert!(
            result.is_ok(),
            "replace_blocks should succeed despite emitter error"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_emitter_error_does_not_fail_upsert_relation() {
        use crucible_core::events::EventError;

        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Create entities
        let entity1 = make_test_entity("src_error_test");
        let entity2 = make_test_entity("tgt_error_test");
        store.upsert_entity(&entity1).await.unwrap();
        store.upsert_entity(&entity2).await.unwrap();

        // Now configure emitter to fail
        emitter.set_error(Some(EventError::unavailable("Test emitter failure")));

        // upsert_relation should still succeed
        let relation = Relation {
            id: Some(RecordId::new("relations", "relation_error_test")),
            from_id: RecordId::new("entities", "src_error_test"),
            to_id: RecordId::new("entities", "tgt_error_test"),
            relation_type: "test".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "test".to_string(),
            position: None,
            metadata: serde_json::json!({}),
            content_category: "note".to_string(),
            created_at: Utc::now(),
        };
        let result = store.upsert_relation(&relation).await;

        assert!(
            result.is_ok(),
            "upsert_relation should succeed despite emitter error"
        );
    }

    // ========================================================================
    // Multiple operations test
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_multiple_operations_emit_multiple_events() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // 1. Create two entities (2 EntityStored events)
        let entity1 = make_test_entity("multi_note_1");
        let entity2 = make_test_entity("multi_note_2");
        store.upsert_entity(&entity1).await.unwrap();
        store.upsert_entity(&entity2).await.unwrap();

        // 2. Replace blocks on one entity (1 BlocksUpdated event)
        let entity_id: RecordId<EntityRecord> = RecordId::new("entities", "multi_note_1");
        let blocks = make_test_blocks("multi_note_1", 2);
        store.replace_blocks(&entity_id, &blocks).await.unwrap();

        // 3. Create a relation (1 RelationStored event)
        let relation = Relation {
            id: Some(RecordId::new("relations", "multi_relation")),
            from_id: RecordId::new("entities", "multi_note_1"),
            to_id: RecordId::new("entities", "multi_note_2"),
            relation_type: "link".to_string(),
            weight: 1.0,
            directed: true,
            confidence: 1.0,
            source: "test".to_string(),
            position: None,
            metadata: serde_json::json!({}),
            content_category: "note".to_string(),
            created_at: Utc::now(),
        };
        store.upsert_relation(&relation).await.unwrap();

        // Verify all 4 events were emitted
        let events = emitter.emitted_events();
        assert_eq!(events.len(), 4, "Expected 4 events, got {}", events.len());

        // Count event types
        let entity_stored_count = events
            .iter()
            .filter(|e| matches!(e, SessionEvent::EntityStored { .. }))
            .count();
        let blocks_updated_count = events
            .iter()
            .filter(|e| matches!(e, SessionEvent::BlocksUpdated { .. }))
            .count();
        let relation_stored_count = events
            .iter()
            .filter(|e| matches!(e, SessionEvent::RelationStored { .. }))
            .count();

        assert_eq!(entity_stored_count, 2, "Should have 2 EntityStored events");
        assert_eq!(
            blocks_updated_count, 1,
            "Should have 1 BlocksUpdated event"
        );
        assert_eq!(
            relation_stored_count, 1,
            "Should have 1 RelationStored event"
        );
    }

    // ========================================================================
    // with_emitter() builder pattern test
    // ========================================================================

    #[tokio::test(flavor = "multi_thread")]
    async fn test_with_emitter_returns_configured_store() {
        let client = setup_client().await;
        let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

        // Use builder pattern
        let store = EAVGraphStore::new(client).with_emitter(emitter.clone());

        // Verify emitter is set
        assert!(
            store.emitter().is_some(),
            "Store should have emitter configured"
        );

        // Verify it's the same emitter
        let entity = make_test_entity("builder_test");
        store.upsert_entity(&entity).await.unwrap();

        assert_eq!(
            emitter.event_count(),
            1,
            "Emitter should have received 1 event"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_without_emitter_returns_none() {
        let client = setup_client().await;

        let store = EAVGraphStore::new(client);

        assert!(
            store.emitter().is_none(),
            "Store without with_emitter() should return None"
        );
    }
}
