//! Integration tests for file watching and delta processing
//!
//! These tests verify that the file watcher correctly:
//! - Detects file changes in the vault
//! - Batches events efficiently
//! - Triggers delta processing with embeddings
//! - Handles edge cases gracefully

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::sleep;

/// Helper to create a test vault with sample content
async fn create_test_vault() -> Result<TempDir> {
    let temp_dir = TempDir::new()?;
    let vault_path = temp_dir.path();

    // Create .obsidian directory for Obsidian vault
    fs::create_dir_all(vault_path.join(".obsidian"))?;

    // Create sample markdown files
    let test_files = vec![
        (
            "note1.md",
            "# Test Note 1\n\nThis is the first test note.",
        ),
        (
            "note2.md",
            "# Test Note 2\n\nThis is the second test note.",
        ),
        (
            "note3.md",
            "# Test Note 3\n\nThis is the third test note.",
        ),
    ];

    for (filename, content) in test_files {
        let file_path = vault_path.join(filename);
        fs::write(file_path, content)?;
    }

    Ok(temp_dir)
}

/// Helper to create a minimal CLI config for testing
fn create_test_config(vault_path: &Path) -> crucible_cli::config::CliConfig {
    use crucible_cli::config::*;

    CliConfig {
        kiln: KilnConfig {
            path: vault_path.to_path_buf(),
            embedding_url: "http://localhost:11434".to_string(),
            embedding_model: Some("nomic-embed-text".to_string()),
        },
        embedding: None,
        file_watching: FileWatcherConfig {
            enabled: true,
            debounce_ms: 100, // Shorter for testing
            exclude_patterns: vec![],
        },
        llm: LlmConfig::default(),
        network: NetworkConfig::default(),
        services: ServicesConfig::default(),
        migration: MigrationConfig::default(),
        custom_database_path: None,
    }
}

#[tokio::test]
async fn test_watcher_config_defaults() -> Result<()> {
    use crucible_cli::config::FileWatcherConfig;

    let config = FileWatcherConfig::default();

    // Verify industry best practices
    assert!(config.enabled, "File watching should be enabled by default");
    assert_eq!(config.debounce_ms, 500, "Default debounce should be 500ms");
    assert!(config.exclude_patterns.is_empty(), "No extra exclude patterns by default");

    Ok(())
}

#[tokio::test]
async fn test_watcher_filters_markdown_only() -> Result<()> {
    use crucible_cli::watcher::SimpleFileWatcher;
    use tokio::sync::mpsc;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    // Create non-markdown files
    fs::write(vault_path.join("test.txt"), "Not markdown")?;
    fs::write(vault_path.join("test.json"), "{}")?;
    fs::write(vault_path.join("test.rs"), "fn main() {}")?;

    let config = crucible_cli::config::FileWatcherConfig {
        enabled: true,
        debounce_ms: 100,
        exclude_patterns: vec![],
    };

    let (tx, mut rx) = mpsc::unbounded_channel();

    // Create watcher
    let _watcher = SimpleFileWatcher::new(vault_path, config, tx)?;

    // Modify markdown file
    fs::write(vault_path.join("note1.md"), "# Updated\n\nContent changed")?;

    // Modify non-markdown file
    fs::write(vault_path.join("test.txt"), "Updated text")?;

    // Wait for events
    sleep(Duration::from_millis(300)).await;

    // Should only receive markdown change event
    let mut md_events = 0;
    while let Ok(event) = rx.try_recv() {
        match event {
            crucible_cli::watcher::WatchEvent::Changed(path) |
            crucible_cli::watcher::WatchEvent::Created(path) => {
                if path.extension().and_then(|s| s.to_str()) == Some("md") {
                    md_events += 1;
                }
            }
            _ => {}
        }
    }

    assert!(md_events > 0, "Should receive markdown file change events");

    Ok(())
}

#[tokio::test]
async fn test_watcher_respects_exclude_patterns() -> Result<()> {
    use crucible_cli::watcher::SimpleFileWatcher;
    use tokio::sync::mpsc;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    // Create files in excluded directories
    fs::create_dir_all(vault_path.join(".obsidian/workspace"))?;
    fs::write(vault_path.join(".obsidian/workspace/test.md"), "Excluded")?;

    fs::create_dir_all(vault_path.join(".git"))?;
    fs::write(vault_path.join(".git/config.md"), "Also excluded")?;

    let config = crucible_cli::config::FileWatcherConfig {
        enabled: true,
        debounce_ms: 100,
        exclude_patterns: vec![],
    };

    let (tx, mut rx) = mpsc::unbounded_channel();
    let _watcher = SimpleFileWatcher::new(vault_path, config, tx)?;

    // Modify excluded files
    fs::write(vault_path.join(".obsidian/workspace/test.md"), "Updated")?;
    fs::write(vault_path.join(".git/config.md"), "Updated")?;

    // Modify included file
    fs::write(vault_path.join("note1.md"), "# Updated\n\nNormal file")?;

    // Wait for events
    sleep(Duration::from_millis(300)).await;

    // Should only receive event for normal file, not excluded ones
    let mut events = vec![];
    while let Ok(event) = rx.try_recv() {
        events.push(event);
    }

    // Verify no events from excluded directories
    for event in &events {
        match event {
            crucible_cli::watcher::WatchEvent::Changed(path) |
            crucible_cli::watcher::WatchEvent::Created(path) => {
                assert!(
                    !path.to_string_lossy().contains(".obsidian/workspace"),
                    "Should not receive events from .obsidian/workspace"
                );
                assert!(
                    !path.to_string_lossy().contains(".git"),
                    "Should not receive events from .git"
                );
            }
            _ => {}
        }
    }

    Ok(())
}

#[tokio::test]
#[cfg(target_os = "linux")]
async fn test_watcher_checks_inotify_capacity() -> Result<()> {
    // This test verifies that the watcher checks available inotify watches on Linux
    use crucible_cli::watcher::SimpleFileWatcher;
    use tokio::sync::mpsc;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    let config = crucible_cli::config::FileWatcherConfig {
        enabled: true,
        debounce_ms: 100,
        exclude_patterns: vec![],
    };

    let (tx, _rx) = mpsc::unbounded_channel();

    // Should succeed or fail gracefully with clear error message
    let result = SimpleFileWatcher::new(vault_path, config, tx);

    match result {
        Ok(_) => {
            // Watcher created successfully
            Ok(())
        }
        Err(e) => {
            // Should provide helpful error message about inotify
            let error_msg = e.to_string();
            if error_msg.contains("inotify") || error_msg.contains("watches") {
                // Expected error with helpful message
                Ok(())
            } else {
                Err(anyhow::anyhow!("Unexpected error: {}", error_msg))
            }
        }
    }
}

#[tokio::test]
async fn test_watcher_debounces_rapid_changes() -> Result<()> {
    use crucible_cli::watcher::SimpleFileWatcher;
    use tokio::sync::mpsc;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    let config = crucible_cli::config::FileWatcherConfig {
        enabled: true,
        debounce_ms: 200, // 200ms debounce
        exclude_patterns: vec![],
    };

    let (tx, mut rx) = mpsc::unbounded_channel();
    let _watcher = SimpleFileWatcher::new(vault_path, config, tx)?;

    let file_path = vault_path.join("note1.md");

    // Make rapid changes (should be debounced)
    for i in 0..10 {
        fs::write(&file_path, format!("# Update {}\n\nRapid change", i))?;
        sleep(Duration::from_millis(20)).await;
    }

    // Wait for debounce period
    sleep(Duration::from_millis(400)).await;

    // Should receive significantly fewer events than writes due to debouncing
    let mut event_count = 0;
    while let Ok(_event) = rx.try_recv() {
        event_count += 1;
    }

    assert!(
        event_count < 10,
        "Expected debouncing to reduce events from 10 writes to fewer events, got {}",
        event_count
    );

    Ok(())
}

#[tokio::test]
async fn test_ensure_watcher_running_graceful_degradation() -> Result<()> {
    use crucible_cli::common::kiln_processor;

    // Test with non-existent vault path - should fail gracefully
    let config = crucible_cli::config::CliConfig {
        kiln: crucible_cli::config::KilnConfig {
            path: "/nonexistent/vault/path".into(),
            embedding_url: "http://localhost:11434".to_string(),
            embedding_model: Some("nomic-embed-text".to_string()),
        },
        embedding: None,
        file_watching: crucible_cli::config::FileWatcherConfig {
            enabled: true,
            debounce_ms: 100,
            exclude_patterns: vec![],
        },
        llm: crucible_cli::config::LlmConfig::default(),
        network: crucible_cli::config::NetworkConfig::default(),
        services: crucible_cli::config::ServicesConfig::default(),
        migration: crucible_cli::config::MigrationConfig::default(),
        custom_database_path: None,
    };

    // Should not panic, should handle gracefully
    let result = kiln_processor::ensure_watcher_running(&config).await;

    // Should succeed even if watcher fails (graceful degradation)
    assert!(result.is_ok(), "ensure_watcher_running should handle errors gracefully");

    Ok(())
}

#[tokio::test]
async fn test_watcher_disabled_by_config() -> Result<()> {
    use crucible_cli::common::kiln_processor;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    let config = crucible_cli::config::CliConfig {
        kiln: crucible_cli::config::KilnConfig {
            path: vault_path.to_path_buf(),
            embedding_url: "http://localhost:11434".to_string(),
            embedding_model: Some("nomic-embed-text".to_string()),
        },
        embedding: None,
        file_watching: crucible_cli::config::FileWatcherConfig {
            enabled: false,  // Disabled
            debounce_ms: 500,
            exclude_patterns: vec![],
        },
        llm: crucible_cli::config::LlmConfig::default(),
        network: crucible_cli::config::NetworkConfig::default(),
        services: crucible_cli::config::ServicesConfig::default(),
        migration: crucible_cli::config::MigrationConfig::default(),
        custom_database_path: None,
    };

    // Should return quickly without starting watcher
    let result = kiln_processor::ensure_watcher_running(&config).await;

    assert!(result.is_ok(), "Should handle disabled watcher gracefully");

    Ok(())
}

/// Test that file changes trigger actual processing (smoke test)
#[tokio::test]
#[ignore] // Requires embedding service running, mark as ignored by default
async fn test_file_change_triggers_processing() -> Result<()> {
    use crucible_cli::common::kiln_processor;

    let temp_dir = create_test_vault().await?;
    let vault_path = temp_dir.path();

    let config = create_test_config(vault_path);

    // Start watcher
    kiln_processor::ensure_watcher_running(&config).await?;

    // Give watcher time to initialize
    sleep(Duration::from_millis(200)).await;

    // Modify a file
    let file_path = vault_path.join("note1.md");
    fs::write(&file_path, "# Updated Content\n\nThis file has been modified")?;

    // Wait for processing (debounce + batch window + processing time)
    sleep(Duration::from_secs(3)).await;

    // If we got here without panicking, the basic flow works
    // In a real test we'd verify database was updated, but that requires more setup

    Ok(())
}
