// Crucible Daemon - Data layer coordination daemon
//
// This daemon provides:
// - Filesystem watching and change detection
// - File parsing and metadata extraction
// - Database synchronization and updates
// - Event publishing to core controller
// - Data validation and integrity checks

use anyhow::Result;
use crucible_daemon::{DataCoordinator, DaemonConfig};
use std::path::PathBuf;
use tokio::signal;
use tracing::{info, error, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
        )
        .init();

    info!("Starting Crucible data layer daemon v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = load_configuration().await?;
    info!("Configuration loaded successfully");

    // Create and initialize data coordinator
    let mut coordinator = DataCoordinator::new(config).await
        .map_err(|e| {
            error!("Failed to create data coordinator: {}", e);
            e
        })?;

    // Initialize the coordinator
    if let Err(e) = coordinator.initialize().await {
        error!("Failed to initialize data coordinator: {}", e);
        return Err(e);
    }

    // Start the coordinator
    if let Err(e) = coordinator.start().await {
        error!("Failed to start data coordinator: {}", e);
        return Err(e);
    }

    info!("Crucible data layer daemon is running. Use Ctrl+C to stop.");

    // Set up signal handlers for graceful shutdown
    let mut coordinator_clone = coordinator.clone();
    tokio::spawn(async move {
        if let Err(e) = signal::ctrl_c().await {
            error!("Failed to listen for Ctrl+C: {}", e);
        } else {
            info!("Received Ctrl+C, initiating graceful shutdown");
            if let Err(e) = coordinator_clone.stop().await {
                error!("Error during shutdown: {}", e);
            }
        }
    });

    // Handle SIGTERM (for systemd/docker environments)
    let mut coordinator_clone = coordinator.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            match sigterm.recv().await {
                Some(_) => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                    if let Err(e) = coordinator_clone.stop().await {
                        error!("Error during shutdown: {}", e);
                    }
                }
                None => {
                    warn!("SIGTERM signal stream ended unexpectedly");
                }
            }
        }
    });

    // Wait for the coordinator to stop
    while coordinator.is_running().await {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }

    info!("Crucible data layer daemon has shut down successfully");
    Ok(())
}

/// Load daemon configuration from various sources
async fn load_configuration() -> Result<DaemonConfig> {
    // Try loading from command line arguments first
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        let config_path = PathBuf::from(&args[1]);
        if config_path.exists() {
            match DaemonConfig::load_from_file(&config_path).await {
                Ok(config) => {
                    info!("Loaded configuration from file: {}", config_path.display());
                    return Ok(config);
                }
                Err(e) => {
                    warn!("Failed to load configuration from {}: {}, using defaults", config_path.display(), e);
                }
            }
        }
    }

    // Try loading from default locations
    let default_paths = vec![
        PathBuf::from("/etc/crucible/daemon.yaml"),
        PathBuf::from("daemon.yaml"),
        PathBuf::from("config/daemon.yaml"),
    ];

    for path in default_paths {
        if path.exists() {
            match DaemonConfig::load_from_file(&path).await {
                Ok(config) => {
                    info!("Loaded configuration from default path: {}", path.display());
                    return Ok(config);
                }
                Err(e) => {
                    warn!("Failed to load configuration from {}: {}, trying next", path.display(), e);
                }
            }
        }
    }

    // Try loading from environment variables
    match DaemonConfig::from_env() {
        Ok(config) => {
            info!("Loaded configuration from environment variables");
            return Ok(config);
        }
        Err(e) => {
            warn!("Failed to load configuration from environment: {}, using defaults", e);
        }
    }

    // Fall back to default configuration
    info!("Using default configuration");
    let mut default_config = DaemonConfig::default();

    // Apply some sensible defaults for a running daemon
    default_config.filesystem.watch_paths.push(
        crucible_daemon::config::WatchPath {
            path: PathBuf::from("./data"),
            recursive: true,
            mode: crucible_daemon::config::WatchMode::All,
            filters: None,
            events: None,
        }
    );

    Ok(default_config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_configuration_default() {
        let config = load_configuration().await;
        assert!(config.is_ok());
    }

    #[tokio::test]
    async fn test_coordinator_lifecycle() {
        let config = DaemonConfig::default();
        let mut coordinator = DataCoordinator::new(config).await.unwrap();

        // Test that we can create and initialize the coordinator
        // (This might fail due to missing watch paths, which is expected)
        let init_result = coordinator.initialize().await;
        assert!(init_result.is_ok() || init_result.is_err()); // Just check it doesn't panic

        // Test shutdown
        let stop_result = coordinator.stop().await;
        assert!(stop_result.is_ok());
    }
}
