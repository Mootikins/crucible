//! Tests for NotifyWatcher filter functionality
//!
//! These tests verify that EventFilter is properly applied by NotifyWatcher.

use crucible_watch::{
    traits::{DebounceConfig, HandlerConfig, WatchConfig, WatchMode},
    EventFilter, FileEvent, FileWatcher, NotifyWatcher,
};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Test that NotifyWatcher applies extension filter from WatchConfig
#[tokio::test]
async fn test_notify_watcher_filters_by_extension() {
    let mut watcher = NotifyWatcher::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Create temp dir
    let temp_dir = TempDir::new().unwrap();

    // Configure filter to only allow .md files
    let filter = EventFilter::new().with_extension("md");

    let config = WatchConfig {
        id: "test-filter".to_string(),
        recursive: true,
        filter: Some(filter),
        debounce: DebounceConfig::default(),
        handler_config: HandlerConfig::default(),
        mode: WatchMode::Standard,
        backend_options: Default::default(),
    };

    watcher
        .watch(temp_dir.path().to_path_buf(), config)
        .await
        .unwrap();

    // Create files - one .md, one .log
    fs::write(temp_dir.path().join("note.md"), "markdown content").unwrap();
    fs::write(temp_dir.path().join("data.log"), "log content").unwrap();

    // Wait for debounce
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Collect received events
    let mut received_paths: Vec<PathBuf> = vec![];
    while let Ok(event) = rx.try_recv() {
        received_paths.push(event.path.clone());
    }

    // Should have received event for .md file
    assert!(
        received_paths.iter().any(|p| p.ends_with("note.md")),
        "Should receive event for .md file, got: {:?}",
        received_paths
    );

    // Should NOT have received event for .log file
    assert!(
        !received_paths.iter().any(|p| p.ends_with("data.log")),
        "Should NOT receive event for .log file, got: {:?}",
        received_paths
    );
}

/// Test that NotifyWatcher excludes directories specified in filter
#[tokio::test]
async fn test_notify_watcher_excludes_directory() {
    let mut watcher = NotifyWatcher::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Create temp dir with .crucible subdirectory
    let temp_dir = TempDir::new().unwrap();
    let crucible_dir = temp_dir.path().join(".crucible");
    fs::create_dir_all(&crucible_dir).unwrap();

    // Configure filter to exclude .crucible directory
    let filter = EventFilter::new().exclude_dir(crucible_dir.clone());

    let config = WatchConfig {
        id: "test-exclude-dir".to_string(),
        recursive: true,
        filter: Some(filter),
        debounce: DebounceConfig::default(),
        handler_config: HandlerConfig::default(),
        mode: WatchMode::Standard,
        backend_options: Default::default(),
    };

    watcher
        .watch(temp_dir.path().to_path_buf(), config)
        .await
        .unwrap();

    // Create files - one in root, one in .crucible
    fs::write(temp_dir.path().join("note.md"), "root note").unwrap();
    fs::write(crucible_dir.join("db.log"), "database log").unwrap();

    // Wait for debounce
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Collect received events
    let mut received_paths: Vec<PathBuf> = vec![];
    while let Ok(event) = rx.try_recv() {
        received_paths.push(event.path.clone());
    }

    // Should have received event for root file
    assert!(
        received_paths.iter().any(|p| p.ends_with("note.md")),
        "Should receive event for root file, got: {:?}",
        received_paths
    );

    // Should NOT have received event for .crucible file
    assert!(
        !received_paths
            .iter()
            .any(|p| p.to_string_lossy().contains(".crucible")),
        "Should NOT receive event for .crucible file, got: {:?}",
        received_paths
    );
}

/// Test combined extension and directory filtering (the actual use case)
#[tokio::test]
async fn test_notify_watcher_combined_filter() {
    let mut watcher = NotifyWatcher::new();
    let (tx, mut rx) = mpsc::unbounded_channel::<FileEvent>();
    watcher.set_event_sender(tx);

    // Create temp dir with .crucible subdirectory
    let temp_dir = TempDir::new().unwrap();
    let crucible_dir = temp_dir.path().join(".crucible");
    let db_dir = crucible_dir.join("kiln.db");
    fs::create_dir_all(&db_dir).unwrap();

    // Configure filter: only .md files AND exclude .crucible directory
    let filter = EventFilter::new()
        .with_extension("md")
        .exclude_dir(crucible_dir.clone());

    let config = WatchConfig {
        id: "test-combined".to_string(),
        recursive: true,
        filter: Some(filter),
        debounce: DebounceConfig::default(),
        handler_config: HandlerConfig::default(),
        mode: WatchMode::Standard,
        backend_options: Default::default(),
    };

    watcher
        .watch(temp_dir.path().to_path_buf(), config)
        .await
        .unwrap();

    // Create various files
    fs::write(temp_dir.path().join("note.md"), "valid note").unwrap();
    fs::write(temp_dir.path().join("readme.txt"), "text file").unwrap();
    fs::write(db_dir.join("000031.log"), "db log file").unwrap();
    fs::write(crucible_dir.join("test.md"), "md in crucible").unwrap();

    // Wait for debounce
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Collect received events
    let mut received_paths: Vec<PathBuf> = vec![];
    while let Ok(event) = rx.try_recv() {
        received_paths.push(event.path.clone());
    }

    // Should ONLY have received event for note.md (root .md file)
    assert!(
        received_paths.iter().any(|p| p.ends_with("note.md")),
        "Should receive event for note.md, got: {:?}",
        received_paths
    );

    // Should NOT have received:
    // - readme.txt (wrong extension)
    assert!(
        !received_paths.iter().any(|p| p.ends_with("readme.txt")),
        "Should NOT receive event for .txt file"
    );

    // - 000031.log (in .crucible and wrong extension)
    assert!(
        !received_paths.iter().any(|p| p.ends_with("000031.log")),
        "Should NOT receive event for .log file in .crucible"
    );

    // - test.md (in .crucible directory, even though it's .md)
    assert!(
        !received_paths
            .iter()
            .any(|p| p.to_string_lossy().contains(".crucible")),
        "Should NOT receive event for any file in .crucible directory"
    );
}
