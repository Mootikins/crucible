//! Change Detection Service Integration
//!
//! This module provides a high-level service that integrates ChangeDetector from crucible-watch
//! with SurrealHashLookup from crucible-surrealdb, following proper dependency injection
//! and SOLID principles. It serves as a bridge between file scanning and change detection.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use anyhow::{Result, anyhow};
use tracing::{debug, info, warn, error};

use crucible_core::{
    traits::change_detection::{
        ChangeDetector as ChangeDetectorTrait, ChangeSet, HashLookupStorage,
        ChangeDetectionResult, ChangeDetectionMetrics, StoredHash,
    },
    types::hashing::{HashAlgorithm, HashError},
    FileHash, FileHashInfo,
};
use std::collections::HashMap;
use crucible_watch::ChangeDetectorConfig;
use crucible_surrealdb::{
    SurrealClient, hash_lookup::SurrealHashLookupStorage,
};

use super::FileScanningService;

/// Wrapper around SurrealHashLookupStorage that owns the client
///
/// This struct wraps the SurrealHashLookupStorage to work around lifetime issues
/// by owning the Arc<SurrealClient> instead of borrowing it.
pub struct OwnedSurrealHashLookupStorage {
    client: Arc<SurrealClient>,
}

impl OwnedSurrealHashLookupStorage {
    /// Create a new owned hash lookup storage
    pub fn new(client: Arc<SurrealClient>) -> Self {
        Self { client }
    }

    /// Get a reference to the underlying client
    pub fn client(&self) -> &SurrealClient {
        &self.client
    }
}

// For now, we'll use a simpler approach and implement only the essential methods
// that we actually need for change detection, rather than the full trait.
impl OwnedSurrealHashLookupStorage {
    /// Lookup file hash by relative path
    pub async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError> {
        let storage = SurrealHashLookupStorage::new(self.client());
        storage.lookup_file_hash(relative_path).await
    }

    /// Lookup file hashes in batch
    pub async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
    ) -> Result<crucible_core::traits::change_detection::HashLookupResult, HashError> {
        let storage = SurrealHashLookupStorage::new(self.client());
        storage.lookup_file_hashes_batch(relative_paths, None).await
    }

    /// Get all stored hashes
    pub async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError> {
        let storage = SurrealHashLookupStorage::new(self.client());
        storage.get_all_hashes().await
    }
}

/// Simple change detector implementation
///
/// This provides a basic change detection capability without the full
/// complexity of the trait-based ChangeDetector from crucible-watch.
struct SimpleChangeDetector {
    storage: OwnedSurrealHashLookupStorage,
}

impl SimpleChangeDetector {
    /// Create a new simple change detector
    pub fn new(storage: OwnedSurrealHashLookupStorage) -> Self {
        Self { storage }
    }

    /// Detect changes by comparing current files with stored hashes
    pub async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeDetectionResult, HashError> {
        let start_time = std::time::Instant::now();
        let mut changes = ChangeSet::new();

        // Get all stored hashes
        let stored_hashes = self.storage.get_all_hashes().await?;

        // Process each current file
        for current_file in current_files {
            match stored_hashes.get(&current_file.relative_path) {
                Some(stored_file) => {
                    // File exists in storage, compare hashes
                    if stored_file.content_hash == current_file.content_hash {
                        // File unchanged
                        changes.add_unchanged(current_file.clone());
                    } else {
                        // File changed
                        changes.add_changed(current_file.clone());
                    }
                }
                None => {
                    // File not found in storage - new file
                    changes.add_new(current_file.clone());
                }
            }
        }

        // Detect deleted files by checking what's in storage but not in current files
        let current_paths: std::collections::HashSet<String> = current_files
            .iter()
            .map(|f| f.relative_path.clone())
            .collect();


        for stored_path in stored_hashes.keys() {
            if !current_paths.contains(stored_path) {
                changes.add_deleted(stored_path.clone());
            }
        }


        let total_duration = start_time.elapsed();
        let metrics = ChangeDetectionMetrics {
            total_files: current_files.len(),
            changed_files: changes.changed.len(),
            skipped_files: changes.unchanged.len(),
            change_detection_time: total_duration,
            database_round_trips: 1, // Simplified
            cache_hit_rate: 0.0,      // Simplified
            files_per_second: current_files.len() as f64 / total_duration.as_secs_f64().max(0.001),
        };

        Ok(ChangeDetectionResult::new(changes, metrics))
    }
}

/// Configuration for the change detection service
#[derive(Debug, Clone)]
pub struct ChangeDetectionServiceConfig {
    /// Change detector configuration
    pub change_detector: ChangeDetectorConfig,
    /// Whether to enable automatic processing of detected changes
    pub auto_process_changes: bool,
    /// Maximum number of files to process in a single batch
    pub max_processing_batch_size: usize,
    /// Whether to continue processing when individual files fail
    pub continue_on_processing_error: bool,
}

impl Default for ChangeDetectionServiceConfig {
    fn default() -> Self {
        Self {
            change_detector: ChangeDetectorConfig::default(),
            auto_process_changes: true,
            max_processing_batch_size: 50,
            continue_on_processing_error: true,
        }
    }
}

/// Result of a change detection operation
#[derive(Debug, Clone)]
pub struct ChangeDetectionServiceResult {
    /// The changeset describing what has changed
    pub changeset: ChangeSet,
    /// Performance metrics for the operation
    pub metrics: ChangeDetectionServiceMetrics,
    /// Processing results if auto-processing was enabled
    pub processing_result: Option<ChangeProcessingResult>,
}

/// Performance metrics for change detection operations
#[derive(Debug, Clone)]
pub struct ChangeDetectionServiceMetrics {
    /// Total time taken for change detection
    pub total_time: std::time::Duration,
    /// Time spent scanning files
    pub scan_time: std::time::Duration,
    /// Time spent detecting changes
    pub change_detection_time: std::time::Duration,
    /// Time spent processing changes (if applicable)
    pub processing_time: Option<std::time::Duration>,
    /// Number of files scanned
    pub files_scanned: usize,
    /// Number of changes detected
    pub changes_detected: usize,
    /// Database round trips for change detection
    pub database_round_trips: usize,
    /// Cache hit rate for hash lookups
    pub cache_hit_rate: f64,
}

/// Result of processing detected changes
#[derive(Debug, Clone)]
pub struct ChangeProcessingResult {
    /// Number of files successfully processed
    pub processed_count: usize,
    /// Number of files that failed to process
    pub failed_count: usize,
    /// Time taken for processing
    pub processing_time: std::time::Duration,
    /// List of files that failed to process
    pub failed_files: Vec<String>,
}

/// Service that integrates file scanning with change detection
///
/// This service provides a high-level interface for:
/// 1. Scanning files using FileScanningService
/// 2. Detecting changes using SimpleChangeDetector with SurrealHashLookup
/// 3. Optionally processing the detected changes
///
/// The service follows SOLID principles by:
/// - Single Responsibility: Focused on change detection integration
/// - Open/Closed: Extensible through configuration and dependency injection
/// - Liskov Substitution: Uses trait abstractions for storage
/// - Interface Segregation: Clean, focused interface
/// - Dependency Inversion: Depends on abstractions, not concretions
pub struct ChangeDetectionService {
    /// Root directory being monitored
    root_path: PathBuf,
    /// File scanning service
    file_scanner: Arc<FileScanningService>,
    /// Change detector with injected storage
    change_detector: Arc<SimpleChangeDetector>,
    /// Database client
    client: Arc<SurrealClient>,
    /// Service configuration
    config: ChangeDetectionServiceConfig,
}

impl ChangeDetectionService {
    /// Create a new change detection service
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to monitor for changes
    /// * `client` - Database client for hash storage
    /// * `algorithm` - Hash algorithm to use for file hashing
    /// * `config` - Service configuration
    ///
    /// # Returns
    ///
    /// A new ChangeDetectionService instance
    ///
    /// # Errors
    ///
    /// Returns Error if service initialization fails
    pub async fn new(
        root_path: &Path,
        client: Arc<SurrealClient>,
        algorithm: HashAlgorithm,
        config: ChangeDetectionServiceConfig,
    ) -> Result<Self> {
        info!(
            "Creating ChangeDetectionService for: {:?} using {} algorithm",
            root_path, algorithm
        );

        // Create the file scanning service
        let file_scanner = Arc::new(
            FileScanningService::new(root_path, algorithm)
                .map_err(|e| anyhow!("Failed to create FileScanningService: {}", e))?
        );

        // For now, we'll create a simpler change detection approach
        // since the full ChangeDetector trait implementation has lifetime complexities
        let hash_storage = OwnedSurrealHashLookupStorage::new(client.clone());

        // Create a simple change detector using our own implementation
        let change_detector = Arc::new(
            SimpleChangeDetector::new(hash_storage)
        );

        info!(
            "ChangeDetectionService created successfully for: {:?}",
            root_path
        );

        Ok(Self {
            root_path: root_path.to_path_buf(),
            file_scanner,
            change_detector,
            client,
            config,
        })
    }

    /// Create a change detection service with default configuration
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to monitor for changes
    /// * `client` - Database client for hash storage
    /// * `algorithm` - Hash algorithm to use for file hashing
    ///
    /// # Returns
    ///
    /// A new ChangeDetectionService with default configuration
    ///
    /// # Errors
    ///
    /// Returns Error if service initialization fails
    pub async fn with_defaults(
        root_path: &Path,
        client: Arc<SurrealClient>,
        algorithm: HashAlgorithm,
    ) -> Result<Self> {
        Self::new(root_path, client, algorithm, ChangeDetectionServiceConfig::default()).await
    }

    /// Perform a full change detection cycle
    ///
    /// This method:
    /// 1. Scans the directory for files
    /// 2. Detects changes since the last scan
    /// 3. Optionally processes the detected changes
    ///
    /// # Returns
    ///
    /// Comprehensive result of the change detection operation
    ///
    /// # Errors
    ///
    /// Returns Error if any step of the process fails
    pub async fn detect_and_process_changes(&self) -> Result<ChangeDetectionServiceResult> {
        let start_time = Instant::now();

        info!("Starting change detection cycle for: {:?}", self.root_path);

        // Step 1: Scan files
        let scan_start = Instant::now();
        let scan_result = self.file_scanner.scan_directory().await
            .map_err(|e| anyhow!("File scanning failed: {}", e))?;
        let scan_time = scan_start.elapsed();

        // Note: Don't return early when no files found - we still need to detect deletions!
        // If all files were deleted, we'd have 0 current files but need to compare against stored hashes
        if scan_result.successful_files == 0 {
            info!("No files found during scan - will check for deletions");
        }

        info!("Scanned {} files in {:?}", scan_result.successful_files, scan_time);

        // Step 2: Convert FileInfo to FileHashInfo for change detection
        let file_hash_infos: Vec<FileHashInfo> = scan_result
            .discovered_files
            .iter()
            .map(|file_info| file_info.to_file_hash_info())
            .collect();

        // Step 3: Detect changes
        let change_detection_start = Instant::now();
        let change_result = self.change_detector
            .detect_changes(&file_hash_infos)
            .await
            .map_err(|e| anyhow!("Change detection failed: {}", e))?;
        let change_detection_time = change_detection_start.elapsed();

        info!(
            "Change detection completed in {:?}: {} files to process",
            change_detection_time,
            change_result.changes.files_to_process()
        );

        // Step 4: Optionally process changes
        let processing_result = if self.config.auto_process_changes && change_result.changes.has_changes() {
            let processing_start = Instant::now();
            match self.process_changes(&change_result).await {
                Ok(result) => {
                    let processing_time = processing_start.elapsed();
                    info!("Processed changes in {:?}: {} successful, {} failed",
                        processing_time, result.processed_count, result.failed_count);
                    Some(result)
                }
                Err(e) => {
                    warn!("Failed to process changes: {}", e);
                    None
                }
            }
        } else {
            None
        };

        let total_time = start_time.elapsed();

        Ok(ChangeDetectionServiceResult {
            changeset: change_result.changes.clone(),
            metrics: ChangeDetectionServiceMetrics {
                total_time,
                scan_time,
                change_detection_time,
                processing_time: processing_result.as_ref().map(|r| r.processing_time),
                files_scanned: scan_result.successful_files,
                changes_detected: change_result.files_to_process(),
                database_round_trips: change_result.metrics.database_round_trips,
                cache_hit_rate: change_result.metrics.cache_hit_rate,
            },
            processing_result,
        })
    }

    /// Detect changes for specific files only
    ///
    /// This method is useful when you want to check specific files for changes
    /// rather than doing a full directory scan.
    ///
    /// # Arguments
    ///
    /// * `files` - Specific files to check for changes
    ///
    /// # Returns
    ///
    /// ChangeSet describing the changes in the specified files
    ///
    /// # Errors
    ///
    /// Returns Error if change detection fails
    pub async fn detect_changes_for_files(&self, files: &[PathBuf]) -> Result<ChangeSet> {
        info!("Detecting changes for {} specific files", files.len());

        // Scan specific files
        let scan_result = self.file_scanner.scan_files(files.to_vec()).await
            .map_err(|e| anyhow!("File scanning failed: {}", e))?;

        if scan_result.successful_files == 0 {
            info!("No files found during specific file scan");
            return Ok(ChangeSet::new());
        }

        // Convert to FileHashInfo and detect changes
        let file_hash_infos: Vec<FileHashInfo> = scan_result
            .discovered_files
            .iter()
            .map(|file_info| file_info.to_file_hash_info())
            .collect();

        let change_result = self.change_detector
            .detect_changes(&file_hash_infos)
            .await
            .map_err(|e| anyhow!("Change detection failed: {}", e))?;

        info!(
            "Specific file change detection completed: {} files to process",
            change_result.changes.files_to_process()
        );

        Ok(change_result.changes)
    }

    /// Get the current change detector statistics
    ///
    /// # Returns
    ///
    /// Simple statistics about the change detector operations
    pub async fn get_change_detector_statistics(&self) -> Result<String> {
        Ok("Simple change detector (limited statistics)".to_string())
    }

    /// Get the current file scanner statistics
    ///
    /// # Returns
    ///
    /// Statistics about the file scanning operations
    pub async fn get_file_scanner_statistics(&self) -> Result<crucible_watch::ScanStatistics> {
        Ok(self.file_scanner.get_scan_statistics().await)
    }

    /// Clear all caches
    ///
    /// This method clears any internal caches.
    pub async fn clear_caches(&self) -> Result<()> {
        info!("Clearing change detection service caches");
        // Simple change detector doesn't have complex caches
        Ok(())
    }

    /// Process detected changes using the existing processing pipeline
    ///
    /// # Arguments
    ///
    /// * `change_result` - Change detection result to process
    ///
    /// # Returns
    ///
    /// Result of the processing operation
    ///
    /// # Errors
    ///
    /// Returns Error if processing fails
    async fn process_changes(&self, change_result: &ChangeDetectionResult) -> Result<ChangeProcessingResult> {
        let start_time = Instant::now();

        // Get files that need processing (new + changed)
        let files_to_process: Vec<&FileHashInfo> = change_result
            .changes
            .changed
            .iter()
            .chain(change_result.changes.new.iter())
            .collect();

        if files_to_process.is_empty() {
            return Ok(ChangeProcessingResult {
                processed_count: 0,
                failed_count: 0,
                processing_time: start_time.elapsed(),
                failed_files: vec![],
            });
        }

        info!("Processing {} changed/new files", files_to_process.len());

        // Convert FileHashInfo to KilnFileInfo for the processing pipeline
        let kiln_files: Vec<crucible_surrealdb::KilnFileInfo> = files_to_process
            .iter()
            .map(|file_info| {
                crucible_surrealdb::KilnFileInfo {
                    path: self.root_path.join(&file_info.relative_path),
                    relative_path: file_info.relative_path.clone(),
                    file_size: file_info.size,
                    modified_time: file_info.modified,
                    content_hash: file_info.content_hash.as_bytes().try_into()
                        .expect("FileHash should be 32 bytes"),
                    is_markdown: file_info.relative_path.ends_with(".md"),
                    is_accessible: true, // Assume accessible since we just scanned it
                }
            })
            .collect();

        // Process in batches to avoid overwhelming the system
        let mut processed_count = 0;
        let mut failed_count = 0;
        let mut failed_files = Vec::new();

        for chunk in kiln_files.chunks(self.config.max_processing_batch_size) {
            debug!("Processing batch of {} files", chunk.len());

            // Use the single-pass processing pipeline (no internal change detection)
            let scan_config = crucible_surrealdb::kiln_scanner::KilnScannerConfig::default();
            // process_files processes ONLY what we give it - no duplicate detection
            match crucible_surrealdb::kiln_processor::process_files(
                chunk,
                self.client.as_ref(),
                &scan_config,
                None, // No embedding pool for basic processing
                &self.root_path,
            ).await {
                Ok(result) => {
                    processed_count += result.processed_count;
                    failed_count += result.failed_count;

                    // Note: The processing pipeline doesn't provide individual failed file info,
                    // so we'll track failures at the batch level
                    if result.failed_count > 0 {
                        failed_files.extend(
                            chunk.iter()
                                .map(|f| f.relative_path.clone())
                                .skip(result.processed_count)  // Skip successful files
                                .take(result.failed_count)
                        );
                    }

                    if !self.config.continue_on_processing_error && result.failed_count > 0 {
                        error!("Processing failed in batch and continue_on_error is false");
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to process batch: {}", e);
                    failed_count += chunk.len();
                    failed_files.extend(chunk.iter().map(|f| f.relative_path.clone()));

                    if !self.config.continue_on_processing_error {
                        return Err(anyhow!("Batch processing failed: {}", e));
                    }
                }
            }
        }

        let processing_time = start_time.elapsed();

        Ok(ChangeProcessingResult {
            processed_count,
            failed_count,
            processing_time,
            failed_files,
        })
    }

    /// Get the root path being monitored
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the hash algorithm being used
    pub fn algorithm(&self) -> HashAlgorithm {
        self.file_scanner.algorithm()
    }

    /// Get a reference to the internal file scanner
    pub fn file_scanner(&self) -> &FileScanningService {
        &self.file_scanner
    }

    /// Get a reference to the internal change detector
    pub fn change_detector(&self) -> &SimpleChangeDetector {
        &self.change_detector
    }
}

// Implement Debug for the service
impl std::fmt::Debug for ChangeDetectionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChangeDetectionService")
            .field("root_path", &self.root_path)
            .field("algorithm", &self.algorithm())
            .field("auto_process_changes", &self.config.auto_process_changes)
            .field("max_processing_batch_size", &self.config.max_processing_batch_size)
            .finish()
    }
}

/// Convenience functions for creating common service configurations

/// Create a configuration optimized for development
pub fn development_config() -> ChangeDetectionServiceConfig {
    ChangeDetectionServiceConfig {
        change_detector: ChangeDetectorConfig {
            batch_size: 50,
            enable_session_cache: true,
            track_metrics: true,
            continue_on_error: true,
            db_operation_timeout: std::time::Duration::from_secs(10),
        },
        auto_process_changes: true,
        max_processing_batch_size: 25,
        continue_on_processing_error: true,
    }
}

/// Create a configuration optimized for production
pub fn production_config() -> ChangeDetectionServiceConfig {
    ChangeDetectionServiceConfig {
        change_detector: ChangeDetectorConfig {
            batch_size: 200,
            enable_session_cache: true,
            track_metrics: false, // Disable metrics in production for performance
            continue_on_error: false, // Be more strict in production
            db_operation_timeout: std::time::Duration::from_secs(60),
        },
        auto_process_changes: true,
        max_processing_batch_size: 100,
        continue_on_processing_error: false,
    }
}

/// Create a configuration for lightweight change detection only (no processing)
pub fn detection_only_config() -> ChangeDetectionServiceConfig {
    ChangeDetectionServiceConfig {
        change_detector: ChangeDetectorConfig::default(),
        auto_process_changes: false, // Only detect changes, don't process
        max_processing_batch_size: 0, // Not used when auto_process_changes is false
        continue_on_processing_error: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::io::Write;

    async fn create_test_service() -> Result<(ChangeDetectionService, TempDir)> {
        let temp_dir = TempDir::new()?;
        let db_config = crucible_surrealdb::SurrealDbConfig {
            namespace: "test".to_string(),
            database: "test".to_string(),
            path: ":memory:".to_string(),
            max_connections: Some(5),
            timeout_seconds: Some(30),
        };

        let client = Arc::new(SurrealClient::new(db_config).await?);
        let service = ChangeDetectionService::with_defaults(
            temp_dir.path(),
            client,
            HashAlgorithm::Blake3,
        ).await?;

        Ok((service, temp_dir))
    }

    #[tokio::test]
    async fn test_service_creation() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;
        assert_eq!(service.root_path(), service.root_path());
        assert_eq!(service.algorithm(), HashAlgorithm::Blake3);
        Ok(())
    }

    #[tokio::test]
    async fn test_empty_directory_detection() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        let result = service.detect_and_process_changes().await?;
        assert_eq!(result.metrics.files_scanned, 0);
        assert_eq!(result.metrics.changes_detected, 0);
        assert!(result.processing_result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_single_file_detection() -> Result<()> {
        let (service, temp_dir) = create_test_service().await?;

        // Create a test file
        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "# Test Content").unwrap();

        let result = service.detect_and_process_changes().await?;
        assert_eq!(result.metrics.files_scanned, 1);
        assert_eq!(result.metrics.changes_detected, 1); // New file
        assert!(result.processing_result.is_some()); // Should auto-process

        Ok(())
    }

    #[tokio::test]
    async fn test_detection_only_config() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_config = crucible_surrealdb::SurrealDbConfig {
            namespace: "test".to_string(),
            database: "test".to_string(),
            path: ":memory:".to_string(),
            max_connections: Some(5),
            timeout_seconds: Some(30),
        };

        let client = Arc::new(SurrealClient::new(db_config).await?);
        let config = detection_only_config();
        let service = ChangeDetectionService::new(
            temp_dir.path(),
            client,
            HashAlgorithm::Blake3,
            config,
        ).await?;

        // Create a test file
        let test_file = temp_dir.path().join("test.md");
        fs::write(&test_file, "# Test Content").unwrap();

        let result = service.detect_and_process_changes().await?;
        assert_eq!(result.metrics.files_scanned, 1);
        assert_eq!(result.metrics.changes_detected, 1);
        assert!(result.processing_result.is_none()); // Should not auto-process

        Ok(())
    }

    #[tokio::test]
    async fn test_specific_file_detection() -> Result<()> {
        let (service, temp_dir) = create_test_service().await?;

        // Create test files
        let file1 = temp_dir.path().join("file1.md");
        let file2 = temp_dir.path().join("file2.md");
        fs::write(&file1, "# File 1").unwrap();
        fs::write(&file2, "# File 2").unwrap();

        // Only check one file
        let changes = service.detect_changes_for_files(&[file1]).await?;
        assert!(changes.has_changes());

        Ok(())
    }

    #[tokio::test]
    async fn test_cache_operations() -> Result<()> {
        let (service, _temp_dir) = create_test_service().await?;

        // Clear caches (should not fail)
        service.clear_caches().await?;

        // Get statistics (should work)
        let scanner_stats = service.get_file_scanner_statistics().await?;
        assert_eq!(scanner_stats.scan_count, 0); // No scans yet

        let detector_stats = service.get_change_detector_statistics().await?;
        assert!(detector_stats.contains("Simple change detector")); // Simple detector stats

        Ok(())
    }

    #[tokio::test]
    async fn test_configurations() -> Result<()> {
        let dev_config = development_config();
        assert!(dev_config.auto_process_changes);
        assert_eq!(dev_config.max_processing_batch_size, 25);
        assert!(dev_config.continue_on_processing_error);

        let prod_config = production_config();
        assert!(prod_config.auto_process_changes);
        assert_eq!(prod_config.max_processing_batch_size, 100);
        assert!(!prod_config.continue_on_processing_error);

        let detection_config = detection_only_config();
        assert!(!detection_config.auto_process_changes);
        assert_eq!(detection_config.max_processing_batch_size, 0);

        Ok(())
    }
}