//! Diagnostic tests for N+1 file change detection bug
//!
//! These tests isolate specific hypotheses about why change detection
//! detects N+1 files as changed instead of N files.

mod common;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::fs;

use crucible_cli::common::{ChangeDetectionService, ChangeDetectionServiceConfig};
use crucible_core::{
    hashing::file_hasher::FileHasher,
    hashing::algorithm::Blake3Algorithm,
    traits::change_detection::{ContentHasher, HashLookupStorage},
    types::hashing::HashAlgorithm,
};
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};
use crucible_watch::ChangeDetectorConfig;

/// Test harness for diagnostic tests (simplified version of E2E harness)
pub struct DiagnosticTestHarness {
    pub temp_dir: TempDir,
    pub client: Arc<SurrealClient>,
    pub service: Arc<ChangeDetectionService>,
    pub file_paths: HashMap<String, PathBuf>,
}

impl DiagnosticTestHarness {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let vault_path = temp_dir.path();

        // Create database client
        let db_config = SurrealDbConfig {
            namespace: "crucible_diagnostic".to_string(),
            database: "diagnostic_test".to_string(),
            path: vault_path.join(".crucible/test.db").to_string_lossy().to_string(),
            max_connections: Some(10),
            timeout_seconds: Some(30),
        };

        let client = Arc::new(SurrealClient::new(db_config).await
            .context("Failed to create SurrealClient")?);

        // Initialize the database schema
        crucible_surrealdb::kiln_integration::initialize_kiln_schema(&client).await
            .context("Failed to initialize kiln schema")?;

        // Create change detection service
        let service = Arc::new(
            ChangeDetectionService::new(
                vault_path,
                client.clone(),
                HashAlgorithm::Blake3,
                ChangeDetectionServiceConfig {
                    change_detector: ChangeDetectorConfig::default(),
                    auto_process_changes: false, // Don't process, just detect
                    continue_on_processing_error: true,
                    max_processing_batch_size: 10,
                },
            )
            .await
            .context("Failed to create ChangeDetectionService")?,
        );

        Ok(Self {
            temp_dir,
            client,
            service,
            file_paths: HashMap::new(),
        })
    }

    pub async fn create_new_file(&mut self, filename: &str, content: &str) -> Result<PathBuf> {
        // Handle subdirectories
        let file_path = self.temp_dir.path().join(filename);

        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        fs::write(&file_path, content).await?;
        self.file_paths.insert(filename.to_string(), file_path.clone());
        println!("📝 Created file: {}", filename);
        Ok(file_path)
    }

    pub async fn run_change_detection(&self) -> Result<(crucible_cli::common::ChangeDetectionServiceResult, Duration)> {
        let start_time = std::time::Instant::now();
        let result = self.service.detect_and_process_changes().await
            .context("Change detection failed")?;
        let elapsed = start_time.elapsed();
        Ok((result, elapsed))
    }

    pub fn vault_path(&self) -> &Path {
        self.temp_dir.path()
    }
}

/// Test #1: Path Normalization Verification (HIGHEST PRIORITY)
///
/// Verifies that paths are stored and retrieved in identical format.
/// If paths differ (e.g., "file.md" vs "./file.md"), files will be counted twice.
#[tokio::test]
async fn test_path_normalization_consistency() -> Result<()> {
    println!("\n=== Test #1: Path Normalization Verification ===\n");

    let mut harness = DiagnosticTestHarness::new().await?;

    // Create a single file
    harness.create_new_file("test.md", "# Test Content\n\nThis is a test.").await?;

    // First scan - process the file
    println!("\n📊 First scan (detect new file):");
    let (result1, elapsed1) = harness.run_change_detection().await?;
    println!("  Time: {:?}", elapsed1);
    println!("  Files scanned: {}", result1.metrics.files_scanned);
    println!("  Changes detected: {}", result1.metrics.changes_detected);
    println!("  New: {}", result1.changeset.new.len());

    assert_eq!(result1.changeset.new.len(), 1, "First scan should detect 1 new file");

    // Extract the path from result
    let stored_path = &result1.changeset.new[0].relative_path;
    println!("\n✓ Stored path format: '{}'", stored_path);
    println!("  Length: {} bytes", stored_path.len());
    println!("  Bytes: {:?}", stored_path.as_bytes());

    // Query the database directly to see what path was stored
    let client = &harness.client;
    let sql = "SELECT path FROM notes ORDER BY path";
    let db_result = client.query(sql, &[]).await?;

    println!("\n📋 Database contents:");
    for (i, record) in db_result.records.iter().enumerate() {
        let db_path = record.data.get("path").and_then(|v| v.as_str()).unwrap_or("<missing>");
        println!("  Record {}: path='{}' (len={}, bytes={:?})",
            i + 1, db_path, db_path.len(), db_path.as_bytes());

        // CRITICAL CHECK: Are the paths identical?
        if db_path != stored_path {
            println!("\n❌ PATH MISMATCH DETECTED!");
            println!("   Stored in changeset: '{}'", stored_path);
            println!("   Stored in database:  '{}'", db_path);
            println!("   This is the root cause of the N+1 bug!");
            panic!("Path normalization inconsistency: stored='{}' vs database='{}'", stored_path, db_path);
        }
    }

    println!("✓ Path formats match");

    // Second scan - should detect 0 changes (CRITICAL TEST)
    println!("\n📊 Second scan (no changes expected):");
    let (result2, elapsed2) = harness.run_change_detection().await?;
    println!("  Time: {:?}", elapsed2);
    println!("  Files scanned: {}", result2.metrics.files_scanned);
    println!("  Changes detected: {}", result2.metrics.changes_detected);
    println!("  New: {}", result2.changeset.new.len());
    println!("  Changed: {}", result2.changeset.changed.len());
    println!("  Unchanged: {}", result2.changeset.unchanged.len());
    println!("  Deleted: {}", result2.changeset.deleted.len());

    // Print paths for debugging
    if !result2.changeset.new.is_empty() {
        println!("\n  New file paths:");
        for file in &result2.changeset.new {
            println!("    - '{}' (bytes={:?})", file.relative_path, file.relative_path.as_bytes());
        }
    }
    if !result2.changeset.changed.is_empty() {
        println!("\n  Changed file paths:");
        for file in &result2.changeset.changed {
            println!("    - '{}' (bytes={:?})", file.relative_path, file.relative_path.as_bytes());
        }
    }

    // CRITICAL ASSERTIONS
    if result2.changeset.new.len() > 0 || result2.changeset.changed.len() > 0 {
        println!("\n❌ TEST FAILED: Second scan detected changes when none should exist!");
        println!("   This confirms the N+1 bug is present.");

        // Debug: Compare paths character by character
        if let Some(new_file) = result2.changeset.new.first() {
            println!("\n   Detailed comparison:");
            println!("   Expected: '{}'", stored_path);
            println!("   Got:      '{}'", new_file.relative_path);
            println!("   Match: {}", stored_path == &new_file.relative_path);
        }
    }

    assert_eq!(result2.changeset.new.len(), 0,
        "Second scan should detect 0 new files");
    assert_eq!(result2.changeset.changed.len(), 0,
        "Second scan should detect 0 changed files");
    assert_eq!(result2.changeset.unchanged.len(), 1,
        "Second scan should detect 1 unchanged file");

    println!("\n✅ SUCCESS: Path normalization is consistent");
    Ok(())
}

/// Test #2: ChangeSet Deduplication Verification (HIGH PRIORITY)
///
/// Verifies that files are not added to multiple ChangeSet categories.
/// If a file appears in both "new" and "unchanged", it will be counted twice.
#[tokio::test]
async fn test_changeset_deduplication() -> Result<()> {
    println!("\n=== Test #2: ChangeSet Deduplication Verification ===\n");

    let mut harness = DiagnosticTestHarness::new().await?;

    // Create and process one file
    harness.create_new_file("single.md", "# Single File\n\nTest content.").await?;

    println!("\n📊 First scan:");
    let (result1, _) = harness.run_change_detection().await?;
    println!("  Changes detected: {}", result1.metrics.changes_detected);

    // Second scan - no modifications
    println!("\n📊 Second scan (no changes):");
    let (result2, _) = harness.run_change_detection().await?;
    println!("  Files scanned: {}", result2.metrics.files_scanned);
    println!("  Changes detected: {}", result2.metrics.changes_detected);

    // Create a unified set of ALL paths across all categories
    let mut all_paths = std::collections::HashSet::new();
    let mut duplicates = Vec::new();

    println!("\n📋 Checking for duplicate paths:");

    // Check new files
    for file in &result2.changeset.new {
        let path = &file.relative_path;
        if !all_paths.insert(path.clone()) {
            duplicates.push((path.clone(), "new"));
            println!("  ❌ DUPLICATE: '{}' appears in 'new' (already seen)", path);
        } else {
            println!("  ✓ Unique: '{}' in 'new'", path);
        }
    }

    // Check changed files
    for file in &result2.changeset.changed {
        let path = &file.relative_path;
        if !all_paths.insert(path.clone()) {
            duplicates.push((path.clone(), "changed"));
            println!("  ❌ DUPLICATE: '{}' appears in 'changed' (already seen)", path);
        } else {
            println!("  ✓ Unique: '{}' in 'changed'", path);
        }
    }

    // Check unchanged files
    for file in &result2.changeset.unchanged {
        let path = &file.relative_path;
        if !all_paths.insert(path.clone()) {
            duplicates.push((path.clone(), "unchanged"));
            println!("  ❌ DUPLICATE: '{}' appears in 'unchanged' (already seen)", path);
        } else {
            println!("  ✓ Unique: '{}' in 'unchanged'", path);
        }
    }

    // Check deleted files
    for path in &result2.changeset.deleted {
        if !all_paths.insert(path.clone()) {
            duplicates.push((path.clone(), "deleted"));
            println!("  ❌ DUPLICATE: '{}' appears in 'deleted' (already seen)", path);
        } else {
            println!("  ✓ Unique: '{}' in 'deleted'", path);
        }
    }

    // Verify total count matches individual counts
    let total_individual = result2.changeset.new.len() + result2.changeset.changed.len()
        + result2.changeset.unchanged.len() + result2.changeset.deleted.len();

    println!("\n📊 Totals:");
    println!("  Unique paths: {}", all_paths.len());
    println!("  Total individual: {}", total_individual);
    println!("  Duplicates found: {}", duplicates.len());

    if !duplicates.is_empty() {
        println!("\n❌ TEST FAILED: ChangeSet contains duplicate files!");
        for (path, category) in &duplicates {
            println!("  - '{}' in category '{}'", path, category);
        }
        panic!("ChangeSet contains {} duplicate files", duplicates.len());
    }

    assert_eq!(all_paths.len(), total_individual,
        "ChangeSet contains duplicate files: unique={}, total={}",
        all_paths.len(), total_individual);

    println!("\n✅ SUCCESS: No duplicate paths in ChangeSet");
    Ok(())
}

/// Test #3: Hash Storage/Retrieval Round-Trip (MEDIUM PRIORITY)
///
/// Verifies that hash storage and retrieval works correctly without timing delays.
#[tokio::test]
async fn test_hash_storage_retrieval_roundtrip() -> Result<()> {
    println!("\n=== Test #3: Hash Storage/Retrieval Round-Trip ===\n");

    let mut harness = DiagnosticTestHarness::new().await?;

    // Create a file and calculate its hash
    let test_content = "# Test Content\n\nThis is a test for hash round-trip.";
    harness.create_new_file("roundtrip.md", test_content).await?;

    // Calculate expected hash directly
    let hasher = FileHasher::new(Blake3Algorithm);
    let expected_hash = hasher.hash_file(&harness.vault_path().join("roundtrip.md")).await?;

    println!("✓ Expected hash (direct calculation): {}", expected_hash.to_hex());

    // First scan - process and store
    println!("\n📊 First scan (store hash):");
    let (result1, _) = harness.run_change_detection().await?;
    println!("  Changes detected: {}", result1.metrics.changes_detected);
    assert_eq!(result1.changeset.new.len(), 1, "Should detect 1 new file");

    // Query database IMMEDIATELY (no 500ms delay)
    println!("\n📋 Query database immediately after processing:");
    let client = &harness.client;
    let sql = "SELECT path, file_hash FROM notes WHERE path = 'roundtrip.md'";
    let db_result = client.query(sql, &[]).await?;

    if db_result.records.is_empty() {
        println!("❌ TEST FAILED: File not found in database immediately after processing!");
        println!("   This indicates a timing/storage issue.");
        panic!("File not found in database immediately after processing");
    }

    let stored_hash_hex = db_result.records[0]
        .data.get("file_hash")
        .and_then(|v| v.as_str())
        .expect("file_hash field missing");

    println!("✓ Stored hash (from database): {}", stored_hash_hex);

    // Verify hash matches
    if expected_hash.to_hex() != stored_hash_hex {
        println!("\n❌ TEST FAILED: Hash mismatch!");
        println!("   Expected: {}", expected_hash.to_hex());
        println!("   Stored:   {}", stored_hash_hex);
        panic!("Hash mismatch: expected={}, stored={}", expected_hash.to_hex(), stored_hash_hex);
    }

    println!("✓ Hash round-trip successful");

    // Now test retrieval through change detection (no delay)
    println!("\n📊 Second scan (immediate, no delay):");
    let (result2, _) = harness.run_change_detection().await?;
    println!("  Files scanned: {}", result2.metrics.files_scanned);
    println!("  New: {}", result2.changeset.new.len());
    println!("  Changed: {}", result2.changeset.changed.len());
    println!("  Unchanged: {}", result2.changeset.unchanged.len());

    // If this fails, it's a timing issue
    if result2.changeset.unchanged.len() != 1 {
        println!("\n❌ TEST FAILED: File not detected as unchanged immediately!");
        println!("   This indicates change detection cannot retrieve stored hash properly.");
    }

    assert_eq!(result2.changeset.unchanged.len(), 1,
        "File should be detected as unchanged immediately after storage");

    println!("\n✅ SUCCESS: Hash storage/retrieval round-trip works correctly");
    Ok(())
}

/// Test #4: File Scanner Path Format Investigation (MEDIUM PRIORITY)
///
/// Verifies that FileScanningService generates paths in consistent format.
#[tokio::test]
async fn test_file_scanner_path_format() -> Result<()> {
    println!("\n=== Test #4: File Scanner Path Format Investigation ===\n");

    let mut harness = DiagnosticTestHarness::new().await?;

    // Create files in different subdirectories
    println!("📝 Creating test files:");
    harness.create_new_file("root.md", "# Root").await?;
    harness.create_new_file("sub/nested.md", "# Nested").await?;
    harness.create_new_file("sub/deep/file.md", "# Deep").await?;

    // Run scan and inspect paths
    println!("\n📊 Running scan:");
    let (result, _) = harness.run_change_detection().await?;

    let discovered_paths: Vec<String> = result.changeset.new
        .iter()
        .map(|f| f.relative_path.clone())
        .collect();

    println!("\n📋 Discovered paths ({} total):", discovered_paths.len());
    for path in &discovered_paths {
        println!("  Path: '{}' (len={}, bytes={:?})", path, path.len(), path.as_bytes());
    }

    // Check for path normalization issues
    println!("\n🔍 Checking path format:");
    let mut issues = Vec::new();

    for path in &discovered_paths {
        // Should NOT start with "./" or "/"
        if path.starts_with("./") {
            issues.push(format!("Path starts with './' : '{}'", path));
            println!("  ❌ {}", issues.last().unwrap());
        }
        if path.starts_with("/") {
            issues.push(format!("Path starts with '/' : '{}'", path));
            println!("  ❌ {}", issues.last().unwrap());
        }

        // Should NOT end with "/"
        if path.ends_with("/") {
            issues.push(format!("Path ends with '/' : '{}'", path));
            println!("  ❌ {}", issues.last().unwrap());
        }

        // Should use forward slashes consistently (no backslashes)
        if path.contains("\\") {
            issues.push(format!("Path contains backslashes: '{}'", path));
            println!("  ❌ {}", issues.last().unwrap());
        }

        // Should not contain double slashes
        if path.contains("//") {
            issues.push(format!("Path contains double slashes: '{}'", path));
            println!("  ❌ {}", issues.last().unwrap());
        }

        if issues.is_empty() || issues.len() < discovered_paths.len() {
            println!("  ✓ Path is well-formed: '{}'", path);
        }
    }

    if !issues.is_empty() {
        println!("\n❌ TEST FAILED: Found {} path format issues!", issues.len());
        for issue in &issues {
            println!("  - {}", issue);
        }
        panic!("File scanner generates paths with format issues");
    }

    println!("\n✅ SUCCESS: All paths are well-formed and consistent");
    Ok(())
}

/// Test #5: Database Query Consistency Check (LOW PRIORITY)
///
/// Verifies that get_all_hashes() returns same results as batch lookup.
#[tokio::test]
async fn test_database_query_consistency() -> Result<()> {
    println!("\n=== Test #5: Database Query Consistency Check ===\n");

    use crucible_surrealdb::hash_lookup::SurrealHashLookupStorage;

    let mut harness = DiagnosticTestHarness::new().await?;

    // Create and process files
    println!("📝 Creating test files:");
    harness.create_new_file("file1.md", "# File 1\n\nContent 1.").await?;
    harness.create_new_file("file2.md", "# File 2\n\nContent 2.").await?;

    println!("\n📊 Processing files:");
    let (_result, _) = harness.run_change_detection().await?;

    // Query using get_all_hashes()
    println!("\n📋 Testing get_all_hashes():");
    let storage = SurrealHashLookupStorage::new(harness.client.as_ref());
    let all_hashes = storage.get_all_hashes().await?;

    println!("  Returned {} files", all_hashes.len());
    for (path, info) in &all_hashes {
        let hash_preview = &info.content_hash.to_hex()[..16];
        println!("    Path: '{}', Hash: {}...", path, hash_preview);
    }

    // Query using batch lookup
    println!("\n📋 Testing batch lookup:");
    let paths: Vec<String> = all_hashes.keys().cloned().collect();
    let batch_result = storage.lookup_file_hashes_batch(&paths, None).await?;

    println!("  Found {} files", batch_result.found_files.len());

    // Compare results
    if all_hashes.len() != batch_result.found_files.len() {
        println!("\n❌ TEST FAILED: Different counts!");
        println!("   get_all_hashes(): {}", all_hashes.len());
        println!("   batch lookup: {}", batch_result.found_files.len());
        panic!("get_all_hashes() and batch lookup returned different counts");
    }

    println!("\n🔍 Comparing hashes:");
    let mut mismatches = Vec::new();

    for (path, hash_info) in &all_hashes {
        match batch_result.found_files.get(path) {
            Some(batch_hash) => {
                if hash_info.content_hash != batch_hash.content_hash {
                    mismatches.push(format!(
                        "Path '{}': all_hashes={}, batch={}",
                        path,
                        hash_info.content_hash.to_hex(),
                        batch_hash.content_hash.to_hex()
                    ));
                    println!("  ❌ Hash mismatch for '{}'", path);
                } else {
                    println!("  ✓ Hash match for '{}'", path);
                }
            }
            None => {
                mismatches.push(format!("Path '{}' not found in batch result", path));
                println!("  ❌ Path '{}' missing from batch result", path);
            }
        }
    }

    if !mismatches.is_empty() {
        println!("\n❌ TEST FAILED: Found {} mismatches!", mismatches.len());
        for mismatch in &mismatches {
            println!("  - {}", mismatch);
        }
        panic!("Database query methods returned different results");
    }

    println!("\n✅ SUCCESS: Both query methods return identical results");
    Ok(())
}
