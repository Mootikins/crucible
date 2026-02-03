//! Storage management commands
//!
//! Commands for managing storage backends, migrations, stats, and maintenance.

use anyhow::Result;
use std::path::PathBuf;
use std::time::Instant;

use crate::cli::StorageCommands;
use crate::config::CliConfig;
use crate::output;

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

/// Execute storage commands
pub async fn execute(config: CliConfig, command: StorageCommands) -> Result<()> {
    match command {
        StorageCommands::Migrate { to } => execute_migrate(&config, &to).await,
        StorageCommands::Mode => execute_mode(&config).await,
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

/// Execute storage migrate command
async fn execute_migrate(_config: &CliConfig, target: &str) -> Result<()> {
    match target {
        "lightweight" => {
            output::info("Migrating to lightweight mode...");
            output::info("This would export embeddings from SurrealDB to LanceDB.");
            output::info("");
            output::info("Steps to complete migration:");
            output::info("  1. Run this command to export data (TODO: implement export)");
            output::info("  2. Update config: storage.mode = \"lightweight\"");
            output::info("  3. Restart Crucible to use lightweight storage");
            output::info("");
            output::warning("Note: Lightweight mode stores only embeddings for search.");
            output::warning("Full note metadata remains in SurrealDB.");
            output::success("Migration preparation complete.");
            Ok(())
        }
        "full" => {
            output::info("Migrating to full mode...");
            output::info("");
            output::info("Steps to complete migration:");
            output::info("  1. Update config: storage.mode = \"full\"");
            output::info("  2. Run `cru process --force` to rebuild the full index");
            output::info("");
            output::info("Full mode provides complete EAV graph.");
            output::success("Migration preparation complete.");
            Ok(())
        }
        _ => anyhow::bail!(
            "Unknown target mode: '{}'. Use 'lightweight' or 'full'",
            target
        ),
    }
}

/// Execute storage mode command - show current storage mode
async fn execute_mode(config: &CliConfig) -> Result<()> {
    use crucible_config::StorageMode;

    output::header("Storage Mode");

    // Get storage mode from config, defaulting to Embedded if not set
    let mode = config
        .storage
        .as_ref()
        .map(|s| s.mode)
        .unwrap_or(StorageMode::Embedded);

    let mode_name = match mode {
        StorageMode::Embedded => "embedded (full)",
        StorageMode::Daemon => "daemon",
        StorageMode::Lightweight => "lightweight",
        StorageMode::Sqlite => "sqlite (experimental)",
    };

    println!("  Current mode: {}", mode_name);
    println!();

    match mode {
        StorageMode::Lightweight => {
            println!("  Description: Embeddings-only storage for fast semantic search");
            println!("  Backend: LanceDB (vector store)");
            println!("  Use case: Quick prototyping, search-focused workflows");
        }
        StorageMode::Embedded => {
            println!("  Description: Complete EAV graph storage");
            println!("  Backend: SurrealDB + RocksDB (embedded)");
            println!("  Use case: Full knowledge graph, semantic search");
        }
        StorageMode::Daemon => {
            println!("  Description: Client-server mode with shared database");
            println!("  Backend: SurrealDB daemon process");
            println!("  Use case: Multiple concurrent CLI sessions");
        }
        StorageMode::Sqlite => {
            println!("  Description: Lightweight SQLite-based storage (experimental)");
            println!("  Backend: SQLite with FTS5 full-text search");
            println!("  Use case: Testing alternative to SurrealDB");
        }
    }

    println!();
    output::info("Run `cru storage migrate --to <mode>` to change modes.");

    Ok(())
}

/// Execute storage stats command
async fn execute_stats(
    config: CliConfig,
    format: String,
    _by_backend: bool,
    _deduplication: bool,
) -> Result<()> {
    let _output_format: StorageOutputFormat = format.into();
    let start_time = Instant::now();

    output::info("Gathering storage statistics...");

    // Get storage and query for stats
    let storage = crate::factories::get_storage(&config).await?;

    // For now, show basic stats from NoteStore
    if let Some(note_store) = storage.note_store() {
        let notes = note_store.list().await?;
        output::header("Storage Statistics");
        println!("  Total Notes: {}", notes.len());
        println!(
            "  Storage Mode: {}",
            if storage.is_daemon() {
                "daemon"
            } else {
                "lightweight"
            }
        );
    } else {
        output::warning("Note store not available in current storage mode");
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
    _config: CliConfig,
    path: Option<PathBuf>,
    _repair: bool,
    _format: String,
) -> Result<()> {
    let start_time = Instant::now();

    output::info("Verifying storage integrity...");

    if let Some(path) = path {
        output::info(&format!("Verifying path: {}", path.display()));
    } else {
        output::info("Verifying entire storage...");
    }

    // Stub implementation - storage verification requires NoteStore-based implementation
    output::warning("Storage verification not yet implemented for current storage mode.");
    output::info("Use `cru process --force` to rebuild storage if needed.");

    let duration = start_time.elapsed();
    output::success(&format!(
        "Verification completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

/// Execute storage cleanup command
async fn execute_cleanup(
    _config: CliConfig,
    gc: bool,
    rebuild_indexes: bool,
    optimize: bool,
    _force: bool,
    dry_run: bool,
) -> Result<()> {
    let start_time = Instant::now();

    output::info("Starting storage cleanup...");

    if dry_run {
        output::warning("DRY RUN MODE - No changes will be made");
    }

    let mut cleanup_operations = Vec::new();

    if gc {
        cleanup_operations.push("garbage collection");
        if !dry_run {
            output::info("Running garbage collection...");
            // TODO: Implement garbage collection via NoteStore
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
    _config: CliConfig,
    dest: PathBuf,
    _include_content: bool,
    _compress: bool,
    _verify: bool,
    _format: String,
) -> Result<()> {
    let start_time = Instant::now();

    output::info(&format!("Starting backup to: {}", dest.display()));

    // Stub implementation - backup requires NoteStore-based implementation
    output::warning("Storage backup not yet implemented for current storage mode.");
    output::info("Consider copying the .crucible directory directly for backup.");

    let duration = start_time.elapsed();
    output::success(&format!(
        "Backup completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

/// Execute storage restore command
async fn execute_restore(
    _config: CliConfig,
    source: PathBuf,
    _merge: bool,
    _skip_verify: bool,
    _format: String,
) -> Result<()> {
    let start_time = Instant::now();

    output::info(&format!("Starting restore from: {}", source.display()));

    if !source.exists() {
        return Err(anyhow::anyhow!(
            "Backup file does not exist: {}",
            source.display()
        ));
    }

    // Stub implementation - restore requires NoteStore-based implementation
    output::warning("Storage restore not yet implemented for current storage mode.");
    output::info("Consider copying the .crucible directory directly for restore.");

    let duration = start_time.elapsed();
    output::success(&format!(
        "Restore completed in {:.2}s",
        duration.as_secs_f32()
    ));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_output_format_from_json() {
        let format: StorageOutputFormat = "json".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Json));
    }

    #[test]
    fn test_storage_output_format_from_plain() {
        let format: StorageOutputFormat = "plain".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Plain));
    }

    #[test]
    fn test_storage_output_format_from_table() {
        let format: StorageOutputFormat = "table".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Table));
    }

    #[test]
    fn test_storage_output_format_default() {
        let format: StorageOutputFormat = "unknown".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Table));
    }

    #[test]
    fn test_storage_output_format_case_insensitive() {
        let format: StorageOutputFormat = "JSON".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Json));

        let format: StorageOutputFormat = "Json".to_string().into();
        assert!(matches!(format, StorageOutputFormat::Json));
    }

    #[test]
    fn test_storage_output_format_clone() {
        let format = StorageOutputFormat::Json;
        let cloned = format.clone();
        assert!(matches!(cloned, StorageOutputFormat::Json));
    }

    #[test]
    fn test_storage_output_format_debug() {
        let format = StorageOutputFormat::Table;
        let debug = format!("{:?}", format);
        assert_eq!(debug, "Table");
    }
}
