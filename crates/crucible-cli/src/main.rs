use anyhow::Result;
use clap::Parser;
use tracing::{debug, error, info, warn};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*; // For SubscriberExt trait

use crucible_cli::{
    cli::{Cli, Commands},
    commands, config, factories,
    sync::{quick_sync_check, SyncStatus},
};

/// Process files with change detection on startup
///
/// Uses the quick_sync_check to detect files needing processing,
/// then processes them through the NotePipeline.
///
/// # Arguments
///
/// * `config` - CLI configuration with kiln path and settings
///
/// # Returns
///
/// Success or an error describing what failed
async fn process_files_with_change_detection(config: &crate::config::CliConfig) -> Result<()> {
    // Step 1: Get storage handle (backend-agnostic)
    let storage_handle = factories::get_storage(config).await?;
    debug!("Created storage handle");

    // Step 2: Get NoteStore for processing
    let note_store = storage_handle
        .note_store()
        .ok_or_else(|| anyhow::anyhow!("Storage mode does not support NoteStore"))?;

    // Step 3: Run quick sync check to find files needing processing
    let kiln_path = &config.kiln_path;

    // For embedded SurrealDB mode, we can use the quick_sync_check
    // For other modes, we skip this optimization and process all files
    let sync_status = if let Some(surreal) = storage_handle.try_embedded() {
        quick_sync_check(surreal, kiln_path).await?
    } else {
        // Create a minimal sync status that processes all markdown files
        debug!("Non-embedded mode: skipping quick_sync_check, will process all files");
        SyncStatus::all_new(kiln_path)?
    };

    debug!(
        "Sync check complete: {} fresh, {} stale, {} new, {} deleted",
        sync_status.fresh_count,
        sync_status.stale_files.len(),
        sync_status.new_files.len(),
        sync_status.deleted_files.len()
    );

    // Step 4: Process files if needed
    if sync_status.needs_processing() {
        let pending = sync_status.pending_count();
        info!("Processing {} files...", pending);

        // Create pipeline for processing (backend-agnostic)
        let pipeline = factories::create_pipeline(note_store, config, false).await?;

        // Process each file
        let files_to_process = sync_status.files_to_process();
        let mut success_count = 0;
        let mut failure_count = 0;

        for file in files_to_process {
            match pipeline.process(&file).await {
                Ok(_) => {
                    debug!("Successfully processed: {}", file.display());
                    success_count += 1;
                }
                Err(e) => {
                    warn!("Failed to process {}: {}", file.display(), e);
                    failure_count += 1;
                }
            }
        }

        info!(
            "File processing complete: {} succeeded, {} failed",
            success_count, failure_count
        );
    } else {
        debug!("No files need processing - all up to date");
    }

    Ok(())
}
/// Parse log level string to LevelFilter
fn parse_log_level(level: &str) -> Option<LevelFilter> {
    match level.to_lowercase().as_str() {
        "off" => Some(LevelFilter::OFF),
        "error" => Some(LevelFilter::ERROR),
        "warn" => Some(LevelFilter::WARN),
        "info" => Some(LevelFilter::INFO),
        "debug" => Some(LevelFilter::DEBUG),
        "trace" => Some(LevelFilter::TRACE),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load configuration first (before logging) to get config file log level
    let config = config::CliConfig::load(
        cli.config.clone(),
        cli.embedding_url.clone(),
        cli.embedding_model.clone(),
    )?;

    // Check if the command uses stdio for communication (needs file logging)
    // MCP and Chat use stdio (stdin/stdout) for JSON-RPC, so we must avoid stderr output
    let uses_stdio = match &cli.command {
        Some(Commands::Mcp { stdio, .. }) => *stdio,
        Some(Commands::Chat { .. }) => true,
        Some(Commands::DbServer { .. }) => true, // Runs in background, needs file logging
        _ => false,
    };

    // Determine base log level from CLI flags or config
    // Priority: --log-level flag > --verbose flag > config file > default
    // Default: WARN for stdio commands (always log errors/warnings), OFF for others
    let base_level: LevelFilter = if let Some(level) = cli.log_level {
        level.into()
    } else if cli.verbose {
        LevelFilter::DEBUG
    } else if let Some(config_level) = config.logging_level() {
        parse_log_level(&config_level).unwrap_or(if uses_stdio {
            LevelFilter::WARN
        } else {
            LevelFilter::OFF
        })
    } else if uses_stdio {
        LevelFilter::WARN // Default to WARN for chat/mcp (always capture errors)
    } else {
        LevelFilter::OFF
    };

    // Build env filter: RUST_LOG overrides, with base_level as fallback
    // This allows: RUST_LOG=crucible_rig=info,crucible_cli=debug cargo run -- chat
    let env_filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(base_level.into())
        .from_env_lossy();

    // Initialize logging based on command type
    if base_level != LevelFilter::OFF || std::env::var("RUST_LOG").is_ok() {
        if uses_stdio {
            // File-only logging for stdio-based commands (MCP, Chat)
            // Default to ~/.crucible/<command>.log, override with CRUCIBLE_LOG_FILE
            let log_file_name = match &cli.command {
                Some(Commands::Mcp { .. }) => "mcp.log",
                Some(Commands::Chat { .. }) => "chat.log",
                Some(Commands::DbServer { .. }) => "db-server.log",
                _ => "crucible.log",
            };

            let log_file_path = std::env::var("CRUCIBLE_LOG_FILE").unwrap_or_else(|_| {
                let home = dirs::home_dir().expect("Failed to get home directory");
                home.join(".crucible")
                    .join(log_file_name)
                    .to_string_lossy()
                    .to_string()
            });

            // Create parent directory if it doesn't exist
            if let Some(parent) = std::path::Path::new(&log_file_path).parent() {
                std::fs::create_dir_all(parent)?;
            }

            let log_file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file_path)?;

            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::sync::Arc::new(log_file))
                .with_ansi(false) // No ANSI codes in log files
                .with_target(true)
                .with_thread_ids(true);

            tracing_subscriber::registry()
                .with(file_layer)
                .with(env_filter)
                .init();
        } else {
            // Normal stderr logging for other commands
            tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .init();
        }
    }

    // Log configuration in verbose mode
    if cli.verbose {
        config.log_config();
    }

    // Note: Storage/Core initialization moved to individual commands that need it.
    // Creating it here caused database lock conflicts as multiple commands would
    // try to open the same RocksDB file. Each command now creates its own client
    // when needed, and the Arc-wrapped SurrealClient ensures cheap cloning.

    // Process any pending files on startup using integrated blocking processing
    // Skip for REPL mode, db-server (needs fast startup), or when explicitly disabled
    match &cli.command {
        None => {
            // Chat mode - process files in background like other commands
            debug!("No command specified, will use chat mode");
        }
        Some(Commands::DbServer { .. }) => {
            // db-server must start quickly - it's forked by ensure_daemon() which
            // waits with a timeout for the socket to become available. File processing
            // would delay socket binding and cause connection timeouts.
            debug!("db-server mode - skipping file processing for fast startup");
        }
        _ => {
            if cli.no_process {
                info!("File processing skipped due to --no-process flag");
                info!("CLI commands may operate on stale data");
            } else {
                // Process files before command execution to ensure up-to-date data
                debug!(
                    "Starting file processing with timeout: {} seconds",
                    cli.process_timeout
                );
                // Set timeout for file processing
                let timeout_duration = if cli.process_timeout == 0 {
                    None // No timeout
                } else {
                    Some(std::time::Duration::from_secs(cli.process_timeout))
                };

                let result = tokio::time::timeout(
                    timeout_duration.unwrap_or(std::time::Duration::from_secs(u64::MAX)),
                    process_files_with_change_detection(&config),
                )
                .await;

                match result {
                    Ok(process_result) => {
                        match process_result {
                            Ok(()) => {
                                debug!("File processing completed successfully");
                            }
                            Err(e) => {
                                error!("File processing failed: {}", e);
                                info!("CLI commands may operate on stale data");
                                // Continue execution even if processing fails (graceful degradation)
                            }
                        }
                    }
                    Err(_timeout_err) => {
                        warn!(
                            "File processing timed out after {} seconds",
                            cli.process_timeout
                        );
                        info!("CLI commands may operate on partially updated data");
                        // Continue execution even if processing times out (graceful degradation)
                    }
                }
            }
        }
    }

    // Execute command
    match cli.command {
        // New ACP-based commands
        Some(Commands::Chat {
            query,
            agent,
            resume,
            env,
            internal,
            provider,
            max_context,
            no_context,
            context_size,
            act,
            fullscreen,
        }) => {
            commands::chat::execute(
                config,
                agent,
                query,
                !act, // read_only = !act (plan mode is default)
                no_context,
                cli.no_process, // Pass the global --no-process flag
                Some(context_size),
                internal,
                provider,
                max_context,
                env,        // Pass env vars to agent spawning
                resume,     // Session ID to resume
                fullscreen, // Use fullscreen mode instead of inline viewport
            )
            .await?
        }

        Some(Commands::Mcp {
            stdio,
            port,
            kiln_path,
            just_dir,
            no_just,
            no_rune,
            log_file,
        }) => {
            let args = commands::mcp::McpArgs {
                stdio,
                port,
                kiln_path,
                just_dir,
                no_just,
                no_rune,
                log_file,
            };
            commands::mcp::execute(config, args).await?
        }

        Some(Commands::Process {
            path,
            force,
            watch,
            dry_run,
            parallel,
        }) => {
            commands::process::execute(config, path, force, watch, cli.verbose, dry_run, parallel)
                .await?
        }

        Some(Commands::Stats) => commands::stats::execute(config).await?,

        Some(Commands::Config(cmd)) => commands::config::execute(cmd).await?,

        Some(Commands::Status {
            path,
            format,
            detailed,
            recent,
        }) => commands::status::execute(config, path, format, detailed, recent).await?,

        Some(Commands::Storage(cmd)) => commands::storage::execute(config, cmd).await?,

        Some(Commands::Agents { command }) => commands::agents::execute(config, command).await?,

        Some(Commands::Tasks { file, command }) => {
            commands::tasks::execute(config, file, command).await?
        }

        Some(Commands::Daemon(cmd)) => {
            commands::daemon::handle(cmd).await?;
        }

        Some(Commands::Skills(cmd)) => {
            commands::skills::execute(config, cmd).await?;
        }

        Some(Commands::DbServer {
            socket,
            idle_timeout,
        }) => {
            commands::db_server::execute(config, socket, idle_timeout).await?;
        }

        Some(Commands::Init {
            path,
            force,
            interactive,
        }) => {
            commands::init::execute(path, force, interactive).await?;
        }

        Some(Commands::Session(cmd)) => {
            commands::session::execute(config, cmd).await?;
        }

        None => {
            // Default to chat when no command is provided
            commands::chat::execute(
                config,
                None,           // No agent specified - use default
                None,           // No query provided - start interactive mode
                true,           // read_only = true (plan mode is default)
                false,          // no_context = false
                cli.no_process, // Pass the global --no-process flag
                Some(5),        // default context_size = 5
                false,          // internal = false (use ACP agent by default)
                None,           // No provider specified
                16384,          // default max_context = 16384
                vec![],         // No env overrides
                None,           // No session to resume
                false,          // fullscreen = false (use inline mode by default)
            )
            .await?
        }
    }

    // Graceful shutdown: close all cached storage connections
    // This ensures RocksDB flushes WAL/SST files properly
    factories::shutdown_storage();

    Ok(())
}
