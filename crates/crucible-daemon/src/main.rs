// Crucible Daemon - Terminal-first knowledge management daemon
//
// This daemon provides:
// - File watching and indexing
// - SurrealQL REPL for queries
// - Tool execution (built-in + Rune scripts)
// - Real-time logging TUI

use anyhow::Result;
use tokio::sync::watch;
use tracing::{info, error};

use crucible_daemon::repl::{Repl, ReplConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    info!("Starting Crucible daemon");

    // Create shutdown channel
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    // Load configuration (use defaults for now)
    let config = ReplConfig::default();

    // Create and run REPL
    let mut repl = Repl::new(config, shutdown_tx).await?;

    // Run REPL in main task, listen for shutdown in background
    tokio::select! {
        result = repl.run() => {
            if let Err(e) = result {
                error!("REPL error: {}", e);
                return Err(e);
            }
        }
        _ = shutdown_rx.changed() => {
            info!("Shutdown signal received");
        }
    }

    info!("Crucible daemon exiting");
    Ok(())
}
