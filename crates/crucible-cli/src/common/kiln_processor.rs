//! Daemon management utilities for CLI
//!
//! Provides functionality to spawn and manage the crucible-daemon process
//! for one-shot vault processing when embeddings are missing.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Daemon process manager for one-shot processing
pub struct DaemonManager {
    progress_bar: Option<ProgressBar>,
}

impl DaemonManager {
    /// Create a new daemon manager
    pub fn new() -> Self {
        Self { progress_bar: None }
    }

    /// Check if embeddings exist in the database
    pub async fn check_embeddings_exist(
        &self,
        client: &crucible_surrealdb::SurrealClient,
    ) -> Result<bool> {
        info!("Checking if embeddings exist in database...");

        match crucible_surrealdb::vault_integration::get_database_stats(client).await {
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
    pub async fn spawn_daemon_for_processing(
        &mut self,
        vault_path: &std::path::Path,
    ) -> Result<DaemonResult> {
        info!("Starting daemon for one-shot kiln processing...");

        // Validate kiln path exists
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Kiln path '{}' does not exist or is not accessible",
                vault_path.display()
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
        pb.set_message("Starting vault processing...");
        pb.enable_steady_tick(Duration::from_millis(100));
        self.progress_bar = Some(pb);

        let start_time = Instant::now();

        // Spawn the daemon process - use the built binary from target directory
        let crate_root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string()
        });

        // Try debug build first, then release build as fallback
        let daemon_path_debug =
            std::path::PathBuf::from(&crate_root).join("../../target/debug/crucible-daemon");
        let daemon_path_release =
            std::path::PathBuf::from(&crate_root).join("../../target/release/crucible-daemon");

        let daemon_path = if daemon_path_debug.exists() {
            daemon_path_debug
        } else if daemon_path_release.exists() {
            daemon_path_release
        } else {
            return Err(anyhow::anyhow!(
                "crucible-daemon binary not found at {} or {}. \
                Run 'cargo build -p crucible-daemon' to build the daemon.",
                daemon_path_debug.display(),
                daemon_path_release.display()
            ));
        };

        let mut child = Command::new(&daemon_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("OBSIDIAN_VAULT_PATH", vault_path.to_string_lossy().as_ref())
            // Inherit all relevant environment variables for security
            .env_clear()
            .envs(std::env::vars().filter(|(k, _)| {
                // Keep only essential environment variables for security
                matches!(
                    k.as_str(),
                    "OBSIDIAN_VAULT_PATH"
                        | "EMBEDDING_ENDPOINT"
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
                    "Failed to spawn crucible-daemon from {}: {}. \
                Make sure the daemon is built (run 'cargo build -p crucible-daemon').",
                    daemon_path.display(),
                    e
                )
            })?;

        if let Some(pb) = &self.progress_bar {
            pb.set_message("Processing vault files... (this may take a few minutes)");
        }

        // Wait for the daemon to complete
        let start_wait = Instant::now();
        debug!("Waiting for daemon process to complete...");

        match child.wait().await {
            Ok(status) => {
                let elapsed = start_time.elapsed();
                let wait_elapsed = start_wait.elapsed();

                if let Some(pb) = self.progress_bar.take() {
                    let message = format!("Processing completed in {:.1}s", elapsed.as_secs_f64());
                    pb.finish_with_message(message);
                    self.progress_bar = Some(pb);
                }

                let result = DaemonResult {
                    success: status.success(),
                    exit_code: status.code(),
                    processing_time: elapsed,
                    wait_time: wait_elapsed,
                    vault_path: Some(vault_path.to_string_lossy().to_string()),
                };

                if result.success {
                    info!(
                        "Daemon processing completed successfully in {:.1}s",
                        elapsed.as_secs_f64()
                    );
                    Ok(result)
                } else {
                    let error_msg = match result.exit_code {
                        Some(1) => "Configuration error (missing OBSIDIAN_VAULT_PATH)",
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
                error!("Failed to wait for daemon process: {}", e);
                Err(anyhow::anyhow!("Failed to wait for daemon process: {}", e))
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

impl Drop for DaemonManager {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Result from daemon processing operation
#[derive(Debug, Clone)]
pub struct DaemonResult {
    /// Whether the processing was successful
    pub success: bool,
    /// Exit code from daemon process
    pub exit_code: Option<i32>,
    /// Total processing time
    pub processing_time: Duration,
    /// Time spent waiting for process
    pub wait_time: Duration,
    /// Vault path that was processed
    pub vault_path: Option<String>,
}

impl DaemonResult {
    /// Get human-readable status message
    pub fn status_message(&self) -> String {
        if self.success {
            format!(
                "Vault processed successfully ({:.1}s)",
                self.processing_time.as_secs_f64()
            )
        } else {
            format!("Processing failed (exit code: {:?})", self.exit_code)
        }
    }

    /// Get detailed processing information
    pub fn processing_info(&self) -> String {
        match &self.vault_path {
            Some(path) => format!(
                "Processed vault: {} (took {:.1}s)",
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

/// Ensure file watcher is running for the vault
///
/// Spawns a background task that watches for file changes and triggers delta processing.
/// Follows industry best practices: enabled by default, graceful degradation on failure.
pub async fn ensure_watcher_running(config: &crate::config::CliConfig) -> Result<()> {
    use crate::watcher::{SimpleFileWatcher, get_fix_instructions};

    // Check if enabled in config (default: true)
    if !config.file_watching.enabled {
        debug!("File watching disabled by configuration");
        return Ok(());
    }

    info!("Initializing file watcher for: {}", config.kiln.path.display());

    // Spawn the entire watcher + event handling in a background task
    let vault_path = config.kiln.path.clone();
    let watcher_config = config.file_watching.clone();
    let config_clone = config.clone();

    std::thread::spawn(move || {
        // Create a new Tokio runtime for this thread to avoid lifetime issues
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(async move {
            run_file_watcher(vault_path, watcher_config, config_clone).await;
        });
    });

    // Give the watcher a moment to initialize before continuing
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    Ok(())
}

/// Run file watcher in background task (extracted to avoid HRTB lifetime issues)
async fn run_file_watcher(
    vault_path: std::path::PathBuf,
    watcher_config: crate::config::FileWatcherConfig,
    config: crate::config::CliConfig,
) {
    use crate::watcher::{SimpleFileWatcher, get_fix_instructions};

    let debounce_ms = watcher_config.debounce_ms;

    // Create channel for watch events
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    // Create watcher (with pre-flight checks)
    match SimpleFileWatcher::new(&vault_path, watcher_config, event_tx) {
        Ok(_watcher) => {
            info!("✓ File watching enabled (debounce: {}ms)", debounce_ms);

            // Handle events - this keeps the watcher alive
            if let Err(e) = handle_watch_events(event_rx, config).await {
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

/// Handle watch events in background task
async fn handle_watch_events(
    mut event_rx: mpsc::UnboundedReceiver<crate::watcher::WatchEvent>,
    config: crate::config::CliConfig,
) -> Result<()> {
    use std::collections::HashSet;
    use tokio::time::{sleep, Duration};

    debug!("File watcher event handler started");

    // Batch events to avoid processing the same file multiple times
    let mut pending_files: HashSet<std::path::PathBuf> = HashSet::new();
    let batch_window = Duration::from_secs(1);

    loop {
        // Wait for first event or timeout
        tokio::select! {
            Some(event) = event_rx.recv() => {
                // Add file to pending set
                match event {
                    crate::watcher::WatchEvent::Changed(path) |
                    crate::watcher::WatchEvent::Created(path) => {
                        debug!("Queued for processing: {}", path.display());
                        pending_files.insert(path);
                    }
                    crate::watcher::WatchEvent::Deleted(path) => {
                        debug!("File deleted, removing from processing queue: {}", path.display());
                        pending_files.remove(&path);
                    }
                }

                // Collect more events for batch window
                sleep(batch_window).await;

                // Drain any additional events that arrived during wait
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        crate::watcher::WatchEvent::Changed(path) |
                        crate::watcher::WatchEvent::Created(path) => {
                            pending_files.insert(path);
                        }
                        crate::watcher::WatchEvent::Deleted(path) => {
                            pending_files.remove(&path);
                        }
                    }
                }

                // Process batched files
                if !pending_files.is_empty() {
                    let file_count = pending_files.len();
                    info!("🔄 Processing {} changed file(s)", file_count);

                    // Process the changed files (pass owned values to avoid lifetime issues)
                    let files: Vec<_> = pending_files.drain().collect();
                    let config_clone = config.clone();
                    match process_changed_files(files, config_clone).await {
                        Ok(count) => {
                            info!("✅ Processed {} file(s) successfully", count);
                        }
                        Err(e) => {
                            error!("Failed to process changed files: {}", e);
                        }
                    }
                }
            }
        }
    }
}

/// Process specific changed files (delta processing)
async fn process_changed_files(
    changed_files: Vec<std::path::PathBuf>,
    config: crate::config::CliConfig,
) -> Result<usize> {
    use crucible_surrealdb::{
        embedding_pool::create_embedding_thread_pool_with_crucible_config,
        vault_scanner::{VaultScannerConfig, ErrorHandlingMode, ChangeDetectionMethod},
        vault_processor::process_vault_delta,
        vault_integration::initialize_vault_schema,
        SurrealClient, SurrealDbConfig, EmbeddingConfig,
    };

    // Create database client
    let db_config = SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "vault".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = SurrealClient::new(db_config)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    // Initialize schema (idempotent)
    initialize_vault_schema(&client)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize schema: {}", e))?;

    // Handle empty input
    if changed_files.is_empty() {
        debug!("No files to process");
        return Ok(0);
    }

    debug!("Processing {} changed files", changed_files.len());

    // Create scanner config for processing with embeddings enabled
    let scanner_config = VaultScannerConfig {
        max_file_size_bytes: 50 * 1024 * 1024, // 50MB
        max_recursion_depth: 10,
        recursive_scan: true,
        include_hidden_files: false,
        file_extensions: vec!["md".to_string(), "markdown".to_string()],
        parallel_processing: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4),
        batch_processing: true,
        batch_size: 16,
        enable_embeddings: true,  // Enabled - we're in a separate thread now
        process_embeds: true,
        process_wikilinks: true,
        enable_incremental: true,  // Delta processing
        track_file_changes: true,
        change_detection_method: ChangeDetectionMethod::ContentHash,
        error_handling_mode: ErrorHandlingMode::ContinueOnError,
        max_error_count: 100,
        error_retry_attempts: 3,
        error_retry_delay_ms: 500,
        skip_problematic_files: true,
        log_errors_detailed: true,
        error_threshold_circuit_breaker: 10,
        circuit_breaker_timeout_ms: 30000,
        processing_timeout_ms: 30000,
    };

    // Create embedding provider config and pool
    let provider_config = create_provider_config_from_cli(&config)?;
    let pool_config = EmbeddingConfig::default();
    let embedding_pool = create_embedding_thread_pool_with_crucible_config(
        pool_config,
        provider_config,
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create embedding pool: {}", e))?;

    // Process the files using delta processor with embeddings
    debug!("Processing {} files with delta update and embeddings", changed_files.len());
    let result = process_vault_delta(
        changed_files,
        &client,
        &scanner_config,
        Some(&embedding_pool),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to process delta files: {}", e))?;

    Ok(result.processed_count)
}

/// Convert CLI configuration to crucible-config provider configuration
fn create_provider_config_from_cli(config: &crate::config::CliConfig) -> Result<crucible_config::EmbeddingProviderConfig> {
    use crucible_config::EmbeddingProviderConfig;

    // Extract model name from CLI config
    let model_name = config.kiln.embedding_model.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Embedding model is not configured. Please set it in ~/.config/crucible/config.toml"
        )
    })?;

    // Determine provider type and create appropriate config
    if config.kiln.embedding_url.contains("api.openai.com") {
        // OpenAI provider
        let api_key = std::env::var("OPENAI_API_KEY").ok();
        Ok(EmbeddingProviderConfig::openai(
            api_key.unwrap_or_default(),
            Some(model_name.clone()),
        ))
    } else {
        // Default to Ollama for local/custom endpoints
        Ok(EmbeddingProviderConfig::ollama(
            Some(config.kiln.embedding_url.clone()),
            Some(model_name.clone()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_daemon_manager_creation() {
        let manager = DaemonManager::new();
        assert!(manager.progress_bar.is_none());
    }

    #[tokio::test]
    async fn test_missing_vault_path_error() {
        let mut manager = DaemonManager::new();

        // Temporarily clear the environment variable
        let original_path = std::env::var("OBSIDIAN_VAULT_PATH").ok();
        std::env::remove_var("OBSIDIAN_VAULT_PATH");

        let result = manager
            .spawn_daemon_for_processing(std::path::Path::new("/nonexistent"))
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("does not exist or is not accessible"));

        // Restore original value
        if let Some(path) = original_path {
            std::env::set_var("OBSIDIAN_VAULT_PATH", path);
        }
    }

    #[tokio::test]
    async fn test_daemon_result_creation() {
        let result = DaemonResult {
            success: true,
            exit_code: Some(0),
            processing_time: Duration::from_secs(5),
            wait_time: Duration::from_secs(5),
            vault_path: Some("/test/vault".to_string()),
        };

        assert!(result.success);
        assert_eq!(
            result.status_message(),
            "Vault processed successfully (5.0s)"
        );
        assert!(result
            .processing_info()
            .contains("Processed vault: /test/vault"));
    }
}
