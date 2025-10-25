//! Daemon management utilities for CLI
//!
//! Provides functionality to spawn and manage the crucible-daemon process
//! for one-shot vault processing when embeddings are missing.

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{info, warn, error, debug};
use std::time::{Duration, Instant};

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
    pub async fn check_embeddings_exist(&self, client: &crucible_surrealdb::SurrealClient) -> Result<bool> {
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
                let result = client.query(embeddings_sql, &[]).await
                    .map_err(|e| anyhow::anyhow!("Failed to query embeddings: {}", e))?;

                let embeddings_count = result.records.first()
                    .and_then(|r| r.data.get("total"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                info!("Found {} embeddings in database (fallback query)", embeddings_count);
                Ok(embeddings_count > 0)
            }
        }
    }

    /// Spawn daemon for one-shot processing with progress feedback
    pub async fn spawn_daemon_for_processing(&mut self, vault_path: &std::path::Path) -> Result<DaemonResult> {
        info!("Starting daemon for one-shot vault processing...");

        // Validate vault path exists
        if !vault_path.exists() {
            return Err(anyhow::anyhow!(
                "Vault path '{}' does not exist or is not accessible",
                vault_path.display()
            ));
        }

        // Create progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg:.cyan} [{elapsed_precise}]")
                .unwrap()
                .progress_chars("#>-")
        );
        pb.set_message("Starting vault processing...");
        pb.enable_steady_tick(Duration::from_millis(100));
        self.progress_bar = Some(pb);

        let start_time = Instant::now();

        // Spawn the daemon process - use the built binary from target directory
        let crate_root = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| std::env::current_dir().unwrap().to_string_lossy().to_string());

        // Try debug build first, then release build as fallback
        let daemon_path_debug = std::path::PathBuf::from(&crate_root)
            .join("../../target/debug/crucible-daemon");
        let daemon_path_release = std::path::PathBuf::from(&crate_root)
            .join("../../target/release/crucible-daemon");

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
                matches!(k.as_str(),
                    "OBSIDIAN_VAULT_PATH" |
                    "EMBEDDING_ENDPOINT" |
                    "EMBEDDING_MODEL" |
                    "HOME" |
                    "PATH" |
                    "RUST_LOG" |
                    "CRUCIBLE_DB_PATH" |
                    "CRUCIBLE_CONFIG_PATH"
                )
            }))
            .spawn()
            .map_err(|e| anyhow::anyhow!(
                "Failed to spawn crucible-daemon from {}: {}. \
                Make sure the daemon is built (run 'cargo build -p crucible-daemon').",
                daemon_path.display(),
                e
            ))?;

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
                    info!("Daemon processing completed successfully in {:.1}s", elapsed.as_secs_f64());
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
                        "Daemon processing failed: {}. Check that your vault path is correct and accessible.",
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
            format!(
                "Processing failed (exit code: {:?})",
                self.exit_code
            )
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

        let result = manager.spawn_daemon_for_processing(Path::new("/nonexistent")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist or is not accessible"));

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
        assert_eq!(result.status_message(), "Vault processed successfully (5.0s)");
        assert!(result.processing_info().contains("Processed vault: /test/vault"));
    }
}