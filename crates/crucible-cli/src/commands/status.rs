//! Status command - show current storage and system status
//!
//! Displays information about the current storage backend, note count,
//! and system configuration.

use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

use crate::config::CliConfig;
use crate::formatting::OutputFormat;
use crate::output;

/// Execute status command
pub async fn execute(
    config: CliConfig,
    path: Option<PathBuf>,
    format: String,
    detailed: bool,
    recent: bool,
) -> Result<()> {
    let output_format = OutputFormat::from(format);
    let start_time = Instant::now();

    // Get storage
    let storage = crate::factories::get_storage(&config).await?;

    // Gather status information
    if let Some(path) = path {
        output::info(&format!("Analyzing path: {}", path.display()));
        show_path_status(&path, detailed)?;
    } else {
        output::info("Gathering global storage status...");
        show_global_status(&config, &storage, output_format, detailed, recent).await?;
    }

    // Show performance metrics
    let duration = start_time.elapsed();
    output::success(&format!(
        "Status completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

/// Show status for a specific path
fn show_path_status(path: &std::path::Path, detailed: bool) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to get metadata for path '{}': {}",
            path.display(),
            e
        )
    })?;

    output::header("Path Status");
    println!("  Path: {}", path.display());
    println!("  Exists: {}", path.exists());
    println!(
        "  Type: {}",
        if metadata.is_file() {
            "file"
        } else if metadata.is_dir() {
            "directory"
        } else {
            "other"
        }
    );
    println!("  Size: {} bytes", metadata.len());

    if detailed && path.is_dir() {
        let entries = std::fs::read_dir(path)?;
        let count = entries.count();
        println!("  Entries: {}", count);
    }

    Ok(())
}

/// Show global storage status
async fn show_global_status(
    config: &CliConfig,
    storage: &crate::factories::StorageHandle,
    output_format: OutputFormat,
    detailed: bool,
    _recent: bool,
) -> Result<()> {
    // Determine storage mode
    #[cfg(feature = "storage-surrealdb")]
    let is_embedded = storage.is_embedded();
    #[cfg(not(feature = "storage-surrealdb"))]
    let is_embedded = false;

    let mode = if is_embedded {
        "embedded"
    } else if storage.is_daemon() {
        "daemon"
    } else if storage.is_lightweight() {
        "lightweight"
    } else {
        #[cfg(feature = "storage-sqlite")]
        if storage.is_sqlite() {
            "sqlite"
        } else {
            "unknown"
        }
        #[cfg(not(feature = "storage-sqlite"))]
        "unknown"
    };

    // Get note count if available
    let note_count = if let Some(note_store) = storage.note_store() {
        Some(note_store.list().await?.len())
    } else {
        None
    };

    match output_format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "storage_mode": mode,
                "kiln_path": config.kiln_path.to_string_lossy(),
                "note_count": note_count,
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        _ => {
            output::header("Storage Status");
            println!("  Storage Mode: {}", mode);
            println!("  Kiln Path: {}", config.kiln_path.display());
            if let Some(count) = note_count {
                println!("  Total Notes: {}", count);
            }

            if detailed {
                println!();
                output::header("Configuration");
                println!("  Database Path: {}", config.database_path().display());
                if let Some(storage_config) = &config.storage {
                    println!("  Idle Timeout: {}s", storage_config.idle_timeout_secs);
                }
            }
        }
    }

    Ok(())
}
