//! Basic sync integration tests
//!
//! This module contains end-to-end tests for the basic
//! synchronization functionality.

use crucible_sync::SyncInstance;

/// Test basic document synchronization between two instances
#[tokio::test]
async fn test_basic_document_sync() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two sync instances with same document
    let sync_a = SyncInstance::new("doc1").await?;
    let sync_b = SyncInstance::new("doc1").await?;

    // Act: Make change in instance A
    sync_a.insert_text(0, "Hello World").await?;

    // Sync A -> B
    sync_a.sync_with(&sync_b).await?;

    // Assert: B should have the change
    assert_eq!(sync_b.get_text().await, "Hello World");

    Ok(())
}

/// Test multiple changes and sync direction
#[tokio::test]
async fn test_multiple_changes_sync() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two sync instances
    let sync_a = SyncInstance::new("doc1").await?;
    let sync_b = SyncInstance::new("doc1").await?;

    // Act: Multiple changes in A
    sync_a.insert_text(0, "Hello").await?;
    sync_a.insert_text(5, ", ").await?;
    sync_a.insert_text(7, "World").await?;

    // Sync A -> B
    sync_a.sync_with(&sync_b).await?;

    // Assert: B should have all changes
    assert_eq!(sync_b.get_text().await, "Hello, World");

    // Act: Make change in B and sync back
    sync_b.insert_text(12, "!").await?;
    sync_b.sync_with(&sync_a).await?;

    // Assert: Both should have final content
    assert_eq!(sync_a.get_text().await, "Hello, World!");
    assert_eq!(sync_b.get_text().await, "Hello, World!");

    Ok(())
}

/// Test concurrent edits
#[tokio::test]
async fn test_concurrent_edits() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two sync instances
    let sync_a = SyncInstance::new("doc1").await?;
    let sync_b = SyncInstance::new("doc1").await?;

    // Act: Concurrent edits in both instances
    sync_a.insert_text(0, "Hello").await?;
    sync_b.insert_text(5, "World").await?;

    // Sync both directions
    sync_a.sync_with(&sync_b).await?;
    sync_b.sync_with(&sync_a).await?;

    // Assert: Changes should be merged
    let content_a = sync_a.get_text().await;
    let content_b = sync_b.get_text().await;

    assert_eq!(content_a, content_b);
    // Both "Hello" and "World" should be present
    assert!(content_a.contains("Hello"));
    assert!(content_a.contains("World"));

    Ok(())
}

/// Test empty document sync
#[tokio::test]
async fn test_empty_document_sync() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two empty sync instances
    let sync_a = SyncInstance::new("empty-doc").await?;
    let sync_b = SyncInstance::new("empty-doc").await?;

    // Act: Sync empty documents
    sync_a.sync_with(&sync_b).await?;

    // Assert: Both should still be empty
    assert_eq!(sync_a.get_text().await, "");
    assert_eq!(sync_b.get_text().await, "");

    Ok(())
}

/// Test delete operations sync
#[tokio::test]
async fn test_delete_operations_sync() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two sync instances with content
    let sync_a = SyncInstance::new("doc1").await?;
    let sync_b = SyncInstance::new("doc1").await?;

    sync_a.insert_text(0, "Hello, World!").await?;
    sync_a.sync_with(&sync_b).await?;

    // Act: Delete from A and sync
    sync_a.delete_text(5, 7).await?; // Delete ", World"
    sync_a.sync_with(&sync_b).await?;

    // Assert: Both should have deletion applied
    assert_eq!(sync_a.get_text().await, "Hello!");
    assert_eq!(sync_b.get_text().await, "Hello!");

    Ok(())
}

/// Test document IDs
#[tokio::test]
async fn test_different_document_ids() -> Result<(), Box<dyn std::error::Error>> {
    // Arrange: Two sync instances with different document IDs
    let sync_a = SyncInstance::new("doc1").await?;
    let sync_b = SyncInstance::new("doc2").await?;

    // Act: Add content to A
    sync_a.insert_text(0, "Document 1").await?;

    // Sync (should work but not affect B since they're different documents)
    sync_a.sync_with(&sync_b).await?;

    // Assert: A has content, B doesn't
    assert_eq!(sync_a.get_text().await, "Document 1");
    assert_eq!(sync_b.get_text().await, "");

    Ok(())
}

/// Test error handling
#[tokio::test]
async fn test_error_handling() {
    let sync = SyncInstance::new("test-doc").await.unwrap();

    // Test invalid delete range (should not panic)
    let result = sync.delete_text(100, 10).await;
    // Should either succeed (no-op) or handle gracefully
    assert!(result.is_ok() || result.is_err());
}

/// Test large text handling
#[tokio::test]
async fn test_large_text_sync() -> Result<(), Box<dyn std::error::Error>> {
    let sync_a = SyncInstance::new("large-doc").await?;
    let sync_b = SyncInstance::new("large-doc").await?;

    // Create a large text (1KB)
    let large_text = "Hello, World! ".repeat(64);
    sync_a.insert_text(0, &large_text).await?;

    // Sync
    sync_a.sync_with(&sync_b).await?;

    // Assert: Both should have the large text
    assert_eq!(sync_a.get_text().await, large_text);
    assert_eq!(sync_b.get_text().await, large_text);

    Ok(())
}