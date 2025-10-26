//! Daemon management commands for CLI
//!
//! This module provides CLI commands for managing the crucible-daemon process,
//! including starting, stopping, and checking status.

use crate::common::DaemonManager;
use crate::config::CliConfig;
use anyhow::Result;
use tracing::{error, info};

/// Execute daemon commands
pub async fn execute(config: CliConfig, command: DaemonCommands) -> Result<()> {
    info!("Executing daemon command: {:?}", command);

    match command {
        DaemonCommands::Start { wait, background } => {
            execute_start_command(config, wait, background).await
        }
        DaemonCommands::Stop { force } => execute_stop_command(config, force).await,
        DaemonCommands::Status => execute_status_command(config).await,
        DaemonCommands::Restart { wait, force } => {
            execute_restart_command(config, wait, force).await
        }
    }
}

/// Execute start command
async fn execute_start_command(config: CliConfig, wait: bool, background: bool) -> Result<()> {
    info!("Starting daemon for kiln: {}", config.kiln.path.display());

    println!("🚀 Starting crucible-daemon...");
    println!("📁 Kiln path: {}", config.kiln.path.display());

    let mut daemon_manager = DaemonManager::new();

    if background {
        println!("🔄 Starting daemon in background mode...");
        // For now, we'll use the existing spawn_daemon_for_processing method
        // In the future, this could start a persistent background daemon
        match daemon_manager
            .spawn_daemon_for_processing(&config.kiln.path)
            .await
        {
            Ok(result) => {
                println!("✅ {}", result.status_message());
                println!("📊 {}", result.processing_info());
            }
            Err(e) => {
                error!("Failed to start daemon: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("🔄 Starting daemon in one-shot mode...");
        match daemon_manager
            .spawn_daemon_for_processing(&config.kiln.path)
            .await
        {
            Ok(result) => {
                println!("✅ {}", result.status_message());
                println!("📊 {}", result.processing_info());
            }
            Err(e) => {
                error!("Failed to start daemon: {}", e);
                return Err(e);
            }
        }
    }

    if wait {
        println!("⏳ Waiting for daemon to complete processing...");
        // The spawn_daemon_for_processing already waits for completion
        println!("✅ Daemon processing completed");
    }

    Ok(())
}

/// Execute stop command
async fn execute_stop_command(_config: CliConfig, force: bool) -> Result<()> {
    info!("Stopping daemon (force: {})", force);

    if force {
        println!("🛑 Force stopping crucible-daemon...");
    } else {
        println!("🛑 Stopping crucible-daemon...");
    }

    // TODO: Implement daemon stopping logic
    // This would require the daemon to be running in background mode
    // with proper PID management and signal handling
    println!("⚠️  Daemon stopping not yet implemented");
    println!("💡 Use Ctrl+C to stop running daemon processes");

    Ok(())
}

/// Execute status command
async fn execute_status_command(config: CliConfig) -> Result<()> {
    info!(
        "Checking daemon status for kiln: {}",
        config.kiln.path.display()
    );

    println!("🔍 Checking daemon status...");
    println!("📁 Kiln path: {}", config.kiln.path.display());

    // Check if embeddings exist (indirect daemon status check)
    let daemon_manager = DaemonManager::new();

    // Initialize database connection to check embeddings
    let db_config = crucible_surrealdb::SurrealDbConfig {
        namespace: "crucible".to_string(),
        database: "vault".to_string(),
        path: config.database_path_str()?,
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    match crucible_surrealdb::SurrealClient::new(db_config).await {
        Ok(client) => match daemon_manager.check_embeddings_exist(&client).await {
            Ok(true) => {
                println!("✅ Daemon has processed this kiln");
                println!("📊 Embeddings are available for semantic search");
            }
            Ok(false) => {
                println!("❌ No embeddings found");
                println!("💡 Run 'crucible daemon start' to process the kiln");
            }
            Err(e) => {
                println!("⚠️  Could not check embeddings: {}", e);
            }
        },
        Err(e) => {
            println!("❌ Could not connect to database: {}", e);
            println!("💡 Make sure the daemon has run at least once");
        }
    }

    Ok(())
}

/// Execute restart command
async fn execute_restart_command(config: CliConfig, wait: bool, force: bool) -> Result<()> {
    info!("Restarting daemon (wait: {}, force: {})", wait, force);

    println!("🔄 Restarting crucible-daemon...");

    // Stop existing daemon (if implemented)
    if force {
        println!("🛑 Force stopping any existing daemon...");
    }

    // Start new daemon
    execute_start_command(config, wait, false).await
}

#[derive(clap::Parser, Debug)]
pub enum DaemonCommands {
    /// Start the daemon process
    Start {
        /// Wait for daemon to complete processing
        #[arg(short, long)]
        wait: bool,

        /// Start daemon in background mode
        #[arg(short, long)]
        background: bool,
    },

    /// Stop the daemon process
    Stop {
        /// Force stop the daemon
        #[arg(short, long)]
        force: bool,
    },

    /// Check daemon status
    Status,

    /// Restart the daemon process
    Restart {
        /// Wait for daemon to complete processing
        #[arg(short, long)]
        wait: bool,

        /// Force restart the daemon
        #[arg(short, long)]
        force: bool,
    },
}
