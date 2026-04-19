//! Mock change detector.

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::traits::change_detection::{
    ChangeDetectionMetrics, ChangeDetectionResult, ChangeDetector, ChangeSet, ChangeStatistics,
    HashLookupStorage,
};
use crate::types::hashing::{FileHash, FileHashInfo, HashAlgorithm, HashError};

use super::hash_lookup::MockHashLookupStorage;

/// Mock change detector for testing
///
/// Provides a simple implementation of change detection that combines
/// mock storage and hashing for comprehensive testing.
#[derive(Debug, Clone)]
pub struct MockChangeDetector {
    storage: MockHashLookupStorage,
}

impl MockChangeDetector {
    /// Create a new mock change detector
    pub fn new() -> Self {
        Self {
            storage: MockHashLookupStorage::new(),
        }
    }

    /// Create with specific storage
    pub fn with_storage(storage: MockHashLookupStorage) -> Self {
        Self { storage }
    }

    /// Get the underlying storage for configuration
    pub fn storage(&self) -> &MockHashLookupStorage {
        &self.storage
    }
}

impl Default for MockChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChangeDetector for MockChangeDetector {
    async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError> {
        let mut changes = ChangeSet::new();
        let mut current_paths = std::collections::HashSet::new();

        for file in current_files {
            current_paths.insert(file.relative_path.clone());

            match self.storage.lookup_file_hash(&file.relative_path).await? {
                Some(stored) => {
                    if stored.content_hash != file.content_hash {
                        changes.add_changed(file.clone());
                    } else {
                        changes.add_unchanged(file.clone());
                    }
                }
                None => {
                    changes.add_new(file.clone());
                }
            }
        }

        // Find deleted files
        let all_stored = self.storage.get_all_hashes().await?;
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
        let start = std::time::Instant::now();
        let changes = self.detect_changes(current_files).await?;
        let elapsed = start.elapsed();

        let metrics = ChangeDetectionMetrics {
            total_files: current_files.len(),
            changed_files: changes.changed.len() + changes.new.len(),
            skipped_files: changes.unchanged.len(),
            change_detection_time: elapsed,
            database_round_trips: 1,
            cache_hit_rate: 0.8,
            files_per_second: current_files.len() as f64 / elapsed.as_secs_f64().max(0.001),
        };

        Ok(ChangeDetectionResult::new(changes, metrics))
    }

    async fn detect_changes_for_paths(&self, paths: &[String]) -> Result<ChangeSet, HashError> {
        let mut changes = ChangeSet::new();

        for path in paths {
            match self.storage.lookup_file_hash(path).await? {
                Some(_) => {
                    // For mock, just mark as unchanged
                    let file_info = FileHashInfo::new(
                        FileHash::new([0u8; 32]),
                        1024,
                        SystemTime::now(),
                        HashAlgorithm::Blake3,
                        path.clone(),
                    );
                    changes.add_unchanged(file_info);
                }
                None => {
                    changes.add_deleted(path.clone());
                }
            }
        }

        Ok(changes)
    }

    async fn check_file_changed(&self, path: &str) -> Result<Option<FileHashInfo>, HashError> {
        match self.storage.lookup_file_hash(path).await? {
            Some(stored) => Ok(Some(stored.to_file_hash_info(HashAlgorithm::Blake3))),
            None => Ok(None),
        }
    }

    async fn get_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<FileHashInfo>, HashError> {
        let stored = self
            .storage
            .lookup_changed_files_since(since, limit)
            .await?;
        Ok(stored
            .into_iter()
            .map(|s| s.to_file_hash_info(HashAlgorithm::Blake3))
            .collect())
    }

    async fn batch_check_files_changed(
        &self,
        paths: &[String],
    ) -> Result<HashMap<String, bool>, HashError> {
        let mut results = HashMap::new();
        for path in paths {
            let exists = self.storage.lookup_file_hash(path).await?.is_some();
            results.insert(path.clone(), exists);
        }
        Ok(results)
    }

    async fn detect_deleted_files(
        &self,
        current_paths: &[String],
    ) -> Result<Vec<String>, HashError> {
        let all_stored = self.storage.get_all_hashes().await?;
        let current_set: std::collections::HashSet<_> = current_paths.iter().collect();

        Ok(all_stored
            .keys()
            .filter(|path| !current_set.contains(path))
            .cloned()
            .collect())
    }

    async fn get_change_statistics(&self) -> Result<ChangeStatistics, HashError> {
        let all_hashes = self.storage.get_all_hashes().await?;

        Ok(ChangeStatistics {
            total_tracked_files: all_hashes.len(),
            average_changes_per_day: 2.5,
            most_recent_change: Some(chrono::Utc::now()),
            oldest_tracked_file: Some(chrono::Utc::now() - chrono::Duration::days(7)),
            typical_change_rate: 0.1,
            average_database_round_trips: 1.5,
            average_cache_hit_rate: 0.8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::change_detection::StoredHash;

    #[tokio::test]
    async fn test_mock_change_detector() {
        let detector = MockChangeDetector::new();

        // Add stored file
        let stored = StoredHash::new(
            "mock:existing_md".to_string(),
            "existing.md".to_string(),
            FileHash::new([1u8; 32]),
            1024,
            chrono::Utc::now(),
        );
        detector
            .storage()
            .add_stored_hash("existing.md".to_string(), stored);

        // Current files: one changed, one new
        let current_files = vec![
            FileHashInfo::new(
                FileHash::new([2u8; 32]), // Different hash
                1024,
                SystemTime::now(),
                HashAlgorithm::Blake3,
                "existing.md".to_string(),
            ),
            FileHashInfo::new(
                FileHash::new([3u8; 32]),
                2048,
                SystemTime::now(),
                HashAlgorithm::Blake3,
                "new.md".to_string(),
            ),
        ];

        // Detect changes
        let changes = detector.detect_changes(&current_files).await.unwrap();
        assert_eq!(changes.changed.len(), 1);
        assert_eq!(changes.new.len(), 1);
        assert!(changes.has_changes());

        // Detect with metrics
        let result = detector
            .detect_changes_with_metrics(&current_files)
            .await
            .unwrap();
        assert!(result.has_changes());
        assert_eq!(result.metrics.total_files, 2);
    }
}
