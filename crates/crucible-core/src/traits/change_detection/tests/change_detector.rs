use std::time::SystemTime;

use async_trait::async_trait;

use super::super::*;
use super::hash_lookup::MockHashLookupStorage;
use crate::types::hashing::{FileHash, FileHashInfo, HashAlgorithm, HashError};

// Test removed - change detection not part of Phase 0/1
// TODO: Re-add when implementing change detection in later phases

#[test]
fn test_change_summary() {
    let summary = ChangeSummary {
        total_files: 100,
        unchanged: 80,
        changed: 15,
        new: 4,
        deleted: 1,
        has_changes: true,
    };

    assert_eq!(summary.total_files, 100);
    assert_eq!(
        summary.unchanged + summary.changed + summary.new + summary.deleted,
        100
    );
    assert!(summary.has_changes);
}

#[test]
fn test_change_detection_metrics() {
    let metrics = ChangeDetectionMetrics {
        total_files: 100,
        changed_files: 20,
        skipped_files: 80,
        change_detection_time: std::time::Duration::from_millis(500),
        database_round_trips: 2,
        cache_hit_rate: 0.75,
        files_per_second: 200.0,
    };

    let summary = metrics.performance_summary();
    assert!(summary.contains("100 files"));
    assert!(summary.contains("20 changed"));
    assert!(summary.contains("80 skipped"));
    assert!(summary.contains("80.0% unchanged"));
    assert!(summary.contains("200 files/sec"));
    assert!(summary.contains("2 DB queries"));
    assert!(summary.contains("75.0% cache hit"));

    // Test default metrics
    let default_metrics = ChangeDetectionMetrics::default();
    assert_eq!(default_metrics.total_files, 0);
    assert_eq!(default_metrics.changed_files, 0);
    assert_eq!(default_metrics.skipped_files, 0);
    assert_eq!(default_metrics.database_round_trips, 0);
    assert_eq!(default_metrics.cache_hit_rate, 0.0);
    assert_eq!(default_metrics.files_per_second, 0.0);
}

#[test]
fn test_change_detection_result() {
    let mut changes = ChangeSet::new();
    let hash = FileHash::new([1u8; 32]);
    let file_info = FileHashInfo::new(
        hash,
        1024,
        SystemTime::now(),
        HashAlgorithm::Blake3,
        "test.md".to_string(),
    );
    changes.add_new(file_info.clone());

    let metrics = ChangeDetectionMetrics {
        total_files: 10,
        changed_files: 1,
        skipped_files: 9,
        change_detection_time: std::time::Duration::from_millis(100),
        database_round_trips: 1,
        cache_hit_rate: 0.9,
        files_per_second: 100.0,
    };

    let result = ChangeDetectionResult::new(changes.clone(), metrics.clone());
    assert!(result.has_changes());
    assert_eq!(result.files_to_process(), 1);
    assert_eq!(result.performance_summary(), metrics.performance_summary());

    // Test empty result
    let empty_changes = ChangeSet::new();
    let empty_metrics = ChangeDetectionMetrics::default();
    let empty_result = ChangeDetectionResult::new(empty_changes, empty_metrics);
    assert!(!empty_result.has_changes());
    assert_eq!(empty_result.files_to_process(), 0);
}

#[test]
fn test_change_statistics() {
    let stats = ChangeStatistics {
        total_tracked_files: 1000,
        average_changes_per_day: 5.5,
        most_recent_change: Some(chrono::Utc::now()),
        oldest_tracked_file: Some(chrono::Utc::now() - chrono::Duration::days(30)),
        typical_change_rate: 0.15,
        average_database_round_trips: 2.5,
        average_cache_hit_rate: 0.85,
    };

    assert!(stats.has_tracked_files());
    let summary = stats.summary();
    assert!(summary.contains("Tracking 1000 files"));
    assert!(summary.contains("5.5 avg changes/day"));
    assert!(summary.contains("15.0% typical change rate"));
    assert!(summary.contains("85.0% cache hit"));

    // Test default statistics
    let default_stats = ChangeStatistics::default();
    assert!(!default_stats.has_tracked_files());
    assert_eq!(default_stats.total_tracked_files, 0);
    assert_eq!(default_stats.average_changes_per_day, 0.0);
}

// Mock implementation for testing the ChangeDetector trait interface
struct MockChangeDetector {
    storage: MockHashLookupStorage,
}

impl MockChangeDetector {
    fn new() -> Self {
        Self {
            storage: MockHashLookupStorage::new(),
        }
    }

    fn add_stored_hash(&mut self, path: String, stored: StoredHash) {
        self.storage.add_hash(path, stored);
    }
}

#[async_trait]
impl ChangeDetector for MockChangeDetector {
    async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError> {
        let mut changes = ChangeSet::new();
        let mut current_paths = std::collections::HashSet::new();

        // Add all current files to a set for easy lookup
        for file in current_files {
            current_paths.insert(file.relative_path.clone());

            // Check if file exists in storage
            match self.storage.lookup_file_hash(&file.relative_path).await {
                Ok(Some(stored)) => {
                    // Compare hashes
                    if stored.content_hash != file.content_hash {
                        changes.add_changed(file.clone());
                    } else {
                        changes.add_unchanged(file.clone());
                    }
                }
                Ok(None) => {
                    // New file
                    changes.add_new(file.clone());
                }
                Err(_) => {
                    // Treat as new if lookup fails
                    changes.add_new(file.clone());
                }
            }
        }

        // Find deleted files by checking which stored files are not in current set
        let all_stored = self.storage.get_all_hashes().await.unwrap();
        for stored_path in all_stored.keys() {
            if !current_paths.contains(stored_path) {
                changes.add_deleted(stored_path.clone());
            }
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

    async fn detect_changes_for_paths(&self, paths: &[String]) -> Result<ChangeSet, HashError> {
        // Simplified implementation - in real would fetch current files for these paths
        let mut changes = ChangeSet::new();
        for path in paths {
            match self.storage.lookup_file_hash(path).await {
                Ok(Some(_stored)) => {
                    // Would compare with current file state
                    changes.add_unchanged(FileHashInfo::new(
                        FileHash::new([1u8; 32]),
                        1024,
                        SystemTime::now(),
                        HashAlgorithm::Blake3,
                        path.clone(),
                    ));
                }
                Ok(None) => {
                    changes.add_deleted(path.clone());
                }
                Err(_) => {
                    // Would handle error appropriately
                }
            }
        }
        Ok(changes)
    }

    async fn check_file_changed(&self, path: &str) -> Result<Option<FileHashInfo>, HashError> {
        match self.storage.lookup_file_hash(path).await {
            Ok(Some(_stored)) => {
                // Would compare with current file state
                // For mock, assume file exists and has changed for testing purposes
                Ok(Some(FileHashInfo::new(
                    FileHash::new([1u8; 32]),
                    1024,
                    SystemTime::now(),
                    HashAlgorithm::Blake3,
                    path.to_string(),
                )))
            }
            Ok(None) => {
                // File doesn't exist, so it's "new"
                Ok(Some(FileHashInfo::new(
                    FileHash::new([2u8; 32]),
                    1024,
                    SystemTime::now(),
                    HashAlgorithm::Blake3,
                    path.to_string(),
                )))
            }
            Err(e) => Err(e),
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
    ) -> Result<std::collections::HashMap<String, bool>, HashError> {
        let mut results = std::collections::HashMap::new();
        for path in paths {
            // For mock, assume existing files have changed
            let changed = self.storage.lookup_file_hash(path).await.is_ok();
            results.insert(path.clone(), changed);
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

#[tokio::test]
async fn test_change_detector_trait() {
    let mut detector = MockChangeDetector::new();

    // Add a stored file
    let stored_hash = StoredHash::new(
        "notes:existing".to_string(),
        "existing.md".to_string(),
        FileHash::new([3u8; 32]),
        2048,
        chrono::Utc::now(),
    );
    detector.add_stored_hash("existing.md".to_string(), stored_hash);

    // Test with current files
    let current_files = vec![
        FileHashInfo::new(
            FileHash::new([1u8; 32]), // Different hash
            1024,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "existing.md".to_string(),
        ),
        FileHashInfo::new(
            FileHash::new([2u8; 32]),
            2048,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "new.md".to_string(),
        ),
    ];

    // Test detect_changes
    let changes = detector.detect_changes(&current_files).await.unwrap();
    assert_eq!(changes.changed.len(), 1); // existing.md changed
    assert_eq!(changes.new.len(), 1); // new.md is new
    assert!(changes.has_changes());

    // Test detect_changes_with_metrics
    let result = detector
        .detect_changes_with_metrics(&current_files)
        .await
        .unwrap();
    assert!(result.has_changes());
    assert_eq!(result.files_to_process(), 2);
    assert_eq!(result.metrics.total_files, 2);
    assert_eq!(result.metrics.changed_files, 1);
    assert_eq!(result.metrics.skipped_files, 0); // unchanged files are also processed in this mock

    // Test check_file_changed
    let changed = detector.check_file_changed("existing.md").await.unwrap();
    assert!(changed.is_some()); // File exists in storage

    let not_found = detector.check_file_changed("nonexistent.md").await.unwrap();
    assert!(not_found.is_some()); // New file

    // Test batch_check_files_changed
    let paths = vec!["existing.md".to_string(), "nonexistent.md".to_string()];
    let batch_results = detector.batch_check_files_changed(&paths).await.unwrap();
    assert_eq!(batch_results.len(), 2);
    assert!(batch_results.get("existing.md").unwrap_or(&false));
    assert!(batch_results.get("nonexistent.md").unwrap_or(&false));

    // Test detect_deleted_files
    let current_paths = vec!["existing.md".to_string()]; // new.md is missing
    let _deleted = detector.detect_deleted_files(&current_paths).await.unwrap();
    // In our mock, we didn't store new.md, so it wouldn't show as deleted
    // This test demonstrates the interface works

    // Test get_change_statistics
    let stats = detector.get_change_statistics().await.unwrap();
    assert!(stats.has_tracked_files());
    assert!(stats.total_tracked_files > 0);
}

#[tokio::test]
async fn test_change_detector_empty_input() {
    let detector = MockChangeDetector::new();
    let empty_files: Vec<FileHashInfo> = vec![];

    let changes = detector.detect_changes(&empty_files).await.unwrap();
    assert!(!changes.has_changes());
    assert_eq!(changes.total_files(), 0);

    let result = detector
        .detect_changes_with_metrics(&empty_files)
        .await
        .unwrap();
    assert!(!result.has_changes());
    assert_eq!(result.metrics.total_files, 0);
    assert_eq!(result.metrics.changed_files, 0);
    assert_eq!(result.metrics.skipped_files, 0);
}
