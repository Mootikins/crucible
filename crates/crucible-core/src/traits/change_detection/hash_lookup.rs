use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::hashing::{FileHash, FileHashInfo, HashAlgorithm, HashError};

/// Information about a stored file hash from the database
///
/// This type represents hash information retrieved from storage, including
/// both the hash data and metadata needed for change detection operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredHash {
    /// The database record ID (implementation-specific, e.g., "notes:Projects_file_md")
    pub record_id: String,
    /// The relative file path from the vault root (e.g., "Projects/file.md")
    pub relative_path: String,
    /// The stored content hash
    pub content_hash: FileHash,
    /// File size in bytes
    pub file_size: u64,
    /// Last modification timestamp
    pub modified_at: chrono::DateTime<chrono::Utc>,
}

impl StoredHash {
    pub fn new(
        record_id: String,
        relative_path: String,
        content_hash: FileHash,
        file_size: u64,
        modified_at: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        Self {
            record_id,
            relative_path,
            content_hash,
            file_size,
            modified_at,
        }
    }

    /// Convert to FileHashInfo
    pub fn to_file_hash_info(&self, algorithm: HashAlgorithm) -> FileHashInfo {
        FileHashInfo::new(
            self.content_hash,
            self.file_size,
            self.modified_at.into(),
            algorithm,
            self.relative_path.clone(),
        )
    }

    pub fn hash_hex(&self) -> String {
        self.content_hash.to_hex()
    }
}

/// Result of a hash lookup operation
///
/// This type provides comprehensive information about the results of a batch
/// hash lookup operation, including performance metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct HashLookupResult {
    /// Files that were found in the database with their stored hashes
    pub found_files: HashMap<String, StoredHash>, // Key: relative_path
    /// Files that were not found in the database
    pub missing_files: Vec<String>, // relative_path values
    /// Total number of files queried
    pub total_queried: usize,
    /// Number of database round trips performed
    pub database_round_trips: usize,
}

impl HashLookupResult {
    pub fn new() -> Self {
        Self {
            found_files: HashMap::new(),
            missing_files: Vec::new(),
            total_queried: 0,
            database_round_trips: 0,
        }
    }

    /// Check if any files were found
    pub fn has_found_files(&self) -> bool {
        !self.found_files.is_empty()
    }

    /// Check if any files were missing
    pub fn has_missing_files(&self) -> bool {
        !self.missing_files.is_empty()
    }

    /// Get the success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_queried == 0 {
            1.0
        } else {
            self.found_files.len() as f64 / self.total_queried as f64
        }
    }

    pub fn summary(&self) -> HashLookupSummary {
        HashLookupSummary {
            total_queried: self.total_queried,
            found_files: self.found_files.len(),
            missing_files: self.missing_files.len(),
            database_round_trips: self.database_round_trips,
            success_rate: self.success_rate(),
        }
    }
}

impl Default for HashLookupResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for hash lookup operations
#[derive(Debug, Clone, PartialEq)]
pub struct HashLookupSummary {
    /// Total number of files queried
    pub total_queried: usize,
    /// Number of files found
    pub found_files: usize,
    /// Number of files missing
    pub missing_files: usize,
    /// Number of database round trips
    pub database_round_trips: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f64,
}

/// Batch hash lookup configuration
///
/// This type provides configuration options for batch hash lookup operations,
/// allowing optimization for different database systems and use cases.
#[derive(Debug, Clone)]
pub struct BatchLookupConfig {
    /// Maximum number of files to query in a single database round trip
    pub max_batch_size: usize,
    /// Whether to use parameterized queries (recommended for security)
    pub use_parameterized_queries: bool,
    /// Whether to cache results during the scanning session
    pub enable_session_cache: bool,
}

impl Default for BatchLookupConfig {
    fn default() -> Self {
        Self {
            // Most databases handle IN clauses well up to a few hundred items
            // 100 is a safe default that balances performance and memory
            max_batch_size: 100,
            use_parameterized_queries: true,
            enable_session_cache: true,
        }
    }
}

/// Represents the three possible states of a cache lookup.
///
/// - `NotCached`: The key has never been inserted into the cache.
/// - `Found(StoredHash)`: The key was cached and the file exists with this hash.
/// - `NotFound`: The key was explicitly cached as "file does not exist".
#[derive(Debug, Clone, PartialEq)]
pub enum CacheEntry {
    NotCached,
    Found(StoredHash),
    NotFound,
}

/// Session cache for hash lookups during scanning
///
/// This type provides an in-memory cache for hash lookup results during
/// a scanning session to reduce database queries and improve performance.
#[derive(Debug, Clone, Default)]
pub struct HashLookupCache {
    cache: HashMap<String, Option<StoredHash>>,
    hits: u64,
    misses: u64,
}

impl HashLookupCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &str) -> CacheEntry {
        match self.cache.get(key) {
            None => CacheEntry::NotCached,
            Some(None) => CacheEntry::NotFound,
            Some(Some(hash)) => CacheEntry::Found(hash.clone()),
        }
    }

    pub fn set(&mut self, key: String, value: Option<StoredHash>) {
        self.cache.insert(key, value);
    }

    /// Get multiple values from cache, returning which ones are cached and which are not
    pub fn get_cached_keys(
        &self,
        keys: &[String],
    ) -> (HashMap<String, Option<StoredHash>>, Vec<String>) {
        let mut cached = HashMap::new();
        let mut uncached = Vec::new();

        for key in keys {
            match self.get(key) {
                CacheEntry::NotCached => {
                    uncached.push(key.clone());
                }
                CacheEntry::Found(hash) => {
                    cached.insert(key.clone(), Some(hash));
                }
                CacheEntry::NotFound => {
                    cached.insert(key.clone(), None);
                }
            }
        }

        (cached, uncached)
    }

    /// Cache multiple values
    pub fn set_batch(&mut self, values: HashMap<String, Option<StoredHash>>) {
        for (key, value) in values {
            self.set(key, value);
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in cache
    pub entries: usize,
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Hit rate as a percentage (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Trait for storing and retrieving hash information
///
/// This trait defines the comprehensive interface for persistent storage of hash data,
/// enabling change detection by comparing current and previous hash values. The trait
/// supports both individual and batch operations for optimal performance.
///
/// # Design Principles
///
/// - **Efficient Lookup**: Support for batch hash retrieval with configurable batch sizes
/// - **Atomic Operations**: All operations should be atomic where possible
/// - **Error Recovery**: Comprehensive error handling with specific error types
/// - **Performance Optimized**: Support for bulk operations and caching
/// - **Flexible Queries**: Support for various query patterns (by path, by hash, by timestamp)
/// - **Object Safety**: Can be used as `dyn HashLookupStorage`
/// - **Send + Sync**: Safe to use across threads
///
/// # Examples
///
#[async_trait]
pub trait HashLookupStorage: Send + Sync {
    /// Lookup a single file hash by relative path
    ///
    /// This method should efficiently retrieve hash information for a single file.
    ///
    /// # Arguments
    ///
    /// * `relative_path` - Relative path of the file to lookup
    ///
    /// # Returns
    ///
    /// `Some(StoredHash)` if found, `None` if not found
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError>;

    /// Lookup file hashes for multiple files in batches for optimal performance
    ///
    /// This method should efficiently handle large numbers of file paths by processing
    /// them in configurable batches to avoid overwhelming the database.
    ///
    /// # Arguments
    ///
    /// * `relative_paths` - Vector of relative file paths to lookup
    /// * `config` - Optional batch configuration for performance tuning
    ///
    /// # Returns
    ///
    /// Comprehensive result including found files, missing files, and performance metrics
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
        config: Option<BatchLookupConfig>,
    ) -> Result<HashLookupResult, HashError>;

    /// Lookup files by their content hashes (for finding duplicate content)
    ///
    /// This method should find all files that have the specified content hashes,
    /// which is useful for deduplication and content-based queries.
    ///
    /// # Arguments
    ///
    /// * `content_hashes` - Vector of content hashes to search for
    ///
    /// # Returns
    ///
    /// HashMap mapping content hashes to vectors of files with that hash
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn lookup_files_by_content_hashes(
        &self,
        content_hashes: &[FileHash],
    ) -> Result<HashMap<String, Vec<StoredHash>>, HashError>;

    /// Get all files that have changed since a given timestamp
    ///
    /// This method should find all files modified after the specified timestamp,
    /// which is useful for incremental updates and change detection.
    ///
    /// # Arguments
    ///
    /// * `since` - Timestamp to find files modified after
    /// * `limit` - Optional limit on number of results
    ///
    /// # Returns
    ///
    /// Vector of files modified since the given timestamp
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn lookup_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredHash>, HashError>;

    /// Check if a file needs to be reprocessed based on hash comparison
    ///
    /// This method should compare the stored hash of a file with a provided hash
    /// to determine if the file needs to be reprocessed.
    ///
    /// # Arguments
    ///
    /// * `relative_path` - Relative path of the file to check
    /// * `new_hash` - New hash to compare against stored value
    ///
    /// # Returns
    ///
    /// `true` if the file needs updating, `false` if unchanged
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn check_file_needs_update(
        &self,
        relative_path: &str,
        new_hash: &FileHash,
    ) -> Result<bool, HashError>;

    /// Store hash information for multiple files
    ///
    /// This method should atomically store hash information for multiple
    /// files. Existing entries should be updated with new values.
    ///
    /// # Arguments
    ///
    /// * `files` - Vector of file hash information to store
    ///
    /// # Errors
    ///
    /// Returns `HashError` if storage operations fail
    async fn store_hashes(&self, files: &[FileHashInfo]) -> Result<(), HashError>;

    /// Remove hash information for specific files
    ///
    /// This method should remove hash information for the specified
    /// file paths. Missing files should not cause an error.
    ///
    /// # Arguments
    ///
    /// * `paths` - Vector of file paths to remove
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn remove_hashes(&self, paths: &[String]) -> Result<(), HashError>;

    /// Get all stored hash information
    ///
    /// This method should return all stored hash information, which
    /// can be useful for complete synchronization operations.
    ///
    /// # Returns
    ///
    /// HashMap of all stored hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError>;

    /// Clear all stored hash information
    ///
    /// This method should remove all stored hash information. Use with
    /// caution as this is a destructive operation.
    ///
    /// # Errors
    ///
    /// Returns `HashError` if database operations fail
    async fn clear_all_hashes(&self) -> Result<(), HashError>;
}
