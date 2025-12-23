//! ContentHasher Trait Demonstration
//!
//! This example demonstrates the usage of the ContentHasher trait and its
//! implementation for file hashing and change detection as part of the
//! architectural refactoring.
//!
//! ## Running the Example
//!
//! ```bash
//! cargo run --example content_hasher_demo --package crucible-core
//! ```

use crucible_core::{
    hashing::{
        algorithm::{Blake3Algorithm, Sha256Algorithm},
        FileHasher,
    },
    traits::change_detection::{
        BatchLookupConfig, ChangeDetectionMetrics, ChangeDetectionResult, ChangeDetector,
        ChangeStatistics, ContentHasher, HashLookupResult, HashLookupStorage, StoredHash,
    },
    types::hashing::{FileHash, FileHashInfo, HashError},
    ChangeSet,
};

use async_trait::async_trait;
use std::collections::HashMap;
use tempfile::tempdir;
use tokio::fs;

/// Mock implementation of HashLookupStorage for demonstration
#[derive(Debug, Default)]
struct MockHashStorage {
    hashes: std::sync::Arc<tokio::sync::RwLock<HashMap<String, FileHashInfo>>>,
}

impl MockHashStorage {
    fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl HashLookupStorage for MockHashStorage {
    async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError> {
        let storage = self.hashes.read().await;
        if let Some(info) = storage.get(relative_path) {
            Ok(Some(StoredHash::new(
                format!("notes:{}", relative_path),
                relative_path.to_string(),
                info.content_hash,
                info.size,
                info.modified.into(),
            )))
        } else {
            Ok(None)
        }
    }

    async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
        _config: Option<BatchLookupConfig>,
    ) -> Result<HashLookupResult, HashError> {
        let storage = self.hashes.read().await;
        let mut result = HashLookupResult::new();
        result.total_queried = relative_paths.len();
        result.database_round_trips = 1;

        for path in relative_paths {
            if let Some(info) = storage.get(path) {
                let stored = StoredHash::new(
                    format!("notes:{}", path),
                    path.clone(),
                    info.content_hash,
                    info.size,
                    info.modified.into(),
                );
                result.found_files.insert(path.clone(), stored);
            } else {
                result.missing_files.push(path.clone());
            }
        }

        Ok(result)
    }

    async fn lookup_files_by_content_hashes(
        &self,
        _content_hashes: &[FileHash],
    ) -> Result<HashMap<String, Vec<StoredHash>>, HashError> {
        // Simplified mock implementation
        Ok(HashMap::new())
    }

    async fn lookup_changed_files_since(
        &self,
        _since: chrono::DateTime<chrono::Utc>,
        _limit: Option<usize>,
    ) -> Result<Vec<StoredHash>, HashError> {
        // Simplified mock implementation
        Ok(Vec::new())
    }

    async fn check_file_needs_update(
        &self,
        relative_path: &str,
        new_hash: &FileHash,
    ) -> Result<bool, HashError> {
        let storage = self.hashes.read().await;
        match storage.get(relative_path) {
            Some(stored) => Ok(stored.content_hash != *new_hash),
            None => Ok(true), // File doesn't exist, needs processing
        }
    }

    async fn store_hashes(&self, files: &[FileHashInfo]) -> Result<(), HashError> {
        let mut storage = self.hashes.write().await;
        for file in files {
            storage.insert(file.relative_path.clone(), file.clone());
        }
        Ok(())
    }

    async fn remove_hashes(&self, paths: &[String]) -> Result<(), HashError> {
        let mut storage = self.hashes.write().await;
        for path in paths {
            storage.remove(path);
        }
        Ok(())
    }

    async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError> {
        let storage = self.hashes.read().await;
        Ok(storage.clone())
    }

    async fn clear_all_hashes(&self) -> Result<(), HashError> {
        let mut storage = self.hashes.write().await;
        storage.clear();
        Ok(())
    }
}

/// Simple implementation of ChangeDetector for demonstration
struct SimpleChangeDetector {
    storage: std::sync::Arc<dyn HashLookupStorage>,
}

impl SimpleChangeDetector {
    fn new(storage: std::sync::Arc<dyn HashLookupStorage>) -> Self {
        Self { storage }
    }
}

#[async_trait]
impl ChangeDetector for SimpleChangeDetector {
    async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError> {
        let paths: Vec<String> = current_files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect();

        let lookup_result = self.storage.lookup_file_hashes_batch(&paths, None).await?;
        let mut changes = ChangeSet::new();

        for current_file in current_files {
            match lookup_result.found_files.get(&current_file.relative_path) {
                Some(stored) => {
                    if stored.content_hash == current_file.content_hash {
                        changes.add_unchanged(current_file.clone());
                    } else {
                        changes.add_changed(current_file.clone());
                    }
                }
                None => {
                    changes.add_new(current_file.clone());
                }
            }
        }

        // Find deleted files (in storage but not in current scan)
        let stored_paths: std::collections::HashSet<_> = lookup_result.found_files.keys().collect();
        let current_paths: std::collections::HashSet<_> =
            current_files.iter().map(|f| &f.relative_path).collect();

        for path in stored_paths.difference(&current_paths) {
            changes.add_deleted((*path).clone());
        }

        Ok(changes)
    }

    async fn detect_changes_with_metrics(
        &self,
        current_files: &[FileHashInfo],
    ) -> Result<ChangeDetectionResult, HashError> {
        let start_time = std::time::Instant::now();
        let changes = self.detect_changes(current_files).await?;
        let elapsed = start_time.elapsed();

        let metrics = ChangeDetectionMetrics {
            total_files: current_files.len(),
            changed_files: changes.changed.len(),
            skipped_files: changes.unchanged.len(),
            change_detection_time: elapsed,
            database_round_trips: 1, // Simplified
            cache_hit_rate: 0.8,     // Simplified
            files_per_second: current_files.len() as f64 / elapsed.as_secs_f64().max(0.001),
        };

        Ok(ChangeDetectionResult::new(changes, metrics))
    }

    async fn detect_changes_for_paths(&self, _paths: &[String]) -> Result<ChangeSet, HashError> {
        // For this demo, we'll return an empty change set
        Ok(ChangeSet::new())
    }

    async fn check_file_changed(&self, path: &str) -> Result<Option<FileHashInfo>, HashError> {
        let lookup_result = self
            .storage
            .lookup_file_hashes_batch(&[path.to_string()], None)
            .await?;
        if lookup_result.found_files.contains_key(path) {
            // In a real implementation, we would hash the current file and compare
            // For this demo, we'll return None to indicate no current file state
            Ok(None)
        } else {
            // File doesn't exist in storage, so it's "new"
            Ok(None)
        }
    }

    async fn get_changed_files_since(
        &self,
        _since: chrono::DateTime<chrono::Utc>,
        _limit: Option<usize>,
    ) -> Result<Vec<FileHashInfo>, HashError> {
        // Simplified mock implementation
        Ok(Vec::new())
    }

    async fn batch_check_files_changed(
        &self,
        paths: &[String],
    ) -> Result<HashMap<String, bool>, HashError> {
        let mut results = HashMap::new();
        for path in paths {
            // For mock, assume files in storage haven't changed
            let lookup_result = self
                .storage
                .lookup_file_hashes_batch(std::slice::from_ref(path), None)
                .await?;
            let has_stored = lookup_result.found_files.contains_key(path);
            results.insert(path.clone(), has_stored);
        }
        Ok(results)
    }

    async fn detect_deleted_files(
        &self,
        current_paths: &[String],
    ) -> Result<Vec<String>, HashError> {
        let all_stored = self.storage.get_all_hashes().await?;
        let mut deleted = Vec::new();

        for stored_path in all_stored.keys() {
            if !current_paths.contains(stored_path) {
                deleted.push(stored_path.clone());
            }
        }

        Ok(deleted)
    }

    async fn get_change_statistics(&self) -> Result<ChangeStatistics, HashError> {
        let all_stored = self.storage.get_all_hashes().await?;
        Ok(ChangeStatistics {
            total_tracked_files: all_stored.len(),
            average_changes_per_day: 2.5,
            most_recent_change: Some(chrono::Utc::now()),
            oldest_tracked_file: Some(chrono::Utc::now() - chrono::Duration::days(7)),
            typical_change_rate: 0.1,
            average_database_round_trips: 1.5,
            average_cache_hit_rate: 0.8,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔐 ContentHasher Trait Demonstration");
    println!("====================================\n");

    // Create a temporary directory for our demo files
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();

    println!("📁 Working directory: {:?}", temp_path);

    // 1. Basic File Hashing
    println!("\n1️⃣  Basic File Hashing");
    println!("--------------------");

    let hasher = FileHasher::new(Blake3Algorithm);

    // Create test files
    let file1_path = temp_path.join("note.md");
    let file2_path = temp_path.join("readme.txt");

    fs::write(&file1_path, "# Hello World\n\nThis is a test note.").await?;
    fs::write(&file2_path, "This is a simple readme file.").await?;

    println!("Created test files:");
    println!("  - {:?}", file1_path);
    println!("  - {:?}", file2_path);

    // Hash the files
    let hash1 = hasher.hash_file(&file1_path).await?;
    let hash2 = hasher.hash_file(&file2_path).await?;

    println!("File hashes:");
    println!("  - note.md: {}", hash1);
    println!("  - readme.txt: {}", hash2);

    // 2. File Info with Metadata
    println!("\n2️⃣  File Hash Info with Metadata");
    println!("---------------------------------");

    let info1 = hasher
        .hash_file_info(&file1_path, "note.md".to_string())
        .await?;
    let info2 = hasher
        .hash_file_info(&file2_path, "readme.txt".to_string())
        .await?;

    println!("File info:");
    println!(
        "  - note.md: {} bytes, modified {:?}",
        info1.size, info1.modified
    );
    println!(
        "  - readme.txt: {} bytes, modified {:?}",
        info2.size, info2.modified
    );

    // 3. Batch Operations
    println!("\n3️⃣  Batch Hashing Operations");
    println!("----------------------------");

    let paths = vec![file1_path.clone(), file2_path.clone()];
    let batch_hashes = hasher.hash_files_batch(&paths).await?;

    println!("Batch hashing results:");
    for (i, path) in paths.iter().enumerate() {
        println!("  - {:?}: {}", path.file_name().unwrap(), batch_hashes[i]);
    }

    // 4. Block Hashing
    println!("\n4️⃣  Content Block Hashing");
    println!("-------------------------");

    let heading_content = "# Introduction";
    let paragraph_content = "This is the introduction paragraph.";

    let heading_hash = hasher.hash_block(heading_content).await?;
    let paragraph_hash = hasher.hash_block(paragraph_content).await?;

    println!("Block hashes:");
    println!("  - Heading: {}", heading_hash);
    println!("  - Paragraph: {}", paragraph_hash);

    // Create block info
    let block_info = hasher
        .hash_block_info(
            heading_content,
            "heading".to_string(),
            0,
            heading_content.len(),
        )
        .await?;

    println!("Block info:");
    println!("  - Type: {}", block_info.block_type);
    println!("  - Content length: {}", block_info.content_length());
    println!("  - Hash: {}", block_info.hash());

    // 5. Hash Verification
    println!("\n5️⃣  Hash Verification");
    println!("--------------------");

    let is_valid = hasher.verify_file_hash(&file1_path, &hash1).await?;
    println!("File verification (should be true): {}", is_valid);

    let wrong_hash = FileHash::new([0u8; 32]);
    let is_invalid = hasher.verify_file_hash(&file1_path, &wrong_hash).await?;
    println!("Wrong hash verification (should be false): {}", is_invalid);

    // 6. Change Detection Workflow
    println!("\n6️⃣  Change Detection Workflow");
    println!("---------------------------");

    // Create storage and change detector
    let storage = std::sync::Arc::new(MockHashStorage::new());
    let change_detector =
        SimpleChangeDetector::new(storage.clone() as std::sync::Arc<dyn HashLookupStorage>);

    // Store initial hashes
    let initial_files = vec![info1.clone(), info2.clone()];
    storage.store_hashes(&initial_files).await?;
    println!("Stored {} initial files in storage", initial_files.len());

    // Detect changes (should show no changes)
    let current_files = vec![info1.clone(), info2.clone()];
    let changes = change_detector.detect_changes(&current_files).await?;
    let summary = changes.summary();

    println!("Initial change detection:");
    println!("  - Total files: {}", summary.total_files);
    println!("  - Unchanged: {}", summary.unchanged);
    println!("  - Changed: {}", summary.changed);
    println!("  - New: {}", summary.new);
    println!("  - Deleted: {}", summary.deleted);
    println!("  - Has changes: {}", summary.has_changes);

    // Modify a file and detect changes
    fs::write(
        &file1_path,
        "# Modified Title\n\nThis content has been changed.",
    )
    .await?;
    let modified_info = hasher
        .hash_file_info(&file1_path, "note.md".to_string())
        .await?;

    let current_files = vec![modified_info.clone(), info2.clone()];
    let changes = change_detector.detect_changes(&current_files).await?;
    let summary = changes.summary();

    println!("\nAfter modifying note.md:");
    println!("  - Total files: {}", summary.total_files);
    println!("  - Unchanged: {}", summary.unchanged);
    println!("  - Changed: {}", summary.changed);
    println!("  - New: {}", summary.new);
    println!("  - Deleted: {}", summary.deleted);
    println!("  - Has changes: {}", summary.has_changes);

    // Add a new file
    let file3_path = temp_path.join("new.md");
    fs::write(&file3_path, "# New Note\n\nThis is a new file.").await?;
    let new_info = hasher
        .hash_file_info(&file3_path, "new.md".to_string())
        .await?;

    let current_files = vec![modified_info.clone(), info2.clone(), new_info.clone()];
    let changes = change_detector.detect_changes(&current_files).await?;
    let summary = changes.summary();

    println!("\nAfter adding new.md:");
    println!("  - Total files: {}", summary.total_files);
    println!("  - Unchanged: {}", summary.unchanged);
    println!("  - Changed: {}", summary.changed);
    println!("  - New: {}", summary.new);
    println!("  - Deleted: {}", summary.deleted);
    println!("  - Has changes: {}", summary.has_changes);

    // 7. Algorithm Comparison
    println!("\n7️⃣  Algorithm Comparison");
    println!("-------------------------");

    let test_content = "This is test content for algorithm comparison.";

    let blake3_hasher = FileHasher::new(Blake3Algorithm);
    let sha256_hasher = FileHasher::new(Sha256Algorithm);

    let blake3_hash = blake3_hasher.hash_block(test_content).await?;
    let sha256_hash = sha256_hasher.hash_block(test_content).await?;

    println!("Algorithm comparison for: \"{}\"", test_content);
    println!("  - BLAKE3: {}", blake3_hash);
    println!("  - SHA256: {}", sha256_hash);

    // 8. Performance Characteristics
    println!("\n8️⃣  Performance Notes");
    println!("----------------------");
    println!("✅ Streaming I/O for large files (constant memory usage)");
    println!("✅ Async operations throughout (non-blocking)");
    println!("✅ Batch processing support for better throughput");
    println!("✅ Thread-safe implementation (Send + Sync)");
    println!("✅ Comprehensive error handling");

    println!("\n🎉 ContentHasher Demo Complete!");
    println!("The ContentHasher trait provides a solid foundation for");
    println!("the file system operations architectural refactoring.");

    Ok(())
}
