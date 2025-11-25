//! Tests for Merkle tree persistence with file paths containing spaces
//!
//! Bug: File paths with spaces cause SurrealDB parse errors due to backslash escaping
//! This test suite verifies the fix using parameterized queries with type::thing()

use crucible_merkle::HybridMerkleTree;
use crucible_core::parser::{ParsedNote, NoteContent, Paragraph};
use crucible_surrealdb::{MerklePersistence, SurrealClient, SurrealDbConfig};
use std::path::PathBuf;

/// Create a test SurrealDB client with in-memory storage
async fn create_test_client() -> SurrealClient {
    let config = SurrealDbConfig {
        path: ":memory:".to_string(),
        namespace: "test_spaces".to_string(),
        database: "test_spaces".to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };
    SurrealClient::new(config)
        .await
        .expect("Failed to create test client")
}

/// Create a simple test note for tree generation
fn create_test_note(content: &str) -> ParsedNote {
    let mut note = ParsedNote::default();
    note.path = PathBuf::from("test.md");

    // Parse the content into structured note content
    let mut note_content = NoteContent::new();
    note_content.paragraphs.push(Paragraph {
        text: content.to_string(),
        offset: 0,
    });
    note.content = note_content;

    note
}

/// Create a test HybridMerkleTree from content
fn create_test_tree(content: &str) -> HybridMerkleTree {
    let note = create_test_note(content);
    HybridMerkleTree::from_document(&note)
}

#[tokio::test]
async fn test_store_tree_with_spaces_in_path() {
    // Given: A file path with spaces
    let tree_id = "Projects/Rune MCP/YouTube Transcript Tool - Implementation.md";
    let client = create_test_client().await.unwrap();
    let persistence = MerklePersistence::new(client);
    let tree = create_test_tree("# Test\n\nContent with some text.");

    // When: We store the tree
    let result = persistence.store(tree_id, &tree).await;

    // Then: It should succeed (currently fails with parse error)
    assert!(
        result.is_ok(),
        "Should store tree with spaces in path, got error: {:?}",
        result.err()
    );

    // And: We should be able to retrieve it
    let retrieved = persistence.retrieve(tree_id).await;
    assert!(
        retrieved.is_ok(),
        "Should retrieve tree with spaces in path, got error: {:?}",
        retrieved.err()
    );

    let retrieved_tree = retrieved.unwrap();
    assert_eq!(
        tree.root_hash,
        retrieved_tree.root_hash,
        "Retrieved tree should have same root hash"
    );
}

#[tokio::test]
async fn test_store_tree_with_multiple_spaces_and_special_chars() {
    // Given: A complex path with multiple spaces and special characters
    let tree_id = "My Projects/Research Notes/AI & ML/Deep Learning - Part 1.md";
    let client = create_test_client().await.unwrap();
    let persistence = MerklePersistence::new(client);
    let tree = create_test_tree("# Deep Learning\n\n## Introduction\n\nSome content here.");

    // When: We store the tree
    let result = persistence.store(tree_id, &tree).await;

    // Then: It should succeed
    assert!(
        result.is_ok(),
        "Should handle complex paths with spaces and special chars, got: {:?}",
        result.err()
    );

    // And: Retrieval should work
    let retrieved = persistence.retrieve(tree_id).await;
    assert!(retrieved.is_ok(), "Should retrieve complex path");
    assert_eq!(tree.root_hash, retrieved.unwrap().root_hash);
}

#[tokio::test]
async fn test_update_tree_incremental_with_spaces() {
    // Given: A tree stored with spaces in path
    let tree_id = "Notes/Daily Notes/2025-01-15 Meeting Notes.md";
    let client = create_test_client().await.unwrap();
    let persistence = MerklePersistence::new(client);

    // Store initial tree
    let tree_v1 = create_test_tree("# Meeting\n\nInitial notes.");
    persistence.store(tree_id, &tree_v1).await.unwrap();

    // When: We update with new content
    let tree_v2 = create_test_tree("# Meeting\n\nInitial notes.\n\n## Action Items\n\nNew section.");
    let result = persistence.store(tree_id, &tree_v2).await;

    // Then: Update should succeed
    assert!(
        result.is_ok(),
        "Should update tree with spaces in path, got: {:?}",
        result.err()
    );

    // And: Retrieved tree should have new hash
    let retrieved = persistence.retrieve(tree_id).await.unwrap();
    assert_eq!(tree_v2.root_hash, retrieved.root_hash, "Should have updated hash");
    assert_ne!(tree_v1.root_hash, retrieved.root_hash, "Hash should have changed");
}

#[tokio::test]
async fn test_delete_tree_with_spaces() {
    // Given: A tree stored with spaces
    let tree_id = "Archive/Old Projects/Legacy System.md";
    let client = create_test_client().await.unwrap();
    let persistence = MerklePersistence::new(client);
    let tree = create_test_tree("# Legacy\n\nOld content.");

    persistence.store(tree_id, &tree).await.unwrap();

    // Verify it exists
    assert!(persistence.retrieve(tree_id).await.is_ok());

    // When: We delete the tree
    let result = persistence.delete(tree_id).await;

    // Then: Deletion should succeed
    assert!(
        result.is_ok(),
        "Should delete tree with spaces in path, got: {:?}",
        result.err()
    );

    // And: Tree should no longer exist
    let retrieved = persistence.retrieve(tree_id).await;
    assert!(
        retrieved.is_err(),
        "Tree should not exist after deletion"
    );
}

#[tokio::test]
async fn test_paths_without_spaces_still_work() {
    // Given: A normal path without spaces (regression test)
    let tree_id = "projects/notes/test.md";
    let client = create_test_client().await.unwrap();
    let persistence = MerklePersistence::new(client);
    let tree = create_test_tree("# Test\n\nNormal content.");

    // When/Then: All operations should still work
    persistence.store(tree_id, &tree).await.unwrap();
    let retrieved = persistence.retrieve(tree_id).await.unwrap();
    assert_eq!(tree.root_hash, retrieved.root_hash);
    persistence.delete(tree_id).await.unwrap();
}
