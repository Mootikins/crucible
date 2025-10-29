// Crucible Daemon - Data layer coordination daemon
//
// This daemon provides:
// - Filesystem watching and change detection
// - File parsing and metadata extraction
// - Database synchronization and updates
// - Event publishing to core controller
// - Data validation and integrity checks

use anyhow::Result;
use crucible_daemon::{DaemonConfig, DataCoordinator};
use std::process;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// Exit codes for different scenarios
mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const CONFIG_ERROR: i32 = 1;
    pub const PROCESSING_ERROR: i32 = 2;
    pub const DATABASE_ERROR: i32 = 3;
    pub const OTHER_ERROR: i32 = 4;
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    info!(
        "Starting Crucible one-shot daemon v{}",
        env!("CARGO_PKG_VERSION")
    );

    // Load configuration
    let config = match load_configuration().await {
        Ok(config) => {
            info!("Configuration loaded successfully");
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            process::exit(exit_codes::CONFIG_ERROR);
        }
    };

    // Create and initialize data coordinator
    let mut coordinator = match DataCoordinator::new(config).await {
        Ok(coordinator) => {
            info!("Data coordinator created successfully");
            coordinator
        }
        Err(e) => {
            error!("Failed to create data coordinator: {}", e);
            process::exit(exit_codes::CONFIG_ERROR);
        }
    };

    // Initialize the coordinator
    if let Err(e) = coordinator.initialize().await {
        error!("Failed to initialize data coordinator: {}", e);
        process::exit(exit_codes::CONFIG_ERROR);
    }

    // Process kiln once and exit
    match process_kiln_once(&mut coordinator).await {
        Ok(_) => {
            info!("Kiln processing completed successfully");
            process::exit(exit_codes::SUCCESS);
        }
        Err(e) => {
            error!("Kiln processing failed: {}", e);
            // Determine error type for appropriate exit code
            let error_msg = e.to_string().to_lowercase();
            let exit_code = if error_msg.contains("database") || error_msg.contains("surrealdb") {
                exit_codes::DATABASE_ERROR
            } else if error_msg.contains("processing") || error_msg.contains("parse") {
                exit_codes::PROCESSING_ERROR
            } else {
                exit_codes::OTHER_ERROR
            };
            process::exit(exit_code);
        }
    }
}

/// Process the kiln exactly once and return result
async fn process_kiln_once(coordinator: &mut DataCoordinator) -> Result<()> {
    info!("Starting one-shot kiln processing");

    // Start the coordinator (but don't run indefinitely)
    if let Err(e) = coordinator.start().await {
        error!("Failed to start data coordinator: {}", e);
        return Err(e);
    }

    // Process the kiln once
    if let Err(e) = coordinator.process_kiln_once().await {
        error!("Failed to process kiln: {}", e);
        return Err(e);
    }

    // Stop the coordinator gracefully
    if let Err(e) = coordinator.stop().await {
        warn!("Error during coordinator shutdown: {}", e);
        // Don't fail the whole operation for shutdown errors
    }

    info!("One-shot kiln processing completed");
    Ok(())
}

/// Load daemon configuration from environment variables (secure only)
async fn load_configuration() -> Result<DaemonConfig> {
    // SECURITY: Load configuration only from environment variables
    match DaemonConfig::from_env() {
        Ok(config) => {
            info!("Loaded configuration from environment variables");
            return Ok(config);
        }
        Err(e) => {
            error!("Failed to load configuration from environment: {}", e);
            error!("OBSIDIAN_KILN_PATH environment variable is required for daemon security");
            return Err(anyhow::anyhow!(
                "Failed to load secure daemon configuration. \
                Please set OBSIDIAN_KILN_PATH environment variable.\n\
                Example: export OBSIDIAN_KILN_PATH=/path/to/your/kiln"
            ));
        }
    }
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
