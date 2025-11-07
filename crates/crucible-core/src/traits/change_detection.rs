//! Traits for content hashing and change detection
//!
//! This module defines the core abstractions for content hashing and change detection
//! throughout the Crucible system. These traits enable dependency inversion by allowing
//! higher-level modules to depend on abstractions rather than concrete implementations.
//!
//! ## Architecture
//!
//! The traits are designed to support the file system operations refactoring:
//! - `ContentHasher`: Pure hashing operations for files and content blocks
//! - `HashLookupStorage`: Database operations for storing and retrieving hashes
//! - `ChangeDetector`: High-level change detection logic
//!
//! ## Usage Pattern
//!
//! ```rust
//! use crucible_core::traits::change_detection::ContentHasher;
//! use std::path::Path;
//!
//! async fn hash_file_example<H: ContentHasher>(hasher: &H, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
//!     let hash = hasher.hash_file(path).await?;
//!     println!("File hash: {}", hash);
//!     Ok(())
//! }
//! ```

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use chrono;
use serde::{Deserialize, Serialize};

use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

/// Trait for content hashing operations
///
/// This trait provides the interface for hashing files and content blocks.
/// Implementations should be thread-safe and support async operations.
/// The trait is object-safe and can be used as a trait object.
///
/// # Design Principles
///
/// - **Async Support**: All methods are async to support non-blocking I/O
/// - **Error Handling**: Comprehensive error handling with specific error types
/// - **Algorithm Agnostic**: Support for multiple hash algorithms
/// - **Object Safety**: Can be used as `dyn ContentHasher`
/// - **Send + Sync**: Safe to use across threads
///
/// # Examples
///
/// ```rust
/// use crucible_core::traits::change_detection::ContentHasher;
/// use crucible_core::types::hashing::{FileHash, HashAlgorithm};
/// use std::path::Path;
///
/// struct MockHasher;
///
/// #[async_trait]
/// impl ContentHasher for MockHasher {
///     async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError> {
///         // Implementation would read file and compute hash
///         todo!()
///     }
///
///     async fn hash_files_batch(&self, paths: &[std::path::PathBuf]) -> Result<Vec<FileHash>, HashError> {
///         // Implementation would hash multiple files in parallel
///         todo!()
///     }
///
///     // ... implement other methods
/// }
/// ```
#[async_trait]
pub trait ContentHasher: Send + Sync {
    /// Get the hash algorithm used by this hasher
    fn algorithm(&self) -> HashAlgorithm;

    /// Hash a single file using streaming I/O
    ///
    /// This method should read the file efficiently using streaming operations
    /// to handle large files without loading everything into memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to hash
    ///
    /// # Returns
    ///
    /// The content hash of the file
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the file cannot be read or hashing fails
    async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError>;

    /// Hash multiple files in parallel
    ///
    /// This method should efficiently hash multiple files, potentially using
    /// parallel processing for better performance on multi-core systems.
    ///
    /// # Arguments
    ///
    /// * `paths` - Slice of file paths to hash
    ///
    /// # Returns
    ///
    /// Vector of hashes in the same order as the input paths
    ///
    /// # Errors
    ///
    /// Returns `HashError` if any file cannot be read or hashing fails
    async fn hash_files_batch(&self, paths: &[std::path::PathBuf]) -> Result<Vec<FileHash>, HashError>;

    /// Hash a content block (e.g., heading, paragraph, code block)
    ///
    /// This method hashes individual content blocks extracted from documents.
    /// It should be fast and efficient since blocks are typically small.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    ///
    /// # Returns
    ///
    /// The block hash
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_block(&self, content: &str) -> Result<BlockHash, HashError>;

    /// Hash multiple blocks in batch
    ///
    /// This method efficiently hashes multiple content blocks, which is useful
    /// when processing a complete document.
    ///
    /// # Arguments
    ///
    /// * `contents` - Vector of content strings to hash
    ///
    /// # Returns
    ///
    /// Vector of block hashes in the same order as input
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_blocks_batch(&self, contents: &[String]) -> Result<Vec<BlockHash>, HashError>;

    /// Create comprehensive file hash info including metadata
    ///
    /// This method hashes a file and includes important metadata for
    /// change detection and file system operations.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to hash
    /// * `relative_path` - Relative path from vault root
    ///
    /// # Returns
    ///
    /// Complete file hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if file operations fail
    async fn hash_file_info(&self, path: &Path, relative_path: String) -> Result<FileHashInfo, HashError>;

    /// Create comprehensive block hash info
    ///
    /// This method hashes a content block and includes metadata for
    /// content addressing and change detection.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    /// * `block_type` - Type of block (heading, paragraph, code, etc.)
    /// * `start_offset` - Start position in source document
    /// * `end_offset` - End position in source document
    ///
    /// # Returns
    ///
    /// Complete block hash information
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn hash_block_info(
        &self,
        content: &str,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<BlockHashInfo, HashError>;

    /// Verify a file's hash matches the expected value
    ///
    /// This method is useful for integrity checking and validation.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the file to verify
    /// * `expected_hash` - Expected hash value
    ///
    /// # Returns
    ///
    /// `true` if the hash matches, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns `HashError` if file operations fail
    async fn verify_file_hash(&self, path: &Path, expected_hash: &FileHash) -> Result<bool, HashError>;

    /// Verify a block's hash matches the expected value
    ///
    /// This method is useful for content integrity checking.
    ///
    /// # Arguments
    ///
    /// * `content` - Content to verify
    /// * `expected_hash` - Expected hash value
    ///
    /// # Returns
    ///
    /// `true` if the hash matches, `false` otherwise
    ///
    /// # Errors
    ///
    /// Returns `HashError` if hashing fails
    async fn verify_block_hash(&self, content: &str, expected_hash: &BlockHash) -> Result<bool, HashError>;
}

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
    /// Create a new StoredHash
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

    /// Get the hash as a hex string for display
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
    /// Create a new empty result
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

    /// Get summary statistics
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
    /// Create a new cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a value from the cache
    pub fn get(&self, key: &str) -> Option<Option<StoredHash>> {
        self.cache.get(key).cloned()
    }

    /// Set a value in the cache
    pub fn set(&mut self, key: String, value: Option<StoredHash>) {
        self.cache.insert(key, value);
    }

    /// Get multiple values from cache, returning which ones are cached and which are not
    pub fn get_cached_keys(&self, keys: &[String]) -> (HashMap<String, Option<StoredHash>>, Vec<String>) {
        let mut cached = HashMap::new();
        let mut uncached = Vec::new();

        for key in keys {
            match self.get(key) {
                Some(value) => {
                    cached.insert(key.clone(), value);
                }
                None => {
                    uncached.push(key.clone());
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

    /// Get cache statistics
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
/// ```rust
/// use crucible_core::traits::change_detection::{
///     HashLookupStorage, HashLookupResult, BatchLookupConfig
/// };
/// use crucible_core::types::hashing::FileHashInfo;
/// use std::collections::HashMap;
///
/// struct MockStorage;
///
/// #[async_trait]
/// impl HashLookupStorage for MockStorage {
///     async fn lookup_file_hash(&self, path: &str) -> Result<Option<StoredHash>, HashError> {
///         // Implementation would query database for single file
///         todo!()
///     }
///
///     async fn lookup_file_hashes_batch(
///         &self,
///         paths: &[String],
///         config: Option<BatchLookupConfig>
///     ) -> Result<HashLookupResult, HashError> {
///         // Implementation would query database for multiple files
///         todo!()
///     }
///
///     // ... implement other methods
/// }
/// ```
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

/// Information about changes detected in a file set
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSet {
    /// Files that have not changed
    pub unchanged: Vec<FileHashInfo>,

    /// Files that have changed (different hash)
    pub changed: Vec<FileHashInfo>,

    /// New files (not previously stored)
    pub new: Vec<FileHashInfo>,

    /// Deleted files (stored but not found in current scan)
    pub deleted: Vec<String>,
}

impl ChangeSet {
    /// Create a new empty ChangeSet
    pub fn new() -> Self {
        Self {
            unchanged: Vec::new(),
            changed: Vec::new(),
            new: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.changed.is_empty() || !self.new.is_empty() || !self.deleted.is_empty()
    }

    /// Get total number of files processed
    pub fn total_files(&self) -> usize {
        self.unchanged.len() + self.changed.len() + self.new.len() + self.deleted.len()
    }

    /// Get number of files that need processing
    pub fn files_to_process(&self) -> usize {
        self.changed.len() + self.new.len() + self.deleted.len()
    }

    /// Add an unchanged file
    pub fn add_unchanged(&mut self, file: FileHashInfo) {
        self.unchanged.push(file);
    }

    /// Add a changed file
    pub fn add_changed(&mut self, file: FileHashInfo) {
        self.changed.push(file);
    }

    /// Add a new file
    pub fn add_new(&mut self, file: FileHashInfo) {
        self.new.push(file);
    }

    /// Add a deleted file
    pub fn add_deleted(&mut self, path: String) {
        self.deleted.push(path);
    }

    /// Get summary statistics
    pub fn summary(&self) -> ChangeSummary {
        ChangeSummary {
            total_files: self.total_files(),
            unchanged: self.unchanged.len(),
            changed: self.changed.len(),
            new: self.new.len(),
            deleted: self.deleted.len(),
            has_changes: self.has_changes(),
        }
    }
}

impl Default for ChangeSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary statistics for a change set
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Total number of files processed
    pub total_files: usize,

    /// Number of unchanged files
    pub unchanged: usize,

    /// Number of changed files
    pub changed: usize,

    /// Number of new files
    pub new: usize,

    /// Number of deleted files
    pub deleted: usize,

    /// Whether any changes were detected
    pub has_changes: bool,
}

/// Performance metrics for change detection operations
///
/// This type provides comprehensive performance information about change detection
/// operations, including timing, cache efficiency, and processing statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChangeDetectionMetrics {
    /// Total number of files scanned
    pub total_files: usize,
    /// Number of files that had changes
    pub changed_files: usize,
    /// Number of files skipped (unchanged)
    pub skipped_files: usize,
    /// Time taken for change detection
    pub change_detection_time: std::time::Duration,
    /// Number of database round trips
    pub database_round_trips: usize,
    /// Cache hit rate (0.0 to 1.0)
    pub cache_hit_rate: f64,
    /// Files processed per second
    pub files_per_second: f64,
}

impl ChangeDetectionMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate performance summary
    pub fn performance_summary(&self) -> String {
        format!(
            "Scanned {} files: {} changed, {} skipped ({:.1}% unchanged) \
             in {:?} ({:.0} files/sec, {} DB queries, {:.1}% cache hit)",
            self.total_files,
            self.changed_files,
            self.skipped_files,
            if self.total_files > 0 {
                (self.skipped_files as f64 / self.total_files as f64) * 100.0
            } else {
                0.0
            },
            self.change_detection_time,
            self.files_per_second,
            self.database_round_trips,
            self.cache_hit_rate * 100.0
        )
    }

    /// Log performance metrics
    pub fn log_metrics(&self) {
        tracing::info!("📊 Change Detection Performance:");
        tracing::info!("  📁 Total files scanned: {}", self.total_files);
        tracing::info!("  📝 Files that changed: {}", self.changed_files);
        tracing::info!("  ⏭️  Files skipped: {} ({:.1}%)",
              self.skipped_files,
              if self.total_files > 0 {
                  (self.skipped_files as f64 / self.total_files as f64) * 100.0
              } else {
                  0.0
              });
        tracing::info!("  ⏱️  Change detection time: {:?}", self.change_detection_time);
        tracing::info!("  🗄️  Database round trips: {}", self.database_round_trips);
        tracing::info!("  🚀 Processing speed: {:.0} files/second", self.files_per_second);
        tracing::info!("  💾 Cache hit rate: {:.1}%", self.cache_hit_rate * 100.0);

        if self.skipped_files > 0 {
            let time_saved = self.change_detection_time.mul_f64(
                self.skipped_files as f64 / self.total_files.max(1) as f64
            );
            tracing::info!("  ⚡ Estimated time saved: {:?}", time_saved);
        }
    }
}

/// Result of a change detection operation with performance metrics
///
/// This type combines the change set with comprehensive performance metrics
/// to provide complete information about the change detection operation.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeDetectionResult {
    /// The detected changes
    pub changes: ChangeSet,
    /// Performance metrics for the operation
    pub metrics: ChangeDetectionMetrics,
}

impl ChangeDetectionResult {
    /// Create a new change detection result
    pub fn new(changes: ChangeSet, metrics: ChangeDetectionMetrics) -> Self {
        Self { changes, metrics }
    }

    /// Check if any changes were detected
    pub fn has_changes(&self) -> bool {
        self.changes.has_changes()
    }

    /// Get the number of files that need processing
    pub fn files_to_process(&self) -> usize {
        self.changes.files_to_process()
    }

    /// Get performance summary
    pub fn performance_summary(&self) -> String {
        self.metrics.performance_summary()
    }

    /// Log performance metrics
    pub fn log_metrics(&self) {
        self.metrics.log_metrics();
    }
}

/// Trait for detecting changes in file sets
///
/// This trait provides the comprehensive interface for high-level change detection
/// by comparing current file states with previously stored hash information. It
/// supports both individual and batch operations with detailed performance metrics.
///
/// # Design Principles
///
/// - **Efficient Comparison**: Compare hashes instead of full content for maximum performance
/// - **Batch Processing**: Handle large file sets efficiently with configurable batch sizes
/// - **Comprehensive Reporting**: Provide detailed change information with performance metrics
/// - **Error Resilience**: Continue processing even if some files fail with graceful degradation
/// - **Performance Monitoring**: Track database round trips, cache hit rates, and processing speed
/// - **Object Safety**: Can be used as `dyn ChangeDetector` for dependency injection
/// - **Send + Sync**: Safe to use across threads
///
/// # Architecture Integration
///
/// This trait is the final piece in the three-trait architecture:
/// - `ContentHasher`: Computes content hashes for files and blocks
/// - `HashLookupStorage`: Stores and retrieves hash information from databases
/// - `ChangeDetector`: High-level change detection using the above services
///
/// # Examples
///
/// ```rust
/// use crucible_core::traits::change_detection::{
///     ChangeDetector, ChangeDetectionResult, FileHashInfo
/// };
/// use crucible_core::types::hashing::{FileHash, HashAlgorithm};
/// use std::time::SystemTime;
///
/// struct MockChangeDetector;
///
/// #[async_trait]
/// impl ChangeDetector for MockChangeDetector {
///     async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError> {
///         // Implementation would compare current files with stored hashes
///         todo!()
///     }
///
///     async fn detect_changes_with_metrics(
///         &self,
///         current_files: &[FileHashInfo]
///     ) -> Result<ChangeDetectionResult, HashError> {
///         // Implementation would provide detailed performance metrics
///         todo!()
///     }
///
///     // ... implement other methods
/// }
/// ```
#[async_trait]
pub trait ChangeDetector: Send + Sync {
    /// Detect changes by comparing current files with stored hashes
    ///
    /// This is the primary method for change detection. It compares the current state
    /// of files with previously stored hash information and categorizes them into
    /// unchanged, changed, new, and deleted files.
    ///
    /// # Arguments
    ///
    /// * `current_files` - Current file hash information to compare
    ///
    /// # Returns
    ///
    /// ChangeSet describing what has changed
    ///
    /// # Errors
    ///
    /// Returns `HashError` if change detection fails
    async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError>;

    /// Detect changes with comprehensive performance metrics
    ///
    /// This method provides the same functionality as `detect_changes` but includes
    /// detailed performance metrics such as database round trips, cache hit rates,
    /// and processing speed. This is useful for monitoring and optimization.
    ///
    /// # Arguments
    ///
    /// * `current_files` - Current file hash information to compare
    ///
    /// # Returns
    ///
    /// ChangeDetectionResult with both changes and performance metrics
    ///
    /// # Errors
    ///
    /// Returns `HashError` if change detection fails
    async fn detect_changes_with_metrics(
        &self,
        current_files: &[FileHashInfo],
    ) -> Result<ChangeDetectionResult, HashError>;

    /// Detect changes for a subset of files efficiently
    ///
    /// This method optimizes for the common case where only a subset of files
    /// need to be checked for changes. It uses targeted queries and avoids
    /// scanning the entire file set.
    ///
    /// # Arguments
    ///
    /// * `paths` - Specific file paths to check for changes
    ///
    /// # Returns
    ///
    /// ChangeSet for the specified paths
    ///
    /// # Errors
    ///
    /// Returns `HashError` if change detection fails
    async fn detect_changes_for_paths(&self, paths: &[String]) -> Result<ChangeSet, HashError>;

    /// Check if a specific file has changed quickly
    ///
    /// This method provides a fast way to check if a single file has changed
    /// since it was last processed. It's optimized for common operations like
    /// file watcher notifications.
    ///
    /// # Arguments
    ///
    /// * `path` - File path to check for changes
    ///
    /// # Returns
    ///
    /// `Some(FileHashInfo)` if the file has changed, `None` if unchanged
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the check fails
    async fn check_file_changed(&self, path: &str) -> Result<Option<FileHashInfo>, HashError>;

    /// Get all files that have changed since a given timestamp
    ///
    /// This method is useful for periodic synchronization operations and
    /// backup systems that need to process changes within a time window.
    ///
    /// # Arguments
    ///
    /// * `since` - Timestamp to find files modified after
    /// * `limit` - Optional limit on number of results
    ///
    /// # Returns
    ///
    /// Vector of file information for files changed since the timestamp
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the query fails
    async fn get_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<FileHashInfo>, HashError>;

    /// Batch check multiple files for changes efficiently
    ///
    /// This method optimizes checking multiple files by using batch database
    /// queries and parallel processing where appropriate. It's designed for
    /// scenarios where many files need to be checked simultaneously.
    ///
    /// # Arguments
    ///
    /// * `paths` - Multiple file paths to check for changes
    ///
    /// # Returns
    ///
    /// HashMap mapping paths to their change status (true if changed)
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the batch check fails
    async fn batch_check_files_changed(
        &self,
        paths: &[String],
    ) -> Result<std::collections::HashMap<String, bool>, HashError>;

    /// Detect deleted files by comparing current files with stored ones
    ///
    /// This method identifies files that were previously stored but are no
    /// longer present in the current file scan. This is important for
    /// cleanup operations and maintaining database consistency.
    ///
    /// # Arguments
    ///
    /// * `current_paths` - Set of currently existing file paths
    ///
    /// # Returns
    ///
    /// Vector of deleted file paths
    ///
    /// # Errors
    ///
    /// Returns `HashError` if the comparison fails
    async fn detect_deleted_files(
        &self,
        current_paths: &[String],
    ) -> Result<Vec<String>, HashError>;

    /// Get comprehensive change statistics
    ///
    /// This method provides statistical information about the change detection
    /// system, including total stored files, average change frequency, and
    /// performance characteristics.
    ///
    /// # Returns
    ///
    /// Statistical information about the change detection system
    ///
    /// # Errors
    ///
    /// Returns `HashError` if statistics cannot be retrieved
    async fn get_change_statistics(&self) -> Result<ChangeStatistics, HashError>;
}

/// Statistics about the change detection system
///
/// This type provides aggregated statistical information about the
/// change detection system, useful for monitoring and optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeStatistics {
    /// Total number of files tracked
    pub total_tracked_files: usize,
    /// Average number of changes per day (over the last 30 days)
    pub average_changes_per_day: f64,
    /// Most recent change timestamp
    pub most_recent_change: Option<chrono::DateTime<chrono::Utc>>,
    /// Oldest tracked file timestamp
    pub oldest_tracked_file: Option<chrono::DateTime<chrono::Utc>>,
    /// Percentage of files that typically change on each scan
    pub typical_change_rate: f64,
    /// Average database round trips per change detection operation
    pub average_database_round_trips: f64,
    /// Average cache hit rate across operations
    pub average_cache_hit_rate: f64,
}

impl ChangeStatistics {
    /// Create new statistics
    pub fn new() -> Self {
        Self {
            total_tracked_files: 0,
            average_changes_per_day: 0.0,
            most_recent_change: None,
            oldest_tracked_file: None,
            typical_change_rate: 0.0,
            average_database_round_trips: 0.0,
            average_cache_hit_rate: 0.0,
        }
    }

    /// Check if the system has any tracked files
    pub fn has_tracked_files(&self) -> bool {
        self.total_tracked_files > 0
    }

    /// Get a summary of the statistics
    pub fn summary(&self) -> String {
        format!(
            "Tracking {} files, {:.1} avg changes/day, {:.1}% typical change rate, {:.1}% cache hit",
            self.total_tracked_files,
            self.average_changes_per_day,
            self.typical_change_rate * 100.0,
            self.average_cache_hit_rate * 100.0
        )
    }
}

impl Default for ChangeStatistics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

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
        result.found_files.insert("found.md".to_string(), stored.clone());
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
        assert_eq!(cache.get("test.md"), None);
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
        assert_eq!(cache.get("test.md"), Some(Some(stored)));

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
        assert_eq!(cache.get("test.md"), None);
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn test_change_set() {
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
        changes.add_changed(file_info.clone());
        changes.add_deleted("old.md".to_string());

        assert!(changes.has_changes());
        assert_eq!(changes.total_files(), 3);
        assert_eq!(changes.files_to_process(), 2);

        let summary = changes.summary();
        assert_eq!(summary.new, 1);
        assert_eq!(summary.changed, 1);
        assert_eq!(summary.deleted, 1);
        assert!(summary.has_changes);
    }

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
        assert_eq!(summary.unchanged + summary.changed + summary.new + summary.deleted, 100);
        assert!(summary.has_changes);
    }

    // Mock implementation for testing the trait interface
    struct MockHashLookupStorage {
        hashes: HashMap<String, StoredHash>,
    }

    impl MockHashLookupStorage {
        fn new() -> Self {
            Self {
                hashes: HashMap::new(),
            }
        }

        fn add_hash(&mut self, path: String, stored: StoredHash) {
            self.hashes.insert(path, stored);
        }
    }

    #[async_trait]
    impl HashLookupStorage for MockHashLookupStorage {
        async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError> {
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
        let batch_result = storage.lookup_file_hashes_batch(&paths, None).await.unwrap();
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
        assert_eq!(changes.new.len(), 1);     // new.md is new
        assert!(changes.has_changes());

        // Test detect_changes_with_metrics
        let result = detector.detect_changes_with_metrics(&current_files).await.unwrap();
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

        let result = detector.detect_changes_with_metrics(&empty_files).await.unwrap();
        assert!(!result.has_changes());
        assert_eq!(result.metrics.total_files, 0);
        assert_eq!(result.metrics.changed_files, 0);
        assert_eq!(result.metrics.skipped_files, 0);
    }
}