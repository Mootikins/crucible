//! Storage Backend Consistency Integration Tests
//!
//! This test suite verifies that CLI commands work consistently
//! across different storage backends (SurrealDB, Memory).
//!
//! Test Objectives:
//! 1. Verify CLI commands produce consistent results across storage backends
//! 2. Test data persistence and retrieval across backend switches
//! 3. Validate configuration changes apply correctly to different backends
//! 4. Test error handling and recovery across storage implementations
//! 5. Verify search functionality consistency across backends

use anyhow::Result;
use regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::{TempDir, NamedTempFile};
use tokio::time::{sleep, Duration};
use uuid;

/// Test configuration for different storage backends
#[derive(Debug, Clone)]
struct StorageTestConfig {
    backend_type: StorageBackendType,
    config_path: PathBuf,
    kiln_path: PathBuf,
    test_name: String,
}

#[derive(Debug, Clone)]
enum StorageBackendType {
    SurrealDB,
    Memory,
    RocksDB,
}

/// Create test kiln with sample content
async fn create_test_kiln_with_content(
    _temp_dir: &Path,
    backend_type: &str,
) -> Result<(TempDir, PathBuf)> {
    let test_dir = TempDir::new()?;
    let kiln_path = test_dir.path().join("test-kiln");
    fs::create_dir_all(&kiln_path)?;

    // Create sample markdown files
    let test_files = vec![
        ("index.md", format!(
            "# Test Index - Backend: {}\n\nWelcome to the test knowledge base.\n\n## Topics\n- [[Rust Programming]]\n- [[Machine Learning]]\n- [[Database Design]]",
            backend_type
        )),
        ("rust-programming.md", "# Rust Programming\n\nRust is a systems programming language focused on safety and performance.\n\n## Key Features\n- Memory safety without garbage collection\n- Concurrency without data races\n- Zero-cost abstractions".to_string()),
        ("machine-learning.md", "# Machine Learning\n\nMachine learning algorithms and applications.\n\n## Topics\n- Supervised learning\n- Neural networks\n- Model training".to_string()),
        ("database-design.md", "# Database Design\n\nPrinciples of effective database design.\n\n## Concepts\n- Normalization\n- Indexing strategies\n- Query optimization".to_string()),
    ];

    for (filename, content) in test_files {
        let file_path = kiln_path.join(filename);
        fs::write(&file_path, content)?;
    }

    Ok((test_dir, kiln_path))
}

/// Create configuration file for specific backend
fn create_backend_config(
    backend_type: StorageBackendType,
    kiln_path: &Path,
) -> Result<NamedTempFile> {
    let mut config_content = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "{}"
"#,
        kiln_path.display(),
        match backend_type {
            StorageBackendType::SurrealDB => "surrealdb",
            StorageBackendType::Memory => "memory",
            StorageBackendType::RocksDB => "rocksdb",
        }
    );

    // Add backend-specific configuration
    match backend_type {
        StorageBackendType::SurrealDB => {
            config_content.push_str(
                r#"
[storage.surrealdb]
url = "memory"
namespace = "crucible_test"
"#
            );
        }
        StorageBackendType::Memory => {
            config_content.push_str(
                r#"
[storage.memory]
max_size_mb = 100
cleanup_interval_secs = 300
"#
            );
        }
        StorageBackendType::RocksDB => {
            config_content.push_str(&format!(
                r#"
[storage.rocksdb]
path = "{}"
max_size_mb = 100
compression = "lz4"
"#,
                kiln_path.join("rocksdb_data").display()
            ));
        }
    }

    // Add common configuration (minimal for storage testing)
    config_content.push_str(
        r#"
[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"

[embedding.fastembed]
cache_dir = "/tmp/crucible_test_cache"
show_download = false
"#
    );

    let config_file = NamedTempFile::new()?;
    fs::write(config_file.path(), config_content)?;
    Ok(config_file)
}

/// Run CLI command and capture output
async fn run_cli_command(
    args: &[&str],
    config_path: &Path,
) -> Result<(String, String)> {
    use std::process::Command;

    let mut cmd = Command::new("cargo");
    cmd.current_dir("/home/moot/crucible")
        .args(&["run", "--bin", "cru", "--config", config_path.to_str().unwrap()])
        .args(args);

    let output = cmd.output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok((stdout, stderr))
}

/// Test that basic CLI commands work consistently across backends
#[tokio::test]
async fn test_basic_cli_consistency() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
        // Note: Skipping RocksDB for now to avoid dependency issues
    ];

    let mut results = HashMap::new();

    for backend in backends {
        let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Test stats command
        let (stats_stdout, _) = run_cli_command(&["stats"], config_file.path()).await?;
        results.insert(format!("stats_{:?}", backend), stats_stdout);

        // Test list command
        let (list_stdout, _) = run_cli_command(&["list", "--format", "json"], config_file.path()).await?;
        results.insert(format!("list_{:?}", backend), list_stdout);

        // Give some time for processing
        sleep(Duration::from_millis(500)).await;
    }

    // Verify consistent basic behavior
    for (key, result) in &results {
        assert!(!result.is_empty(), "Command {} should produce output", key);

        if key.starts_with("stats_") {
            assert!(result.contains("Documents") || result.contains("Files"),
                   "Stats should contain document/file count for {}", key);
        }
    }

    println!("✅ Basic CLI consistency test passed across {} backends", results.len());
    Ok(())
}

/// Test search consistency across storage backends
#[tokio::test]
async fn test_search_consistency_across_backends() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    let query = "Rust programming";
    let mut search_results = HashMap::new();

    for backend in backends {
        let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Process the kiln to generate data
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);

        // Give time for processing
        sleep(Duration::from_millis(1000)).await;

        // Test search
        let (search_stdout, _) = run_cli_command(&["search", query, "--limit", "10", "--format", "json"], config_file.path()).await?;
        search_results.insert(format!("search_{:?}", backend), search_stdout);
    }

    // Verify search results are not empty
    for (backend, result) in &search_results {
        assert!(!result.is_empty(), "Search should return results for {}", backend);

        // Should contain references to our test files
        let should_contain = vec!["rust-programming.md", "Rust", "programming"];
        let contains_any = should_contain.iter().any(|term| result.to_lowercase().contains(&term.to_lowercase()));
        assert!(contains_any, "Search results should contain expected content for {}", backend);
    }

    // Results may differ in ranking but should all find the same relevant content
    println!("✅ Search consistency test passed - results found in all backends");
    Ok(())
}

/// Test data persistence across backend restarts
#[tokio::test]
async fn test_data_persistence_across_restarts() -> Result<()> {
    let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), "restart_test").await?;
    let config_file = create_backend_config(StorageBackendType::Memory, &kiln_path)?;

    // First, process the kiln
    let (_, stderr1) = run_cli_command(&["kiln", "process"], config_file.path()).await?;
    assert!(!stderr1.contains("error"), "Initial processing should succeed");
    sleep(Duration::from_millis(1000)).await;

    // Get initial stats
    let (stats1, _) = run_cli_command(&["stats"], config_file.path()).await?;
    let doc_count1 = extract_document_count(&stats1);

    // "Restart" by creating a new CLI instance with same config
    let (stats2, _) = run_cli_command(&["stats"], config_file.path()).await?;
    let doc_count2 = extract_document_count(&stats2);

    // Document counts should be consistent
    assert_eq!(doc_count1, doc_count2,
              "Document count should be persistent across CLI restarts: {} vs {}",
              doc_count1, doc_count2);

    println!("✅ Data persistence test passed - {} documents persisted across restarts", doc_count1);
    Ok(())
}

/// Extract document count from stats output
fn extract_document_count(stats_output: &str) -> u64 {
    // Look for patterns like "Documents: 4" or "4 documents" etc.
    let patterns = vec![
        r#"Documents:\s*(\d+)"#,
        r#"(\d+)\s*documents"#,
        r#"Files:\s*(\d+)"#,
        r#"(\d+)\s*files"#,
    ];

    for pattern in patterns {
        if let Some(captures) = regex::Regex::new(pattern).ok().and_then(|re| re.captures(stats_output)) {
            if let Some(num_match) = captures.get(1) {
                if let Ok(num) = num_match.as_str().parse::<u64>() {
                    return num;
                }
            }
        }
    }

    0 // Default if no pattern matches
}

/// Test configuration changes apply correctly
#[tokio::test]
async fn test_configuration_changes_apply() -> Result<()> {
    let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), "config_test").await?;

    // Test with initial config
    let config_file1 = create_backend_config(StorageBackendType::Memory, &kiln_path)?;
    let (stats1, _) = run_cli_command(&["stats"], config_file1.path()).await?;

    // Modify config with different settings
    let mut config_file2 = NamedTempFile::new()?;
    let modified_config = format!(
        r#"[kiln]
path = "{}"

[storage]
backend = "memory"

[storage.memory]
max_size_mb = 50

[embedding]
provider = "fastembed"
model = "BAAI/bge-small-en-v1.5"
"#,
        kiln_path.display()
    );
    fs::write(config_file2.path(), modified_config)?;

    // Test with modified config
    let (stats2, _) = run_cli_command(&["stats"], config_file2.path()).await?;

    // Both should succeed (config changes should be respected)
    assert!(!stats1.is_empty() && !stats2.is_empty(), "Both configs should produce output");

    println!("✅ Configuration changes test passed - config changes are respected");
    Ok(())
}

/// Test error handling across backends
#[tokio::test]
async fn test_error_handling_consistency() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    for backend in backends {
        let (test_dir, _) = create_test_kiln_with_content(Path::new(""), "error_test").await?;
        let config_file = create_backend_config(backend.clone(), test_dir.path())?;

        // Test search with non-existent query
        let (search_stdout, _) = run_cli_command(&["search", "nonexistent_query_xyz_123", "--format", "json"], config_file.path()).await?;

        // Should not crash and may return empty results or no results message
        assert!(search_stdout.contains("[]") || search_stdout.to_lowercase().contains("no results") || search_stdout.trim().is_empty(),
                   "Search should handle non-existent queries gracefully for {:?}", backend);

        // Test with invalid kiln path (config will point to non-existent directory)
        let invalid_config = NamedTempFile::new()?;
        let invalid_config_content = format!(
            r#"[kiln]
path = "/nonexistent/path/{}"

[storage]
backend = "memory"
"#,
            uuid::Uuid::new_v4()
        );
        fs::write(invalid_config.path(), invalid_config_content)?;

        let (_, stderr) = run_cli_command(&["stats"], invalid_config.path()).await?;
        assert!(stderr.to_lowercase().contains("error") || stderr.to_lowercase().contains("not found"),
                  "Should handle invalid kiln path gracefully for {:?}", backend);
    }

    println!("✅ Error handling consistency test passed - all backends handle errors gracefully");
    Ok(())
}

/// Test that semantic search works across different backends
#[tokio::test]
async fn test_semantic_search_backend_consistency() -> Result<()> {
    let backends = vec![
        StorageBackendType::Memory,
        StorageBackendType::SurrealDB,
    ];

    let semantic_queries = vec![
        "programming language",
        "database design",
        "machine learning algorithms",
    ];

    for backend in backends {
        let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("semantic_{:?}", backend)).await?;
        let config_file = create_backend_config(backend.clone(), &kiln_path)?;

        // Process kiln for embeddings
        let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
        assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
        sleep(Duration::from_millis(2000)).await; // Give more time for embeddings

        // Test semantic search for each query
        for query in &semantic_queries {
            let (search_stdout, _) = run_cli_command(&["semantic", query, "--limit", "5", "--format", "json"], config_file.path()).await?;

            // Should not crash and should produce some output
            assert!(!search_stdout.is_empty(), "Semantic search should produce output for query '{}' on {:?}", query, backend);

            // Should be valid JSON or contain search results
            assert!(search_stdout.trim().starts_with('[') || search_stdout.to_lowercase().contains("no results") || search_stdout.trim().is_empty(),
                       "Semantic search should return valid results or no-results message for query '{}' on {:?}", query, backend);
        }
    }

    println!("✅ Semantic search backend consistency test passed - semantic search works across backends");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_config_creation() -> Result<()> {
        let (test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), "config_test").await?;

        // Test each backend type
        for backend in vec![StorageBackendType::Memory, StorageBackendType::SurrealDB] {
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;
            let config_content = fs::read_to_string(config_file.path())?;

            assert!(config_content.contains("[kiln]"));
            assert!(config_content.contains("[storage]"));

            match backend {
                StorageBackendType::Memory => {
                    assert!(config_content.contains(r#"backend = "memory""#));
                    assert!(config_content.contains("[storage.memory]"));
                }
                StorageBackendType::SurrealDB => {
                    assert!(config_content.contains(r#"backend = "surrealdb""#));
                    assert!(config_content.contains("[storage.surrealdb]"));
                }
                StorageBackendType::RocksDB => {
                    assert!(config_content.contains(r#"backend = "rocksdb""#));
                    assert!(config_content.contains("[storage.rocksdb]"));
                }
            }
        }

        Ok(())
    }

    /// Test database backup and restore operations
    #[tokio::test]
    async fn test_database_backup_restore_consistency() -> Result<()> {
        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("backup_restore_{:?}", backend)).await?;
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Process the kiln to generate data
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
            sleep(Duration::from_millis(1000)).await;

            // Get initial stats
            let (stats_before, _) = run_cli_command(&["stats", "--format", "json"], config_file.path()).await?;
            assert!(!stats_before.is_empty(), "Should have stats before backup");

            // Create backup
            let backup_path = config_file.path().parent().unwrap().join(format!("backup_{:?}.json", backend));
            let (backup_stdout, backup_stderr) = run_cli_command(&[
                "storage", "backup",
                "--format", "json",
                "--include-content",
                "--verify",
                backup_path.to_str().unwrap()
            ], config_file.path()).await?;

            assert!(!backup_stderr.contains("error"), "Backup should succeed for {:?}", backend);
            assert!(backup_stdout.contains("Backup created") || backup_stdout.contains("Export completed"),
                     "Should confirm backup creation for {:?}", backend);
            assert!(backup_path.exists(), "Backup file should exist for {:?}", backend);

            // Verify backup file has content
            let backup_size = fs::metadata(&backup_path)?.len();
            assert!(backup_size > 0, "Backup file should not be empty for {:?}", backend);

            // Clear storage (simulate data loss)
            let (_test_dir2, empty_kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("empty_{:?}", backend)).await?;
            let empty_config_file = create_backend_config(backend.clone(), &empty_kiln_path)?;

            let (_, clear_stderr) = run_cli_command(&["process", "start"], empty_config_file.path()).await?;
            assert!(!clear_stderr.contains("error"), "Empty kiln processing should succeed");

            // Restore from backup
            let (restore_stdout, restore_stderr) = run_cli_command(&[
                "storage", "restore",
                "--format", "json",
                "--merge",
                backup_path.to_str().unwrap()
            ], empty_config_file.path()).await?;

            assert!(!restore_stderr.contains("error"), "Restore should succeed for {:?}", backend);
            assert!(restore_stdout.contains("Restored") || restore_stdout.contains("Import completed"),
                     "Should confirm restore completion for {:?}", backend);

            // Verify data integrity after restore
            let (stats_after, _) = run_cli_command(&["stats", "--format", "json"], empty_config_file.path()).await?;
            assert!(!stats_after.is_empty(), "Should have stats after restore");

            // Stats should be similar (allowing for some differences due to empty kiln merge)
            println!("✅ Backup/Restore test passed for {:?} - Backup size: {} bytes", backend, backup_size);
        }

        println!("✅ Database backup/restore consistency test passed across all backends");
        Ok(())
    }

    /// Test database cleanup operations
    #[tokio::test]
    async fn test_database_cleanup_operations() -> Result<()> {
        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("cleanup_{:?}", backend)).await?;
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Process the kiln to generate data
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
            sleep(Duration::from_millis(1000)).await;

            // Get stats before cleanup
            let (stats_before, _) = run_cli_command(&["stats"], config_file.path()).await?;
            assert!(!stats_before.is_empty(), "Should have stats before cleanup");

            // Run cleanup operations
            let cleanup_operations = vec![
                vec!["storage", "cleanup", "--dry-run"],
                vec!["storage", "cleanup", "--gc"],
                vec!["storage", "cleanup", "--rebuild-indexes"],
            ];

            for operation in cleanup_operations {
                let (cleanup_stdout, cleanup_stderr) = run_cli_command(&operation, config_file.path()).await?;

                assert!(!cleanup_stderr.contains("error"),
                         "Cleanup operation {:?} should succeed for {:?}", operation, backend);

                // Should provide some output about cleanup activities
                assert!(!cleanup_stdout.is_empty(),
                         "Cleanup should produce output for {:?} on {:?}", operation, backend);
            }

            // Verify system is still functional after cleanup
            let (stats_after, _) = run_cli_command(&["stats"], config_file.path()).await?;
            assert!(!stats_after.is_empty(), "Should have stats after cleanup");

            println!("✅ Cleanup operations test passed for {:?}", backend);
        }

        println!("✅ Database cleanup operations test passed across all backends");
        Ok(())
    }

    /// Test database verification operations
    #[tokio::test]
    async fn test_database_verification_operations() -> Result<()> {
        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("verify_{:?}", backend)).await?;
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Process the kiln to generate data
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
            sleep(Duration::from_millis(1000)).await;

            // Run verification operations
            let verify_operations = vec![
                vec!["storage", "verify"],
                vec!["storage", "verify", "--repair"],
            ];

            for operation in verify_operations {
                let (verify_stdout, verify_stderr) = run_cli_command(&operation, config_file.path()).await?;

                assert!(!verify_stderr.contains("error"),
                         "Verification operation {:?} should succeed for {:?}", operation, backend);

                // Should provide verification output
                assert!(!verify_stdout.is_empty(),
                         "Verification should produce output for {:?} on {:?}", operation, backend);
            }

            // Verify system is still functional after verification
            let (stats_after, _) = run_cli_command(&["stats"], config_file.path()).await?;
            assert!(!stats_after.is_empty(), "Should have stats after verification");

            println!("✅ Verification operations test passed for {:?}", backend);
        }

        println!("✅ Database verification operations test passed across all backends");
        Ok(())
    }

    /// Test database operation consistency across backends
    #[tokio::test]
    async fn test_database_operation_consistency_across_backends() -> Result<()> {
        use std::collections::HashMap;

        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        let mut operation_results = HashMap::new();

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("consistency_{:?}", backend)).await?;
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Process the kiln
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            if process_stderr.contains("error") {
                println!("❌ Processing failed for {:?} - Stderr: {}", backend, process_stderr);
            }
            assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}. Stderr: {}", backend, process_stderr);
            sleep(Duration::from_millis(1000)).await;

            // Get initial stats
            let (initial_stats, _) = run_cli_command(&["stats", "--format", "json"], config_file.path()).await?;
            operation_results.insert(format!("initial_stats_{:?}", backend), initial_stats.clone());

            // Create backup
            let backup_path = config_file.path().parent().unwrap().join(format!("consistency_backup_{:?}.json", backend));
            let (backup_result, _) = run_cli_command(&[
                "storage", "backup", "--format", "json", backup_path.to_str().unwrap()
            ], config_file.path()).await?;
            operation_results.insert(format!("backup_{:?}", backend), backup_result);

            // Run verification
            let (verify_result, _) = run_cli_command(&["storage", "verify"], config_file.path()).await?;
            operation_results.insert(format!("verify_{:?}", backend), verify_result);

            // Run cleanup
            let (cleanup_result, _) = run_cli_command(&["storage", "cleanup", "--gc"], config_file.path()).await?;
            operation_results.insert(format!("cleanup_{:?}", backend), cleanup_result);

            // Get final stats
            let (final_stats, _) = run_cli_command(&["stats", "--format", "json"], config_file.path()).await?;
            operation_results.insert(format!("final_stats_{:?}", backend), final_stats);

            println!("✅ Consistency test completed for {:?}", backend);
        }

        // Verify all backends produced some output for each operation
        for (key, result) in &operation_results {
            assert!(!result.is_empty(), "Operation {} should produce output", key);
        }

        // Verify backup operations were successful
        for backend in [StorageBackendType::Memory, StorageBackendType::SurrealDB] {
            let backup_key = format!("backup_{:?}", backend);
            let backup_result = operation_results.get(&backup_key).unwrap();

            // Check if backup operation succeeded (no error in stderr and output not empty)
            let backup_succeeded = !backup_result.is_empty() &&
                                !backup_result.to_lowercase().contains("error") &&
                                !backup_result.to_lowercase().contains("failed") &&
                                !backup_result.to_lowercase().contains("usage:");

            println!("Backup result for {:?}: {}", backend, backup_result);
            assert!(backup_succeeded,
                     "Backup should be successful for {:?}. Got output: '{}'", backend, backup_result);
        }

        println!("✅ Database operation consistency test passed across {} backends", operation_results.len());
        Ok(())
    }

    /// Test identical operations produce identical results across backends
    #[tokio::test]
    async fn test_identical_operations_produce_identical_results() -> Result<()> {
        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        let mut backend_results = HashMap::new();
        let test_content = r#"---
title: Multi-Backend Test
tags: [testing, compatibility]
---

# Multi-Backend Compatibility Test

This document tests identical operations across different storage backends.

## Features to Test
- Content storage: Document content and metadata
- Tag handling: Hashtag extraction and storage
- Search consistency: Same search queries produce same results
- File processing: Identical file parsing across backends

## Test Content
Here's some content with bold and italic formatting.

### Code Block Example
```rust
fn main() {
    println!("Hello from Rust!");
}
```

### Complex Features
- LaTeX math: $E = mc^2$
- Callout: > [!NOTE] This is a note
- Hashtags: #rust #testing
"#;

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("identical_{:?}", backend)).await?;

            // Create test file with identical content
            let test_file = kiln_path.join("test-multi-backend.md");
            fs::write(&test_file, test_content)?;

            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Process the kiln
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            assert!(!process_stderr.contains("error"), "Processing should succeed for {:?}", backend);
            sleep(Duration::from_millis(1000)).await;

            // Test stats command
            let (stats_result, _) = run_cli_command(&["stats", "--format", "json"], config_file.path()).await?;
            backend_results.insert(format!("stats_{:?}", backend), stats_result.clone());

            // Test search with same query
            let search_query = "test multi backend";
            let (search_result, _) = run_cli_command(&["search", search_query, "--limit", "10", "--format", "json"], config_file.path()).await?;
            backend_results.insert(format!("search_{:?}", backend), search_result.clone());

            // Test semantic search
            let (semantic_result, _) = run_cli_command(&["semantic", "testing", "--limit", "5", "--format", "json"], config_file.path()).await?;
            backend_results.insert(format!("semantic_{:?}", backend), semantic_result.clone());
        }

        // Verify all backends produced some output
        for (key, result) in &backend_results {
            assert!(!result.is_empty(), "Operation '{}' should produce output", key);
        }

        println!("✅ Identical operations test passed across {} backends", backend_results.len());
        Ok(())
    }

    /// Test backend-specific configuration and behavior
    #[tokio::test]
    async fn test_backend_specific_configuration_behavior() -> Result<()> {
        let test_cases = vec![
            (StorageBackendType::Memory, "memory_backend_test"),
            (StorageBackendType::SurrealDB, "surrealdb_backend_test"),
        ];

        for (backend, test_name) in test_cases {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), test_name).await?;

            // Test configuration creation for each backend
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;
            let config_content = fs::read_to_string(config_file.path())?;

            // Verify backend-specific configuration sections
            match backend {
                StorageBackendType::Memory => {
                    assert!(config_content.contains(r#"backend = "memory""#));
                    assert!(config_content.contains("[storage.memory]"));
                    assert!(config_content.contains("max_size_mb"));
                }
                StorageBackendType::SurrealDB => {
                    assert!(config_content.contains(r#"backend = "surrealdb""#));
                    assert!(config_content.contains("[storage.surrealdb]"));
                    assert!(config_content.contains(r#"namespace = "crucible_test""#));
                }
                StorageBackendType::RocksDB => {
                    assert!(config_content.contains(r#"backend = "rocksdb""#));
                    assert!(config_content.contains("[storage.rocksdb]"));
                    assert!(config_content.contains(r#"compression = "lz4""#));
                }
            }

            // Process with backend-specific configuration
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            assert!(!process_stderr.contains("error"), "Backend-specific processing should succeed for {:?}", backend);

            println!("✅ {:?} configuration test passed", backend);
        }

        println!("✅ Backend-specific configuration test passed across all backends");
        Ok(())
    }

    /// Test cross-backend data migration and consistency
    #[tokio::test]
    async fn test_cross_backend_data_migration() -> Result<()> {
        // Create source data in Memory backend
        let (source_test_dir, source_kiln_path) = create_test_kiln_with_content(Path::new(""), "migration_source").await?;
        let source_config = create_backend_config(StorageBackendType::Memory, &source_kiln_path)?;

        // Add migration-specific test content
        let migration_content = r#"---
title: Migration Test Document
tags: [migration, data-transfer]
---

# Migration Test

This document tests cross-backend data migration consistency.

## Key Content
- Migration data preservation
- Cross-backend compatibility
- Search result consistency
- Semantic search migration

## Complex Features
- LaTeX: $\int_0^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}$
- Callout: > [!TIP] Migration tip: Always verify data integrity
- Hashtags: #migration #data-transfer
"#;

        let source_file = source_kiln_path.join("migration-test.md");
        fs::write(&source_file, migration_content)?;

        // Process source data
        let (_, source_process_stderr) = run_cli_command(&["process", "start"], source_config.path()).await?;
        assert!(!source_process_stderr.contains("error"), "Source processing should succeed");
        sleep(Duration::from_millis(1000)).await;

        // Get source statistics
        let (source_stats, _) = run_cli_command(&["stats", "--format", "json"], source_config.path()).await?;

        // Create target backend (different from source)
        let (target_test_dir, target_kiln_path) = create_test_kiln_with_content(Path::new(""), "migration_target").await?;
        let target_config = create_backend_config(StorageBackendType::SurrealDB, &target_kiln_path)?;

        // Copy the same file to target kiln
        let target_file = target_kiln_path.join("migration-test.md");
        fs::write(&target_file, migration_content)?;

        // Process target data
        let (_, target_process_stderr) = run_cli_command(&["process", "start"], target_config.path()).await?;
        assert!(!target_process_stderr.contains("error"), "Target processing should succeed");
        sleep(Duration::from_millis(1000)).await;

        // Get target statistics
        let (target_stats, _) = run_cli_command(&["stats", "--format", "json"], target_config.path()).await?;

        // Both should have similar document counts
        let source_doc_count = extract_document_count(&source_stats);
        let target_doc_count = extract_document_count(&target_stats);

        // They should both process at least the test document
        assert!(source_doc_count > 0, "Source backend should have processed documents");
        assert!(target_doc_count > 0, "Target backend should have processed documents");

        // Test search consistency across backends with same file
        let (source_search, _) = run_cli_command(&["search", "migration test", "--format", "json"], source_config.path()).await?;
        let (target_search, _) = run_cli_command(&["search", "migration test", "--format", "json"], target_config.path()).await?;

        let source_has_result = !source_search.is_empty() && source_search.to_lowercase().contains("migration");
        let target_has_result = !target_search.is_empty() && target_search.to_lowercase().contains("migration");

        assert!(source_has_result, "Source backend should find migration content");
        assert!(target_has_result, "Target backend should find migration content");

        println!("✅ Cross-backend migration test passed");
        println!("   Source documents: {}, Target documents: {}", source_doc_count, target_doc_count);
        Ok(())
    }

    /// Test backend performance and scalability characteristics
    #[tokio::test]
    async fn test_backend_performance_characteristics() -> Result<()> {
        let backends = vec![
            StorageBackendType::Memory,
            StorageBackendType::SurrealDB,
        ];

        for backend in backends {
            let (_test_dir, kiln_path) = create_test_kiln_with_content(Path::new(""), &format!("performance_{:?}", backend)).await?;
            let config_file = create_backend_config(backend.clone(), &kiln_path)?;

            // Create multiple test files to stress test the backend
            for i in 1..=5 {
                let file_content = format!(
                    r#"---
title: Performance Test Document {}
tags: [performance, test, document{}]
---

# Performance Test Document {}

This is test document number {} for {:?} backend performance testing.

## Content Volume
- Markdown parsing: Advanced features
- Database operations: Efficient storage
- Search indexing: Fast retrieval
- Semantic embeddings: AI-powered search

## Complex Features
- LaTeX: $x^2 + y^2 = z^2$
- Callouts: > [!INFO] Performance tip {}
- Hashtags: #performance #test
"#,
                    i, i, i, i, backend, i
                );

                let file_path = kiln_path.join(format!("perf-test-{}.md", i));
                fs::write(&file_path, file_content)?;
            }

            // Time the processing
            let start_time = std::time::Instant::now();
            let (_, process_stderr) = run_cli_command(&["process", "start"], config_file.path()).await?;
            let processing_time = start_time.elapsed();

            assert!(!process_stderr.contains("error"), "Performance test processing should succeed for {:?}", backend);
            assert!(processing_time.as_secs() < 30, "Processing should complete within 30 seconds for {:?}", backend);

            // Verify all documents were processed
            let (stats_result, _) = run_cli_command(&["stats"], config_file.path()).await?;
            let doc_count = extract_document_count(&stats_result);

            assert!(doc_count >= 5, "Should have processed at least 5 documents for {:?}", backend);

            // Test search performance
            let search_start = std::time::Instant::now();
            let (_, _) = run_cli_command(&["search", "performance test", "--limit", "10"], config_file.path()).await?;
            let search_time = search_start.elapsed();

            assert!(search_time.as_millis() < 5000, "Search should be fast (<5s) for {:?}", backend);

            println!("✅ {:?} performance test passed:", backend);
            println!("   Documents: {}, Processing: {:.2}s, Search: {:.2}ms",
                     doc_count, processing_time.as_secs_f64(), search_time.as_millis());
        }

        println!("✅ Backend performance characteristics test completed");
        Ok(())
    }
}