//! Integration tests for PropertyStorage trait implementation
//!
//! Tests the complete pipeline:
//! 1. Frontmatter parsing (Phase 1.1)
//! 2. Property mapping (Phase 1.2)
//! 3. PropertyStorage trait (Phase 1.3)

use chrono::Utc;
use crucible_core::parser::FrontmatterPropertyMapper;
use crucible_core::storage::{PropertyNamespace, PropertyStorage, PropertyValue};
use crucible_parser::{Frontmatter, FrontmatterFormat};
use crucible_surrealdb::eav_graph::{apply_eav_graph_schema, EAVGraphStore};
use crucible_surrealdb::SurrealClient;
use serde_json::json;

// ============================================================================
// QA Checkpoint 1: End-to-End Pipeline Test
// ============================================================================

#[tokio::test]
async fn test_frontmatter_to_storage_pipeline() {
    // Setup: Create in-memory database and apply schema
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client.clone());

    // Step 1: Parse frontmatter (Phase 1.1)
    let yaml = r#"
title: My Test Note
author: John Doe
count: 42
published: true
created: 2024-11-08
tags: ["rust", "testing"]
"#;
    let frontmatter = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);

    // Step 2: Map to properties (Phase 1.2)
    let mapper = FrontmatterPropertyMapper::new("note:test123");
    let properties = mapper.map_to_properties(frontmatter.properties().clone());

    assert_eq!(
        properties.len(),
        6,
        "Should have 6 properties from frontmatter"
    );

    // Step 3: Store in database (Phase 1.3)
    let count = store.batch_upsert_properties(properties).await.unwrap();
    assert_eq!(count, 6, "Should store all 6 properties");

    // Step 4: Retrieve and verify (QA Checkpoint)
    let retrieved = store
        .get_properties_by_namespace("note:test123", &PropertyNamespace::frontmatter())
        .await
        .unwrap();

    assert_eq!(retrieved.len(), 6, "Should retrieve all 6 properties");

    // Verify each property value
    let title = retrieved.iter().find(|p| p.key == "title").unwrap();
    assert_eq!(title.value, PropertyValue::Text("My Test Note".to_string()));

    let author = retrieved.iter().find(|p| p.key == "author").unwrap();
    assert_eq!(author.value, PropertyValue::Text("John Doe".to_string()));

    let count_prop = retrieved.iter().find(|p| p.key == "count").unwrap();
    assert_eq!(count_prop.value, PropertyValue::Number(42.0));

    let published = retrieved.iter().find(|p| p.key == "published").unwrap();
    assert_eq!(published.value, PropertyValue::Bool(true));

    let created = retrieved.iter().find(|p| p.key == "created").unwrap();
    match &created.value {
        PropertyValue::Date(d) => {
            assert_eq!(d.to_string(), "2024-11-08");
        }
        _ => panic!("Expected Date value"),
    }

    let tags = retrieved.iter().find(|p| p.key == "tags").unwrap();
    assert_eq!(tags.value, PropertyValue::Json(json!(["rust", "testing"])));
}

// ============================================================================
// PropertyStorage Trait Tests
// ============================================================================

#[tokio::test]
async fn test_batch_upsert_properties() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: PropertyValue::Text("Test Note".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "count".to_string(),
            value: PropertyValue::Number(42.0),
            created_at: now,
            updated_at: now,
        },
    ];

    let count = store.batch_upsert_properties(properties).await.unwrap();
    assert_eq!(count, 2);

    let retrieved = store.get_properties("note:test").await.unwrap();
    assert_eq!(retrieved.len(), 2);
}

#[tokio::test]
async fn test_get_properties_by_namespace() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: PropertyValue::Text("Test".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::core(),
            key: "hash".to_string(),
            value: PropertyValue::Text("abc123".to_string()),
            created_at: now,
            updated_at: now,
        },
    ];

    store.batch_upsert_properties(properties).await.unwrap();

    // Get only frontmatter properties
    let frontmatter = store
        .get_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
        .await
        .unwrap();

    assert_eq!(frontmatter.len(), 1);
    assert_eq!(frontmatter[0].key, "title");

    // Get only core properties
    let core_props = store
        .get_properties_by_namespace("note:test", &PropertyNamespace::core())
        .await
        .unwrap();

    assert_eq!(core_props.len(), 1);
    assert_eq!(core_props[0].key, "hash");
}

#[tokio::test]
async fn test_get_property_single() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: PropertyValue::Text("Test".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "author".to_string(),
            value: PropertyValue::Text("Jane".to_string()),
            created_at: now,
            updated_at: now,
        },
    ];

    store.batch_upsert_properties(properties).await.unwrap();

    // Get specific property
    let title = store
        .get_property("note:test", &PropertyNamespace::frontmatter(), "title")
        .await
        .unwrap();

    assert!(title.is_some());
    let title = title.unwrap();
    assert_eq!(title.key, "title");
    assert_eq!(title.value, PropertyValue::Text("Test".to_string()));

    // Get non-existent property
    let missing = store
        .get_property("note:test", &PropertyNamespace::frontmatter(), "missing")
        .await
        .unwrap();

    assert!(missing.is_none());
}

#[tokio::test]
async fn test_delete_properties() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: PropertyValue::Text("Test".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::core(),
            key: "hash".to_string(),
            value: PropertyValue::Text("abc123".to_string()),
            created_at: now,
            updated_at: now,
        },
    ];

    store.batch_upsert_properties(properties).await.unwrap();

    // Delete all properties
    let deleted = store.delete_properties("note:test").await.unwrap();
    assert_eq!(deleted, 2);

    // Verify deletion
    let remaining = store.get_properties("note:test").await.unwrap();
    assert_eq!(remaining.len(), 0);
}

#[tokio::test]
async fn test_delete_properties_by_namespace() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "title".to_string(),
            value: PropertyValue::Text("Test".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "author".to_string(),
            value: PropertyValue::Text("Jane".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::core(),
            key: "hash".to_string(),
            value: PropertyValue::Text("abc123".to_string()),
            created_at: now,
            updated_at: now,
        },
    ];

    store.batch_upsert_properties(properties).await.unwrap();

    // Delete only frontmatter properties
    let deleted = store
        .delete_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
        .await
        .unwrap();
    assert_eq!(deleted, 2);

    // Verify frontmatter properties are gone
    let frontmatter = store
        .get_properties_by_namespace("note:test", &PropertyNamespace::frontmatter())
        .await
        .unwrap();
    assert_eq!(frontmatter.len(), 0);

    // Verify core properties still exist
    let core_props = store
        .get_properties_by_namespace("note:test", &PropertyNamespace::core())
        .await
        .unwrap();
    assert_eq!(core_props.len(), 1);
}

#[tokio::test]
async fn test_upsert_semantics() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();

    // Insert initial property
    let properties = vec![crucible_core::storage::Property {
        entity_id: "note:test".to_string(),
        namespace: PropertyNamespace::frontmatter(),
        key: "title".to_string(),
        value: PropertyValue::Text("Original Title".to_string()),
        created_at: now,
        updated_at: now,
    }];

    store.batch_upsert_properties(properties).await.unwrap();

    // Update same property
    let updated = vec![crucible_core::storage::Property {
        entity_id: "note:test".to_string(),
        namespace: PropertyNamespace::frontmatter(),
        key: "title".to_string(),
        value: PropertyValue::Text("Updated Title".to_string()),
        created_at: now,
        updated_at: Utc::now(),
    }];

    store.batch_upsert_properties(updated).await.unwrap();

    // Should still have only 1 property (upsert, not insert)
    let retrieved = store.get_properties("note:test").await.unwrap();
    assert_eq!(retrieved.len(), 1);

    // Verify value was updated
    let title = &retrieved[0];
    assert_eq!(
        title.value,
        PropertyValue::Text("Updated Title".to_string())
    );
}

#[tokio::test]
async fn test_all_property_value_types() {
    let client = SurrealClient::new_memory().await.unwrap();
    apply_eav_graph_schema(&client).await.unwrap();
    let store = EAVGraphStore::new(client);

    let now = Utc::now();
    let properties = vec![
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "text_value".to_string(),
            value: PropertyValue::Text("hello".to_string()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "number_value".to_string(),
            value: PropertyValue::Number(42.5),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "bool_value".to_string(),
            value: PropertyValue::Bool(true),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "date_value".to_string(),
            value: PropertyValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 11, 8).unwrap()),
            created_at: now,
            updated_at: now,
        },
        crucible_core::storage::Property {
            entity_id: "note:test".to_string(),
            namespace: PropertyNamespace::frontmatter(),
            key: "json_value".to_string(),
            value: PropertyValue::Json(json!(["tag1", "tag2"])),
            created_at: now,
            updated_at: now,
        },
    ];

    store.batch_upsert_properties(properties).await.unwrap();

    let retrieved = store.get_properties("note:test").await.unwrap();
    assert_eq!(retrieved.len(), 5, "Should have all 5 property types");

    // Verify each type
    let text = retrieved.iter().find(|p| p.key == "text_value").unwrap();
    assert_eq!(text.value, PropertyValue::Text("hello".to_string()));

    let number = retrieved.iter().find(|p| p.key == "number_value").unwrap();
    assert_eq!(number.value, PropertyValue::Number(42.5));

    let bool_val = retrieved.iter().find(|p| p.key == "bool_value").unwrap();
    assert_eq!(bool_val.value, PropertyValue::Bool(true));

    let date = retrieved.iter().find(|p| p.key == "date_value").unwrap();
    match &date.value {
        PropertyValue::Date(d) => assert_eq!(d.to_string(), "2024-11-08"),
        _ => panic!("Expected Date value"),
    }

    let json_val = retrieved.iter().find(|p| p.key == "json_value").unwrap();
    assert_eq!(json_val.value, PropertyValue::Json(json!(["tag1", "tag2"])));
}
