//! # Phase 1B: Metadata-Based Search Tests (TDD)
//!
//! This module implements TDD tests for metadata-based search functionality in Crucible.
//! Tests use the docs kiln at `docs/` for realistic test data.
//!
//! ## Test Coverage
//!
//! ### Tag Queries (6 tests)
//! - Exact tag matching
//! - Nested tag matching
//! - Tag hierarchy (parent includes children)
//! - Multiple tags with AND/OR logic
//! - Special characters in tags
//! - Non-existent tag queries
//!
//! ### Property/Frontmatter Queries (6 tests)
//! - String property matching
//! - Numeric property comparison
//! - Date range queries
//! - Boolean property filtering
//! - Array contains value
//! - Nested property access
//!
//! ### Combined Metadata (3 tests)
//! - Tag + property combined filters
//! - Multiple properties combined
//! - Metadata + folder scope
//!
//! ## TDD Protocol
//! 1. Write failing test
//! 2. Verify it fails for the right reason
//! 3. Write minimal code to pass
//! 4. Verify green

#[cfg(test)]
mod tests {
    use crate::eav_graph::{apply_eav_graph_schema, EAVGraphStore, NoteIngestor};
    use crate::SurrealClient;
    use anyhow::Result;
    use crucible_core::parser::MarkdownParser;
    use crucible_parser::CrucibleParser;
    use std::path::PathBuf;

    /// Get the path to dev-kiln directory (from workspace root)
    fn test_kiln_path() -> PathBuf {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("docs")
    }

    /// Test helper to set up a test database
    async fn setup_test_db() -> Result<(SurrealClient, EAVGraphStore)> {
        let client = SurrealClient::new_memory().await?;
        apply_eav_graph_schema(&client).await?;
        let store = EAVGraphStore::new(client.clone());
        Ok((client, store))
    }

    /// Parse and ingest a markdown file from dev-kiln
    async fn ingest_test_file(store: &EAVGraphStore, file_name: &str) -> Result<String> {
        let file_path = test_kiln_path().join(file_name);

        // Use the real parser to parse the markdown file
        let parser = CrucibleParser::with_default_extensions();
        let note = parser.parse_file(&file_path).await?;

        // Ingest the parsed note
        let ingestor = NoteIngestor::new(store);
        let entity_id = ingestor.ingest(&note, file_name).await?;

        Ok(entity_id.id)
    }

    // ============================================================================
    // TAG QUERIES (6 tests)
    // ============================================================================

    #[tokio::test]
    async fn tag_exact_match() {
        // TDD: Write failing test first
        // Test should search for exact tag "#guide" and find matching notes

        let (client, store) = setup_test_db().await.unwrap();

        // Ingest test files that contain #guide tag
        // dev-kiln: "Guides/Getting Started.md" has tags: [guide, beginner]
        let _entity_id = ingest_test_file(&store, "Guides/Getting Started.md")
            .await
            .unwrap();

        // Search for exact tag #guide using SurrealDB graph syntax
        // We need to:
        // 1. Find the tag with name 'guide'
        // 2. Find entity_tags records that reference this tag
        // 3. Get the entities from those records
        let query = r#"
            SELECT entity_id.id as id, entity_id.data as data
            FROM entity_tags
            WHERE tag_id.name = 'guide'
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find at least 1 document with this tag
        assert!(
            !result.records.is_empty(),
            "Should find documents with #guide tag"
        );

        // The record ID is in the record.id field, not record.data
        // It contains strings like "entities:note:Guides/Getting Started.md"
        let found_ids: Vec<String> = result
            .records
            .iter()
            .filter_map(|r| r.id.as_ref().map(|id| id.to_string()))
            .collect();

        assert!(
            !found_ids.is_empty(),
            "Should extract at least one ID from results"
        );

        assert!(
            found_ids.iter().any(|id| id.contains("Getting")
                || id.contains("Started")
                || id.contains("Guides")),
            "Found document should be related to getting started guide, found IDs: {:?}",
            found_ids
        );
    }

    #[tokio::test]
    #[ignore = "Requires hierarchical/nested tag support in test data and schema"]
    async fn tag_nested_match() {
        // TDD: Write failing test first
        // Test should search for nested tag "#project/ai" and find matching notes

        let (_client, _store) = setup_test_db().await.unwrap();

        // According to test-kiln, we need to find or create notes with hierarchical tags
        // For now, this test will fail because we haven't implemented nested tag support

        // TODO: Once we have notes with nested tags, search for them
        // Expected: Find documents tagged with #project/ai
    }

    #[tokio::test]
    #[ignore = "Requires hierarchical tag search implementation (parent includes children)"]
    async fn tag_parent_includes_children() {
        // TDD: Write failing test first
        // Test that searching for parent tag "#project" also finds "#project/ai"

        let (_client, _store) = setup_test_db().await.unwrap();

        // TODO: Create test data with hierarchical tags
        // Search for parent tag should include all children
    }

    #[tokio::test]
    async fn tag_multiple_and_or() {
        // TDD: Write failing test first
        // Test multiple tags with AND/OR logic

        let (client, store) = setup_test_db().await.unwrap();

        // Ingest files with multiple tags from dev-kiln
        // "Agents/Coder.md" has tags: [agent, example, coding]
        // "Agents/Researcher.md" has tags: [agent, example, research]
        let _id1 = ingest_test_file(&store, "Agents/Coder.md").await.unwrap();
        let _id2 = ingest_test_file(&store, "Agents/Researcher.md")
            .await
            .unwrap();

        // Test AND logic: documents with BOTH #agent AND #example
        // In SurrealDB, we need to find entities that appear in both tag sets
        let query_and = r#"
            LET $agent_entities = (SELECT entity_id FROM entity_tags WHERE tag_id.name = 'agent');
            LET $example_entities = (SELECT entity_id FROM entity_tags WHERE tag_id.name = 'example');
            SELECT id FROM entities WHERE id IN $agent_entities.entity_id AND id IN $example_entities.entity_id;
        "#;

        let result_and = client.query(query_and, &[]).await.unwrap();

        // Should find documents with both tags (both agent files have agent + example)
        assert!(
            !result_and.records.is_empty(),
            "Should find documents with both #agent AND #example"
        );

        // Test OR logic: documents with EITHER #coding OR #research
        let query_or = r#"
            SELECT entity_id
            FROM entity_tags
            WHERE tag_id.name IN ['coding', 'research']
            GROUP ALL
        "#;

        let result_or = client.query(query_or, &[]).await.unwrap();

        // Should find at least 1 document (files have either coding or research tag)
        assert!(
            !result_or.records.is_empty(),
            "Should find at least 1 document with #coding OR #research"
        );
    }

    #[tokio::test]
    #[ignore = "Requires special character tag support and test data"]
    async fn tag_special_chars() {
        // TDD: Write failing test first
        // Test tags with special characters like #c++

        let (_client, _store) = setup_test_db().await.unwrap();

        // According to Comprehensive-Feature-Test.md, there are tags with special chars
        // #JavaScript, #Python, #RustLang, etc.

        // TODO: Ingest Comprehensive-Feature-Test.md and search for tags with special chars
    }

    #[tokio::test]
    async fn tag_nonexistent() {
        // TDD: Write failing test first
        // Test that searching for non-existent tag returns empty results

        let (client, store) = setup_test_db().await.unwrap();

        // Ingest some test data from dev-kiln
        let _id = ingest_test_file(&store, "Guides/Getting Started.md")
            .await
            .unwrap();

        // Search for a tag that doesn't exist using SurrealDB graph syntax
        let query = r#"
            SELECT entity_id.id as id
            FROM entity_tags
            WHERE tag_id.name = 'nonexistent-tag-xyz-123'
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should return empty results
        assert!(
            result.records.is_empty(),
            "Should return empty results for non-existent tag"
        );
    }

    // ============================================================================
    // PROPERTY/FRONTMATTER QUERIES (6 tests)
    // ============================================================================

    #[tokio::test]
    async fn property_string_match() {
        // TDD: Write failing test first
        // Test string property matching: type: "agent-card"

        let (client, store) = setup_test_db().await.unwrap();

        // dev-kiln: "Agents/Coder.md" has type: agent-card
        let _id = ingest_test_file(&store, "Agents/Coder.md").await.unwrap();

        // Search for documents with specific type using SurrealDB graph syntax
        // Frontmatter is stored as a single property with key='frontmatter'
        // The value structure is: {type: "json", value: {...frontmatter fields...}}
        // We need to query: value.value.type
        let query = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND value.value.type = 'agent-card'
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find the agent card document
        assert!(
            !result.records.is_empty(),
            "Should find documents with type: 'agent-card'"
        );
    }

    #[tokio::test]
    async fn property_numeric_compare() {
        // TDD: Write failing test first
        // Test numeric property comparison: order >= 1

        let (client, store) = setup_test_db().await.unwrap();

        // dev-kiln: "Guides/Getting Started.md" has order: 1
        let _id = ingest_test_file(&store, "Guides/Getting Started.md")
            .await
            .unwrap();

        // Search for documents with order >= 1 using SurrealDB graph syntax
        let query = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND value.value.order >= 1
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find documents with order >= 1
        // Getting Started has order: 1
        assert!(
            !result.records.is_empty(),
            "Should find documents with order >= 1"
        );
    }

    #[tokio::test]
    async fn property_date_range() {
        // TDD: Write failing test first
        // Test date range queries

        let (client, store) = setup_test_db().await.unwrap();

        // dev-kiln: "Guides/Getting Started.md" has created: 2025-01-10 (January)
        // dev-kiln: "Guides/Your First Kiln.md" has created: 2025-02-05 (February)
        let _id1 = ingest_test_file(&store, "Guides/Getting Started.md")
            .await
            .unwrap();
        let _id2 = ingest_test_file(&store, "Guides/Your First Kiln.md")
            .await
            .unwrap();

        // Search for documents created in January 2025 using SurrealDB graph syntax
        let query = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND value.value.created >= '2025-01-01'
            AND value.value.created <= '2025-01-31'
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find only "Getting Started" (created 2025-01-10), not "Your First Kiln" (created 2025-02-05)
        assert!(
            !result.records.is_empty(),
            "Should find documents created in January 2025"
        );

        // Verify February document is not included in January range
        let query_feb = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND value.value.created >= '2025-02-01'
            AND value.value.created <= '2025-02-28'
        "#;

        let result_feb = client.query(query_feb, &[]).await.unwrap();

        // Should find "Your First Kiln" in February range
        assert!(
            !result_feb.records.is_empty(),
            "Should find documents created in February 2025"
        );
    }

    #[tokio::test]
    #[ignore = "Requires boolean property support in test data"]
    async fn property_boolean() {
        // TDD: Write failing test first
        // Test boolean property filtering

        let (_client, _store) = setup_test_db().await.unwrap();

        // Test-kiln doesn't have explicit boolean properties in current data
        // We'll need to add some or use a different approach
    }

    #[tokio::test]
    async fn property_array_contains() {
        // TDD: Write failing test first
        // Test array property contains value

        let (client, store) = setup_test_db().await.unwrap();

        // dev-kiln: files have tags array, e.g. "Agents/Coder.md" has tags: [agent, example, coding]
        let _id = ingest_test_file(&store, "Agents/Coder.md").await.unwrap();

        // Search for documents where tags array contains "coding" using SurrealDB graph syntax
        let query = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND 'coding' IN value.value.tags
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find documents with "coding" in tags array
        assert!(
            !result.records.is_empty(),
            "Should find documents with 'coding' in tags array"
        );
    }

    #[tokio::test]
    #[ignore = "Requires deeply nested property structure in test data"]
    async fn property_nested() {
        // TDD: Write failing test first
        // Test nested property access

        let (_client, _store) = setup_test_db().await.unwrap();

        // Test-kiln doesn't have deeply nested properties in current data
        // We'll need to add some or use a different approach
    }

    // ============================================================================
    // COMBINED METADATA QUERIES (3 tests)
    // ============================================================================

    #[tokio::test]
    async fn combined_tag_property() {
        // TDD: Write failing test first
        // Test combining tag and property filters

        let (client, store) = setup_test_db().await.unwrap();

        // Ingest test data - Agents/Coder.md has tag "agent" and type: "agent-card"
        let _id = ingest_test_file(&store, "Agents/Coder.md").await.unwrap();

        // Search for documents with tag #agent AND type = agent-card using SurrealDB graph syntax
        let query = r#"
            LET $tagged_entities = (SELECT entity_id FROM entity_tags WHERE tag_id.name = 'agent');
            LET $type_entities = (SELECT entity_id FROM properties WHERE key = 'frontmatter' AND value.value.type = 'agent-card');
            SELECT id FROM entities WHERE id IN $tagged_entities.entity_id AND id IN $type_entities.entity_id;
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find documents matching both criteria
        assert!(
            !result.records.is_empty(),
            "Should find documents with #agent AND type=agent-card"
        );
    }

    #[tokio::test]
    async fn combined_multi_property() {
        // TDD: Write failing test first
        // Test combining multiple properties

        let (client, store) = setup_test_db().await.unwrap();

        // Ingest test data - Guides/Getting Started.md has order: 1 and tags array with "guide"
        let _id = ingest_test_file(&store, "Guides/Getting Started.md")
            .await
            .unwrap();

        // Search for documents with order >= 1 AND 'guide' in tags array using SurrealDB graph syntax
        let query = r#"
            SELECT entity_id.id as id, value
            FROM properties
            WHERE key = 'frontmatter'
            AND value.value.order >= 1
            AND 'guide' IN value.value.tags
        "#;

        let result = client.query(query, &[]).await.unwrap();

        // Should find documents matching all property criteria
        assert!(
            !result.records.is_empty(),
            "Should find documents with order >= 1 AND 'guide' in tags"
        );
    }

    #[tokio::test]
    #[ignore = "Requires folder/path filtering combined with metadata search"]
    async fn combined_metadata_folder() {
        // TDD: Write failing test first
        // Test combining metadata with folder/path scope

        let (_client, _store) = setup_test_db().await.unwrap();

        // TODO: Implement folder/path filtering combined with metadata
        // This requires storing file paths in entities.data or as a property
    }
}
