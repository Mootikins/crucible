use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::hashing::{FileHashInfo, HashError};

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

    pub fn total_files(&self) -> usize {
        self.unchanged.len() + self.changed.len() + self.new.len() + self.deleted.len()
    }

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
        tracing::info!(
            "  ⏭️  Files skipped: {} ({:.1}%)",
            self.skipped_files,
            if self.total_files > 0 {
                (self.skipped_files as f64 / self.total_files as f64) * 100.0
            } else {
                0.0
            }
        );
        tracing::info!(
            "  ⏱️  Change detection time: {:?}",
            self.change_detection_time
        );
        tracing::info!("  🗄️  Database round trips: {}", self.database_round_trips);
        tracing::info!(
            "  🚀 Processing speed: {:.0} files/second",
            self.files_per_second
        );
        tracing::info!("  💾 Cache hit rate: {:.1}%", self.cache_hit_rate * 100.0);

        if self.skipped_files > 0 {
            let time_saved = self
                .change_detection_time
                .mul_f64(self.skipped_files as f64 / self.total_files.max(1) as f64);
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
    pub fn new(changes: ChangeSet, metrics: ChangeDetectionMetrics) -> Self {
        Self { changes, metrics }
    }

    /// Check if any changes were detected
    pub fn has_changes(&self) -> bool {
        self.changes.has_changes()
    }

    pub fn files_to_process(&self) -> usize {
        self.changes.files_to_process()
    }

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
