//! File scanning module with dependency injection
//!
//! This module provides a high-level interface for file scanning operations
//! using the FileScanner from crucible-watch with proper dependency injection
//! of the FileHasher from crucible-core.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crucible_core::hashing::file_hasher::FileHasher;
use crucible_core::traits::change_detection::ContentHasher;
use crucible_core::types::hashing::HashAlgorithm;
use crucible_watch::{
    FileScanner, FileInfo, FileType, NoOpProgressReporter, ScanConfig, ScanResult, WatchConfig,
};

/// File scanning service with dependency injection
///
/// This struct provides a high-level interface for file scanning operations
/// with proper dependency injection of hashing services.
pub struct FileScanningService {
    /// Root directory to scan
    root_path: PathBuf,
    /// File scanner instance
    scanner: Arc<FileScanner>,
    /// File hasher implementation
    hasher: Arc<dyn ContentHasher>,
}

impl FileScanningService {
    /// Create a new file scanning service
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to scan
    /// * `algorithm` - Hash algorithm to use (Blake3 or Sha256)
    ///
    /// # Returns
    ///
    /// A new FileScanningService instance
    ///
    /// # Errors
    ///
    /// Returns Error if the root path doesn't exist or isn't accessible
    pub fn new(root_path: &Path, algorithm: HashAlgorithm) -> Result<Self, crucible_watch::Error> {
        // Create the hasher implementation
        let hasher = Arc::new(FileHasher::new(algorithm));

        // Create the progress reporter
        let progress_reporter = Arc::new(NoOpProgressReporter);

        // Create scan configuration with sensible defaults
        let scan_config = ScanConfig::default();

        // Create the file scanner with dependency injection
        let scanner = Arc::new(FileScanner::new(
            root_path,
            scan_config,
            hasher.clone(),
            progress_reporter,
        )?);

        info!(
            "Created FileScanningService for: {:?} using {} algorithm",
            root_path,
            algorithm
        );

        Ok(Self {
            root_path: root_path.to_path_buf(),
            scanner,
            hasher,
        })
    }

    /// Create a file scanning service with custom configuration
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root directory to scan
    /// * `algorithm` - Hash algorithm to use
    /// * `scan_config` - Custom scan configuration
    ///
    /// # Returns
    ///
    /// A new FileScanningService instance
    ///
    /// # Errors
    ///
    /// Returns Error if the root path doesn't exist or isn't accessible
    pub fn with_config(
        root_path: &Path,
        algorithm: HashAlgorithm,
        scan_config: ScanConfig,
    ) -> Result<Self, crucible_watch::Error> {
        // Create the hasher implementation
        let hasher = Arc::new(FileHasher::new(algorithm));

        // Create the progress reporter
        let progress_reporter = Arc::new(NoOpProgressReporter);

        // Create the file scanner with dependency injection
        let scanner = Arc::new(FileScanner::new(
            root_path,
            scan_config,
            hasher.clone(),
            progress_reporter,
        )?);

        info!(
            "Created FileScanningService for: {:?} using {} algorithm with custom config",
            root_path,
            algorithm
        );

        Ok(Self {
            root_path: root_path.to_path_buf(),
            scanner,
            hasher,
        })
    }

    /// Scan the entire directory tree
    ///
    /// This method performs a complete scan of the directory tree according
    /// to the configuration, discovering and processing all eligible files.
    ///
    /// # Returns
    ///
    /// Comprehensive scan result with discovered files, errors, and statistics
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use crucible_cli::common::file_scanner::FileScanningService;
    /// use crucible_core::types::hashing::HashAlgorithm;
    /// use std::path::Path;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let service = FileScanningService::new(Path::new("/my/kiln"), HashAlgorithm::Blake3)?;
    ///     let result = service.scan_directory().await?;
    ///
    ///     println!("Found {} files", result.successful_files);
    ///     Ok(())
    /// }
    /// ```
    pub async fn scan_directory(&self) -> Result<ScanResult, crucible_watch::Error> {
        info!("Starting directory scan of: {:?}", self.root_path);
        let start_time = std::time::Instant::now();

        let result = self.scanner.scan_directory().await?;

        let elapsed = start_time.elapsed();
        info!(
            "Directory scan completed in {:?}: {} files processed, {} skipped, {} errors",
            elapsed,
            result.successful_files,
            result.skipped_files,
            result.scan_errors.len()
        );

        Ok(result)
    }

    /// Scan specific files only
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
    pub async fn scan_files(&self, files: Vec<PathBuf>) -> Result<ScanResult, crucible_watch::Error> {
        info!("Scanning {} specific files", files.len());
        let start_time = std::time::Instant::now();

        let result = self.scanner.scan_files(files).await?;

        let elapsed = start_time.elapsed();
        info!(
            "File scan completed in {:?}: {} files processed, {} errors",
            elapsed,
            result.successful_files,
            result.scan_errors.len()
        );

        Ok(result)
    }

    /// Scan only files that have changed since the last scan
    ///
    /// This method uses the file metadata from the last scan to determine
    /// which files need to be rescanned based on modification times and sizes.
    ///
    /// # Returns
    ///
    /// Scan result for changed files only
    pub async fn scan_changed_files(&self) -> Result<ScanResult, crucible_watch::Error> {
        info!("Scanning for changed files since last scan");
        let start_time = std::time::Instant::now();

        let result = self.scanner.scan_changed_files().await?;

        let elapsed = start_time.elapsed();
        info!(
            "Changed files scan completed in {:?}: {} files changed",
            elapsed,
            result.successful_files
        );

        Ok(result)
    }

    /// Set up file watching for the root directory
    ///
    /// This method sets up file watching for the root directory using the
    /// specified configuration.
    ///
    /// # Arguments
    ///
    /// * `watch_config` - Configuration for file watching
    ///
    /// # Returns
    ///
    /// Watch result indicating success and any warnings
    pub async fn watch_directory(&self, watch_config: WatchConfig) -> Result<crucible_watch::WatchResult, crucible_watch::Error> {
        info!("Setting up file watching for: {:?}", self.root_path);

        let result = self.scanner.watch_directory(watch_config).await?;

        if result.success {
            info!("File watching established successfully for {} files", result.watched_files);
        } else {
            warn!("File watching setup failed with {} warnings", result.warnings.len());
            for warning in &result.warnings {
                warn!("File watching warning: {}", warning);
            }
        }

        Ok(result)
    }

    /// Get statistics about previous scans
    ///
    /// # Returns
    ///
    /// Statistics about scan history and performance
    pub async fn get_scan_statistics(&self) -> crucible_watch::ScanStatistics {
        self.scanner.get_scan_statistics().await
    }

    /// Get the currently discovered files from the last scan
    ///
    /// # Returns
    ///
    /// Vector of files discovered in the last scan
    pub async fn get_discovered_files(&self) -> Vec<FileInfo> {
        self.scanner.get_discovered_files().await
    }

    /// Get the hash algorithm being used
    pub fn algorithm(&self) -> HashAlgorithm {
        self.hasher.algorithm()
    }

    /// Get the root path being scanned
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }
}

/// Convenience functions for creating common scanning configurations

/// Create a default scanning configuration for a typical kiln
pub fn default_kiln_scan_config() -> ScanConfig {
    ScanConfig::default()
}

/// Create a scanning configuration optimized for performance
pub fn performance_scan_config() -> ScanConfig {
    let mut config = ScanConfig::default();
    // Enable hash calculation for change detection
    config.calculate_hashes = true;
    // Include common file types
    config.include_types = vec![
        FileType::Markdown,
        FileType::Text,
        FileType::Code,
    ];
    // Exclude common non-content files
    config.exclude_patterns = vec![
        "*.git*".to_string(),
        "node_modules/**".to_string(),
        "target/**".to_string(),
        "*.log".to_string(),
        "*.tmp".to_string(),
    ];
    config
}

/// Create a scanning configuration for development environments
pub fn development_scan_config() -> ScanConfig {
    let mut config = performance_scan_config();
    // Lower max file size for development (exclude large binaries)
    config.max_file_size = 10 * 1024 * 1024; // 10MB
    // Shallower depth for faster scans during development
    config.max_depth = Some(10);
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::io::Write;

    #[tokio::test]
    async fn test_file_scanning_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();

        assert_eq!(service.root_path(), temp_dir.path());
        assert_eq!(service.algorithm(), HashAlgorithm::Blake3);
    }

    #[tokio::test]
    async fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();

        let result = service.scan_directory().await.unwrap();
        assert_eq!(result.successful_files, 0);
        assert!(result.is_successful());
    }

    #[tokio::test]
    async fn test_scan_directory_with_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(temp_dir.path().join("test.md"), "# Test Content").unwrap();
        fs::write(temp_dir.path().join("code.rs"), "fn main() {}").unwrap();

        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();
        let result = service.scan_directory().await.unwrap();

        // Should find at least the files we created
        assert!(result.successful_files >= 2);
        assert!(result.is_successful());
    }

    #[tokio::test]
    async fn test_scan_specific_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create test files
        let file1 = temp_dir.path().join("file1.md");
        let file2 = temp_dir.path().join("file2.md");
        fs::write(&file1, "# File 1").unwrap();
        fs::write(&file2, "# File 2").unwrap();

        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();

        let files_to_scan = vec![file1.clone(), file2.clone()];
        let result = service.scan_files(files_to_scan).await.unwrap();

        assert_eq!(result.total_considered, 2);
        assert!(result.is_successful());
    }

    #[tokio::test]
    async fn test_get_scan_statistics() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();

        // Perform a scan first
        service.scan_directory().await.unwrap();

        let stats = service.get_scan_statistics().await;
        assert_eq!(stats.scan_count, 1);
        assert_eq!(stats.root_path, temp_dir.path());
        assert_eq!(stats.hash_algorithm, HashAlgorithm::Blake3);
    }

    #[test]
    fn test_scan_configurations() {
        let default_config = default_kiln_scan_config();
        let perf_config = performance_scan_config();
        let dev_config = development_scan_config();

        // Performance config should enable hash calculation
        assert!(perf_config.calculate_hashes);
        assert!(perf_config.include_types.len() > 0);
        assert!(perf_config.exclude_patterns.len() > 0);

        // Development config should have lower max file size
        assert!(dev_config.max_file_size < default_config.max_file_size);
        assert!(dev_config.max_depth.is_some());
    }

    #[tokio::test]
    async fn test_watch_directory() {
        let temp_dir = TempDir::new().unwrap();
        let service = FileScanningService::new(temp_dir.path(), HashAlgorithm::Blake3).unwrap();

        let watch_config = WatchConfig::default();
        let result = service.watch_directory(watch_config).await.unwrap();

        // Should succeed (though this is a placeholder implementation)
        assert!(result.success);
    }
}