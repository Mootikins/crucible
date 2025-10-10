// Tests for tag search functionality via ObsidianClient
//
// These tests verify that tag search works correctly with the Obsidian plugin API

use crucible_mcp::obsidian_client::ObsidianClient;

/// Test that ObsidianClient can connect and search by tags
///
/// Note: This is an integration test that requires the Obsidian plugin to be running
#[tokio::test]
#[ignore] // Ignore by default since it requires Obsidian to be running
async fn test_obsidian_client_search_by_tags() {
    // This test requires Obsidian to be running with the Crucible plugin
    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping test: Obsidian plugin not accessible: {}", e);
            return;
        }
    };

    // Test searching for a single tag
    let results = client
        .search_by_tags(&["ai".to_string()])
        .await
        .expect("Failed to search by tags");

    // Should find at least one file with 'ai' tag
    assert!(
        !results.is_empty(),
        "Expected to find files with 'ai' tag"
    );

    // Verify result structure
    for file in &results {
        assert!(!file.path.is_empty(), "File path should not be empty");
        assert!(!file.name.is_empty(), "File name should not be empty");
        assert!(file.size > 0, "File size should be greater than 0");
    }
}

/// Test searching by multiple tags
#[tokio::test]
#[ignore] // Ignore by default since it requires Obsidian to be running
async fn test_obsidian_client_search_multiple_tags() {
    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping test: Obsidian plugin not accessible: {}", e);
            return;
        }
    };

    // Test searching for multiple tags
    let results = client
        .search_by_tags(&["ai".to_string(), "project".to_string()])
        .await
        .expect("Failed to search by multiple tags");

    // Should find files that match the tag criteria
    // The exact behavior (AND vs OR) depends on the Obsidian plugin implementation
    println!("Found {} files with tags ['ai', 'project']", results.len());

    for file in &results {
        println!("  - {} ({})", file.path, file.folder);
    }
}

/// Test searching for non-existent tag
#[tokio::test]
#[ignore] // Ignore by default since it requires Obsidian to be running
async fn test_obsidian_client_search_nonexistent_tag() {
    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping test: Obsidian plugin not accessible: {}", e);
            return;
        }
    };

    // Test searching for a tag that doesn't exist
    let results = client
        .search_by_tags(&["nonexistenttag123456".to_string()])
        .await
        .expect("Failed to search by tags");

    // Should return empty results
    assert!(
        results.is_empty(),
        "Expected no results for non-existent tag"
    );
}

/// Test that empty tag array is handled correctly
#[tokio::test]
#[ignore] // Ignore by default since it requires Obsidian to be running
async fn test_obsidian_client_search_empty_tags() {
    let client = match ObsidianClient::new() {
        Ok(client) => client,
        Err(e) => {
            println!("Skipping test: Obsidian plugin not accessible: {}", e);
            return;
        }
    };

    // Test searching with empty tag array
    let result = client.search_by_tags(&[]).await;

    // This might succeed with empty results or fail with an error
    // depending on the Obsidian plugin's behavior
    match result {
        Ok(results) => {
            println!("Empty tag search returned {} results", results.len());
        }
        Err(e) => {
            println!("Empty tag search returned error (expected): {}", e);
        }
    }
}
