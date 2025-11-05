//! Kiln processing commands for CLI
//!
//! This module provides CLI commands for managing the crucible-kiln processor,
//! including starting, stopping, and checking status.

use crate::common::KilnProcessor;
use crate::config::CliConfig;
use anyhow::Result;
use tracing::{error, info};

/// Execute processor commands
pub async fn execute(config: CliConfig, command: ProcessCommands) -> Result<()> {
    info!("Executing processor command: {:?}", command);

    match command {
        ProcessCommands::Start { wait, background } => {
            execute_start_command(config, wait, background).await
        }
        ProcessCommands::Stop { force } => execute_stop_command(config, force).await,
        ProcessCommands::Status => execute_status_command(config).await,
        ProcessCommands::Restart { wait, force } => {
            execute_restart_command(config, wait, force).await
        }
    }
}

/// Execute start command
async fn execute_start_command(config: CliConfig, wait: bool, background: bool) -> Result<()> {
    info!(
        "Starting processor for kiln: {}",
        config.kiln.path.display()
    );

    println!("🚀 Starting kiln processor...");
    println!("📁 Kiln path: {}", config.kiln.path.display());

    // Initialize database connection
    let db_config = crucible_surrealdb::SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let client = match crucible_surrealdb::SurrealClient::new(db_config).await {
        Ok(client) => client,
        Err(e) => {
            error!("Failed to connect to database: {}", e);
            return Err(anyhow::anyhow!("Database connection failed: {}", e));
        }
    };

    let mut processor_manager = KilnProcessor::new();

    if background {
        println!("🔄 Starting processor in integrated mode...");
        match processor_manager.process_kiln_integrated(&config.kiln.path, &client).await {
            Ok(result) => {
                println!("✅ {}", result.status_message());
                println!("📊 {}", result.processing_info());
            }
            Err(e) => {
                error!("Failed to start processor: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("🔄 Starting processor in one-shot mode...");
        match processor_manager.process_kiln_integrated(&config.kiln.path, &client).await {
            Ok(result) => {
                println!("✅ {}", result.status_message());
                println!("📊 {}", result.processing_info());
            }
            Err(e) => {
                error!("Failed to start processor: {}", e);
                return Err(e);
            }
        }
    }

    if wait {
        println!("⏳ Waiting for processor to complete processing...");
        // The process_kiln_integrated already waits for completion
        println!("✅ Processing completed");
    }

    Ok(())
}

/// Execute stop command
async fn execute_stop_command(_config: CliConfig, force: bool) -> Result<()> {
    info!("Stopping processor (force: {})", force);

    if force {
        println!("🛑 Force stopping kiln processor...");
    } else {
        println!("🛑 Stopping kiln processor...");
    }

    // TODO: Implement processor stopping logic
    // This would require the processor to be running in background mode
    // with proper PID management and signal handling
    println!("⚠️  Processor stopping not yet implemented");
    println!("💡 Use Ctrl+C to stop running kiln processes");

    Ok(())
}

/// Execute status command
async fn execute_status_command(config: CliConfig) -> Result<()> {
    info!(
        "Checking processor status for kiln: {}",
        config.kiln.path.display()
    );

    println!("🔍 Checking processor status...");
    println!("📁 Kiln path: {}", config.kiln.path.display());

    // Check if embeddings exist (indirect processor status check)
    let processor_manager = KilnProcessor::new();

    // Initialize database connection to check embeddings
    let db_config = crucible_surrealdb::SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    match crucible_surrealdb::SurrealClient::new(db_config).await {
        Ok(client) => match processor_manager.check_embeddings_exist(&client).await {
            Ok(true) => {
                println!("✅ Kiln has been processed");
                println!("📊 Embeddings are available for semantic search");
            }
            Ok(false) => {
                println!("❌ No embeddings found");
                println!("💡 Run 'crucible processor start' to process the kiln");
            }
            Err(e) => {
                println!("⚠️  Could not check embeddings: {}", e);
            }
        },
        Err(e) => {
            println!("❌ Could not connect to database: {}", e);
            println!("💡 Make sure the processor has run at least once");
        }
    }

    Ok(())
}

/// Execute restart command
async fn execute_restart_command(config: CliConfig, wait: bool, force: bool) -> Result<()> {
    info!("Restarting processor (wait: {}, force: {})", wait, force);

    println!("🔄 Restarting kiln processor...");

    // Stop existing processor (if implemented)
    if force {
        println!("🛑 Force stopping any existing processor...");
    }

    // Start new processor
    execute_start_command(config, wait, false).await
}

#[derive(clap::Parser, Debug)]
pub enum ProcessCommands {
    /// Start the kiln processor
    Start {
        /// Wait for processor to complete processing
        #[arg(short, long)]
        wait: bool,

        /// Start processor in background mode
        #[arg(short, long)]
        background: bool,
    },

    /// Stop the kiln processor
    Stop {
        /// Force stop the processor
        #[arg(short, long)]
        force: bool,
    },

    /// Check processor status
    Status,

    /// Restart the kiln processor
    Restart {
        /// Wait for processor to complete processing
        #[arg(short, long)]
        wait: bool,

        /// Force restart the processor
        #[arg(short, long)]
        force: bool,
    },
}
