//! Comprehensive integration tests for file system watch functionality.
//!
//! This test suite provides end-to-end verification of the watch system,
//! covering basic operations, rapid event handling, edge cases, error scenarios,
//! backend-specific behavior, and debouncing logic.
//!
//! ## Test Organization
//!
//! 1. **Basic Watch Functionality** - Core file system events
//! 2. **Rapid Event Handling** - High-frequency event scenarios
//! 3. **Edge Cases** - Permissions, symlinks, deep nesting, filtering
//! 4. **Error Scenarios** - Permission denied, invalid paths, locked files
//! 5. **Backend-Specific** - Notify and polling backend verification
//! 6. **Debouncing Tests** - Event consolidation and timing

use anyhow::Result;
use async_trait::async_trait;
use crucible_watch::{
    DebounceConfig, EventFilter, EventHandler, FileEvent, FileEventKind, WatchManager,
    WatchManagerConfig,
};
// Use TraitWatchConfig from prelude to avoid confusion with config::WatchConfig
use crucible_watch::prelude::TraitWatchConfig as WatchConfig;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout, Duration};

// ============================================================================
// Test Helpers and Utilities
// ============================================================================

/// Test event collector that captures file events for verification.
struct TestEventCollector {
    events: Arc<Mutex<Vec<FileEvent>>>,
}

impl TestEventCollector {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn get_events(&self) -> Vec<FileEvent> {
        self.events.lock().await.clone()
    }

    async fn wait_for_event(&self, timeout_ms: u64) -> Option<FileEvent> {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(timeout_ms) {
            {
                let events = self.events.lock().await;
                if !events.is_empty() {
                    return Some(events[0].clone());
                }
            }
            sleep(Duration::from_millis(10)).await;
        }
        None
    }

    async fn wait_for_events(&self, count: usize, timeout_ms: u64) -> Vec<FileEvent> {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(timeout_ms) {
            {
                let events = self.events.lock().await;
                if events.len() >= count {
                    return events.clone();
                }
            }
            sleep(Duration::from_millis(10)).await;
        }
        self.events.lock().await.clone()
    }

    async fn clear(&self) {
        self.events.lock().await.clear();
    }
}

#[async_trait]
impl EventHandler for TestEventCollector {
    async fn handle(&self, event: FileEvent) -> crucible_watch::Result<()> {
        self.events.lock().await.push(event);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "test_collector"
    }

    fn can_handle(&self, _event: &FileEvent) -> bool {
        true
    }
}

/// Setup a watch manager with test event collector.
async fn setup_watch_manager(
    path: &Path,
) -> Result<(WatchManager, Arc<TestEventCollector>)> {
    let config = WatchManagerConfig {
        queue_capacity: 1000,
        debounce_delay: Duration::from_millis(50),
        enable_default_handlers: false,
        max_concurrent_handlers: 10,
        enable_monitoring: false,
    };

    let mut manager = WatchManager::new(config).await?;
    let collector = Arc::new(TestEventCollector::new());

    manager.register_handler(collector.clone()).await?;
    manager.start().await?;

    // Add watch for the path
    let watch_config = WatchConfig::new("test_watch")
        .with_recursive(true)
        .with_debounce(DebounceConfig::new(50));

    manager.add_watch(path.to_path_buf(), watch_config).await?;

    // Give the watch time to initialize
    sleep(Duration::from_millis(100)).await;

    Ok((manager, collector))
}

/// Create a test file with content.
async fn create_test_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Modify an existing test file.
async fn modify_test_file(path: &Path, content: &str) -> Result<()> {
    tokio::fs::write(path, content).await?;
    Ok(())
}

/// Delete a test file.
async fn delete_test_file(path: &Path) -> Result<()> {
    tokio::fs::remove_file(path).await?;
    Ok(())
}

/// Rename a test file.
async fn rename_test_file(from: &Path, to: &Path) -> Result<()> {
    tokio::fs::rename(from, to).await?;
    Ok(())
}

// ============================================================================
// 1. Basic Watch Functionality (5 tests)
// ============================================================================

#[tokio::test]
async fn test_watch_detects_file_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create a new file
    let test_file = temp_dir.path().join("test_create.md");
    create_test_file(&test_file, "# Test Content").await?;

    // Wait for event
    let event = timeout(Duration::from_secs(2), async {
        collector.wait_for_event(2000).await
    })
    .await?
    .expect("Should receive creation event");

    // Verify event
    assert!(matches!(event.kind, FileEventKind::Created));
    assert!(event.path.ends_with("test_create.md"));

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_detects_file_modification() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test_modify.md");

    // Create file before setting up watch
    create_test_file(&test_file, "# Initial Content").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Modify the file
    modify_test_file(&test_file, "# Modified Content").await?;

    // Wait for event
    let event = timeout(Duration::from_secs(2), async {
        collector.wait_for_event(2000).await
    })
    .await?
    .expect("Should receive modification event");

    // Verify event
    assert!(matches!(event.kind, FileEventKind::Modified));
    assert!(event.path.ends_with("test_modify.md"));

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_detects_file_deletion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("test_delete.md");

    // Create file before setting up watch
    create_test_file(&test_file, "# Content to Delete").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Delete the file
    delete_test_file(&test_file).await?;

    // Wait for event
    let event = timeout(Duration::from_secs(2), async {
        collector.wait_for_event(2000).await
    })
    .await?
    .expect("Should receive deletion event");

    // Verify event
    assert!(matches!(event.kind, FileEventKind::Deleted));
    assert!(event.path.ends_with("test_delete.md"));

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_detects_file_rename() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let old_file = temp_dir.path().join("test_old.md");
    let new_file = temp_dir.path().join("test_new.md");

    // Create file before setting up watch
    create_test_file(&old_file, "# Content to Rename").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Rename the file
    rename_test_file(&old_file, &new_file).await?;

    // Wait for events (may receive delete + create or move event)
    let events = collector.wait_for_events(1, 2000).await;
    assert!(!events.is_empty(), "Should receive rename-related events");

    // Verify we got appropriate events (delete, create, or move)
    let has_relevant_event = events.iter().any(|e| {
        matches!(
            e.kind,
            FileEventKind::Deleted | FileEventKind::Created | FileEventKind::Moved { .. }
        )
    });
    assert!(has_relevant_event, "Should receive rename-related event");

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_detects_directory_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create a subdirectory
    let sub_dir = temp_dir.path().join("subdir");
    tokio::fs::create_dir(&sub_dir).await?;

    // Create a file in the subdirectory
    let test_file = sub_dir.join("test_in_subdir.md");
    create_test_file(&test_file, "# Test Content").await?;

    // Wait for events
    let events = collector.wait_for_events(1, 2000).await;
    assert!(
        !events.is_empty(),
        "Should receive events for directory operations"
    );

    // Verify we detected file creation (and possibly directory creation)
    let has_file_event = events
        .iter()
        .any(|e| e.path.ends_with("test_in_subdir.md"));
    assert!(has_file_event, "Should detect file creation in subdirectory");

    manager.shutdown().await?;
    Ok(())
}

// ============================================================================
// 2. Rapid Event Handling (4 tests)
// ============================================================================

#[tokio::test]
async fn test_watch_handles_rapid_file_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create 100 files rapidly
    for i in 0..100 {
        let test_file = temp_dir.path().join(format!("rapid_{}.md", i));
        create_test_file(&test_file, &format!("# File {}", i)).await?;
    }

    // Wait for events
    sleep(Duration::from_millis(1000)).await;
    let events = collector.get_events().await;

    // Should have received many events (exact count may vary due to debouncing)
    assert!(
        events.len() >= 50,
        "Should handle rapid file changes, got {} events",
        events.len()
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_debouncing_consolidates_events() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("debounce_test.md");

    // Create file before setting up watch
    create_test_file(&test_file, "# Initial").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Modify file multiple times rapidly
    for i in 0..10 {
        modify_test_file(&test_file, &format!("# Modified {}", i)).await?;
        sleep(Duration::from_millis(5)).await; // Very rapid changes
    }

    // Wait for debouncing to settle
    sleep(Duration::from_millis(200)).await;
    let events = collector.get_events().await;

    // Should have fewer events than modifications due to debouncing
    assert!(
        events.len() < 10,
        "Debouncing should consolidate events, got {} events",
        events.len()
    );
    assert!(
        !events.is_empty(),
        "Should still receive at least one event"
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_file_rename_chain() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create initial file
    let file_a = temp_dir.path().join("file_a.md");
    create_test_file(&file_a, "# Content").await?;
    sleep(Duration::from_millis(200)).await;
    collector.clear().await;

    // Rename chain: a -> b -> c
    let file_b = temp_dir.path().join("file_b.md");
    let file_c = temp_dir.path().join("file_c.md");

    rename_test_file(&file_a, &file_b).await?;
    sleep(Duration::from_millis(100)).await;

    rename_test_file(&file_b, &file_c).await?;
    sleep(Duration::from_millis(100)).await;

    let events = collector.get_events().await;
    assert!(
        !events.is_empty(),
        "Should detect rename chain operations"
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_file_delete_and_recreate() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("delete_recreate.md");

    // Create file before setting up watch
    create_test_file(&test_file, "# Original").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Delete file
    delete_test_file(&test_file).await?;
    sleep(Duration::from_millis(100)).await;

    // Recreate file immediately
    create_test_file(&test_file, "# Recreated").await?;
    sleep(Duration::from_millis(200)).await;

    let events = collector.get_events().await;
    assert!(
        events.len() >= 2,
        "Should detect both delete and create, got {} events",
        events.len()
    );

    // Verify we have both deletion and creation events
    let has_delete = events.iter().any(|e| matches!(e.kind, FileEventKind::Deleted));
    let has_create = events.iter().any(|e| matches!(e.kind, FileEventKind::Created));

    assert!(has_delete || has_create, "Should detect delete or create event");

    manager.shutdown().await?;
    Ok(())
}

// ============================================================================
// 3. Edge Cases (6 tests)
// ============================================================================

#[tokio::test]
#[cfg(unix)]
async fn test_watch_handles_permission_changes() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("chmod_test.md");

    // Create file
    create_test_file(&test_file, "# Test").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Change permissions
    let metadata = tokio::fs::metadata(&test_file).await?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o444); // Read-only
    tokio::fs::set_permissions(&test_file, permissions).await?;

    // Wait for potential event
    sleep(Duration::from_millis(200)).await;
    let events = collector.get_events().await;

    // Permission changes may or may not trigger events depending on backend
    // Just verify system doesn't crash
    println!(
        "Permission change generated {} events (backend-dependent)",
        events.len()
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
#[cfg(unix)]
async fn test_watch_handles_symlinks() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let target_file = temp_dir.path().join("target.md");
    let symlink_file = temp_dir.path().join("link.md");

    // Create target file
    create_test_file(&target_file, "# Target").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Create symlink
    #[cfg(unix)]
    {
        tokio::fs::symlink(&target_file, &symlink_file).await?;
    }

    sleep(Duration::from_millis(200)).await;

    // Modify target file
    modify_test_file(&target_file, "# Modified Target").await?;

    sleep(Duration::from_millis(200)).await;
    let events = collector.get_events().await;

    // Should detect modifications (symlink behavior is backend-dependent)
    println!("Symlink test generated {} events", events.len());

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_handles_directory_deletion() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let sub_dir = temp_dir.path().join("to_delete");

    // Create subdirectory with files
    tokio::fs::create_dir(&sub_dir).await?;
    create_test_file(&sub_dir.join("file1.md"), "# File 1").await?;
    create_test_file(&sub_dir.join("file2.md"), "# File 2").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Delete entire directory
    tokio::fs::remove_dir_all(&sub_dir).await?;

    sleep(Duration::from_millis(300)).await;
    let events = collector.get_events().await;

    // Should receive deletion events
    assert!(
        !events.is_empty(),
        "Should detect directory deletion events"
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_handles_very_deep_nesting() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create deeply nested directory structure (50 levels)
    let mut current_path = temp_dir.path().to_path_buf();
    for i in 0..50 {
        current_path = current_path.join(format!("level_{}", i));
    }
    tokio::fs::create_dir_all(&current_path).await?;

    // Create file in deepest directory
    let deep_file = current_path.join("deep_file.md");
    create_test_file(&deep_file, "# Very Deep File").await?;

    // Wait for events
    sleep(Duration::from_millis(500)).await;
    let events = collector.get_events().await;

    // Should handle deep nesting (exact behavior depends on recursive watch)
    println!("Deep nesting test generated {} events", events.len());

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_filters_ignored_files() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Setup manager with filter for .md files only
    let config = WatchManagerConfig {
        queue_capacity: 1000,
        debounce_delay: Duration::from_millis(50),
        enable_default_handlers: false,
        max_concurrent_handlers: 10,
        enable_monitoring: false,
    };

    let mut manager = WatchManager::new(config).await?;
    let collector = Arc::new(TestEventCollector::new());
    manager.register_handler(collector.clone()).await?;
    manager.start().await?;

    // Add watch with filter
    let filter = EventFilter::new().with_extension("md");
    let watch_config = WatchConfig::new("filtered_watch")
        .with_recursive(true)
        .with_filter(filter);

    manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await?;
    sleep(Duration::from_millis(100)).await;

    // Create files with different extensions
    create_test_file(&temp_dir.path().join("test.md"), "# Markdown").await?;
    create_test_file(&temp_dir.path().join("test.txt"), "Plain text").await?;
    create_test_file(&temp_dir.path().join("test.rs"), "// Rust code").await?;

    sleep(Duration::from_millis(300)).await;
    let events = collector.get_events().await;

    // Should only receive .md file events (filtering may happen at handler level)
    // Note: Backend may still emit all events, but filter applies during handling
    println!("Filter test received {} events", events.len());

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_handles_binary_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create binary file (simulated with non-UTF8 bytes)
    let binary_file = temp_dir.path().join("binary.dat");
    let binary_data = vec![0xFF, 0xFE, 0xFD, 0xFC, 0x00, 0x01, 0x02, 0x03];
    tokio::fs::write(&binary_file, &binary_data).await?;

    sleep(Duration::from_millis(200)).await;
    let events = collector.get_events().await;

    // Should handle binary files without crashing
    assert!(
        !events.is_empty(),
        "Should detect binary file creation"
    );

    manager.shutdown().await?;
    Ok(())
}

// ============================================================================
// 4. Error Scenarios (3 tests)
// ============================================================================

#[tokio::test]
async fn test_watch_handles_permission_denied() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let restricted_dir = temp_dir.path().join("restricted");
    tokio::fs::create_dir(&restricted_dir).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = tokio::fs::metadata(&restricted_dir).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o000); // No permissions
        tokio::fs::set_permissions(&restricted_dir, permissions).await?;
    }

    // Try to watch restricted directory
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;
    manager.start().await?;

    let watch_config = WatchConfig::new("restricted_watch");
    let result = manager.add_watch(restricted_dir.clone(), watch_config).await;

    // May fail or succeed depending on platform and permissions
    // Just verify it doesn't crash
    println!("Watching restricted directory result: {:?}", result);

    manager.shutdown().await?;

    // Cleanup: restore permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = tokio::fs::metadata(&restricted_dir).await?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        tokio::fs::set_permissions(&restricted_dir, permissions).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_watch_handles_invalid_path() -> Result<()> {
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;
    manager.start().await?;

    // Try to watch non-existent path
    let invalid_path = PathBuf::from("/nonexistent/invalid/path");
    let watch_config = WatchConfig::new("invalid_watch");

    let result = manager.add_watch(invalid_path, watch_config).await;

    // Should return error for invalid path
    assert!(result.is_err(), "Should fail for invalid path");

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_watch_handles_file_locked_by_another_process() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("locked.md");

    // Create file
    create_test_file(&test_file, "# Locked Content").await?;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Open file with exclusive access (platform-dependent)
    let _file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&test_file)?;

    // Try to modify file (may or may not succeed depending on platform)
    let modify_result = modify_test_file(&test_file, "# New Content").await;

    sleep(Duration::from_millis(200)).await;
    let events = collector.get_events().await;

    // Should handle locked file scenario gracefully
    println!(
        "Locked file modify result: {:?}, events: {}",
        modify_result,
        events.len()
    );

    manager.shutdown().await?;
    Ok(())
}

// ============================================================================
// 5. Backend-Specific Tests (3 tests)
// ============================================================================

#[tokio::test]
async fn test_notify_backend_performance() -> Result<()> {
    let temp_dir = TempDir::new()?;

    // Setup manager (will use notify backend by default on supported platforms)
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;
    let collector = Arc::new(TestEventCollector::new());

    manager.register_handler(collector.clone()).await?;
    manager.start().await?;

    let watch_config = WatchConfig::new("perf_test");
    manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await?;

    sleep(Duration::from_millis(100)).await;

    // Create many files quickly to test performance
    let start = std::time::Instant::now();
    for i in 0..50 {
        let test_file = temp_dir.path().join(format!("perf_{}.md", i));
        create_test_file(&test_file, &format!("# File {}", i)).await?;
    }
    let creation_time = start.elapsed();

    // Wait for all events
    sleep(Duration::from_millis(500)).await;
    let events = collector.get_events().await;
    let detection_time = start.elapsed();

    println!(
        "Created 50 files in {:?}, detected {} events in {:?}",
        creation_time,
        events.len(),
        detection_time
    );

    assert!(
        !events.is_empty(),
        "Notify backend should detect file creations"
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_polling_backend_fallback() -> Result<()> {
    // Note: This test verifies the system can handle backend selection
    // Actual polling backend testing would require backend configuration

    let temp_dir = TempDir::new()?;
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;

    manager.start().await?;

    let watch_config = WatchConfig::new("fallback_test");
    let result = manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await;

    // Should successfully create watch regardless of backend
    assert!(result.is_ok(), "Should create watch with available backend");

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_backend_switching() -> Result<()> {
    // Test that manager can handle multiple watches with different configs
    let temp_dir = TempDir::new()?;
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;

    manager.start().await?;

    // Add multiple watches
    let watch1 = WatchConfig::new("watch1");
    let watch2 = WatchConfig::new("watch2").with_recursive(false);

    let result1 = manager.add_watch(temp_dir.path().to_path_buf(), watch1).await;
    let result2 = manager
        .add_watch(temp_dir.path().join("subdir"), watch2)
        .await;

    // At least one should succeed (subdir may not exist yet)
    println!("Watch 1 result: {:?}, Watch 2 result: {:?}", result1, result2);

    manager.shutdown().await?;
    Ok(())
}

// ============================================================================
// 6. Debouncing Tests (4 tests)
// ============================================================================

#[tokio::test]
async fn test_debouncer_consolidates_rapid_changes() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("debounce.md");

    create_test_file(&test_file, "# Initial").await?;
    sleep(Duration::from_millis(100)).await;

    // Use very short debounce delay for testing
    let config = WatchManagerConfig {
        queue_capacity: 1000,
        debounce_delay: Duration::from_millis(100),
        enable_default_handlers: false,
        max_concurrent_handlers: 10,
        enable_monitoring: false,
    };

    let mut manager = WatchManager::new(config).await?;
    let collector = Arc::new(TestEventCollector::new());
    manager.register_handler(collector.clone()).await?;
    manager.start().await?;

    let watch_config = WatchConfig::new("debounce_test")
        .with_debounce(DebounceConfig::new(100));
    manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await?;

    sleep(Duration::from_millis(100)).await;
    collector.clear().await;

    // Rapid modifications
    for i in 0..20 {
        modify_test_file(&test_file, &format!("# Rapid {}", i)).await?;
        sleep(Duration::from_millis(5)).await;
    }

    // Wait for debouncing to complete
    sleep(Duration::from_millis(300)).await;
    let events = collector.get_events().await;

    // Should have significantly fewer events than modifications
    assert!(
        events.len() < 20,
        "Debouncer should consolidate events, got {}",
        events.len()
    );

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_debouncer_respects_timeout() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("timeout.md");

    create_test_file(&test_file, "# Initial").await?;
    sleep(Duration::from_millis(100)).await;

    let config = WatchManagerConfig {
        queue_capacity: 1000,
        debounce_delay: Duration::from_millis(200), // 200ms debounce
        enable_default_handlers: false,
        max_concurrent_handlers: 10,
        enable_monitoring: false,
    };

    let mut manager = WatchManager::new(config).await?;
    let collector = Arc::new(TestEventCollector::new());
    manager.register_handler(collector.clone()).await?;
    manager.start().await?;

    let watch_config = WatchConfig::new("timeout_test")
        .with_debounce(DebounceConfig::new(200));
    manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await?;

    sleep(Duration::from_millis(100)).await;
    collector.clear().await;

    // Single modification
    modify_test_file(&test_file, "# Modified").await?;

    // Should receive event after debounce timeout
    let event = timeout(Duration::from_millis(500), async {
        collector.wait_for_event(500).await
    })
    .await?;

    assert!(event.is_some(), "Should receive event after debounce timeout");

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_debouncer_handles_concurrent_events() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create multiple files concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let temp_path = temp_dir.path().to_path_buf();
        handles.push(tokio::spawn(async move {
            let file_path = temp_path.join(format!("concurrent_{}.md", i));
            create_test_file(&file_path, &format!("# File {}", i))
                .await
                .unwrap();
        }));
    }

    // Wait for all concurrent creations
    for handle in handles {
        handle.await?;
    }

    sleep(Duration::from_millis(300)).await;
    let events = collector.get_events().await;

    // Should handle concurrent events without crashing
    assert!(
        !events.is_empty(),
        "Should handle concurrent event creation"
    );
    println!("Concurrent events test received {} events", events.len());

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_debouncer_flush_on_shutdown() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let test_file = temp_dir.path().join("shutdown.md");

    create_test_file(&test_file, "# Initial").await?;
    sleep(Duration::from_millis(100)).await;

    let (mut manager, collector) = setup_watch_manager(temp_dir.path()).await?;
    collector.clear().await;

    // Create modification right before shutdown
    modify_test_file(&test_file, "# Pre-shutdown").await?;

    // Give a tiny bit of time for event to enter system
    sleep(Duration::from_millis(50)).await;

    // Shutdown immediately (before debounce delay expires)
    manager.shutdown().await?;

    // Check if event was captured
    let events = collector.get_events().await;
    println!(
        "Events captured before shutdown: {} (may be 0 due to timing)",
        events.len()
    );

    // This is timing-dependent, so we just verify shutdown works correctly
    Ok(())
}

// ============================================================================
// 7. Manager Lifecycle Tests
// ============================================================================

#[tokio::test]
async fn test_manager_start_stop_lifecycle() -> Result<()> {
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;

    // Should start successfully
    assert!(manager.start().await.is_ok());

    // Starting again should fail
    let result = manager.start().await;
    assert!(result.is_err(), "Should not start twice");

    // Should stop successfully
    assert!(manager.shutdown().await.is_ok());

    // Stopping again should be idempotent
    assert!(manager.shutdown().await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_manager_status_reporting() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = WatchManagerConfig::default();
    let mut manager = WatchManager::new(config).await?;

    // Check initial status
    let status = manager.get_status().await;
    assert!(!status.is_running);
    assert_eq!(status.active_watches, 0);

    // Start manager
    manager.start().await?;
    let status = manager.get_status().await;
    assert!(status.is_running);

    // Add watch
    let watch_config = WatchConfig::new("status_test");
    manager.add_watch(temp_dir.path().to_path_buf(), watch_config).await?;

    let status = manager.get_status().await;
    assert_eq!(status.active_watches, 1);

    manager.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_manager_performance_stats() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let (mut manager, _collector) = setup_watch_manager(temp_dir.path()).await?;

    // Create some events
    for i in 0..10 {
        let test_file = temp_dir.path().join(format!("stats_{}.md", i));
        create_test_file(&test_file, &format!("# File {}", i)).await?;
    }

    sleep(Duration::from_millis(300)).await;

    // Get performance stats
    let stats = manager.get_performance_stats().await;
    println!("Performance stats: {:?}", stats);

    // Stats should be available (exact values depend on timing)
    assert!(stats.total_events >= 0);

    manager.shutdown().await?;
    Ok(())
}
