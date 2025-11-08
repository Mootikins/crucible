//! Kiln processing utilities for CLI
//!
//! Provides functionality for integrated kiln processing using the
//! single-binary architecture without external daemon processes.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Kiln processor for integrated file processing
pub struct KilnProcessor {
    progress_bar: Option<ProgressBar>,
}

impl Default for KilnProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl KilnProcessor {
    /// Create a new kiln processor
    pub fn new() -> Self {
        Self { progress_bar: None }
    }

    /// Check if embeddings exist in the database
    pub async fn check_embeddings_exist(
        &self,
        client: &crucible_surrealdb::SurrealClient,
    ) -> Result<bool> {
        info!("Checking if embeddings exist in database...");

        match crucible_surrealdb::kiln_integration::get_database_stats(client).await {
            Ok(stats) => {
                info!("Found {} embeddings in database", stats.total_embeddings);
                Ok(stats.total_embeddings > 0)
            }
            Err(e) => {
                warn!("Failed to get database stats: {}", e);
                // Fallback to direct query if stats function fails
                let embeddings_sql = "SELECT count() as total FROM embeddings LIMIT 1";
                let result = client
                    .query(embeddings_sql, &[])
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to query embeddings: {}", e))?;

                let embeddings_count = result
                    .records
                    .first()
                    .and_then(|r| r.data.get("total"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                info!(
                    "Found {} embeddings in database (fallback query)",
                    embeddings_count
                );
                Ok(embeddings_count > 0)
            }
        }
    }

    /// Process kiln files directly using the integrated processing pipeline
    /// This replaces the legacy daemon spawning with in-process file processing
    pub async fn process_kiln_integrated(
        &mut self,
        kiln_path: &std::path::Path,
        client: &crucible_surrealdb::SurrealClient,
    ) -> Result<ProcessingResult> {
        info!("Starting integrated kiln processing...");

        // Validate kiln path exists
        if !kiln_path.exists() {
            return Err(anyhow::anyhow!(
                "Kiln path '{}' does not exist or is not accessible",
                kiln_path.display()
            ));
        }

        // Create progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg:.cyan} [{elapsed_precise}]")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Scanning kiln files...");
        pb.enable_steady_tick(Duration::from_millis(100));
        self.progress_bar = Some(pb);

        let start_time = Instant::now();

        // Use the integrated processing pipeline
        let config = crucible_surrealdb::kiln_scanner::KilnScannerConfig::default();
        let files = crucible_surrealdb::kiln_processor::scan_kiln_directory(
            &std::path::PathBuf::from(kiln_path),
            &config,
        )
        .await?;

        if let Some(pb) = &self.progress_bar {
            pb.set_message(format!("Processing {} files...", files.len()));
        }

        // Process files using the integrated pipeline
        let result = crucible_surrealdb::kiln_processor::process_kiln_files(
            &files, client, &config, None, // No embedding pool for basic processing
            kiln_path,
        )
        .await;

        let elapsed = start_time.elapsed();

        match result {
            Ok(process_result) => {
                if let Some(pb) = self.progress_bar.take() {
                    let message = format!(
                        "Processed {} files successfully in {:.1}s",
                        process_result.processed_count,
                        elapsed.as_secs_f64()
                    );
                    pb.finish_with_message(message);
                    self.progress_bar = Some(pb);
                }

                let processing_result = ProcessingResult {
                    success: process_result.failed_count == 0,
                    exit_code: if process_result.failed_count == 0 {
                        Some(0)
                    } else {
                        Some(1)
                    },
                    processing_time: elapsed,
                    wait_time: elapsed,
                    kiln_path: Some(kiln_path.to_string_lossy().to_string()),
                };

                if processing_result.success {
                    info!(
                        "Integrated processing completed successfully: {} files in {:.1}s",
                        process_result.processed_count,
                        elapsed.as_secs_f64()
                    );
                    Ok(processing_result)
                } else {
                    warn!(
                        "Integrated processing completed with {} failures out of {} files",
                        process_result.failed_count,
                        files.len()
                    );
                    Ok(processing_result)
                }
            }
            Err(e) => {
                if let Some(pb) = self.progress_bar.take() {
                    pb.abandon_with_message("Processing failed".to_string());
                    self.progress_bar = Some(pb);
                }
                error!("Integrated processing failed: {}", e);
                Err(anyhow::anyhow!("Integrated processing failed: {}", e))
            }
        }
    }

    /// Update progress message during processing
    pub fn update_progress(&mut self, message: String) {
        if let Some(pb) = &self.progress_bar {
            pb.set_message(message);
        }
    }

    /// Clean up progress bar
    pub fn cleanup(&mut self) {
        if let Some(pb) = self.progress_bar.take() {
            pb.finish_and_clear();
        }
    }
}

impl Drop for KilnProcessor {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Result from kiln processoring operation
#[derive(Debug, Clone)]
pub struct ProcessingResult {
    /// Whether the processing was successful
    pub success: bool,
    /// Exit code from kiln processor
    pub exit_code: Option<i32>,
    /// Total processing time
    pub processing_time: Duration,
    /// Time spent waiting for process
    pub wait_time: Duration,
    /// Kiln path that was processed
    pub kiln_path: Option<String>,
}

impl ProcessingResult {
    /// Get human-readable status message
    pub fn status_message(&self) -> String {
        if self.success {
            format!(
                "Kiln processed successfully ({:.1}s)",
                self.processing_time.as_secs_f64()
            )
        } else {
            format!("Processing failed (exit code: {:?})", self.exit_code)
        }
    }

    /// Get detailed processing information
    pub fn processing_info(&self) -> String {
        match &self.kiln_path {
            Some(path) => format!(
                "Processed kiln: {} (took {:.1}s)",
                path,
                self.processing_time.as_secs_f64()
            ),
            None => format!(
                "Processing completed in {:.1}s",
                self.processing_time.as_secs_f64()
            ),
        }
    }
}

/// Process all pending files on startup using integrated blocking processing
///
/// This function implements the single-binary architecture by:
/// 1. Scanning for all files that need processing
/// 2. Processing them immediately using the integrated pipeline
/// 3. Waiting for completion before returning
/// 4. Providing progress feedback to the user
/// 5. Using BatchAwareSurrealClient for consistency
///
/// This replaces the legacy daemon-based file watching with in-process processing.
pub async fn process_files_on_startup(config: &crate::config::CliConfig) -> Result<()> {
    use std::time::Instant;

    // Check if file processing is enabled (default: true)
    if !config.file_watching.enabled {
        debug!("File processing disabled by configuration");
        return Ok(());
    }

    info!(
        "🚀 Starting integrated file processing for kiln: {}",
        config.kiln.path.display()
    );

    let start_time = Instant::now();

    // Initialize database connection
    let db_config = crucible_surrealdb::SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = match crucible_surrealdb::SurrealClient::new(db_config).await {
        Ok(client) => client,
        Err(e) => {
            warn!("Failed to connect to database for file processing: {}", e);
            info!("Continuing without file processing - database unavailable");
            return Ok(()); // Graceful degradation
        }
    };

    // Use ChangeDetectionService for proper single-pass architecture
    use crate::common::{ChangeDetectionService, ChangeDetectionServiceConfig};
    use crucible_core::HashAlgorithm;
    use std::sync::Arc;

    // Create change detection service
    let service = match ChangeDetectionService::new(
        &config.kiln.path,
        Arc::new(client.clone()),
        HashAlgorithm::Blake3,
        ChangeDetectionServiceConfig {
            change_detector: crucible_watch::ChangeDetectorConfig::default(),
            auto_process_changes: true, // Process changes automatically
            continue_on_processing_error: true, // Continue on errors
            max_processing_batch_size: 10,
        },
    )
    .await
    {
        Ok(service) => service,
        Err(e) => {
            error!("Failed to create change detection service: {}", e);
            return Err(anyhow::anyhow!(
                "Change detection initialization failed: {}",
                e
            ));
        }
    };

    // Detect and process changes in a single pass
    let result = match service.detect_and_process_changes().await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to detect and process changes: {}", e);
            return Err(anyhow::anyhow!("File processing failed: {}", e));
        }
    };

    // Check if any changes were detected
    if result.metrics.changes_detected == 0 {
        info!("✅ All files are up to date");
        // Explicitly drop the client to ensure database connections are closed
        drop(client);
        return Ok(());
    }

    // Extract processing results
    let (processed_count, failed_count) = if let Some(processing_result) = &result.processing_result
    {
        (
            processing_result.processed_count,
            processing_result.failed_count,
        )
    } else {
        (0, 0)
    };

    info!(
        "🔄 Processed {} file(s) with {} failures...",
        processed_count, failed_count
    );

    let total_time = start_time.elapsed();

    info!(
        "🎯 File processing completed: {} changes detected, {} processed, {} failed in {:?}",
        result.metrics.changes_detected, processed_count, failed_count, total_time
    );

    if failed_count > 0 {
        warn!("Some files failed to process - check logs for details");
    }

    // Explicitly drop the client to ensure all database connections are closed
    // This prevents "lock hold by current process" errors when CLI commands
    // try to create new database connections after file processing
    drop(client);

    Ok(())
}

/// Run file watcher in background task
async fn run_file_watcher(
    kiln_path: std::path::PathBuf,
    watcher_config: crate::config::FileWatcherConfig,
    pending_files: std::sync::Arc<std::sync::RwLock<std::collections::HashSet<std::path::PathBuf>>>,
) {
    use crate::watcher::{get_fix_instructions, SimpleFileWatcher};

    let debounce_ms = watcher_config.debounce_ms;

    // Create channel for watch events
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    // Create watcher (with pre-flight checks)
    match SimpleFileWatcher::new(&kiln_path, watcher_config, event_tx) {
        Ok(_watcher) => {
            info!("✓ File watching enabled (debounce: {}ms)", debounce_ms);

            // Handle events - this keeps the watcher alive
            if let Err(e) = handle_watch_events(event_rx, pending_files).await {
                error!("File watcher event handler error: {}", e);
            }
        }
        Err(e) => {
            // Graceful degradation: log warning but continue
            warn!("⚠ File watching unavailable: {}", e);
            warn!("  Crucible will continue without real-time updates.");
            warn!("  You can still use all features normally.");

            // Provide actionable fix instructions
            let fix = get_fix_instructions(&e);
            if !fix.is_empty() {
                warn!("\n  How to fix:\n  {}", fix.replace('\n', "\n  "));
            }
        }
    }
}

/// Handle watch events in background task - queues files for processing
async fn handle_watch_events(
    mut event_rx: mpsc::UnboundedReceiver<crate::watcher::WatchEvent>,
    pending_files_queue: std::sync::Arc<
        std::sync::RwLock<std::collections::HashSet<std::path::PathBuf>>,
    >,
) -> Result<()> {
    use std::collections::HashSet;
    use tokio::time::{sleep, Duration};

    debug!("File watcher event handler started");

    // Batch events locally to avoid processing the same file multiple times
    let mut local_pending: HashSet<std::path::PathBuf> = HashSet::new();
    let batch_window = Duration::from_secs(1);

    loop {
        // Wait for first event or timeout
        tokio::select! {
            Some(event) = event_rx.recv() => {
                // Add file to local pending set
                match event {
                    crate::watcher::WatchEvent::Changed(path) |
                    crate::watcher::WatchEvent::Created(path) => {
                        debug!("Queued for processing: {}", path.display());
                        local_pending.insert(path);
                    }
                    crate::watcher::WatchEvent::Deleted(path) => {
                        debug!("File deleted, removing from processing queue: {}", path.display());
                        local_pending.remove(&path);
                    }
                }

                // Collect more events for batch window
                sleep(batch_window).await;

                // Drain any additional events that arrived during wait
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        crate::watcher::WatchEvent::Changed(path) |
                        crate::watcher::WatchEvent::Created(path) => {
                            local_pending.insert(path);
                        }
                        crate::watcher::WatchEvent::Deleted(path) => {
                            local_pending.remove(&path);
                        }
                    }
                }

                // Move batched files to shared queue for main process to handle
                if !local_pending.is_empty() {
                    let file_count = local_pending.len();
                    info!("📋 Queuing {} changed file(s) for processing", file_count);

                    // Write lock to add files to shared queue
                    match pending_files_queue.write() {
                        Ok(mut queue) => {
                            queue.extend(local_pending.drain());
                            info!("✅ Files queued successfully (total pending: {})", queue.len());
                        }
                        Err(e) => {
                            error!("Failed to acquire write lock on pending files queue: {}", e);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kiln_processor_creation() {
        let processor = KilnProcessor::new();
        assert!(processor.progress_bar.is_none());
    }

    #[tokio::test]
    async fn test_missing_kiln_path_error() {
        let mut processor = KilnProcessor::new();

        let result = processor
            .process_kiln_integrated(
                std::path::Path::new("/nonexistent"),
                &crucible_surrealdb::SurrealClient::new_memory()
                    .await
                    .unwrap(),
            )
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not exist or is not accessible"));
    }

    #[tokio::test]
    async fn test_processing_result_creation() {
        let result = ProcessingResult {
            success: true,
            exit_code: Some(0),
            processing_time: Duration::from_secs(5),
            wait_time: Duration::from_secs(5),
            kiln_path: Some("/test/kiln".to_string()),
        };

        assert!(result.success);
        assert_eq!(
            result.status_message(),
            "Kiln processed successfully (5.0s)"
        );
        assert!(result
            .processing_info()
            .contains("Processed kiln: /test/kiln"));
    }
}
