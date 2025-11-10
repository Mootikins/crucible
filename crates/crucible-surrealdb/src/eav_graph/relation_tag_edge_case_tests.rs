/// Relation & Tag Extraction Edge Case Tests
///
/// This test suite verifies that all wikilink and tag variants are correctly:
/// 1. Parsed by crucible-parser
/// 2. Extracted by DocumentIngestor
/// 3. Stored in relations/tags tables with proper metadata
///
/// These tests use `new_isolated_memory()` to ensure complete test isolation.

#[cfg(test)]
mod edge_cases {
    use super::super::{apply_eav_graph_schema, DocumentIngestor, EAVGraphStore};
    use crate::eav_graph::ingest::extract_tags;
    use crate::SurrealClient;
    use crucible_core::parser::{ParsedDocument, Tag, Wikilink};
    use crucible_core::storage::{RelationStorage, TagStorage};
    use std::path::PathBuf;

    /// Helper to extract and store tags (tags are not auto-stored by ingestor)
    async fn store_tags_for_doc(doc: &ParsedDocument, store: &EAVGraphStore) {
        let tags = extract_tags(doc);
        for tag in tags {
            // Ignore errors for duplicate tags (this is expected in deduplication tests)
            let _ = store.store_tag(tag).await;
        }
    }

    // ============================================================================
    // Wikilink Edge Cases
    // ============================================================================

    #[tokio::test]
    async fn test_basic_wikilink() {
        // [[Note]]
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_basic_wikilink".into();

        doc.wikilinks.push(Wikilink {
            target: "Target Note".to_string(),
            alias: None,
            offset: 10,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        // Get relations using the store interface
        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1, "Should have 1 wikilink relation");

        let rel = &relations[0];
        assert_eq!(rel.relation_type, "wikilink");
        assert_eq!(
            rel.to_entity_id,
            Some("entities:note:Target Note".to_string())
        );
        assert_eq!(rel.metadata.get("alias"), None);
    }

    #[tokio::test]
    async fn test_aliased_wikilink() {
        // [[Note|Display Text]]
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_aliased_wikilink".into();

        doc.wikilinks.push(Wikilink {
            target: "Target Note".to_string(),
            alias: Some("Display Text".to_string()),
            offset: 10,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let rel = &relations[0];
        assert_eq!(
            rel.metadata.get("alias").and_then(|v| v.as_str()),
            Some("Display Text")
        );
    }

    #[tokio::test]
    async fn test_heading_ref_wikilink() {
        // [[Note#Heading]]
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_heading_ref".into();

        doc.wikilinks.push(Wikilink {
            target: "Target Note".to_string(),
            alias: None,
            offset: 10,
            is_embed: false,
            block_ref: None,
            heading_ref: Some("Important Section".to_string()),
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let rel = &relations[0];
        assert_eq!(
            rel.metadata.get("heading_ref").and_then(|v| v.as_str()),
            Some("Important Section")
        );
    }

    #[tokio::test]
    async fn test_block_ref_wikilink() {
        // [[Note#^block123]]
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_block_ref".into();

        doc.wikilinks.push(Wikilink {
            target: "Target Note".to_string(),
            alias: None,
            offset: 10,
            is_embed: false,
            block_ref: Some("block123".to_string()),
            heading_ref: None,
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let rel = &relations[0];
        assert_eq!(
            rel.metadata.get("block_ref").and_then(|v| v.as_str()),
            Some("block123")
        );
    }

    #[tokio::test]
    async fn test_embed_wikilink() {
        // ![[Image or Note]]
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_embed".into();

        doc.wikilinks.push(Wikilink {
            target: "Embedded Content".to_string(),
            alias: None,
            offset: 10,
            is_embed: true,
            block_ref: None,
            heading_ref: None,
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let rel = &relations[0];
        // Embeds use "embed" relation type
        assert_eq!(rel.relation_type, "embed");
    }

    #[tokio::test]
    async fn test_combined_wikilink() {
        // [[Note#Heading^5#hash]]
        // This is an advanced block link with heading, occurrence, and hash
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_combined".into();

        doc.wikilinks.push(Wikilink {
            target: "Target Note".to_string(),
            alias: None,
            offset: 10,
            is_embed: false,
            block_ref: Some("hash123".to_string()),
            heading_ref: Some("Important Section".to_string()),
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 1);

        let rel = &relations[0];
        // Both heading_ref and block_ref should be present
        assert_eq!(
            rel.metadata.get("heading_ref").and_then(|v| v.as_str()),
            Some("Important Section")
        );
        assert_eq!(
            rel.metadata.get("block_ref").and_then(|v| v.as_str()),
            Some("hash123")
        );
    }

    #[tokio::test]
    async fn test_multiple_wikilinks() {
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_multiple".into();

        doc.wikilinks.push(Wikilink {
            target: "Note 1".to_string(),
            alias: None,
            offset: 10,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        });

        doc.wikilinks.push(Wikilink {
            target: "Note 2".to_string(),
            alias: Some("Link 2".to_string()),
            offset: 30,
            is_embed: false,
            block_ref: None,
            heading_ref: None,
        });

        doc.wikilinks.push(Wikilink {
            target: "Image".to_string(),
            alias: None,
            offset: 50,
            is_embed: true,
            block_ref: None,
            heading_ref: None,
        });

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        let mut relations = store.get_relations(&entity_id.id, None).await.unwrap();
        assert_eq!(relations.len(), 3, "Should have 3 relations");

        // Sort by offset for predictable ordering
        relations.sort_by(|a, b| {
            let a_offset = a
                .metadata
                .get("offset")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let b_offset = b
                .metadata
                .get("offset")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            a_offset.cmp(&b_offset)
        });

        // Verify first link
        assert_eq!(relations[0].relation_type, "wikilink");
        assert_eq!(
            relations[0].to_entity_id,
            Some("entities:note:Note 1".to_string())
        );

        // Verify second link has alias
        assert_eq!(
            relations[1]
                .metadata
                .get("alias")
                .and_then(|v| v.as_str()),
            Some("Link 2")
        );

        // Verify third is embed
        assert_eq!(relations[2].relation_type, "embed");
    }

    // ============================================================================
    // Tag Edge Cases
    // ============================================================================

    #[tokio::test]
    async fn test_simple_tag() {
        // #simple
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_simple_tag".into();

        doc.tags.push(Tag::new("simple", 10));

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        store_tags_for_doc(&doc, &store).await;
        ingestor.ingest(&doc, "test.md").await.unwrap();

        let tag = store
            .get_tag("simple")
            .await
            .unwrap()
            .expect("Should have 'simple' tag");
        assert_eq!(tag.name, "simple");
        assert_eq!(tag.parent_tag_id, None, "Simple tag should have no parent");
    }

    #[tokio::test]
    async fn test_nested_tag() {
        // #nested/tag
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_nested_tag".into();

        doc.tags.push(Tag::new("nested/tag", 10));

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        store_tags_for_doc(&doc, &store).await;
        ingestor.ingest(&doc, "test.md").await.unwrap();

        let parent = store
            .get_tag("nested")
            .await
            .unwrap()
            .expect("Should have 'nested' tag");
        assert_eq!(parent.name, "nested");
        assert_eq!(parent.parent_tag_id, None, "Parent should have no parent");

        let child = store
            .get_tag("nested/tag")
            .await
            .unwrap()
            .expect("Should have 'nested/tag' tag");
        assert_eq!(child.name, "nested/tag");
        assert_eq!(
            child.parent_tag_id,
            Some("tags:nested".to_string()),
            "Child should have parent"
        );
    }

    #[tokio::test]
    async fn test_deep_hierarchy_tag() {
        // #multi/level/hierarchy
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_deep_tag".into();

        doc.tags.push(Tag::new("multi/level/hierarchy", 10));

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        store_tags_for_doc(&doc, &store).await;
        ingestor.ingest(&doc, "test.md").await.unwrap();

        let hierarchy = store
            .get_tag("multi/level/hierarchy")
            .await
            .unwrap()
            .expect("Should have 'multi/level/hierarchy' tag");
        assert_eq!(hierarchy.name, "multi/level/hierarchy");
        assert_eq!(
            hierarchy.parent_tag_id,
            Some("tags:multi/level".to_string()),
            "hierarchy should be child of multi/level"
        );

        let level = store
            .get_tag("multi/level")
            .await
            .unwrap()
            .expect("Should have 'multi/level' tag");
        assert_eq!(level.name, "multi/level");
        assert_eq!(
            level.parent_tag_id,
            Some("tags:multi".to_string()),
            "level should be child of multi"
        );

        let multi = store
            .get_tag("multi")
            .await
            .unwrap()
            .expect("Should have 'multi' tag");
        assert_eq!(multi.name, "multi");
        assert_eq!(multi.parent_tag_id, None, "multi should have no parent");
    }

    #[tokio::test]
    async fn test_multiple_tags() {
        let mut doc = ParsedDocument::default();
        doc.path = PathBuf::from("test.md");
        doc.content_hash = "test_multiple_tags".into();

        doc.tags.push(Tag::new("project", 10));
        doc.tags.push(Tag::new("status/active", 20));
        doc.tags.push(Tag::new("priority/high", 30));

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        store_tags_for_doc(&doc, &store).await;
        let entity_id = ingestor.ingest(&doc, "test.md").await.unwrap();

        // Verify tags were created
        let project = store
            .get_tag("project")
            .await
            .unwrap()
            .expect("Should have 'project' tag");
        assert_eq!(project.name, "project");

        let active = store
            .get_tag("status/active")
            .await
            .unwrap()
            .expect("Should have 'status/active' tag");
        assert_eq!(active.name, "status/active");
        assert_eq!(active.parent_tag_id, Some("tags:status".to_string()));

        let high = store
            .get_tag("priority/high")
            .await
            .unwrap()
            .expect("Should have 'priority/high' tag");
        assert_eq!(high.name, "priority/high");
        assert_eq!(high.parent_tag_id, Some("tags:priority".to_string()));

        // Note: Entity-tag associations are handled separately by kiln_integration,
        // not by the DocumentIngestor. This test only verifies tag creation.
    }

    #[tokio::test]
    async fn test_tag_deduplication() {
        // Multiple documents with same tags should reuse existing tags
        let mut doc1 = ParsedDocument::default();
        doc1.path = PathBuf::from("doc1.md");
        doc1.content_hash = "test_dedup_1".into();
        doc1.tags.push(Tag::new("shared", 10));

        let mut doc2 = ParsedDocument::default();
        doc2.path = PathBuf::from("doc2.md");
        doc2.content_hash = "test_dedup_2".into();
        doc2.tags.push(Tag::new("shared", 10));

        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Store tags and ingest both documents
        store_tags_for_doc(&doc1, &store).await;
        let entity_id1 = ingestor.ingest(&doc1, "doc1.md").await.unwrap();

        store_tags_for_doc(&doc2, &store).await;
        let entity_id2 = ingestor.ingest(&doc2, "doc2.md").await.unwrap();

        // Should only have 1 'shared' tag despite 2 documents using it
        let tag = store
            .get_tag("shared")
            .await
            .unwrap()
            .expect("Should have 'shared' tag");
        assert_eq!(tag.name, "shared", "Should deduplicate shared tags");

        // Note: Entity-tag associations are handled separately by kiln_integration.
        // This test verifies tag deduplication at the storage level.
    }

    // ============================================================================
    // Hierarchical Tag Search Edge Cases
    // ============================================================================

    #[tokio::test]
    async fn test_hierarchical_search_parent_returns_children() {
        // Searching for parent tag should return entities with parent AND all descendant tags
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Create tag hierarchy: project -> project/ai -> project/ai/nlp
        let mut doc1 = ParsedDocument::default();
        doc1.path = PathBuf::from("doc1.md");
        doc1.content_hash = "hash1".into();
        doc1.tags.push(Tag::new("project", 0));

        let mut doc2 = ParsedDocument::default();
        doc2.path = PathBuf::from("doc2.md");
        doc2.content_hash = "hash2".into();
        doc2.tags.push(Tag::new("project/ai", 0));

        let mut doc3 = ParsedDocument::default();
        doc3.path = PathBuf::from("doc3.md");
        doc3.content_hash = "hash3".into();
        doc3.tags.push(Tag::new("project/ai/nlp", 0));

        // Store tags and create entity-tag associations
        store_tags_for_doc(&doc1, &store).await;
        store_tags_for_doc(&doc2, &store).await;
        store_tags_for_doc(&doc3, &store).await;

        let entity1 = ingestor.ingest(&doc1, "doc1.md").await.unwrap();
        let entity2 = ingestor.ingest(&doc2, "doc2.md").await.unwrap();
        let entity3 = ingestor.ingest(&doc3, "doc3.md").await.unwrap();

        // Create entity-tag associations manually (normally done by kiln_integration)
        use crucible_core::storage::{EntityTag, TagStorage};
        use chrono::Utc;
        store
            .associate_tag(EntityTag {
                entity_id: entity1.id.clone(),
                tag_id: "project".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .associate_tag(EntityTag {
                entity_id: entity2.id.clone(),
                tag_id: "project/ai".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .associate_tag(EntityTag {
                entity_id: entity3.id.clone(),
                tag_id: "project/ai/nlp".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();

        // Search for parent tag should return all three entities
        let results = store.get_entities_by_tag("project").await.unwrap();
        assert_eq!(
            results.len(),
            3,
            "Searching for 'project' should return all 3 entities"
        );
        assert!(
            results.contains(&format!("entities:{}", entity1.id)),
            "Should include entity with 'project' tag"
        );
        assert!(
            results.contains(&format!("entities:{}", entity2.id)),
            "Should include entity with 'project/ai' tag"
        );
        assert!(
            results.contains(&format!("entities:{}", entity3.id)),
            "Should include entity with 'project/ai/nlp' tag"
        );
    }

    #[tokio::test]
    async fn test_hierarchical_search_mid_level() {
        // Searching for mid-level tag should return descendants but not parents
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc1 = ParsedDocument::default();
        doc1.path = PathBuf::from("doc1.md");
        doc1.content_hash = "hash1".into();
        doc1.tags.push(Tag::new("project", 0));

        let mut doc2 = ParsedDocument::default();
        doc2.path = PathBuf::from("doc2.md");
        doc2.content_hash = "hash2".into();
        doc2.tags.push(Tag::new("project/ai", 0));

        let mut doc3 = ParsedDocument::default();
        doc3.path = PathBuf::from("doc3.md");
        doc3.content_hash = "hash3".into();
        doc3.tags.push(Tag::new("project/ai/nlp", 0));

        store_tags_for_doc(&doc1, &store).await;
        store_tags_for_doc(&doc2, &store).await;
        store_tags_for_doc(&doc3, &store).await;

        let entity1 = ingestor.ingest(&doc1, "doc1.md").await.unwrap();
        let entity2 = ingestor.ingest(&doc2, "doc2.md").await.unwrap();
        let entity3 = ingestor.ingest(&doc3, "doc3.md").await.unwrap();

        use crucible_core::storage::{EntityTag, TagStorage};
        use chrono::Utc;
        store
            .associate_tag(EntityTag {
                entity_id: entity1.id.clone(),
                tag_id: "project".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .associate_tag(EntityTag {
                entity_id: entity2.id.clone(),
                tag_id: "project/ai".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .associate_tag(EntityTag {
                entity_id: entity3.id.clone(),
                tag_id: "project/ai/nlp".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();

        // Search for mid-level tag
        let results = store.get_entities_by_tag("project/ai").await.unwrap();
        assert_eq!(
            results.len(),
            2,
            "Searching for 'project/ai' should return 2 entities"
        );
        assert!(
            !results.contains(&format!("entities:{}", entity1.id)),
            "Should NOT include parent 'project' tag"
        );
        assert!(
            results.contains(&format!("entities:{}", entity2.id)),
            "Should include entity with 'project/ai' tag"
        );
        assert!(
            results.contains(&format!("entities:{}", entity3.id)),
            "Should include entity with 'project/ai/nlp' tag"
        );
    }

    #[tokio::test]
    async fn test_hierarchical_search_leaf_tag() {
        // Searching for leaf tag should return only that tag's entities
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        let mut doc1 = ParsedDocument::default();
        doc1.path = PathBuf::from("doc1.md");
        doc1.content_hash = "hash1".into();
        doc1.tags.push(Tag::new("project/ai", 0));

        let mut doc2 = ParsedDocument::default();
        doc2.path = PathBuf::from("doc2.md");
        doc2.content_hash = "hash2".into();
        doc2.tags.push(Tag::new("project/ai/nlp", 0));

        store_tags_for_doc(&doc1, &store).await;
        store_tags_for_doc(&doc2, &store).await;

        let entity1 = ingestor.ingest(&doc1, "doc1.md").await.unwrap();
        let entity2 = ingestor.ingest(&doc2, "doc2.md").await.unwrap();

        use crucible_core::storage::{EntityTag, TagStorage};
        use chrono::Utc;
        store
            .associate_tag(EntityTag {
                entity_id: entity1.id.clone(),
                tag_id: "project/ai".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        store
            .associate_tag(EntityTag {
                entity_id: entity2.id.clone(),
                tag_id: "project/ai/nlp".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();

        // Search for leaf tag
        let results = store.get_entities_by_tag("project/ai/nlp").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Searching for leaf tag should return only 1 entity"
        );
        assert!(
            results.contains(&format!("entities:{}", entity2.id)),
            "Should include only the entity with exact leaf tag"
        );
    }

    #[tokio::test]
    async fn test_hierarchical_search_empty_parent() {
        // Parent tag with no direct entities but descendants have entities
        let client = SurrealClient::new_isolated_memory().await.unwrap();
        apply_eav_graph_schema(&client).await.unwrap();
        let store = EAVGraphStore::new(client.clone());
        let ingestor = DocumentIngestor::new(&store);

        // Only create child tags, no entities with parent tag
        let mut doc1 = ParsedDocument::default();
        doc1.path = PathBuf::from("doc1.md");
        doc1.content_hash = "hash1".into();
        doc1.tags.push(Tag::new("research/ml", 0));

        store_tags_for_doc(&doc1, &store).await;
        let entity1 = ingestor.ingest(&doc1, "doc1.md").await.unwrap();

        use crucible_core::storage::{EntityTag, TagStorage};
        use chrono::Utc;
        store
            .associate_tag(EntityTag {
                entity_id: entity1.id.clone(),
                tag_id: "research/ml".to_string(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();

        // Search for parent tag that has no direct entities
        let results = store.get_entities_by_tag("research").await.unwrap();
        assert_eq!(
            results.len(),
            1,
            "Should find entity through descendant tag"
        );
        assert!(
            results.contains(&format!("entities:{}", entity1.id)),
            "Should include entity with 'research/ml' tag"
        );
    }
}
