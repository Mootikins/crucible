use std::collections::HashMap;

use async_trait::async_trait;

use super::super::*;
use crate::types::hashing::{FileHash, FileHashInfo, HashAlgorithm, HashError};

#[test]
fn test_stored_hash() {
    let hash = FileHash::new([1u8; 32]);
    let timestamp = chrono::Utc::now();
    let stored = StoredHash::new(
        "notes:test_file".to_string(),
        "test.md".to_string(),
        hash,
        1024,
        timestamp,
    );

    assert_eq!(stored.record_id, "notes:test_file");
    assert_eq!(stored.relative_path, "test.md");
    assert_eq!(stored.content_hash, hash);
    assert_eq!(stored.file_size, 1024);
    assert_eq!(stored.modified_at, timestamp);
    assert_eq!(stored.hash_hex(), hash.to_hex());

    let file_info = stored.to_file_hash_info(HashAlgorithm::Blake3);
    assert_eq!(file_info.content_hash, hash);
    assert_eq!(file_info.size, 1024);
    assert_eq!(file_info.relative_path, "test.md");
    assert_eq!(file_info.algorithm, HashAlgorithm::Blake3);
}

#[test]
fn test_hash_lookup_result() {
    let mut result = HashLookupResult::new();
    assert!(!result.has_found_files());
    assert!(!result.has_missing_files());
    assert_eq!(result.success_rate(), 1.0); // Empty result has 100% success rate

    // Add some found files
    let hash = FileHash::new([2u8; 32]);
    let stored = StoredHash::new(
        "notes:found".to_string(),
        "found.md".to_string(),
        hash,
        2048,
        chrono::Utc::now(),
    );
    result
        .found_files
        .insert("found.md".to_string(), stored.clone());
    result.missing_files.push("missing.md".to_string());
    result.total_queried = 2;
    result.database_round_trips = 1;

    assert!(result.has_found_files());
    assert!(result.has_missing_files());
    assert_eq!(result.success_rate(), 0.5);

    let summary = result.summary();
    assert_eq!(summary.total_queried, 2);
    assert_eq!(summary.found_files, 1);
    assert_eq!(summary.missing_files, 1);
    assert_eq!(summary.database_round_trips, 1);
    assert_eq!(summary.success_rate, 0.5);
}

#[test]
fn test_batch_lookup_config() {
    let config = BatchLookupConfig::default();
    assert_eq!(config.max_batch_size, 100);
    assert!(config.use_parameterized_queries);
    assert!(config.enable_session_cache);

    let custom_config = BatchLookupConfig {
        max_batch_size: 50,
        use_parameterized_queries: false,
        enable_session_cache: false,
    };
    assert_eq!(custom_config.max_batch_size, 50);
    assert!(!custom_config.use_parameterized_queries);
    assert!(!custom_config.enable_session_cache);
}

#[test]
fn test_hash_lookup_cache() {
    let mut cache = HashLookupCache::new();

    // Test empty cache
    assert_eq!(cache.get("test.md"), CacheEntry::NotCached);
    assert_eq!(cache.stats().entries, 0);

    // Test setting and getting
    let hash = FileHash::new([3u8; 32]);
    let stored = StoredHash::new(
        "notes:test".to_string(),
        "test.md".to_string(),
        hash,
        1024,
        chrono::Utc::now(),
    );

    cache.set("test.md".to_string(), Some(stored.clone()));
    assert_eq!(cache.get("test.md"), CacheEntry::Found(stored));

    // Test batch operations
    let keys = vec!["test.md".to_string(), "missing.md".to_string()];
    let (cached, uncached) = cache.get_cached_keys(&keys);
    assert_eq!(cached.len(), 1);
    assert_eq!(uncached.len(), 1);
    assert!(cached.contains_key("test.md"));
    assert!(uncached.contains(&"missing.md".to_string()));

    // Test cache statistics
    let stats = cache.stats();
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.hits, 0); // get_cached_keys doesn't update hit counter in this test
    assert_eq!(stats.misses, 0);

    // Clear cache
    cache.clear();
    assert_eq!(cache.get("test.md"), CacheEntry::NotCached);
    assert_eq!(cache.stats().entries, 0);
}

// Mock implementation for testing the trait interface
pub(super) struct MockHashLookupStorage {
    pub(super) hashes: HashMap<String, StoredHash>,
}

impl MockHashLookupStorage {
    pub(super) fn new() -> Self {
        Self {
            hashes: HashMap::new(),
        }
    }

    pub(super) fn add_hash(&mut self, path: String, stored: StoredHash) {
        self.hashes.insert(path, stored);
    }
}

#[async_trait]
impl HashLookupStorage for MockHashLookupStorage {
    async fn lookup_file_hash(
        &self,
        relative_path: &str,
    ) -> Result<Option<StoredHash>, HashError> {
        Ok(self.hashes.get(relative_path).cloned())
    }

    async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
        _config: Option<BatchLookupConfig>,
    ) -> Result<HashLookupResult, HashError> {
        let mut result = HashLookupResult::new();
        result.total_queried = relative_paths.len();
        result.database_round_trips = 1;

        for path in relative_paths {
            match self.hashes.get(path) {
                Some(stored) => {
                    result.found_files.insert(path.clone(), stored.clone());
                }
                None => {
                    result.missing_files.push(path.clone());
                }
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
        match self.hashes.get(relative_path) {
            Some(stored) => Ok(stored.content_hash != *new_hash),
            None => Ok(true), // File doesn't exist, needs processing
        }
    }

    async fn store_hashes(&self, _files: &[FileHashInfo]) -> Result<(), HashError> {
        // Mock implementation would store in database
        Ok(())
    }

    async fn remove_hashes(&self, _paths: &[String]) -> Result<(), HashError> {
        // Mock implementation would remove from database
        Ok(())
    }

    async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError> {
        let mut result = HashMap::new();
        for (path, stored) in &self.hashes {
            result.insert(
                path.clone(),
                stored.to_file_hash_info(HashAlgorithm::Blake3),
            );
        }
        Ok(result)
    }

    async fn clear_all_hashes(&self) -> Result<(), HashError> {
        // Mock implementation would clear database
        Ok(())
    }
}

#[tokio::test]
async fn test_hash_lookup_storage_trait() {
    let mut storage = MockHashLookupStorage::new();
    let hash = FileHash::new([4u8; 32]);
    let stored = StoredHash::new(
        "notes:test_trait".to_string(),
        "trait_test.md".to_string(),
        hash,
        4096,
        chrono::Utc::now(),
    );
    storage.add_hash("trait_test.md".to_string(), stored);

    // Test single lookup
    let result = storage.lookup_file_hash("trait_test.md").await.unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().relative_path, "trait_test.md");

    let missing = storage.lookup_file_hash("nonexistent.md").await.unwrap();
    assert!(missing.is_none());

    // Test batch lookup
    let paths = vec![
        "trait_test.md".to_string(),
        "nonexistent.md".to_string(),
        "another_missing.md".to_string(),
    ];
    let batch_result = storage
        .lookup_file_hashes_batch(&paths, None)
        .await
        .unwrap();
    assert_eq!(batch_result.found_files.len(), 1);
    assert_eq!(batch_result.missing_files.len(), 2);
    assert_eq!(batch_result.total_queried, 3);
    assert_eq!(batch_result.database_round_trips, 1);

    // Test check file needs update
    let needs_update_same = storage
        .check_file_needs_update("trait_test.md", &hash)
        .await
        .unwrap();
    assert!(!needs_update_same);

    let different_hash = FileHash::new([5u8; 32]);
    let needs_update_diff = storage
        .check_file_needs_update("trait_test.md", &different_hash)
        .await
        .unwrap();
    assert!(needs_update_diff);

    let needs_update_missing = storage
        .check_file_needs_update("nonexistent.md", &hash)
        .await
        .unwrap();
    assert!(needs_update_missing);

    // Test get all hashes
    let all_hashes = storage.get_all_hashes().await.unwrap();
    assert_eq!(all_hashes.len(), 1);
    assert!(all_hashes.contains_key("trait_test.md"));
}
