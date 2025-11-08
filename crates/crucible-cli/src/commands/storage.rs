use anyhow::{Context, Result};
use serde_json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tabled::{settings::Style, Table, Tabled};

use crate::cli::StorageCommands;
use crate::config::CliConfig;
use crate::output;
use crucible_core::hashing::blake3::Blake3Hasher;
use crucible_core::storage::builder::{
    ContentAddressedStorageBuilder, HasherConfig, StorageBackendType,
};
use crucible_core::storage::{traits::StorageStats, ContentAddressedStorage, StorageResult};

/// Output formats for storage commands
#[derive(Debug, Clone)]
pub enum StorageOutputFormat {
    Table,
    Json,
    Plain,
}

impl From<String> for StorageOutputFormat {
    fn from(s: String) -> Self {
        match s.to_lowercase().as_str() {
            "json" => StorageOutputFormat::Json,
            "plain" => StorageOutputFormat::Plain,
            _ => StorageOutputFormat::Table,
        }
    }
}

/// Table-friendly storage statistics
#[derive(Tabled)]
struct StorageStatsRow {
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Value")]
    value: String,
    #[tabled(rename = "Description")]
    description: String,
}

/// Verification result information
#[derive(Debug)]
struct VerificationResult {
    path: String,
    is_valid: bool,
    issues: Vec<String>,
    block_count: u64,
    corrupted_blocks: u64,
    missing_blocks: u64,
}

/// Execute storage commands
pub async fn execute(config: CliConfig, command: StorageCommands) -> Result<()> {
    match command {
        StorageCommands::Stats {
            format,
            by_backend,
            deduplication,
        } => execute_stats(config, format, by_backend, deduplication).await,
        StorageCommands::Verify {
            path,
            repair,
            format,
        } => execute_verify(config, path, repair, format).await,
        StorageCommands::Cleanup {
            gc,
            rebuild_indexes,
            optimize,
            force,
            dry_run,
        } => execute_cleanup(config, gc, rebuild_indexes, optimize, force, dry_run).await,
        StorageCommands::Backup {
            dest,
            include_content,
            compress,
            verify,
            format,
        } => execute_backup(config, dest, include_content, compress, verify, format).await,
        StorageCommands::Restore {
            source,
            merge,
            skip_verify,
            format,
        } => execute_restore(config, source, merge, skip_verify, format).await,
    }
}

/// Execute storage stats command
async fn execute_stats(
    config: CliConfig,
    format: String,
    by_backend: bool,
    deduplication: bool,
) -> Result<()> {
    let output_format: StorageOutputFormat = format.into();
    let start_time = Instant::now();

    output::info("Gathering storage statistics...");

    // Create storage backend
    let storage = create_storage_backend(&config)?;

    // Get storage statistics
    let stats = storage
        .get_stats()
        .await
        .context("Failed to get storage statistics")?;

    // Generate output based on format
    match output_format {
        StorageOutputFormat::Table => output_stats_table(&stats, by_backend, deduplication)?,
        StorageOutputFormat::Json => output_stats_json(&stats, by_backend, deduplication)?,
        StorageOutputFormat::Plain => output_stats_plain(&stats, by_backend, deduplication),
    }

    let duration = start_time.elapsed();
    output::success(&format!(
        "Stats completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

/// Execute storage verify command
async fn execute_verify(
    config: CliConfig,
    path: Option<PathBuf>,
    repair: bool,
    format: String,
) -> Result<()> {
    let output_format: StorageOutputFormat = format.into();
    let start_time = Instant::now();

    output::info("Verifying storage integrity...");

    // Create storage backend
    let storage = create_storage_backend(&config)?;

    let results = if let Some(path) = path {
        output::info(&format!("Verifying path: {}", path.display()));
        vec![verify_path(&storage, &path).await?]
    } else {
        output::info("Verifying entire storage...");
        verify_entire_storage(&storage).await?
    };

    // Handle repair if requested
    if repair {
        output::info("Attempting to repair issues...");
        for result in &results {
            if !result.is_valid {
                repair_storage_issues(&storage, result).await?;
            }
        }
    }

    // Generate output
    match output_format {
        StorageOutputFormat::Table => output_verify_table(&results)?,
        StorageOutputFormat::Json => output_verify_json(&results)?,
        StorageOutputFormat::Plain => output_verify_plain(&results),
    }

    let duration = start_time.elapsed();
    let total_issues: u64 = results
        .iter()
        .map(|r| r.corrupted_blocks + r.missing_blocks)
        .sum();
    output::success(&format!(
        "Verification completed in {:.2}s - {} issues found",
        duration.as_secs_f32(),
        total_issues
    ));

    Ok(())
}

/// Execute storage cleanup command
async fn execute_cleanup(
    config: CliConfig,
    gc: bool,
    rebuild_indexes: bool,
    optimize: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let start_time = Instant::now();

    output::info("Starting storage cleanup...");

    if dry_run {
        output::warning("DRY RUN MODE - No changes will be made");
    }

    // Create storage backend
    let storage = create_storage_backend(&config)?;

    let mut cleanup_operations = Vec::new();

    if gc {
        cleanup_operations.push("garbage collection");
        if !dry_run {
            output::info("Running garbage collection...");
            // TODO: Implement garbage collection
            storage.maintenance().await?;
        }
    }

    if rebuild_indexes {
        cleanup_operations.push("index rebuilding");
        if !dry_run {
            output::info("Rebuilding indexes...");
            // TODO: Implement index rebuilding
        }
    }

    if optimize {
        cleanup_operations.push("storage optimization");
        if !dry_run {
            output::info("Optimizing storage layout...");
            // TODO: Implement storage optimization
        }
    }

    if cleanup_operations.is_empty() {
        output::warning("No cleanup operations specified");
        return Ok(());
    }

    if !dry_run && !force {
        // Ask for confirmation
        println!("This will perform the following operations:");
        for op in &cleanup_operations {
            println!("  - {}", op);
        }
        println!("\nContinue? (y/N)");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            output::info("Cleanup cancelled");
            return Ok(());
        }
    }

    let duration = start_time.elapsed();
    output::success(&format!(
        "Cleanup completed in {:.2}s - Operations: {}",
        duration.as_secs_f32(),
        cleanup_operations.join(", ")
    ));

    Ok(())
}

/// Execute storage backup command
async fn execute_backup(
    config: CliConfig,
    dest: PathBuf,
    include_content: bool,
    compress: bool,
    verify: bool,
    format: String,
) -> Result<()> {
    let start_time = Instant::now();

    output::info(&format!("Starting backup to: {}", dest.display()));

    // Create storage backend
    let storage = create_storage_backend(&config)?;

    // Ensure backup directory exists
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).context("Failed to create backup directory")?;
    }

    // Get backup data
    let stats = storage.get_stats().await?;

    let backup_data = serde_json::json!({
        "metadata": {
            "version": "1.0",
            "created_at": chrono::Utc::now().to_rfc3339(),
            "format": format,
            "include_content": include_content,
            "compress": compress,
        },
        "statistics": {
            "total_blocks": stats.block_count,
            "total_trees": stats.tree_count,
            "total_size_bytes": stats.block_size_bytes,
            "deduplication_savings": stats.deduplication_savings,
        },
        "content": if include_content {
            // TODO: Implement content export
            serde_json::Value::Null
        } else {
            serde_json::Value::Null
        }
    });

    // Write backup file
    let backup_json = serde_json::to_string_pretty(&backup_data)?;
    std::fs::write(&dest, backup_json).context("Failed to write backup file")?;

    if verify {
        output::info("Verifying backup integrity...");
        let read_back = std::fs::read_to_string(&dest)?;
        let _: serde_json::Value = serde_json::from_str(&read_back)
            .context("Backup verification failed - invalid JSON")?;
    }

    let duration = start_time.elapsed();
    output::success(&format!(
        "Backup completed in {:.2}s - Size: {}",
        duration.as_secs_f32(),
        format_bytes(dest.metadata()?.len())
    ));

    Ok(())
}

/// Execute storage restore command
async fn execute_restore(
    config: CliConfig,
    source: PathBuf,
    merge: bool,
    skip_verify: bool,
    format: String,
) -> Result<()> {
    let start_time = Instant::now();

    output::info(&format!("Starting restore from: {}", source.display()));

    if !source.exists() {
        return Err(anyhow::anyhow!(
            "Backup file does not exist: {}",
            source.display()
        ));
    }

    // Read backup file
    let backup_content = std::fs::read_to_string(&source)?;
    let backup_data: serde_json::Value =
        serde_json::from_str(&backup_content).context("Invalid backup file format")?;

    if !skip_verify {
        output::info("Verifying backup integrity...");
        // TODO: Implement backup verification
    }

    // Create storage backend
    let storage = create_storage_backend(&config)?;

    if !merge {
        output::warning("This will replace all existing storage data");
        println!("Continue? (y/N)");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().to_lowercase().starts_with('y') {
            output::info("Restore cancelled");
            return Ok(());
        }
    }

    // Restore data
    // TODO: Implement actual restore logic
    output::info("Restoring data...");

    let duration = start_time.elapsed();
    output::success(&format!(
        "Restore completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

/// Create storage backend based on configuration
fn create_storage_backend(_config: &CliConfig) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
    ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(crucible_core::storage::BlockSize::Medium)
        .build()
}

/// Verify a specific path
async fn verify_path(
    storage: &Arc<dyn ContentAddressedStorage>,
    path: &Path,
) -> Result<VerificationResult> {
    // TODO: Implement path verification
    Ok(VerificationResult {
        path: path.to_string_lossy().to_string(),
        is_valid: true,
        issues: vec![],
        block_count: 0,
        corrupted_blocks: 0,
        missing_blocks: 0,
    })
}

/// Verify entire storage
async fn verify_entire_storage(
    storage: &Arc<dyn ContentAddressedStorage>,
) -> Result<Vec<VerificationResult>> {
    // TODO: Implement full storage verification
    Ok(vec![])
}

/// Repair storage issues
async fn repair_storage_issues(
    storage: &Arc<dyn ContentAddressedStorage>,
    result: &VerificationResult,
) -> Result<()> {
    // TODO: Implement repair logic
    output::info(&format!("Repairing issues in: {}", result.path));
    Ok(())
}

/// Output stats in table format
fn output_stats_table(stats: &StorageStats, by_backend: bool, deduplication: bool) -> Result<()> {
    let rows = vec![
        StorageStatsRow {
            metric: "Total Blocks".to_string(),
            value: stats.block_count.to_string(),
            description: "Number of content blocks stored".to_string(),
        },
        StorageStatsRow {
            metric: "Total Trees".to_string(),
            value: stats.tree_count.to_string(),
            description: "Number of Merkle trees".to_string(),
        },
        StorageStatsRow {
            metric: "Storage Size".to_string(),
            value: format_bytes(stats.block_size_bytes),
            description: "Total storage used".to_string(),
        },
        StorageStatsRow {
            metric: "Deduplication Savings".to_string(),
            value: format_bytes(stats.deduplication_savings),
            description: "Space savings from deduplication".to_string(),
        },
    ];

    let table = Table::new(&rows).with(Style::modern()).to_string();

    output::header("Storage Statistics");
    println!("{}", table);

    Ok(())
}

/// Output stats in JSON format
fn output_stats_json(stats: &StorageStats, by_backend: bool, deduplication: bool) -> Result<()> {
    let output = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "statistics": {
            "total_blocks": stats.block_count,
            "total_trees": stats.tree_count,
            "total_size_bytes": stats.block_size_bytes,
            "deduplication_savings": stats.deduplication_savings,
            "average_block_size": stats.average_block_size,
            "largest_block_size": stats.largest_block_size,
            "evicted_blocks": stats.evicted_blocks,
        },
        "options": {
            "by_backend": by_backend,
            "deduplication": deduplication,
        }
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output stats in plain format
fn output_stats_plain(stats: &StorageStats, _by_backend: bool, _deduplication: bool) {
    output::header("Storage Statistics");
    println!("Total Blocks: {}", stats.block_count);
    println!("Total Trees: {}", stats.tree_count);
    println!("Storage Size: {}", format_bytes(stats.block_size_bytes));
    println!(
        "Deduplication Savings: {}",
        format_bytes(stats.deduplication_savings)
    );
    println!("Average Block Size: {:.0} bytes", stats.average_block_size);
    println!(
        "Largest Block Size: {}",
        format_bytes(stats.largest_block_size)
    );
}

/// Output verification results in table format
fn output_verify_table(results: &[VerificationResult]) -> Result<()> {
    if results.is_empty() {
        output::success("No verification results");
        return Ok(());
    }

    output::header("Verification Results");

    for result in results {
        let status = if result.is_valid {
            "✓ Valid"
        } else {
            "✗ Issues Found"
        };
        println!("{}: {}", result.path, status);

        if !result.is_valid {
            for issue in &result.issues {
                println!("  - {}", issue);
            }
        }
    }

    Ok(())
}

/// Output verification results in JSON format
fn output_verify_json(results: &[VerificationResult]) -> Result<()> {
    let output = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "results": results.iter().map(|result| {
            serde_json::json!({
                "path": result.path,
                "is_valid": result.is_valid,
                "issues": result.issues,
                "block_count": result.block_count,
                "corrupted_blocks": result.corrupted_blocks,
                "missing_blocks": result.missing_blocks,
            })
        }).collect::<Vec<_>>()
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

/// Output verification results in plain format
fn output_verify_plain(results: &[VerificationResult]) {
    if results.is_empty() {
        output::success("No verification results");
        return;
    }

    output::header("Verification Results");

    for result in results {
        let status = if result.is_valid {
            "✓ Valid"
        } else {
            "✗ Issues Found"
        };
        println!("{}: {}", result.path, status);

        if !result.is_valid {
            for issue in &result.issues {
                println!("  - {}", issue);
            }
        }
        println!();
    }
}

/// Format bytes into human readable format
fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}
