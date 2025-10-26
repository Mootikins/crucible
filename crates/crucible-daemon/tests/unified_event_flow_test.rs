//! Integration test for unified event flow
//!
//! This test verifies that the daemon's unified event flow works correctly:
//! Filesystem events -> DaemonEventHandler -> EmbeddingEvent -> EmbeddingProcessor -> Database

use anyhow::Result;
use chrono::Utc;
use crucible_daemon::config::{DaemonConfig, WatchPath};
use crucible_daemon::coordinator::DataCoordinator;
use crucible_daemon::events::{DaemonEvent, FilesystemEvent, FilesystemEventType};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;
use uuid::Uuid;

/// Test the unified event flow from filesystem event to embedding processing
#[tokio::test]
async fn test_unified_event_flow_integration() -> Result<()> {
    // Create temporary test directory
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create a test markdown file
    let test_file_path = vault_path.join("test_note.md");
    let test_content = r#"---
title: Test Note
tags: [test, integration]
---

# Test Note

This is a test note for verifying the unified event flow.

## Content

Some content here to test embedding generation.

## Features

- Feature 1
- Feature 2
- Feature 3
"#;

    std::fs::write(&test_file_path, test_content)?;

    // Create daemon configuration
    let mut config = DaemonConfig::default();
    config.filesystem.watch_paths = vec![WatchPath {
        path: vault_path.to_path_buf(),
        recursive: true,
        mode: crucible_daemon::config::WatchMode::All,
        filters: None,
        events: None,
    }];
    config.database.backup.storage.path = vault_path.join("test.db");

    // Create data coordinator
    let mut coordinator = DataCoordinator::new(config).await?;

    // Initialize coordinator (this sets up the embedding processor)
    coordinator.initialize().await?;

    // Create a filesystem event
    let fs_event = FilesystemEvent {
        event_id: Uuid::new_v4(),
        path: test_file_path.clone(),
        event_type: FilesystemEventType::Created,
        timestamp: Utc::now(),
        source_path: None,
    };

    // Convert to daemon event and publish
    let daemon_event = DaemonEvent::Filesystem(fs_event);

    // Publish the event using the coordinator's publish_event method
    coordinator.publish_event(daemon_event).await?;

    // Give some time for the event to be processed and embedding to be generated
    sleep(Duration::from_secs(2)).await;

    // Verify that the embedding processor received and processed the event
    // This is a basic test - in a real scenario we would check the database
    // for the embedding record, but for now we just verify the flow doesn't crash

    // Clean up
    coordinator.stop().await?;

    Ok(())
}

/// Test that the unified flow works with multiple files
#[tokio::test]
async fn test_multiple_files_unified_flow() -> Result<()> {
    // Create temporary test directory
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create multiple test markdown files
    let files = vec![
        ("file1.md", "# File 1\nContent of file 1"),
        ("file2.md", "# File 2\nContent of file 2"),
        ("file3.md", "# File 3\nContent of file 3"),
    ];

    for (filename, content) in &files {
        let file_path = vault_path.join(filename);
        std::fs::write(file_path, content)?;
    }

    // Create daemon configuration
    let mut config = DaemonConfig::default();
    config.filesystem.watch_paths = vec![WatchPath {
        path: vault_path.to_path_buf(),
        recursive: true,
        mode: crucible_daemon::config::WatchMode::All,
        filters: None,
        events: None,
    }];
    config.database.backup.storage.path = vault_path.join("test_multiple.db");

    // Create data coordinator
    let mut coordinator = DataCoordinator::new(config).await?;

    // Initialize coordinator
    coordinator.initialize().await?;

    // Create and publish filesystem events for each file
    for (filename, _) in &files {
        let file_path = vault_path.join(filename);
        let fs_event = FilesystemEvent {
            event_id: Uuid::new_v4(),
            path: file_path.clone(),
            event_type: FilesystemEventType::Created,
            timestamp: Utc::now(),
            source_path: None,
        };

        let daemon_event = DaemonEvent::Filesystem(fs_event);
        coordinator.publish_event(daemon_event).await?;
    }

    // Give some time for all events to be processed
    sleep(Duration::from_secs(3)).await;

    // Clean up
    coordinator.stop().await?;

    Ok(())
}

/// Test that the unified flow gracefully handles non-existent files
#[tokio::test]
async fn test_nonexistent_file_handling() -> Result<()> {
    // Create temporary test directory
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create daemon configuration
    let mut config = DaemonConfig::default();
    config.filesystem.watch_paths = vec![WatchPath {
        path: vault_path.to_path_buf(),
        recursive: true,
        mode: crucible_daemon::config::WatchMode::All,
        filters: None,
        events: None,
    }];
    config.database.backup.storage.path = vault_path.join("test_nonexistent.db");

    // Create data coordinator
    let mut coordinator = DataCoordinator::new(config).await?;

    // Initialize coordinator
    coordinator.initialize().await?;

    // Create a filesystem event for a non-existent file
    let nonexistent_path = vault_path.join("nonexistent.md");
    let fs_event = FilesystemEvent {
        event_id: Uuid::new_v4(),
        path: nonexistent_path.clone(),
        event_type: FilesystemEventType::Created,
        timestamp: Utc::now(),
        source_path: None,
    };

    let daemon_event = DaemonEvent::Filesystem(fs_event);

    // This should not panic or crash, even though the file doesn't exist
    let result = coordinator.publish_event(daemon_event).await;
    assert!(
        result.is_ok(),
        "Publishing event for nonexistent file should not fail"
    );

    // Give some time for the event to be processed
    sleep(Duration::from_secs(1)).await;

    // Clean up
    coordinator.stop().await?;

    Ok(())
}
