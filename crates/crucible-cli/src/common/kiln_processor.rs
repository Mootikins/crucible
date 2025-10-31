//! Kiln processing utilities for CLI
//!
//! Provides functionality to spawn and manage the kiln processor process
//! for one-shot kiln processing when embeddings are missing.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Daemon process manager for one-shot processing
pub struct KilnProcessor {
    progress_bar: Option<ProgressBar>,
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

    /// Spawn daemon for one-shot processing with progress feedback
    pub async fn process_kiln(&mut self, kiln_path: &std::path::Path) -> Result<ProcessingResult> {
        info!("Starting daemon for one-shot kiln processing...");

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
        pb.set_message("Starting kiln processing...");
        pb.enable_steady_tick(Duration::from_millis(100));
        self.progress_bar = Some(pb);

        let start_time = Instant::now();

        // Spawn the kiln processor - use the built binary from target directory
        let crate_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string()
        });

        // Try debug build first, then release build as fallback
        let daemon_path_debug =
            std::path::PathBuf::from(&crate_root).join("../../target/debug/kiln processor");
        let daemon_path_release =
            std::path::PathBuf::from(&crate_root).join("../../target/release/kiln processor");

        let daemon_path = if daemon_path_debug.exists() {
            daemon_path_debug
        } else if daemon_path_release.exists() {
            daemon_path_release
        } else {
            return Err(anyhow::anyhow!(
                "kiln processor binary not found at {} or {}. \
                Run 'cargo build -p kiln processor' to build the daemon.",
                daemon_path_debug.display(),
                daemon_path_release.display()
            ));
        };

        let mut child = Command::new(&daemon_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Inherit all relevant environment variables for security
            .env_clear()
            .envs(std::env::vars().filter(|(k, _)| {
                // Keep only essential environment variables for security
                matches!(
                    k.as_str(),
                    "EMBEDDING_ENDPOINT"
                        | "EMBEDDING_MODEL"
                        | "HOME"
                        | "PATH"
                        | "RUST_LOG"
                        | "CRUCIBLE_DB_PATH"
                        | "CRUCIBLE_CONFIG_PATH"
                )
            }))
            .spawn()
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to spawn kiln processor from {}: {}. \
                Make sure the daemon is built (run 'cargo build -p kiln processor').",
                    daemon_path.display(),
                    e
                )
            })?;

        if let Some(pb) = &self.progress_bar {
            pb.set_message("Processing kiln files... (this may take a few minutes)");
        }

        // Wait for the daemon to complete
        let start_wait = Instant::now();
        debug!("Waiting for kiln processor to complete...");

        match child.wait().await {
            Ok(status) => {
                let elapsed = start_time.elapsed();
                let wait_elapsed = start_wait.elapsed();

                if let Some(pb) = self.progress_bar.take() {
                    let message = format!("Processing completed in {:.1}s", elapsed.as_secs_f64());
                    pb.finish_with_message(message);
                    self.progress_bar = Some(pb);
                }

                let result = ProcessingResult {
                    success: status.success(),
                    exit_code: status.code(),
                    processing_time: elapsed,
                    wait_time: wait_elapsed,
                    kiln_path: Some(kiln_path.to_string_lossy().to_string()),
                };

                if result.success {
                    info!(
                        "Daemon processing completed successfully in {:.1}s",
                        elapsed.as_secs_f64()
                    );
                    Ok(result)
                } else {
                    let error_msg = match result.exit_code {
                        Some(1) => "Configuration error (missing kiln path)",
                        Some(2) => "Processing error (file parsing/validation failed)",
                        Some(3) => "Database error (connection/query failed)",
                        Some(4) => "Other error",
                        Some(code) => &format!("Unknown error code: {}", code),
                        None => "Process terminated by signal",
                    };

                    error!("Daemon processing failed: {}", error_msg);
                    Err(anyhow::anyhow!(
                        "Daemon processing failed: {}. Check that your kiln path is correct and accessible.",
                        error_msg
                    ))
                }
            }
            Err(e) => {
                if let Some(pb) = self.progress_bar.take() {
                    pb.abandon_with_message("Processing failed".to_string());
                    self.progress_bar = Some(pb);
                }
                error!("Failed to wait for kiln processor: {}", e);
                Err(anyhow::anyhow!("Failed to wait for kiln processor: {}", e))
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

/// Ensure file watcher is running for the kiln
///
/// Spawns a background task that watches for file changes and queues them for processing.
/// Follows industry best practices: enabled by default, graceful degradation on failure.
///
/// Returns a shared queue of pending files that the main process should periodically check.
pub async fn ensure_watcher_running(
    config: &crate::config::CliConfig,
) -> Result<std::sync::Arc<std::sync::RwLock<std::collections::HashSet<std::path::PathBuf>>>> {
    use crate::watcher::{get_fix_instructions, SimpleFileWatcher};
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    // Check if enabled in config (default: true)
    if !config.file_watching.enabled {
        debug!("File watching disabled by configuration");
        return Ok(Arc::new(RwLock::new(HashSet::new())));
    }

    info!(
        "Initializing file watcher for: {}",
        config.kiln.path.display()
    );

    // Create shared queue for pending files
    let pending_files = Arc::new(RwLock::new(HashSet::new()));
    let pending_files_clone = Arc::clone(&pending_files);

    // Spawn the entire watcher + event handling in a background task
    let kiln_path = config.kiln.path.clone();
    let watcher_config = config.file_watching.clone();

    std::thread::spawn(move || {
        // Create a new Tokio runtime for this thread
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(async move {
            run_file_watcher(kiln_path, watcher_config, pending_files_clone).await;
        });
    });

    // Give the watcher a moment to initialize before continuing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(pending_files)
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

/// Convert CLI configuration to crucible-config provider configuration
fn create_provider_config_from_cli(
    config: &crate::config::CliConfig,
) -> Result<crucible_config::EmbeddingProviderConfig> {
    // Use the unified config conversion method that handles both new [embedding] section
    // and legacy kiln.embedding_* format
    // Note: to_embedding_config() already returns EmbeddingProviderConfig (re-exported as EmbeddingConfig)
    config.to_embedding_config()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_manager_creation() {
        let manager = KilnProcessor::new();
        assert!(manager.progress_bar.is_none());
    }

    #[tokio::test]
    async fn test_missing_kiln_path_error() {
        let mut manager = KilnProcessor::new();

        let result = manager
            .process_kiln(std::path::Path::new("/nonexistent"))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not exist or is not accessible"));
    }

    #[tokio::test]
    async fn test_daemon_result_creation() {
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
