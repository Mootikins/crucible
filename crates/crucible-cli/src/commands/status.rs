use anyhow::{Context, Result};
use serde_json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tabled::Tabled;

use crate::config::CliConfig;
use crate::factories::create_content_addressed_storage;
use crate::formatting::{format_bytes, format_timestamp, render_table, OutputFormat};
use crate::output;
use crucible_core::parser::StorageAwareParser;
use crucible_core::storage::ContentAddressedStorage;
use crucible_parser::CrucibleParser;

// Removed: Now using shared OutputFormat from formatting module

/// Table-friendly storage statistics
#[derive(Tabled)]
struct StorageStatsRow {
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Value")]
    value: String,
    #[tabled(rename = "Details")]
    details: String,
}

/// Table-friendly activity information
#[derive(Tabled)]
struct ActivityRow {
    #[tabled(rename = "Time")]
    timestamp: String,
    #[tabled(rename = "Type")]
    activity_type: String,
    #[tabled(rename = "Description")]
    description: String,
}

/// Status information container
#[derive(Debug)]
struct StatusInfo {
    storage_backend: String,
    total_blocks: u64,
    total_trees: u64,
    storage_size_bytes: u64,
    deduplication_ratio: f32,
    last_activity: SystemTime,
    backend_specific: serde_json::Value,
    recent_changes: Vec<ActivityInfo>,
}

#[derive(Debug, Clone)]
struct ActivityInfo {
    timestamp: SystemTime,
    activity_type: String,
    description: String,
    size_delta: i64,
}

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

    // Create storage backend
    let storage = create_content_addressed_storage(&config)?;

    // Create parser with storage integration
    let block_parser = CrucibleParser::with_default_extensions();
    let parser = StorageAwareParser::new(Box::new(block_parser));

    // Gather status information
    let status_info = if let Some(path) = path {
        output::info(&format!("Analyzing path: {}", path.display()));
        get_path_status(&parser, &path).await?
    } else {
        output::info("Gathering global storage status...");
        get_global_status(&storage).await?
    };

    // Generate output
    match output_format {
        OutputFormat::Table | OutputFormat::Detailed => {
            output_table_format(&status_info, detailed, recent)?
        }
        OutputFormat::Json => output_json_format(&status_info, detailed, recent)?,
        OutputFormat::Plain => output_plain_format(&status_info, detailed, recent),
        OutputFormat::Csv => {
            // CSV format not yet implemented for status command
            output_plain_format(&status_info, detailed, recent)
        }
    }

    // Show performance metrics
    let duration = start_time.elapsed();
    output::success(&format!(
        "Status completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

// Use factory function from crate::factories::create_content_addressed_storage

/// Get status for a specific path
async fn get_path_status(_parser: &StorageAwareParser, path: &Path) -> Result<StatusInfo> {
    // For now, return basic path information
    // TODO: Implement proper status checking when StorageAwareParser API is finalized

    let metadata = std::fs::metadata(path)
        .with_context(|| format!("Failed to get metadata for path: {}", path.display()))?;

    let (total_trees, total_blocks) = if path.is_file() {
        (1, 1) // Simple assumption for files
    } else if path.is_dir() {
        let entries = std::fs::read_dir(path)
            .with_context(|| format!("Failed to read directory: {}", path.display()))?;
        let count = entries.count();
        (count, count) // Simple assumption for directory contents
    } else {
        (0, 0)
    };

    Ok(StatusInfo {
        storage_backend: "InMemory".to_string(),
        total_blocks: total_blocks as u64,
        total_trees: total_trees as u64,
        storage_size_bytes: metadata.len() as u64,
        deduplication_ratio: 1.0, // Calculate based on savings vs total
        last_activity: SystemTime::now(),
        backend_specific: serde_json::json!({
            "path": path.to_string_lossy(),
            "exists": path.exists(),
            "is_file": path.is_file(),
            "is_directory": path.is_dir(),
        }),
        recent_changes: vec![],
    })
}

/// Get global storage status
async fn get_global_status(storage: &Arc<dyn ContentAddressedStorage>) -> Result<StatusInfo> {
    let storage_stats = storage
        .get_stats()
        .await
        .context("Failed to get storage statistics")?;

    Ok(StatusInfo {
        storage_backend: "InMemory".to_string(),
        total_blocks: storage_stats.block_count,
        total_trees: storage_stats.tree_count,
        storage_size_bytes: storage_stats.block_size_bytes,
        deduplication_ratio: 1.0, // Calculate based on savings vs total
        last_activity: SystemTime::now(),
        backend_specific: serde_json::json!({
            "hash_algorithm": "BLAKE3",
            "compression": "none",
            "max_connections": 10,
        }),
        recent_changes: vec![],
    })
}

/// Output in table format
fn output_table_format(status_info: &StatusInfo, detailed: bool, recent: bool) -> Result<()> {
    output::header("Storage Status");

    // Storage Overview
    let overview_rows = vec![
        StorageStatsRow {
            metric: "Backend".to_string(),
            value: status_info.storage_backend.clone(),
            details: "Storage system type".to_string(),
        },
        StorageStatsRow {
            metric: "Total Blocks".to_string(),
            value: status_info.total_blocks.to_string(),
            details: format!("{} unique content blocks", status_info.total_blocks),
        },
        StorageStatsRow {
            metric: "Total Trees".to_string(),
            value: status_info.total_trees.to_string(),
            details: format!("{} Merkle trees", status_info.total_trees),
        },
        StorageStatsRow {
            metric: "Storage Size".to_string(),
            value: format_bytes(status_info.storage_size_bytes),
            details: "Total storage used".to_string(),
        },
        StorageStatsRow {
            metric: "Deduplication Ratio".to_string(),
            value: format!("{:.2}x", status_info.deduplication_ratio),
            details: "Space savings from deduplication".to_string(),
        },
    ];

    println!("{}", render_table(&overview_rows));

    if detailed {
        output::header("Detailed Information");
        println!(
            "Backend Specific: {}",
            serde_json::to_string_pretty(&status_info.backend_specific)?
        );

        // Memory usage (for in-memory backend)
        if status_info.storage_backend == "InMemory" {
            println!("\nMemory Usage:");
            println!(
                "  Blocks: ~{}",
                format_bytes(status_info.total_blocks * 1024)
            ); // Estimate
            println!("  Trees: ~{}", format_bytes(status_info.total_trees * 512)); // Estimate
            println!("  Total: {}", format_bytes(status_info.storage_size_bytes));
        }
    }

    if recent {
        output::header("Recent Activity");
        if status_info.recent_changes.is_empty() {
            println!("No recent activity recorded");
        } else {
            let activity_rows: Vec<ActivityRow> = status_info
                .recent_changes
                .iter()
                .take(10) // Limit to 10 recent activities
                .map(|activity| ActivityRow {
                    timestamp: format_timestamp(activity.timestamp),
                    activity_type: activity.activity_type.clone(),
                    description: activity.description.clone(),
                })
                .collect();

            println!("{}", render_table(&activity_rows));
        }
    }

    Ok(())
}

/// Output in JSON format
fn output_json_format(status_info: &StatusInfo, detailed: bool, recent: bool) -> Result<()> {
    let last_activity_timestamp = status_info
        .last_activity
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut output = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "storage": {
            "backend": status_info.storage_backend,
            "total_blocks": status_info.total_blocks,
            "total_trees": status_info.total_trees,
            "storage_size_bytes": status_info.storage_size_bytes,
            "deduplication_ratio": status_info.deduplication_ratio,
            "last_activity": last_activity_timestamp,
        }
    });

    if detailed {
        output["backend_specific"] = status_info.backend_specific.clone();
    }

    if recent {
        output["recent_changes"] = serde_json::json!(status_info
            .recent_changes
            .iter()
            .map(|activity| {
                let timestamp = activity
                    .timestamp
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                serde_json::json!({
                    "timestamp": timestamp,
                    "type": activity.activity_type,
                    "description": activity.description,
                    "size_delta": activity.size_delta,
                })
            })
            .collect::<Vec<_>>());
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output in plain text format
fn output_plain_format(status_info: &StatusInfo, detailed: bool, recent: bool) {
    output::header("Storage Status");
    println!("Backend: {}", status_info.storage_backend);
    println!("Total Blocks: {}", status_info.total_blocks);
    println!("Total Trees: {}", status_info.total_trees);
    println!(
        "Storage Size: {}",
        format_bytes(status_info.storage_size_bytes)
    );
    println!(
        "Deduplication Ratio: {:.2}x",
        status_info.deduplication_ratio
    );

    if detailed {
        println!("\nBackend Specific:");
        println!(
            "{}",
            serde_json::to_string_pretty(&status_info.backend_specific).unwrap_or_default()
        );
    }

    if recent {
        println!("\nRecent Activity:");
        if status_info.recent_changes.is_empty() {
            println!("No recent activity recorded");
        } else {
            for activity in status_info.recent_changes.iter().take(5) {
                println!(
                    "  [{}] {}: {}",
                    format_timestamp(activity.timestamp),
                    activity.activity_type,
                    activity.description
                );
            }
        }
    }
}

// Removed: Now using shared format_bytes and format_timestamp from formatting module
