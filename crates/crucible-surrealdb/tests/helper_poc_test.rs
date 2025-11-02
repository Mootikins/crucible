//! Proof-of-concept test for helper functions and SurrealQL patterns
//!
//! This test verifies:
//! - Helper functions work correctly
//! - Frontmatter parsing and storage works
//! - Nested metadata property access works (metadata.project.status)
//! - Graph traversal syntax works (->wikilink->notes)
//! - Result parsing works
//!
//! This is a SAFE POC test to validate the foundation before converting other tests.

mod common;

use common::{setup_test_client, create_test_note_with_frontmatter, create_wikilink};

#[tokio::test]
async fn test_helper_functions_with_surrealql_patterns() {
    // Setup
    let (client, kiln_root) = setup_test_client().await;

    // Test 1: Create a note with nested metadata using frontmatter
    let frontmatter = r#"project:
  name: crucible
  status: active
  priority: 1
author: test_user"#;

    let note_a_id = create_test_note_with_frontmatter(
        &client,
        "Projects/TestA.md",
        "This is note A with nested metadata",
        frontmatter,
        &kiln_root,
    )
    .await
    .expect("Failed to create note A with frontmatter");

    println!("Created note A with ID: {}", note_a_id);
    assert!(note_a_id.contains("Projects_TestA_md"), "Record ID should match path");

    // Test 2: Verify the note exists and can be retrieved
    let verify_result = client
        .query(&format!("SELECT * FROM {}", note_a_id), &[])
        .await
        .expect("Failed to query note A");

    assert_eq!(verify_result.records.len(), 1, "Note A should exist");
    println!("Successfully verified note A exists");

    // Test 3: Query using nested property access
    let nested_query = "SELECT path, metadata.project.status FROM notes WHERE metadata.project.status = 'active'";
    let nested_result = client
        .query(nested_query, &[])
        .await
        .expect("Failed to query with nested property access");

    assert!(!nested_result.records.is_empty(), "Should find note with nested metadata.project.status = 'active'");
    println!("Successfully found note using nested property query: {} records", nested_result.records.len());

    // Test 4: Create a second note for wikilink testing
    let note_b_id = create_test_note_with_frontmatter(
        &client,
        "Projects/TestB.md",
        "This is note B - the target of a wikilink",
        "tags: [test, poc]",
        &kiln_root,
    )
    .await
    .expect("Failed to create note B");

    println!("Created note B with ID: {}", note_b_id);

    // Test 5: Create wikilink from A to B
    create_wikilink(
        &client,
        &note_a_id,
        &note_b_id,
        "TestB",
        0,
    )
    .await
    .expect("Failed to create wikilink from A to B");

    println!("Created wikilink from A to B");

    // Test 6: Verify wikilink exists
    let wikilink_check = client
        .query(
            &format!("SELECT * FROM wikilink WHERE in = {}", note_a_id),
            &[],
        )
        .await
        .expect("Failed to check wikilink");

    assert_eq!(wikilink_check.records.len(), 1, "Wikilink should exist");
    println!("Successfully verified wikilink exists");

    // Test 7: Query using graph traversal syntax
    let graph_query = format!("SELECT ->wikilink->notes.path FROM {}", note_a_id);
    let graph_result = client
        .query(&graph_query, &[])
        .await
        .expect("Failed to query graph traversal");

    assert!(!graph_result.records.is_empty(), "Graph traversal should find connected notes");
    println!("Successfully traversed graph from A to B: {} records", graph_result.records.len());

    // Test 8: Verify we can access the connected note's path
    // Graph traversal returns the out-edges with their target properties
    let detailed_graph_query = format!(
        "SELECT ->wikilink->notes.{{ path, id }} FROM {}",
        note_a_id
    );
    let detailed_result = client
        .query(&detailed_graph_query, &[])
        .await
        .expect("Failed to query detailed graph traversal");

    assert!(!detailed_result.records.is_empty(), "Detailed graph traversal should return results");
    println!("Successfully retrieved connected note details");

    println!("\nAll POC tests passed!");
    println!("✓ Helper functions work");
    println!("✓ Frontmatter parsing works");
    println!("✓ Nested metadata access works");
    println!("✓ Graph traversal syntax works");
    println!("✓ Result parsing works");
}
