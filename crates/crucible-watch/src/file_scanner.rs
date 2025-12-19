//! File scanner for discovering and processing files in the crucible-watch system
//!
//! This module provides the main FileScanner component that serves as the primary
//! interface for file discovery, scanning, and watching operations. It integrates
//! with the crucible-core ContentHasher trait for efficient change detection
//! and provides comprehensive configuration options for various scanning scenarios.
//!
//! ## Architecture
//!
//! The FileScanner is built around dependency injection and uses the ContentHasher
//! trait from crucible-core for all hashing operations. This allows for flexible
//! hashing implementations while maintaining a consistent interface.
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │   FileScanner   │───▶│   ContentHasher  │───▶│   File System   │
//! │                 │    │   (Trait)        │    │   Operations    │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//!         │                       │                       │
//!         ▼                       ▼                       ▼
//! ┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
//! │ ScanConfig      │    │   FileInfo       │    │   ScanResult   │
//! │ (Configuration) │    │   (Metadata)     │    │   (Results)     │
//! └─────────────────┘    └──────────────────┘    └─────────────────┘
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace, warn};

use crate::error::Error;
use crate::types::{
    FileInfo, FileType, ScanConfig, ScanError, ScanErrorType, ScanResult, SkipReason, SkipType,
};

// Import the ContentHasher trait from crucible-core
use crucible_core::traits::change_detection::ContentHasher;
use crucible_core::types::hashing::{FileHash, HashAlgorithm};

/// Progress reporter for file scanning operations
///
/// This trait allows callers to receive progress updates during
/// long-running scan operations.
#[async_trait]
pub trait ScanProgressReporter: Send + Sync {
    /// Report progress for the current scan operation
    ///
    /// # Arguments
    ///
    /// * `current` - Current number of files processed
    /// * `total` - Total number of files to process (None if unknown)
    /// * `current_file` - Path to the file currently being processed
    async fn report_progress(
        &self,
        current: usize,
        total: Option<usize>,
        current_file: Option<&Path>,
    );

    /// Report that scanning has started
    async fn scan_started(&self);

    /// Report that scanning has completed
    ///
    /// # Arguments
    ///
    /// * `result` - The final scan result
    async fn scan_completed(&self, result: &ScanResult);

    /// Report an error that occurred during scanning
    ///
    /// # Arguments
    ///
    /// * `error` - The error that occurred
    async fn scan_error(&self, error: &Error);
}

/// Default no-op progress reporter
pub struct NoOpProgressReporter;

#[async_trait]
impl ScanProgressReporter for NoOpProgressReporter {
    async fn report_progress(
        &self,
        _current: usize,
        _total: Option<usize>,
        _current_file: Option<&Path>,
    ) {
        // No-op implementation
    }

    async fn scan_started(&self) {
        // No-op implementation
    }

    async fn scan_completed(&self, _result: &ScanResult) {
        // No-op implementation
    }

    async fn scan_error(&self, _error: &Error) {
        // No-op implementation
    }
}

/// Configuration for file watching operations
///
/// This type provides configuration for setting up file watching
/// on directories that have been scanned.
#[derive(Debug, Clone)]
pub struct WatchConfig {
    /// Whether to watch for file creations
    pub watch_creations: bool,
    /// Whether to watch for file modifications
    pub watch_modifications: bool,
    /// Whether to watch for file deletions
    pub watch_deletions: bool,
    /// Debounce delay for file events
    pub debounce_delay: Duration,
    /// Whether to watch recursively
    pub recursive: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            watch_creations: true,
            watch_modifications: true,
            watch_deletions: true,
            debounce_delay: Duration::from_millis(100),
            recursive: true,
        }
    }
}

/// Result of a directory watch operation
#[derive(Debug, Clone)]
pub struct WatchResult {
    /// Whether watching was successfully established
    pub success: bool,
    /// Number of files being watched
    pub watched_files: usize,
    /// Watch handle (implementation-specific)
    pub watch_handle: Option<String>,
    /// Any warnings that occurred during setup
    pub warnings: Vec<String>,
}

impl WatchResult {
    /// Create a new successful watch result
    pub fn success(watched_files: usize) -> Self {
        Self {
            success: true,
            watched_files,
            watch_handle: None,
            warnings: Vec::new(),
        }
    }

    /// Create a new failed watch result
    pub fn failure(warnings: Vec<String>) -> Self {
        Self {
            success: false,
            watched_files: 0,
            watch_handle: None,
            warnings,
        }
    }
}

/// Main file scanner for the crucible-watch system
///
/// The FileScanner provides comprehensive file discovery and scanning capabilities
/// with dependency injection for hashing operations. It supports recursive directory
/// scanning, file filtering, progress reporting, and integrates with the existing
/// crucible-watch infrastructure.
///
/// # Design Principles
///
/// - **Dependency Injection**: Uses the ContentHasher trait for flexible hashing
/// - **Async First**: All operations are async for non-blocking behavior
/// - **Error Resilient**: Continues processing even when individual files fail
/// - **Progress Tracking**: Provides detailed progress information for large operations
/// - **Configurable**: Extensive configuration options for different use cases
/// - **Thread Safe**: Safe to use across multiple threads with proper synchronization
///
/// # Examples
///
/// // TODO: Add example once API stabilizes
pub struct FileScanner {
    /// Root directory to scan
    root_path: PathBuf,
    /// Scan configuration
    config: ScanConfig,
    /// Content hasher implementation
    hasher: Arc<dyn ContentHasher>,
    /// Progress reporter
    progress_reporter: Arc<dyn ScanProgressReporter>,
    /// Internal state cache
    state: Arc<RwLock<FileScannerState>>,
}

/// Internal state for the FileScanner
#[derive(Debug, Default)]
struct FileScannerState {
    /// Files discovered in previous scans
    discovered_files: Vec<FileInfo>,
    /// Last scan time
    last_scan_time: Option<SystemTime>,
    /// Scan statistics
    scan_count: u64,
    /// Total files processed across all scans
    total_files_processed: u64,
    /// Total errors encountered across all scans
    total_errors: u64,
}

impl FileScanner {
    /// Create a new FileScanner with the given parameters
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to scan
    /// * `config` - Scan configuration
    /// * `hasher` - Content hasher implementation
    /// * `progress_reporter` - Progress reporter for scan operations
    ///
    /// # Returns
    ///
    /// A new FileScanner instance
    ///
    /// # Errors
    ///
    /// Returns Error if the root path doesn't exist or isn't accessible
    pub fn new(
        root_path: &Path,
        config: ScanConfig,
        hasher: Arc<dyn ContentHasher>,
        progress_reporter: Arc<dyn ScanProgressReporter>,
    ) -> Result<Self, Error> {
        // Validate root path
        if !root_path.exists() {
            return Err(Error::FileIoError {
                path: root_path.to_path_buf(),
                error: "Root path does not exist".to_string(),
            });
        }

        if !root_path.is_dir() {
            return Err(Error::FileIoError {
                path: root_path.to_path_buf(),
                error: "Root path is not a directory".to_string(),
            });
        }

        // Validate configuration
        Self::validate_config(&config)?;

        info!(
            "Creating FileScanner for root path: {:?}, algorithm: {}",
            root_path,
            hasher.algorithm()
        );

        Ok(Self {
            root_path: root_path.to_path_buf(),
            config,
            hasher,
            progress_reporter,
            state: Arc::new(RwLock::new(FileScannerState::default())),
        })
    }

    /// Create a FileScanner with default configuration
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to scan
    /// * `hasher` - Content hasher implementation
    /// * `progress_reporter` - Progress reporter for scan operations
    ///
    /// # Returns
    ///
    /// A new FileScanner with default configuration
    ///
    /// # Errors
    ///
    /// Returns Error if the root path doesn't exist or isn't accessible
    pub fn with_defaults(
        root_path: &Path,
        hasher: Arc<dyn ContentHasher>,
        progress_reporter: Arc<dyn ScanProgressReporter>,
    ) -> Result<Self, Error> {
        Self::new(root_path, ScanConfig::default(), hasher, progress_reporter)
    }

    /// Validate the scan configuration
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration to validate
    ///
    /// # Returns
    ///
    /// Ok(()) if valid, Err(Error) if invalid
    fn validate_config(config: &ScanConfig) -> Result<(), Error> {
        if config.max_file_size == 0 {
            return Err(Error::ValidationError {
                field: "max_file_size".to_string(),
                message: "Maximum file size must be greater than 0".to_string(),
            });
        }

        if let Some(max_depth) = config.max_depth {
            if max_depth == 0 {
                return Err(Error::ValidationError {
                    field: "max_depth".to_string(),
                    message: "Maximum depth must be greater than 0".to_string(),
                });
            }
        }

        // Validate exclude patterns
        for pattern in &config.exclude_patterns {
            if pattern.is_empty() {
                warn!("Empty exclude pattern found, ignoring");
            } else if let Err(e) = glob::Pattern::new(pattern) {
                return Err(Error::ValidationError {
                    field: "exclude_patterns".to_string(),
                    message: format!("Invalid glob pattern '{}': {}", pattern, e),
                });
            }
        }

        Ok(())
    }

    /// Scan the root directory and all subdirectories
    ///
    /// This is the main entry point for directory scanning. It will recursively
    /// scan the root directory according to the configuration, applying all
    /// filters and processing eligible files.
    ///
    /// # Returns
    ///
    /// Comprehensive scan result with discovered files, errors, and statistics
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub async fn scan_directory(&self) -> Result<ScanResult, Error> {
        let start_time = Instant::now();
        self.progress_reporter.scan_started().await;

        info!("Starting directory scan of: {:?}", self.root_path);
        debug!("Scan configuration: {:?}", self.config);

        let mut result = ScanResult::new();
        let mut state = self.state.write().await;

        // Reset for new scan
        result.discovered_files.clear();
        result.skipped_paths.clear();
        result.scan_errors.clear();

        // Perform the actual scan
        if let Err(e) = self
            .scan_directory_recursive(&self.root_path, 0, &mut result)
            .await
        {
            error!("Directory scan failed: {}", e);
            self.progress_reporter.scan_error(&e).await;
            return Err(e);
        }

        // Calculate scan duration
        result.scan_duration = start_time.elapsed();

        // Update statistics
        state.scan_count += 1;
        state.total_files_processed += result.successful_files as u64;
        state.total_errors += result.scan_errors.len() as u64;
        state.last_scan_time = Some(SystemTime::now());
        state.discovered_files = result.discovered_files.clone();

        // Calculate total size
        result.total_size = result
            .discovered_files
            .iter()
            .map(|file| file.file_size())
            .sum();

        // Log completion
        info!(
            "Directory scan completed in {:?}: {} files processed, {} skipped, {} errors",
            result.scan_duration,
            result.successful_files,
            result.skipped_files,
            result.scan_errors.len()
        );

        self.progress_reporter.scan_completed(&result).await;

        Ok(result)
    }

    /// Scan specific files or a list of files
    ///
    /// This method scans only the specified files rather than doing a full
    /// directory scan. It's useful for processing specific files that have
    /// been identified as needing updates.
    ///
    /// # Arguments
    ///
    /// * `files` - List of file paths to scan
    ///
    /// # Returns
    ///
    /// Scan result for the specified files
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub async fn scan_files(&self, files: Vec<PathBuf>) -> Result<ScanResult, Error> {
        let start_time = Instant::now();
        self.progress_reporter.scan_started().await;

        info!("Starting scan of {} specific files", files.len());

        let mut result = ScanResult::new();
        result.total_considered = files.len();

        for (index, file_path) in files.iter().enumerate() {
            // Report progress
            self.progress_reporter
                .report_progress(index, Some(files.len()), Some(file_path))
                .await;

            trace!("Processing file: {:?}", file_path);

            // Check if file should be included
            if let Some(skip_reason) = self.should_skip_file(file_path).await {
                result.skipped_files += 1;
                result.skipped_paths.push(skip_reason);
                continue;
            }

            // Process the file
            match self.process_file(file_path).await {
                Ok(file_info) => {
                    result.successful_files += 1;
                    result.discovered_files.push(file_info);
                }
                Err(e) => {
                    error!("Error processing file {:?}: {}", file_path, e);
                    result.scan_errors.push(ScanError {
                        path: file_path.clone(),
                        error_type: ScanErrorType::IoError,
                        message: e.to_string(),
                    });
                }
            }
        }

        // Final progress report
        self.progress_reporter
            .report_progress(files.len(), Some(files.len()), None)
            .await;

        result.scan_duration = start_time.elapsed();
        result.total_size = result
            .discovered_files
            .iter()
            .map(|file| file.file_size())
            .sum();

        info!(
            "File scan completed in {:?}: {} files processed, {} errors",
            result.scan_duration,
            result.successful_files,
            result.scan_errors.len()
        );

        self.progress_reporter.scan_completed(&result).await;

        Ok(result)
    }

    /// Set up file watching for the root directory
    ///
    /// This method sets up file watching for the root directory using the
    /// specified configuration. Note that this is a placeholder implementation
    /// that would need to be integrated with a specific file watching backend.
    ///
    /// # Arguments
    ///
    /// * `watch_config` - Configuration for file watching
    ///
    /// # Returns
    ///
    /// Watch result indicating success and any warnings
    ///
    /// # Examples
    ///
    /// // TODO: Add example once API stabilizes
    pub async fn watch_directory(&self, watch_config: WatchConfig) -> Result<WatchResult, Error> {
        info!("Setting up file watching for: {:?}", self.root_path);
        debug!("Watch configuration: {:?}", watch_config);

        // This is a placeholder implementation
        // In a real implementation, this would integrate with the file watching backend
        let _warnings = if cfg!(target_os = "windows") {
            vec![
                "File watching may have limited functionality on Windows".to_string(),
                "Consider using the Notify backend for better Windows support".to_string(),
            ]
        } else {
            Vec::new()
        };

        let result = WatchResult::success(0); // Placeholder count

        info!("File watching setup completed. Success: {}", result.success);
        for warning in &result.warnings {
            warn!("File watching warning: {}", warning);
        }

        Ok(result)
    }

    /// Get statistics about previous scans
    ///
    /// # Returns
    ///
    /// Statistics about scan history and performance
    pub async fn get_scan_statistics(&self) -> ScanStatistics {
        let state = self.state.read().await;
        ScanStatistics {
            scan_count: state.scan_count,
            total_files_processed: state.total_files_processed,
            total_errors: state.total_errors,
            last_scan_time: state.last_scan_time,
            discovered_files_count: state.discovered_files.len(),
            root_path: self.root_path.clone(),
            hash_algorithm: self.hasher.algorithm(),
        }
    }

    /// Get the currently discovered files
    ///
    /// # Returns
    ///
    /// Vector of files discovered in the last scan
    pub async fn get_discovered_files(&self) -> Vec<FileInfo> {
        let state = self.state.read().await;
        state.discovered_files.clone()
    }

    /// Rescan only files that have changed since the last scan
    ///
    /// This method uses the file metadata from the last scan to determine
    /// which files need to be rescanned based on modification times and sizes.
    ///
    /// # Returns
    ///
    /// Scan result for changed files only
    pub async fn scan_changed_files(&self) -> Result<ScanResult, Error> {
        let state = self.state.read().await;

        if state.discovered_files.is_empty() {
            warn!("No previous scan data available, performing full scan");
            drop(state);
            return self.scan_directory().await;
        }

        info!("Scanning for changed files since last scan");
        let start_time = Instant::now();
        self.progress_reporter.scan_started().await;

        let mut result = ScanResult::new();
        let mut files_to_check = Vec::new();

        // Collect files that need checking
        for file_info in &state.discovered_files {
            files_to_check.push(file_info.path().to_path_buf());
        }

        result.total_considered = files_to_check.len();
        drop(state); // Release the lock

        // Check each file for changes
        for (index, file_path) in files_to_check.iter().enumerate() {
            self.progress_reporter
                .report_progress(index, Some(files_to_check.len()), Some(file_path))
                .await;

            match self.check_file_changed(file_path).await {
                Ok(Some(file_info)) => {
                    // File has changed
                    result.successful_files += 1;
                    result.discovered_files.push(file_info);
                }
                Ok(None) => {
                    // File hasn't changed
                    // Don't include in result
                }
                Err(e) => {
                    error!("Error checking file {:?}: {}", file_path, e);
                    result.scan_errors.push(ScanError {
                        path: file_path.clone(),
                        error_type: ScanErrorType::IoError,
                        message: e.to_string(),
                    });
                }
            }
        }

        result.scan_duration = start_time.elapsed();
        result.total_size = result
            .discovered_files
            .iter()
            .map(|file| file.file_size())
            .sum();

        info!(
            "Changed files scan completed in {:?}: {} files changed",
            result.scan_duration, result.successful_files
        );

        self.progress_reporter.scan_completed(&result).await;

        Ok(result)
    }

    // Private helper methods

    /// Recursively scan a directory
    fn scan_directory_recursive<'a>(
        &'a self,
        dir_path: &'a Path,
        depth: usize,
        result: &'a mut ScanResult,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), Error>> + Send + 'a>> {
        Box::pin(async move {
            // Check depth limit
            if let Some(max_depth) = self.config.max_depth {
                if depth >= max_depth {
                    debug!("Reached maximum depth {} at {:?}", depth, dir_path);
                    return Ok(());
                }
            }

            trace!("Scanning directory: {:?} (depth: {})", dir_path, depth);

            // Read directory entries
            let entries = match std::fs::read_dir(dir_path) {
                Ok(entries) => entries,
                Err(e) => {
                    error!("Error reading directory {:?}: {}", dir_path, e);
                    result.scan_errors.push(ScanError {
                        path: dir_path.to_path_buf(),
                        error_type: ScanErrorType::IoError,
                        message: format!("Failed to read directory: {}", e),
                    });
                    return Ok(());
                }
            };

            // Process each entry
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(e) => {
                        warn!("Error reading directory entry: {}", e);
                        continue;
                    }
                };

                let path = entry.path();
                result.total_considered += 1;

                // Report progress periodically
                if result.total_considered.is_multiple_of(100) {
                    self.progress_reporter
                        .report_progress(result.total_considered, None, Some(&path))
                        .await;
                }

                // Handle directories
                if path.is_dir() {
                    // Check if we should skip this directory
                    if let Some(skip_reason) = self.should_skip_directory(&path).await {
                        result.skipped_files += 1;
                        result.skipped_paths.push(skip_reason);
                        continue;
                    }

                    // Recursively scan subdirectory
                    if let Err(e) = self
                        .scan_directory_recursive(&path, depth + 1, result)
                        .await
                    {
                        error!("Error scanning subdirectory {:?}: {}", path, e);
                        result.scan_errors.push(ScanError {
                            path: path.clone(),
                            error_type: ScanErrorType::IoError,
                            message: e.to_string(),
                        });
                    }
                    continue;
                }

                // Handle files
                if path.is_file() {
                    // Check if we should skip this file
                    if let Some(skip_reason) = self.should_skip_file(&path).await {
                        result.skipped_files += 1;
                        result.skipped_paths.push(skip_reason);
                        continue;
                    }

                    // Process the file
                    match self.process_file(&path).await {
                        Ok(file_info) => {
                            result.successful_files += 1;
                            result.discovered_files.push(file_info);
                        }
                        Err(e) => {
                            error!("Error processing file {:?}: {}", path, e);
                            result.scan_errors.push(ScanError {
                                path: path.clone(),
                                error_type: ScanErrorType::IoError,
                                message: e.to_string(),
                            });
                        }
                    }
                }
            }

            Ok(())
        })
    }

    /// Check if a directory should be skipped
    async fn should_skip_directory(&self, dir_path: &Path) -> Option<SkipReason> {
        // Check if directory is hidden
        if self.config.ignore_hidden && is_hidden(dir_path) {
            return Some(SkipReason {
                path: dir_path.to_path_buf(),
                reason: SkipType::HiddenFile,
            });
        }

        // Early check for common excluded directories to avoid unnecessary traversal
        if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) {
            if dir_name == "node_modules" || dir_name == "target" || dir_name == ".git" {
                return Some(SkipReason {
                    path: dir_path.to_path_buf(),
                    reason: SkipType::ExcludedPattern(format!("excluded directory: {}", dir_name)),
                });
            }
        }

        // Check exclude patterns
        if self.config.matches_exclude_pattern(dir_path) {
            return Some(SkipReason {
                path: dir_path.to_path_buf(),
                reason: SkipType::ExcludedPattern("directory match".to_string()),
            });
        }

        None
    }

    /// Check if a file should be skipped
    async fn should_skip_file(&self, file_path: &Path) -> Option<SkipReason> {
        // Check if file is hidden
        if self.config.ignore_hidden && is_hidden(file_path) {
            return Some(SkipReason {
                path: file_path.to_path_buf(),
                reason: SkipType::HiddenFile,
            });
        }

        // Get file metadata
        let metadata = match std::fs::metadata(file_path) {
            Ok(metadata) => metadata,
            Err(_) => {
                return Some(SkipReason {
                    path: file_path.to_path_buf(),
                    reason: SkipType::NotAccessible("cannot read metadata".to_string()),
                });
            }
        };

        // Check file size
        let file_size = metadata.len();
        if !self.config.should_include_size(file_size) {
            return Some(SkipReason {
                path: file_path.to_path_buf(),
                reason: SkipType::TooLarge(file_size),
            });
        }

        // Check file type
        let file_type = FileType::from_path(file_path);
        if !self.config.should_include_type(file_type) {
            return Some(SkipReason {
                path: file_path.to_path_buf(),
                reason: SkipType::ExcludedType(file_type),
            });
        }

        // Check exclude patterns
        if self.config.matches_exclude_pattern(file_path) {
            return Some(SkipReason {
                path: file_path.to_path_buf(),
                reason: SkipType::ExcludedPattern("file match".to_string()),
            });
        }

        // Note: We don't skip read-only files because we can still read them for scanning.
        // Read-only just means we can't write to them, which is fine for our purposes.

        None
    }

    /// Process a single file and create FileInfo
    async fn process_file(&self, file_path: &Path) -> Result<FileInfo, Error> {
        trace!("Processing file: {:?}", file_path);

        // Get file metadata
        let metadata = std::fs::metadata(file_path).map_err(|e| Error::FileIoError {
            path: file_path.to_path_buf(),
            error: e.to_string(),
        })?;

        let file_size = metadata.len();
        let modified_time = metadata.modified().map_err(|e| Error::FileIoError {
            path: file_path.to_path_buf(),
            error: e.to_string(),
        })?;

        // Determine relative path from root
        let relative_path = file_path
            .strip_prefix(&self.root_path)
            .map_err(|_| Error::FileIoError {
                path: file_path.to_path_buf(),
                error: "File is not under root path".to_string(),
            })?
            .to_string_lossy()
            .to_string();

        let file_type = FileType::from_path(file_path);

        // Calculate content hash if configured
        let content_hash = if self.config.calculate_hashes {
            self.hasher
                .hash_file(file_path)
                .await
                .map_err(|e| Error::FileIoError {
                    path: file_path.to_path_buf(),
                    error: format!("Hash calculation failed: {}", e),
                })?
        } else {
            FileHash::zero()
        };

        // Create FileInfo
        let file_info = FileInfo::builder()
            .path(file_path.to_path_buf())
            .relative_path(relative_path)
            .content_hash(content_hash)
            .file_size(file_size)
            .modified_time(modified_time)
            .file_type(file_type)
            .is_accessible(true)
            .build() // This cannot fail in our current usage
            .map_err(|e| Error::ValidationError {
                field: "file_info".to_string(),
                message: e.to_string(),
            })?;

        trace!("Successfully processed file: {:?}", file_path);
        Ok(file_info)
    }

    /// Check if a file has changed since the last scan
    async fn check_file_changed(&self, file_path: &Path) -> Result<Option<FileInfo>, Error> {
        // Get current metadata
        let metadata = std::fs::metadata(file_path).map_err(|e| Error::FileIoError {
            path: file_path.to_path_buf(),
            error: e.to_string(),
        })?;

        let current_size = metadata.len();
        let current_modified = metadata.modified().map_err(|e| Error::FileIoError {
            path: file_path.to_path_buf(),
            error: e.to_string(),
        })?;

        // Check against previous scan data
        let state = self.state.read().await;

        if let Some(previous_file) = state
            .discovered_files
            .iter()
            .find(|f| f.path() == file_path)
        {
            // Quick metadata check first
            if previous_file.file_size() == current_size
                && previous_file.modified_time() == current_modified
            {
                return Ok(None); // File hasn't changed
            }

            // Metadata changed, check content hash
            if self.config.calculate_hashes {
                let current_hash =
                    self.hasher
                        .hash_file(file_path)
                        .await
                        .map_err(|e| Error::FileIoError {
                            path: file_path.to_path_buf(),
                            error: format!("Hash calculation failed: {}", e),
                        })?;

                if previous_file.content_hash() == current_hash {
                    return Ok(None); // Content hasn't changed
                }

                // File has changed, create updated FileInfo
                let relative_path = file_path
                    .strip_prefix(&self.root_path)
                    .map_err(|_| Error::FileIoError {
                        path: file_path.to_path_buf(),
                        error: "File is not under root path".to_string(),
                    })?
                    .to_string_lossy()
                    .to_string();

                let file_type = FileType::from_path(file_path);

                let updated_file = FileInfo::builder()
                    .path(file_path.to_path_buf())
                    .relative_path(relative_path)
                    .content_hash(current_hash)
                    .file_size(current_size)
                    .modified_time(current_modified)
                    .file_type(file_type)
                    .is_accessible(true)
                    .build()
                    .map_err(|e| Error::ValidationError {
                        field: "file_info".to_string(),
                        message: e.to_string(),
                    })?;

                return Ok(Some(updated_file));
            }
        }

        // File wasn't in previous scan or hash checking is disabled
        // Process it as a new file
        drop(state);
        let file_info = self.process_file(file_path).await?;
        Ok(Some(file_info))
    }
}

/// Statistics about file scanning operations
#[derive(Debug, Clone)]
pub struct ScanStatistics {
    /// Total number of scans performed
    pub scan_count: u64,
    /// Total files processed across all scans
    pub total_files_processed: u64,
    /// Total errors encountered across all scans
    pub total_errors: u64,
    /// Time of the last scan
    pub last_scan_time: Option<SystemTime>,
    /// Number of files discovered in the last scan
    pub discovered_files_count: usize,
    /// Root path being scanned
    pub root_path: PathBuf,
    /// Hash algorithm being used
    pub hash_algorithm: HashAlgorithm,
}

impl ScanStatistics {
    /// Get the average files per scan
    pub fn average_files_per_scan(&self) -> f64 {
        if self.scan_count == 0 {
            0.0
        } else {
            self.total_files_processed as f64 / self.scan_count as f64
        }
    }

    /// Get the error rate as a percentage
    pub fn error_rate(&self) -> f64 {
        if self.total_files_processed == 0 {
            0.0
        } else {
            (self.total_errors as f64 / self.total_files_processed as f64) * 100.0
        }
    }

    /// Get a summary string
    pub fn summary(&self) -> String {
        format!(
            "Scanner stats: {} scans, {:.1} avg files/scan, {:.1}% error rate, last scan: {:?}",
            self.scan_count,
            self.average_files_per_scan(),
            self.error_rate(),
            self.last_scan_time
        )
    }
}

/// Helper function to check if a path is hidden
///
/// A file is considered hidden if its filename (not any parent directory) starts with a dot.
/// For example:
/// - `/path/to/.hidden_file` -> true (filename starts with dot)
/// - `/tmp/.tmpdir/file.txt` -> false (only parent directory has dot, not the filename)
/// - `/.git/config` -> false (we're checking the file, not the directory)
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

impl std::fmt::Debug for FileScanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileScanner")
            .field("root_path", &self.root_path)
            .field("config", &self.config)
            .field("hasher", &"<ContentHasher>")
            .field("progress_reporter", &"<ScanProgressReporter>")
            .finish()
    }
}
