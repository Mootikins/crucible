use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{debug, error, info, warn, Level};
use tracing_subscriber::prelude::*;  // For SubscriberExt trait

use crucible_cli::{
    cli::{Cli, Commands},
    commands, config,
};
use crucible_core::{types::hashing::HashAlgorithm, CrucibleCore};
use crucible_core::traits::KnowledgeRepository;
use crucible_llm::embeddings::{create_provider, EmbeddingConfig, EmbeddingProvider};
use crucible_surrealdb::{adapters, SurrealDbConfig};

/// Process files using the integrated ChangeDetectionService
///
/// This function uses the new ChangeDetectionService that integrates
/// FileScanningService with ChangeDetector and SurrealHashLookup for
/// efficient selective processing based on ChangeSet.
// Streamlined for Phase 5: File processing disabled (removed ChangeDetectionService dependency)
async fn process_files_with_change_detection(_config: &crate::config::CliConfig) -> Result<()> {
    debug!("File processing disabled for Phase 5 integration testing");
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on command type
    // MCP and Chat use stdio (stdin/stdout) for JSON-RPC, so we must avoid stderr output
    // These commands log to file instead to preserve debugging capability
    let uses_stdio = matches!(
        &cli.command,
        Some(Commands::Mcp) | Some(Commands::Chat { .. })
    );

    let log_level = if cli.verbose { "debug" } else { "info" };
    let env_filter = format!("crucible_cli={},crucible_services={}", log_level, log_level);

    if uses_stdio {
        // File-only logging for stdio-based commands (MCP, Chat)
        // Default to ~/.crucible/<command>.log, override with CRUCIBLE_LOG_FILE
        let log_file_name = match &cli.command {
            Some(Commands::Mcp) => "mcp.log",
            Some(Commands::Chat { .. }) => "chat.log",
            _ => "crucible.log",
        };

        let log_file_path = std::env::var("CRUCIBLE_LOG_FILE")
            .unwrap_or_else(|_| {
                let home = dirs::home_dir().expect("Failed to get home directory");
                home.join(".crucible").join(log_file_name).to_string_lossy().to_string()
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
            .with_ansi(false)  // No ANSI codes in log files
            .with_target(true)
            .with_thread_ids(true);

        tracing_subscriber::registry()
            .with(file_layer)
            .with(tracing_subscriber::EnvFilter::new(env_filter))
            .init();
    } else {
        // Normal stderr logging for other commands
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(env_filter))
            .init();
    }

    // Load configuration with CLI overrides
    let mut config = config::CliConfig::load(cli.config, cli.embedding_url, cli.embedding_model)?;

    // Apply database path override if provided
    if let Some(db_path) = cli.db_path {
        config.custom_database_path = Some(db_path.into());
    }

    // Note: Storage/Core initialization moved to individual commands that need it.
    // Creating it here caused database lock conflicts as multiple commands would
    // try to open the same RocksDB file. Each command now creates its own client
    // when needed, and the Arc-wrapped SurrealClient ensures cheap cloning.

    // Process any pending files on startup using integrated blocking processing
    // Skip for interactive fuzzy picker, REPL mode, or when explicitly disabled
    match &cli.command {
        Some(Commands::Fuzzy { .. }) => {
            // Skip processing - fuzzy is interactive and users want immediate results
            debug!("Skipping file processing for fuzzy search command");
        }
        None => {
            // Chat mode - process files in background like other commands
            debug!("No command specified, will use chat mode");
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
            no_context,
            context_size,
            act,
        }) => {
            commands::chat::execute(
                config,
                agent,
                query,
                !act,  // read_only = !act (plan mode is default)
                no_context,
                cli.no_process,  // Pass the global --no-process flag
                Some(context_size),
            )
            .await?
        }

        Some(Commands::Mcp) => {
            commands::mcp::execute(config).await?
        }

        Some(Commands::Process { path, force, watch }) => {
            commands::process::execute(config, path, force, watch).await?
        }

        // Existing commands
        Some(Commands::Search {
            query,
            limit,
            format,
            show_content,
        }) => commands::search::execute(config, query, limit, format, show_content).await?,

        Some(Commands::Fuzzy {
            query,
            content: _, // keep for future use
            tags: _,    // keep for future use
            paths: _,   // keep for future use
            limit,
        }) => {
            // Always use interactive mode
            commands::fuzzy_interactive::execute(config, query.unwrap_or_default(), limit).await?
        }

  
        
        Some(Commands::Stats) => commands::stats::execute(config).await?,

  
        Some(Commands::Config(cmd)) => commands::config::execute(cmd).await?,

    
        Some(Commands::Diff {
            path1,
            path2,
            format,
            show_similarity,
            show_unchanged,
            max_depth,
        }) => {
            commands::diff::execute(
                config,
                path1,
                path2,
                format,
                show_similarity,
                show_unchanged,
                max_depth,
            )
            .await?
        }

        Some(Commands::Status {
            path,
            format,
            detailed,
            recent,
        }) => commands::status::execute(config, path, format, detailed, recent).await?,

        Some(Commands::Storage(cmd)) => commands::storage::execute(config, cmd).await?,

        Some(Commands::Parse {
            path,
            format,
            show_tree,
            show_blocks,
            max_depth,
            continue_on_error,
        }) => {
            commands::parse::execute(
                config,
                path,
                format,
                show_tree,
                show_blocks,
                max_depth,
                continue_on_error,
            )
            .await?
        }

        // Commands::EnhancedChat { // Temporarily disabled
        //     agent,
        //     model,
        //     temperature,
        //     max_tokens,
        //     performance_tracking,
        //     start_message,
        //     history,
        // } => commands::enhanced_chat_session::execute(
        //     config,
        //     agent,
        //     model,
        //     temperature,
        //     max_tokens,
        //     performance_tracking,
        //     start_message,
        //     history,
        // ).await?,

        // Commands::Agent(cmd) => commands::agent_management::execute(config, cmd).await?, // Temporarily disabled
        None => {
            // Default to chat when no command is provided
            commands::chat::execute(
                config,
                None,  // No query provided - start interactive mode
                false, // not act mode by default (plan mode)
                cli.agent,
                cli.no_context,
                cli.context_size,
            ).await?
        }
    }

    Ok(())
}
