use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{debug, info, warn, error};

use crucible_cli::{
    cli::{Cli, Commands},
    commands, config,
};
use crucible_core::CrucibleCore;
use crucible_surrealdb::{SurrealClient, SurrealDbConfig};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    let env_filter = format!("crucible_cli={},crucible_services={}", log_level, log_level);
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(env_filter))
        .init();

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
    // Skip for interactive fuzzy picker or when explicitly disabled
    match &cli.command {
        Some(Commands::Fuzzy { .. }) => {
            // Skip processing - fuzzy is interactive and users want immediate results
            debug!("Skipping file processing for fuzzy search command");
        }
        _ => {
            if cli.no_process {
                info!("⚡ File processing skipped due to --no-process flag");
                info!("ℹ️  CLI commands may operate on stale data");
            } else {
                // Process files before command execution to ensure up-to-date data
                debug!("Starting file processing with timeout: {} seconds", cli.process_timeout);
                // Set timeout for file processing
                let timeout_duration = if cli.process_timeout == 0 {
                    None // No timeout
                } else {
                    Some(std::time::Duration::from_secs(cli.process_timeout))
                };

                let result = tokio::time::timeout(
                    timeout_duration.unwrap_or(std::time::Duration::from_secs(u64::MAX)),
                    crucible_cli::common::kiln_processor::process_files_on_startup(&config)
                ).await;

                match result {
                    Ok(process_result) => {
                        match process_result {
                            Ok(()) => {
                                debug!("File processing completed successfully");
                            }
                            Err(e) => {
                                error!("❌ File processing failed: {}", e);
                                info!("⚠️  CLI commands may operate on stale data");
                                // Continue execution even if processing fails (graceful degradation)
                            }
                        }
                    }
                    Err(timeout_err) => {
                        warn!("⏱️  File processing timed out after {} seconds", cli.process_timeout);
                        info!("⚠️  CLI commands may operate on partially updated data");
                        // Continue execution even if processing times out (graceful degradation)
                    }
                }
            }
        }
    }

    // Execute command (default to REPL if no command provided)
    match cli.command {
        Some(Commands::Search {
            query,
            limit,
            format,
            show_content,
        }) => commands::search::execute(config, query, limit, format, show_content).await?,

        Some(Commands::Fuzzy {
            query,
            content: _,  // keep for future use
            tags: _,     // keep for future use
            paths: _,    // keep for future use
            limit,
        }) => {
            // Always use interactive mode
            commands::fuzzy_interactive::execute(
                config,
                query.unwrap_or_default(),
                limit,
            )
            .await?
        }

        Some(Commands::Semantic {
            query,
            top_k,
            format,
            show_scores,
        }) => commands::semantic::execute(config, query, top_k, format, show_scores).await?,

        Some(Commands::Note(cmd)) => commands::note::execute(config, cmd).await?,

        Some(Commands::Stats) => commands::stats::execute(config).await?,

        Some(Commands::Test) => commands::test_tools::execute(config).await?,

        Some(Commands::Config(cmd)) => commands::config::execute(cmd).await?,

        Some(Commands::Process(cmd)) => commands::process::execute(config, cmd).await?,

        Some(Commands::Diff {
            path1,
            path2,
            format,
            show_similarity,
            show_unchanged,
            max_depth,
        }) => commands::diff::execute(config, path1, path2, format, show_similarity, show_unchanged, max_depth).await?,

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
        }) => commands::parse::execute(config, path, format, show_tree, show_blocks, max_depth, continue_on_error).await?,

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
            // Default to REPL when no command is provided
            // Create storage only for REPL to avoid lock conflicts with other commands
            let storage_config = SurrealDbConfig {
                path: config.database_path_str()?,
                namespace: "crucible".to_string(),
                database: "kiln".to_string(),
                max_connections: Some(10),
                timeout_seconds: Some(30),
            };

            let storage = SurrealClient::new(storage_config)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create storage: {}", e))?;

            let core = Arc::new(
                CrucibleCore::builder()
                    .with_storage(storage)
                    .build()
                    .map_err(|e| anyhow::anyhow!("Failed to build CrucibleCore: {}", e))?
            );

            commands::repl::execute(core, config, cli.non_interactive).await?
        }
    }

    Ok(())
}
