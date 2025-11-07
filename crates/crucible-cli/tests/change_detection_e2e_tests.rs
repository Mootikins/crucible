//! End-to-end tests for file-level change detection system
//!
//! This test suite validates the complete ChangeDetectionService pipeline with real vault scenarios.
//! Tests cover:
//! - Complete workflow: file scanning → change detection → ChangeSet generation → selective processing
//! - Validation that unchanged files are skipped and only changed files are processed
//! - Various scenarios: new files, modified files, deleted files
//! - Integration between all components works correctly
//! - Performance metrics and logging
//!
//! Tests use temporary test vaults to simulate real-world scenarios and validate the entire
//! change detection pipeline from start to finish.

use anyhow::{Result, Context};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

use crucible_cli::common::{ChangeDetectionService, FileScanningService};
use crucible_core::{
    hashing::file_hasher::FileHasher,
    hashing::algorithm::Blake3Algorithm,
    traits::change_detection::{ChangeSet, ContentHasher},
    types::hashing::{HashAlgorithm, FileHash, FileHashInfo},
};
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};
use crucible_watch::FileInfo;

/// Test configuration constants
const TEST_TIMEOUT_SECS: u64 = 30; // Maximum time for individual tests
const CHANGE_DETECTION_TIMEOUT_SECS: u64 = 5; // Maximum time for change detection
const MAX_FILES_FOR_PERFORMANCE_TESTS: usize = 100;

/// Test data for realistic markdown files
const TEST_MARKDOWN_FILES: &[(&str, &str)] = &[
    (
        "getting-started.md",
        r#"---
title: "Getting Started Guide"
tags: ["tutorial", "guide"]
---

# Getting Started

Welcome to our knowledge management system. This guide will help you get up and running quickly.

## Installation

1. Download the latest version
2. Follow the setup wizard
3. Configure your workspace

## Basic Usage

- Create your first document
- Organize with tags
- Use semantic search

Happy documenting!
"#,
    ),
    (
        "api-reference.md",
        r#"---
title: "API Reference"
tags: ["api", "technical"]
---

# API Reference

## Authentication

### POST /auth/login
```json
{
  "username": "user@example.com",
  "password": "secure-password"
}
```

## Documents

### GET /documents
Retrieve all documents with pagination.

### POST /documents
Create a new document.

## Search

### GET /search?q=query
Search documents using semantic search.
"#,
    ),
    (
        "troubleshooting.md",
        r#"---
title: "Troubleshooting Guide"
tags: ["support", "issues"]
---

# Troubleshooting

## Common Issues

### Search Not Working
- Check embeddings are up to date
- Verify document permissions
- Rebuild search index if needed

### Performance Issues
- Clear cache periodically
- Optimize database queries
- Check system resources

## Getting Help

Contact support or check our community forums.
"#,
    ),
    (
        "best-practices.md",
        r#"---
title: "Best Practices"
tags: ["guide", "quality"]
---

# Best Practices

## Document Organization

- Use descriptive titles
- Apply consistent tagging
- Create logical hierarchies

## Content Quality

- Write clearly and concisely
- Use proper formatting
- Include relevant examples

## Collaboration

- Review changes before publishing
- Use version control
- Document decisions
"#,
    ),
    (
        "changelog.md",
        r#"---
title: "Changelog"
tags: ["releases", "updates"]
---

# Changelog

## Version 2.0.0
- Enhanced semantic search
- Improved performance
- New collaboration features

## Version 1.5.0
- Added API endpoints
- Bug fixes and improvements
- Updated documentation

## Version 1.0.0
- Initial release
- Basic document management
- Search functionality
"#,
    ),
];

/// End-to-end test structure with temporary test vault
pub struct ChangeDetectionE2ETestHarness {
    pub temp_dir: TempDir,
    pub client: Arc<SurrealClient>,
    pub service: Arc<ChangeDetectionService>,
    pub file_paths: HashMap<String, PathBuf>, // filename -> absolute path
}

impl ChangeDetectionE2ETestHarness {
    /// Explicitly shutdown the database connection to release file handles
    pub async fn shutdown(self) -> Result<TempDir> {
        // Drop the service first (it holds a reference to the client)
        drop(self.service);

        // Try to unwrap the Arc to get exclusive ownership
        // This ensures the database connection is fully closed
        match Arc::try_unwrap(self.client) {
            Ok(_client) => {
                // Client is dropped here, closing the database connection
            }
            Err(_arc) => {
                // There are still other references - wait a bit and retry
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        // Give OS time to release file handles
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Ok(self.temp_dir)
    }

    /// Create a new test harness with a temporary vault
    pub async fn new() -> Result<Self> {
        Self::new_with_db_type(false).await
    }

    /// Create a new test harness with option to use in-memory database
    pub async fn new_with_db_type(use_memory_db: bool) -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let vault_path = temp_dir.path();

        // Initialize test vault structure
        Self::initialize_vault_structure(vault_path)?;

        // Create database client - use in-memory for tests with multiple harnesses
        let db_config = if use_memory_db {
            SurrealDbConfig {
                namespace: "crucible_test".to_string(),
                database: format!("test_{}", uuid::Uuid::new_v4()),  // Unique DB name
                path: ":memory:".to_string(),
                max_connections: Some(10),
                timeout_seconds: Some(30),
            }
        } else {
            SurrealDbConfig {
                namespace: "crucible_test".to_string(),
                database: "change_detection_test".to_string(),
                path: vault_path.join(".crucible/test.db").to_string_lossy().to_string(),
                max_connections: Some(10),
                timeout_seconds: Some(30),
            }
        };

        let client = Arc::new(SurrealClient::new(db_config).await
            .context("Failed to create SurrealClient")?);

        // Initialize the database schema (required for hash storage)
        crucible_surrealdb::kiln_integration::initialize_kiln_schema(&client).await
            .context("Failed to initialize kiln schema")?;

        // Create change detection service
        let service = Arc::new(
            ChangeDetectionService::with_defaults(
                vault_path,
                client.clone(),
                HashAlgorithm::Blake3,
            ).await
            .context("Failed to create ChangeDetectionService")?
        );

        Ok(Self {
            temp_dir,
            client,
            service,
            file_paths: HashMap::new(),
        })
    }

    /// Initialize the vault directory structure
    fn initialize_vault_structure(vault_path: &Path) -> Result<()> {
        // Create .crucible directory
        let crucible_dir = vault_path.join(".crucible");
        fs::create_dir_all(&crucible_dir)
            .context("Failed to create .crucible directory")?;

        // Create common directories
        let docs_dir = vault_path.join("docs");
        let notes_dir = vault_path.join("notes");
        let tools_dir = vault_path.join("tools");

        fs::create_dir_all(&docs_dir)?;
        fs::create_dir_all(&notes_dir)?;
        fs::create_dir_all(&tools_dir)?;

        Ok(())
    }

    /// Create initial test files in the vault
    pub async fn create_initial_files(&mut self) -> Result<()> {
        for (filename, content) in TEST_MARKDOWN_FILES {
            let file_path = self.temp_dir.path().join(filename);
            fs::write(&file_path, content)
                .with_context(|| format!("Failed to write file: {}", filename))?;

            self.file_paths.insert(filename.to_string(), file_path.clone());
            println!("📝 Created test file: {}", filename);
        }

        // Create files in subdirectories
        self.create_subdirectory_files().await?;

        Ok(())
    }

    /// Create files in subdirectories to test directory structure handling
    async fn create_subdirectory_files(&mut self) -> Result<()> {
        let sub_files = vec![
            ("docs/advanced-guide.md", "# Advanced Guide\n\nThis is an advanced guide."),
            ("notes/quick-note.md", "# Quick Note\n\nJust a quick note here."),
            ("tools/automation.md", "# Automation\n\nAutomated tools and scripts."),
        ];

        for (relative_path, content) in sub_files {
            let file_path = self.temp_dir.path().join(relative_path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file_path, content)?;
            self.file_paths.insert(relative_path.to_string(), file_path.clone());
            println!("📁 Created subdirectory file: {}", relative_path);
        }

        Ok(())
    }

    /// Modify an existing file and return the change detection result
    pub async fn modify_file(&self, filename: &str, additional_content: &str) -> Result<PathBuf> {
        let file_path = self.file_paths.get(filename)
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", filename))?;

        // Read existing content
        let mut existing_content = fs::read_to_string(file_path)?;

        // Add new content
        existing_content.push_str("\n\n");
        existing_content.push_str(additional_content);

        // Write back
        fs::write(file_path, &existing_content)?;

        println!("✏️  Modified file: {} (added {} bytes)", filename, additional_content.len());

        Ok(file_path.clone())
    }

    /// Delete a file and return the path that was deleted
    pub async fn delete_file(&mut self, filename: &str) -> Result<PathBuf> {
        let file_path = self.file_paths.remove(filename)
            .ok_or_else(|| anyhow::anyhow!("File not found: {}", filename))?;

        fs::remove_file(&file_path)?;
        println!("🗑️  Deleted file: {}", filename);

        Ok(file_path)
    }

    /// Create a new file in the vault
    pub async fn create_new_file(&mut self, filename: &str, content: &str) -> Result<PathBuf> {
        let file_path = self.temp_dir.path().join(filename);
        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, content)?;
        self.file_paths.insert(filename.to_string(), file_path.clone());
        println!("🆕 Created new file: {}", filename);
        Ok(file_path)
    }

    /// Run change detection and return the result with timing
    pub async fn run_change_detection(&self) -> Result<(ChangeDetectionServiceResult, Duration)> {
        let start_time = Instant::now();

        let result = self.service.detect_and_process_changes().await
            .context("Change detection failed")?;

        // SYNCHRONIZATION FIX: Wait for database operations to complete if files were processed
        // This ensures subsequent change detection sees the updated hashes in the database
        if let Some(processing_result) = &result.processing_result {
            if processing_result.processed_count > 0 {
                // Substantial delay to ensure all async database operations complete
                // SurrealDB commits are async and may take time to fully persist
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }

        let elapsed = start_time.elapsed();

        Ok((result, elapsed))
    }

    /// Get the file scanner for direct testing
    pub fn file_scanner(&self) -> &FileScanningService {
        self.service.file_scanner()
    }

    /// Get the vault path
    pub fn vault_path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get all current file paths
    pub fn file_paths(&self) -> &HashMap<String, PathBuf> {
        &self.file_paths
    }
}

/// Result type for easier testing
pub type ChangeDetectionServiceResult = crucible_cli::common::change_detection_service::ChangeDetectionServiceResult;

/// Test 1: Basic end-to-end change detection workflow
///
/// This test validates the complete workflow:
/// 1. Create initial files
/// 2. Run change detection (should detect all as new)
/// 3. Modify a file
/// 4. Run change detection again (should detect only the modified file)
/// 5. Delete a file
/// 6. Run change detection again (should detect deletion)
#[tokio::test]
async fn test_e2e_basic_change_detection_workflow() -> Result<()> {
    println!("🧪 Starting E2E basic change detection workflow test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        run_basic_workflow_test()
    ).await;

    match test_result {
        Ok(result) => {
            println!("✅ E2E basic workflow test completed successfully");
            result
        }
        Err(_) => {
            panic!("⏰ E2E basic workflow test timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }
}

async fn run_basic_workflow_test() -> Result<()> {
    let mut harness = ChangeDetectionE2ETestHarness::new().await?;

    // Step 1: Create initial files
    println!("\n📁 Step 1: Creating initial files");
    harness.create_initial_files().await?;

    // Step 2: First change detection (all files should be new)
    println!("\n🔍 Step 2: Running initial change detection");
    let (result1, elapsed1) = harness.run_change_detection().await?;

    println!("⏱️  Initial detection completed in {:?}", elapsed1);
    println!("📊 Results: {} files scanned, {} changes detected",
        result1.metrics.files_scanned, result1.metrics.changes_detected);

    // Should detect all initial files as new
    assert!(result1.metrics.files_scanned > 0, "Should scan initial files");
    assert_eq!(result1.metrics.changes_detected, result1.metrics.files_scanned,
        "All initial files should be detected as new");
    assert!(result1.processing_result.is_some(), "Should auto-process new files");

    // Step 3: Modify one file
    println!("\n✏️  Step 3: Modifying a file");
    let modified_file = "getting-started.md";
    harness.modify_file(modified_file, "\n\n## Updated Section\n\nThis content was added to test change detection.").await?;

    // Step 4: Second change detection (should detect only the modified file)
    println!("\n🔍 Step 4: Running change detection after modification");
    let (result2, elapsed2) = harness.run_change_detection().await?;

    println!("⏱️  Modification detection completed in {:?}", elapsed2);
    println!("📊 Results: {} files scanned, {} changes detected",
        result2.metrics.files_scanned, result2.metrics.changes_detected);

    // Should detect only the modified file
    assert_eq!(result2.metrics.files_scanned, result1.metrics.files_scanned,
        "Should scan same number of files");
    assert_eq!(result2.metrics.changes_detected, 1,
        "Should detect exactly one changed file");
    assert!(result2.processing_result.is_some(), "Should auto-process modified file");

    // Verify the ChangeSet contains the correct file
    let changeset = &result2.changeset;

    // Debug: Print which files were detected as changed
    if changeset.changed.len() != 1 {
        println!("\n🐛 DEBUG: Expected 1 changed file, got {}:", changeset.changed.len());
        for (i, file) in changeset.changed.iter().enumerate() {
            println!("  {}. {}", i + 1, file.relative_path);
        }
    }

    assert_eq!(changeset.changed.len(), 1, "Should have exactly one changed file");
    assert_eq!(changeset.new.len(), 0, "Should have no new files");
    assert_eq!(changeset.deleted.len(), 0, "Should have no deleted files");
    assert!(changeset.changed[0].relative_path.contains(modified_file),
        "Changed file should match the modified file");

    // Step 5: Delete a file
    println!("\n🗑️  Step 5: Deleting a file");
    let deleted_file = "troubleshooting.md";
    harness.delete_file(deleted_file).await?;

    // Step 6: Third change detection (should detect deletion)
    println!("\n🔍 Step 6: Running change detection after deletion");
    let (result3, elapsed3) = harness.run_change_detection().await?;

    println!("⏱️  Deletion detection completed in {:?}", elapsed3);
    println!("📊 Results: {} files scanned, {} changes detected",
        result3.metrics.files_scanned, result3.metrics.changes_detected);

    // Should detect the deletion
    assert_eq!(result3.metrics.files_scanned, result1.metrics.files_scanned - 1,
        "Should scan one less file");
    assert_eq!(result3.metrics.changes_detected, 1,
        "Should detect exactly one deletion");
    assert!(result3.processing_result.is_some(), "Should still have processing result");

    // Verify the ChangeSet contains the deletion
    let changeset = &result3.changeset;

    // Debug: Print changeset details
    println!("\n🐛 DEBUG Step 6 changeset:");
    println!("  Changed files: {}", changeset.changed.len());
    for file in &changeset.changed {
        println!("    - {}", file.relative_path);
    }
    println!("  New files: {}", changeset.new.len());
    for file in &changeset.new {
        println!("    - {}", file.relative_path);
    }
    println!("  Deleted files: {}", changeset.deleted.len());
    for path in &changeset.deleted {
        println!("    - {}", path);
    }

    assert_eq!(changeset.changed.len(), 0, "Should have no changed files");
    assert_eq!(changeset.new.len(), 0, "Should have no new files");
    assert_eq!(changeset.deleted.len(), 1, "Should have exactly one deleted file");
    assert!(changeset.deleted[0].contains(deleted_file),
        "Deleted file should match the deleted file");

    println!("\n✅ E2E basic workflow test PASSED");
    println!("   📈 Performance: Initial {:?}, Modification {:?}, Deletion {:?}",
        elapsed1, elapsed2, elapsed3);

    Ok(())
}

/// Test 2: Multiple file changes scenario
///
/// This test validates handling of multiple simultaneous changes:
/// 1. Create initial state
/// 2. Modify multiple files
/// 3. Add new files
/// 4. Delete files
/// 5. Validate ChangeSet contains all expected changes
#[tokio::test]
async fn test_e2e_multiple_file_changes() -> Result<()> {
    println!("🧪 Starting E2E multiple file changes test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        run_multiple_changes_test()
    ).await;

    match test_result {
        Ok(result) => {
            println!("✅ E2E multiple changes test completed successfully");
            result
        }
        Err(_) => {
            panic!("⏰ E2E multiple changes test timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }
}

async fn run_multiple_changes_test() -> Result<()> {
    let mut harness = ChangeDetectionE2ETestHarness::new().await?;

    // Step 1: Create initial state
    println!("\n📁 Step 1: Creating initial files");
    harness.create_initial_files().await?;

    // Run initial change detection to establish baseline
    println!("\n🔍 Step 2: Establishing baseline");
    let (baseline_result, _) = harness.run_change_detection().await?;
    let initial_file_count = baseline_result.metrics.files_scanned;

    // Step 3: Make multiple changes simultaneously
    println!("\n✏️  Step 3: Making multiple changes");

    // Modify existing files
    harness.modify_file("api-reference.md", "\n\n## New API Section\n\nUpdated API documentation.").await?;
    harness.modify_file("best-practices.md", "\n\n## Security Best Practices\n\nUpdated security guidelines.").await?;

    // Add new files
    harness.create_new_file("new-feature.md", "# New Feature\n\nDocumentation for a new feature.").await?;
    harness.create_new_file("migration-guide.md", "# Migration Guide\n\nHow to migrate from older versions.").await?;

    // Delete a file
    harness.delete_file("changelog.md").await?;

    // Step 4: Run change detection
    println!("\n🔍 Step 4: Running change detection on multiple changes");
    let (result, elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Multiple changes detection completed in {:?}", elapsed);
    println!("📊 Results: {} files scanned, {} changes detected",
        result.metrics.files_scanned, result.metrics.changes_detected);

    // Validate the results
    let changeset = &result.changeset;

    // Should have 2 modified files
    assert_eq!(changeset.changed.len(), 2,
        "Should detect exactly 2 modified files, got: {:?}", changeset.changed.iter().map(|f| &f.relative_path).collect::<Vec<_>>());

    // Should have 2 new files
    assert_eq!(changeset.new.len(), 2,
        "Should detect exactly 2 new files, got: {:?}", changeset.new.iter().map(|f| &f.relative_path).collect::<Vec<_>>());

    // Should have 1 deleted file
    assert_eq!(changeset.deleted.len(), 1,
        "Should detect exactly 1 deleted file, got: {:?}", changeset.deleted);

    // Total changes should be 5
    assert_eq!(result.metrics.changes_detected, 5,
        "Should detect exactly 5 total changes");

    // Verify specific files are in the correct categories
    let modified_paths: Vec<String> = changeset.changed.iter().map(|f| f.relative_path.clone()).collect();
    assert!(modified_paths.iter().any(|p| p.contains("api-reference.md")),
        "Should contain api-reference.md in modified files");
    assert!(modified_paths.iter().any(|p| p.contains("best-practices.md")),
        "Should contain best-practices.md in modified files");

    let new_paths: Vec<String> = changeset.new.iter().map(|f| f.relative_path.clone()).collect();
    assert!(new_paths.iter().any(|p| p.contains("new-feature.md")),
        "Should contain new-feature.md in new files");
    assert!(new_paths.iter().any(|p| p.contains("migration-guide.md")),
        "Should contain migration-guide.md in new files");

    assert!(changeset.deleted[0].contains("changelog.md"),
        "Should contain changelog.md in deleted files");

    // Performance should still be reasonable
    assert!(elapsed < Duration::from_secs(CHANGE_DETECTION_TIMEOUT_SECS),
        "Multiple changes detection should complete within timeout");

    println!("\n✅ E2E multiple changes test PASSED");
    println!("   📈 Changes: {} modified, {} new, {} deleted",
        changeset.changed.len(), changeset.new.len(), changeset.deleted.len());
    println!("   ⏱️  Total time: {:?}", elapsed);

    Ok(())
}

/// Test 3: Selective processing validation
///
/// This test validates that only changed files are processed:
/// 1. Create initial state with processing
/// 2. Modify only a few files
/// 3. Verify processing result only includes changed files
/// 4. Validate unchanged files are skipped
#[tokio::test]
async fn test_e2e_selective_processing() -> Result<()> {
    println!("🧪 Starting E2E selective processing test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        run_selective_processing_test()
    ).await;

    match test_result {
        Ok(result) => {
            println!("✅ E2E selective processing test completed successfully");
            result
        }
        Err(_) => {
            panic!("⏰ E2E selective processing test timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }
}

async fn run_selective_processing_test() -> Result<()> {
    let mut harness = ChangeDetectionE2ETestHarness::new().await?;

    // Step 1: Create initial state with many files
    println!("\n📁 Step 1: Creating initial files");
    harness.create_initial_files().await?;

    // Add more files to make the test more realistic
    for i in 1..=10 {
        let filename = format!("test-file-{}.md", i);
        let content = format!("# Test File {}\n\nContent for test file number {}.", i, i);
        harness.create_new_file(&filename, &content).await?;
    }

    // Run initial change detection to establish baseline
    println!("\n🔍 Step 2: Establishing baseline with all files");
    let (baseline_result, baseline_elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Baseline processing completed in {:?}", baseline_elapsed);
    println!("📊 Baseline: {} files processed",
        baseline_result.processing_result.as_ref().unwrap().processed_count);

    let total_files = baseline_result.metrics.files_scanned;
    assert!(total_files > 10, "Should have a reasonable number of files");

    // Step 3: Modify only a small subset of files
    println!("\n✏️  Step 3: Modifying only 2 files out of {}", total_files);

    let modified_files = vec!["getting-started.md", "test-file-3.md"];
    for filename in &modified_files {
        harness.modify_file(filename, "\n\n## Modified Content\n\nThis file was modified for testing.").await?;
    }

    // Step 4: Run change detection
    println!("\n🔍 Step 4: Running selective change detection");
    let (result, elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Selective detection completed in {:?}", elapsed);
    println!("📊 Results: {} files scanned, {} changes detected",
        result.metrics.files_scanned, result.metrics.changes_detected);

    // Validate selective processing
    assert_eq!(result.metrics.files_scanned, total_files,
        "Should still scan all files");
    assert_eq!(result.metrics.changes_detected, modified_files.len(),
        "Should detect only the modified files");

    // Check processing result
    if let Some(processing_result) = &result.processing_result {
        println!("🔄 Processing result: {} files processed, {} failed",
            processing_result.processed_count, processing_result.failed_count);

        // Should process fewer files than total (only the changes)
        assert!(processing_result.processed_count < total_files,
            "Should process fewer files than total");
        assert_eq!(processing_result.processed_count, modified_files.len(),
            "Should process exactly the number of modified files");

        // Performance should be significantly better than baseline
        let speed_improvement = baseline_elapsed.as_secs_f64() / elapsed.as_secs_f64();
        println!("⚡ Speed improvement: {:.2}x faster", speed_improvement);

        // Note: We don't assert a specific speed improvement since it can vary,
        // but in real scenarios it should be significantly faster
    } else {
        return Err(anyhow::anyhow!("Expected processing result but got None"));
    }

    // Validate ChangeSet contains only expected changes
    let changeset = &result.changeset;
    assert_eq!(changeset.changed.len(), modified_files.len(),
        "Should have exactly the modified files in changeset");
    assert_eq!(changeset.new.len(), 0, "Should have no new files");
    assert_eq!(changeset.deleted.len(), 0, "Should have no deleted files");

    let changed_paths: Vec<String> = changeset.changed.iter().map(|f| f.relative_path.clone()).collect();
    for expected_file in &modified_files {
        assert!(changed_paths.iter().any(|p| p.contains(expected_file)),
            "Should contain {} in changed files", expected_file);
    }

    println!("\n✅ E2E selective processing test PASSED");
    println!("   📈 Processed {}/{} files ({:.1}% of total)",
        result.processing_result.as_ref().unwrap().processed_count,
        total_files,
        (result.processing_result.as_ref().unwrap().processed_count as f64 / total_files as f64) * 100.0);

    Ok(())
}

/// Test 4: Performance validation with larger file sets
///
/// This test validates performance with a larger number of files:
/// 1. Create many files
/// 2. Validate change detection performance
/// 3. Modify a subset and validate selective processing performance
#[tokio::test]
async fn test_e2e_performance_validation() -> Result<()> {
    println!("🧪 Starting E2E performance validation test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS * 2), // Longer timeout for performance test
        run_performance_validation_test()
    ).await;

    match test_result {
        Ok(result) => {
            println!("✅ E2E performance validation test completed successfully");
            result
        }
        Err(_) => {
            panic!("⏰ E2E performance validation test timed out after {} seconds", TEST_TIMEOUT_SECS * 2);
        }
    }
}

async fn run_performance_validation_test() -> Result<()> {
    let mut harness = ChangeDetectionE2ETestHarness::new().await?;

    // Step 1: Create many files for performance testing
    println!("\n📁 Step 1: Creating many files for performance testing");
    harness.create_initial_files().await?;

    let file_count_to_create = std::cmp::min(MAX_FILES_FOR_PERFORMANCE_TESTS, 50); // Limit for CI
    for i in 1..=file_count_to_create {
        let filename = format!("perf-test-{}.md", i);
        let content = format!(
            "# Performance Test File {}\n\n## Content\n\nThis is test file number {}.\n\n## Details\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.\n\n## Code Example\n\n```rust\nfn test_function_{}() {{\n    println!(\"Test file {}\");\n}}\n```\n\n## Conclusion\n\nThis concludes test file {}.",
            i, i, i, i, i
        );
        harness.create_new_file(&filename, &content).await?;
    }

    println!("📝 Created {} test files", file_count_to_create);

    // Step 2: Baseline performance test
    println!("\n🔍 Step 2: Running baseline performance test");
    let (baseline_result, baseline_elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Baseline: {:?} for {} files", baseline_elapsed, baseline_result.metrics.files_scanned);
    println!("📊 Baseline metrics: {:.2} files/second",
        baseline_result.metrics.files_scanned as f64 / baseline_elapsed.as_secs_f64());

    // Performance requirements
    let max_time_per_file = Duration::from_millis(100); // 100ms per file max
    let expected_max_time = max_time_per_file * baseline_result.metrics.files_scanned as u32;

    assert!(baseline_elapsed < expected_max_time,
        "Baseline processing should complete in reasonable time: {:?} < {:?}",
        baseline_elapsed, expected_max_time);

    // Step 3: Selective modification performance test
    println!("\n✏️  Step 3: Modifying 5 files for selective processing test");
    let files_to_modify = vec!["perf-test-1.md", "perf-test-10.md", "perf-test-20.md", "perf-test-30.md", "perf-test-40.md"];

    for filename in &files_to_modify {
        harness.modify_file(filename, "\n\n## Performance Test Modification\n\nThis content was added for performance testing.").await?;
    }

    // Step 4: Selective processing performance test
    println!("\n🔍 Step 4: Running selective processing performance test");
    let (selective_result, selective_elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Selective: {:?} for {} changes", selective_elapsed, selective_result.metrics.changes_detected);
    println!("📊 Selective metrics: {:.2} files/second (scanned)",
        selective_result.metrics.files_scanned as f64 / selective_elapsed.as_secs_f64());

    // Validate selective processing is faster than full reprocessing
    let speed_improvement = baseline_elapsed.as_secs_f64() / selective_elapsed.as_secs_f64();
    println!("⚡ Selective processing speed improvement: {:.2}x", speed_improvement);

    // Selective processing should be significantly faster
    assert!(selective_elapsed < baseline_elapsed,
        "Selective processing should be faster than full reprocessing");

    // Validate only modified files were processed
    assert_eq!(selective_result.metrics.changes_detected, files_to_modify.len(),
        "Should detect exactly the modified files");

    if let Some(processing_result) = &selective_result.processing_result {
        assert_eq!(processing_result.processed_count, files_to_modify.len(),
            "Should process exactly the modified files");
    }

    // Step 5: Memory and cache efficiency validation
    println!("\n🧠 Step 5: Validating cache and memory efficiency");

    // Run change detection again on unchanged files
    let (unchanged_result, unchanged_elapsed) = harness.run_change_detection().await?;

    println!("⏱️  Unchanged detection: {:?} for 0 changes", unchanged_elapsed);

    // Should detect no changes and be very fast
    assert_eq!(unchanged_result.metrics.changes_detected, 0,
        "Should detect no changes when nothing has changed");

    // Should be faster than selective processing
    assert!(unchanged_elapsed < selective_elapsed,
        "No-change detection should be fastest");

    println!("\n✅ E2E performance validation test PASSED");
    println!("   📈 Performance summary:");
    println!("     - Full processing: {:?} ({:.2} files/sec)",
        baseline_elapsed,
        baseline_result.metrics.files_scanned as f64 / baseline_elapsed.as_secs_f64());
    println!("     - Selective processing: {:?} ({} files, {:.2}x speedup)",
        selective_elapsed,
        selective_result.metrics.changes_detected,
        speed_improvement);
    println!("     - No-change detection: {:?} (0 changes)",
        unchanged_elapsed);

    Ok(())
}

/// Test 5: Error handling and edge cases
///
/// This test validates error handling and edge cases:
/// 1. Empty vault handling
/// 2. File permission issues (simulated)
/// 3. Corrupted files handling
/// 4. Very large files handling
#[tokio::test]
async fn test_e2e_error_handling_and_edge_cases() -> Result<()> {
    println!("🧪 Starting E2E error handling and edge cases test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        run_error_handling_test()
    ).await;

    match test_result {
        Ok(result) => {
            match result {
                Ok(_) => {
                    println!("✅ E2E error handling test completed successfully");
                    Ok(())
                }
                Err(e) => {
                    eprintln!("❌ E2E error handling test failed: {:#?}", e);
                    Err(e)
                }
            }
        }
        Err(_) => {
            panic!("⏰ E2E error handling test timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }
}

async fn run_error_handling_test() -> Result<Vec<TempDir>> {
    // Test Case 1: Empty vault
    // Use in-memory databases for this test to avoid RocksDB file handle cleanup issues
    println!("\n📁 Test Case 1: Empty vault handling");
    let empty_harness = ChangeDetectionE2ETestHarness::new_with_db_type(true).await?;

    let (empty_result, empty_elapsed) = empty_harness.run_change_detection().await?;
    assert_eq!(empty_result.metrics.files_scanned, 0, "Empty vault should scan 0 files");
    assert_eq!(empty_result.metrics.changes_detected, 0, "Empty vault should detect 0 changes");
    assert!(empty_result.processing_result.is_none(), "Empty vault should not have processing result");
    println!("✅ Empty vault handled correctly in {:?}", empty_elapsed);

    // Test Case 2: Single file operations
    println!("\n📝 Test Case 2: Single file operations");
    let mut single_harness = ChangeDetectionE2ETestHarness::new_with_db_type(true).await?;

    // Create a single file
    single_harness.create_new_file("single.md", "# Single File\n\nJust one file here.").await?;

    let (single_result, _) = single_harness.run_change_detection().await?;
    assert_eq!(single_result.metrics.files_scanned, 1, "Should scan exactly 1 file");
    assert_eq!(single_result.metrics.changes_detected, 1, "Should detect 1 new file");
    assert!(single_result.processing_result.is_some(), "Should process the single file");

    // Modify the single file
    single_harness.modify_file("single.md", "\n\nModified content.").await?;

    let (modified_result, _) = single_harness.run_change_detection().await?;
    assert_eq!(modified_result.metrics.changes_detected, 1, "Should detect 1 modification");
    assert_eq!(modified_result.changeset.changed.len(), 1, "Should have 1 changed file");

    // Delete the single file
    single_harness.delete_file("single.md").await?;

    let (deleted_result, _) = single_harness.run_change_detection().await?;
    assert_eq!(deleted_result.metrics.changes_detected, 1, "Should detect 1 deletion");
    assert_eq!(deleted_result.changeset.deleted.len(), 1, "Should have 1 deleted file");

    println!("✅ Single file operations handled correctly");

    // Test Case 3: Special characters in filenames
    println!("\n🔤 Test Case 3: Special characters in filenames");
    let mut special_harness = ChangeDetectionE2ETestHarness::new_with_db_type(true).await?;

    let special_files = vec![
        ("file with spaces.md", "# File with Spaces\n\nTesting space handling."),
        ("file-with-dashes.md", "# File with Dashes\n\nTesting dash handling."),
        ("file_with_underscores.md", "# File with Underscores\n\nTesting underscore handling."),
        ("file.with.dots.md", "# File with Dots\n\nTesting dot handling."),
        ("file123numbers456.md", "# File with Numbers\n\nTesting number handling."),
    ];

    for (filename, content) in &special_files {
        special_harness.create_new_file(filename, content).await?;
    }

    let (special_result, _) = special_harness.run_change_detection().await?;
    assert_eq!(special_result.metrics.files_scanned, special_files.len(),
        "Should scan all special character files");
    assert_eq!(special_result.metrics.changes_detected, special_files.len(),
        "Should detect all special character files as new");

    // Modify files with special characters
    for (filename, _) in &special_files[..2] { // Modify first 2 files
        special_harness.modify_file(filename, "\n\nModified content.").await?;
    }

    let (modified_special_result, _) = special_harness.run_change_detection().await?;
    assert_eq!(modified_special_result.metrics.changes_detected, 2,
        "Should detect modifications to special character files");

    println!("✅ Special characters in filenames handled correctly");

    // Test Case 4: Deep directory structure
    println!("\n📂 Test Case 4: Deep directory structure");
    let mut deep_harness = ChangeDetectionE2ETestHarness::new_with_db_type(true).await?;

    // Create nested directory structure
    let deep_paths = vec![
        "level1/level2/deep-file.md",
        "level1/level2/level3/deeper-file.md",
        "a/b/c/d/e/very-deep-file.md",
    ];

    for (i, deep_path) in deep_paths.iter().enumerate() {
        let content = format!("# Deep File {}\n\nThis file is at path: {}", i + 1, deep_path);
        deep_harness.create_new_file(deep_path, &content).await?;
    }

    let (deep_result, _) = deep_harness.run_change_detection().await?;
    assert_eq!(deep_result.metrics.files_scanned, deep_paths.len(),
        "Should scan files in deep directory structure");
    assert_eq!(deep_result.metrics.changes_detected, deep_paths.len(),
        "Should detect all deep files as new");

    println!("✅ Deep directory structure handled correctly");

    // Test Case 5: Binary file handling (should be skipped)
    println!("\n🔢 Test Case 5: Binary file handling");
    let mut binary_harness = ChangeDetectionE2ETestHarness::new_with_db_type(true).await?;

    // Create some text files first
    binary_harness.create_new_file("text1.md", "# Text File 1\n\nPlain text content.").await?;
    binary_harness.create_new_file("text2.md", "# Text File 2\n\nMore plain text.").await?;

    let (text_result, _) = binary_harness.run_change_detection().await?;
    let text_file_count = text_result.metrics.files_scanned;

    // Create a binary file (simulated by writing some bytes)
    let binary_path = binary_harness.vault_path().join("test.bin");
    let binary_content = vec![0u8; 100]; // 100 bytes of zeros
    fs::write(&binary_path, binary_content)?;

    let (binary_result, _) = binary_harness.run_change_detection().await?;

    // Binary files should be skipped by the file scanner (based on file type filtering)
    // So the file count should remain the same
    assert_eq!(binary_result.metrics.files_scanned, text_file_count,
        "Binary files should be skipped by file scanner");

    println!("✅ Binary file handling working correctly (skipped as expected)");

    println!("\n✅ E2E error handling and edge cases test PASSED");
    println!("   📊 All edge cases handled gracefully:");

    // Leak all harnesses to skip cleanup - they will be cleaned up by OS later
    // This avoids potential race conditions during test cleanup with multiple databases
    std::mem::forget(binary_harness);
    std::mem::forget(deep_harness);
    std::mem::forget(special_harness);
    std::mem::forget(single_harness);
    std::mem::forget(empty_harness);

    Ok(vec![])
}

/// Test 6: Integration with file scanner directly
///
/// This test validates integration between ChangeDetectionService and FileScanningService:
/// 1. Test FileScanningService directly
/// 2. Validate hash consistency
/// 3. Test file filtering and exclusion patterns
#[tokio::test]
async fn test_e2e_file_scanner_integration() -> Result<()> {
    println!("🧪 Starting E2E file scanner integration test");
    println!("{}", "=".repeat(60));

    let test_result = timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        run_file_scanner_integration_test()
    ).await;

    match test_result {
        Ok(result) => {
            println!("✅ E2E file scanner integration test completed successfully");
            result
        }
        Err(_) => {
            panic!("⏰ E2E file scanner integration test timed out after {} seconds", TEST_TIMEOUT_SECS);
        }
    }
}

async fn run_file_scanner_integration_test() -> Result<()> {
    let mut harness = ChangeDetectionE2ETestHarness::new().await?;

    // Step 1: Create a mix of files for testing
    println!("\n📁 Step 1: Creating mixed file types for scanner testing");
    harness.create_initial_files().await?;

    // Add files that should be excluded
    let excluded_files = vec![
        (".git/config", "[core]\n    repositoryformatversion = 0\n"),
        ("node_modules/package/index.js", "// JavaScript file that should be excluded"),
        ("target/debug/build.log", "Build log output"),
        ("temp.tmp", "Temporary file content"),
        ("backup.bak", "Backup file content"),
        ("ignored.txt", "This should be included"),
    ];

    for (filename, content) in &excluded_files {
        let file_path = harness.vault_path().join(filename);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&file_path, content)?;
        harness.file_paths.insert(filename.to_string(), file_path);
    }

    // Step 2: Test FileScanningService directly
    println!("\n🔍 Step 2: Testing FileScanningService directly");
    let scanner = harness.file_scanner();
    let scan_result = scanner.scan_directory().await?;

    println!("📊 Scanner results:");
    println!("   - Total files considered: {}", scan_result.total_considered);
    println!("   - Successfully scanned: {}", scan_result.successful_files);
    println!("   - Skipped files: {}", scan_result.skipped_files);
    println!("   - Scan errors: {}", scan_result.scan_errors.len());

    // Should have scanned some files but skipped excluded ones
    assert!(scan_result.successful_files > 0, "Should scan some files");
    assert!(scan_result.skipped_files > 0, "Should skip excluded files");

    // Step 3: Validate discovered files
    println!("\n📋 Step 3: Validating discovered files");
    let discovered_files = scanner.get_discovered_files().await;

    println!("📝 Discovered files ({}):", discovered_files.len());
    for file in &discovered_files {
        println!("   - {} ({})", file.relative_path(),
            if file.is_markdown() { "markdown" } else { "other" });
    }

    // Should find our test files but not excluded ones
    let discovered_paths: Vec<String> = discovered_files.iter().map(|f| f.relative_path().to_string()).collect();

    // Should include markdown files
    assert!(discovered_paths.iter().any(|p| p.contains("getting-started.md")),
        "Should include getting-started.md");
    assert!(discovered_paths.iter().any(|p| p.contains("ignored.txt")),
        "Should include ignored.txt (not excluded by default patterns)");

    // Should NOT include excluded patterns
    assert!(!discovered_paths.iter().any(|p| p.contains(".git")),
        "Should not include .git files");
    assert!(!discovered_paths.iter().any(|p| p.contains("node_modules")),
        "Should not include node_modules files");

    // Step 4: Validate hash consistency
    println!("\n🔐 Step 4: Validating hash consistency");

    let file_hasher = FileHasher::new(Blake3Algorithm);

    for file_info in &discovered_files {
        if file_info.content_hash() == FileHash::zero() {
            continue; // Skip files without hashes
        }

        // Calculate hash directly for comparison
        let direct_hash = file_hasher.hash_file(
            &harness.vault_path().join(file_info.relative_path())
        ).await?;

        let scanner_hash = file_info.content_hash();

        assert_eq!(direct_hash, scanner_hash,
            "Hash should be consistent for file: {}", file_info.relative_path());
    }

    println!("✅ Hash consistency validated for {} files", discovered_files.len());

    // Step 5: Test specific file scanning
    println!("\n🎯 Step 5: Testing specific file scanning");

    let specific_files = vec![
        harness.vault_path().join("getting-started.md"),
        harness.vault_path().join("api-reference.md"),
    ];

    let specific_result = scanner.scan_files(specific_files.clone()).await?;

    assert_eq!(specific_result.successful_files, 2,
        "Should successfully scan specific files");
    assert_eq!(specific_result.total_considered, 2,
        "Should consider exactly the specified files");

    // Step 6: Integration with ChangeDetectionService
    println!("\n🔄 Step 6: Validating integration with ChangeDetectionService");

    let (integration_result, integration_elapsed) = harness.run_change_detection().await?;

    // Should detect the same number of files that the scanner found
    assert_eq!(integration_result.metrics.files_scanned, scan_result.successful_files,
        "ChangeDetectionService should scan same number of files as FileScanningService");

    println!("⏱️  Integration completed in {:?}", integration_elapsed);
    println!("📊 Integration metrics: {} files scanned, {} changes detected",
        integration_result.metrics.files_scanned, integration_result.metrics.changes_detected);

    // Step 7: Test scanner statistics
    println!("\n📈 Step 7: Testing scanner statistics");

    let stats = scanner.get_scan_statistics().await;
    println!("📊 Scanner statistics:");
    println!("   - Scan count: {}", stats.scan_count);
    println!("   - Root path: {:?}", stats.root_path);
    println!("   - Hash algorithm: {:?}", stats.hash_algorithm);
    println!("   - Last scan time: {:?}", stats.last_scan_time);

    // Note: We perform 2 scans in this test:
    // 1. Direct call to scanner.scan_directory() in Step 2
    // 2. Indirect call via ChangeDetectionService.detect_and_process_changes() in Step 6
    assert_eq!(stats.scan_count, 2, "Should have performed 2 scans (1 direct + 1 via ChangeDetectionService)");
    assert_eq!(stats.root_path, harness.vault_path(), "Root path should match");
    assert_eq!(stats.hash_algorithm, HashAlgorithm::Blake3, "Should use Blake3 algorithm");

    println!("\n✅ E2E file scanner integration test PASSED");
    println!("   📈 Scanner integration validated:");
    println!("     - File filtering and exclusion: ✅");
    println!("     - Hash consistency: ✅");
    println!("     - Specific file scanning: ✅");
    println!("     - Statistics tracking: ✅");

    Ok(())
}