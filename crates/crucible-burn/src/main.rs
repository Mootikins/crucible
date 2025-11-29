use anyhow::Result;
use clap::Parser;
use tracing::{info};
use tracing_subscriber::prelude::*;

use crucible_burn::{
    cli::Cli,
    config::BurnConfig,
    hardware::HardwareInfo,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose { "debug" } else { "info" };
    let env_filter = format!("crucible_burn={},burn={}", log_level, log_level);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(env_filter)))
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting burn-test CLI");
    if cli.verbose {
        println!("Command: {:?}", cli.command);
    }

    // Load configuration
    let config = BurnConfig::load(cli.config.as_deref()).await?;
    if cli.verbose {
        println!("Loaded configuration: {:?}", config);
    }

    // Detect hardware and select backend
    let hardware_info = HardwareInfo::detect().await?;
    info!("Detected hardware: {:?}", hardware_info);

    // Process commands
    match cli.command {
        Some(command) => {
            crucible_burn::cli::handle_command(command, config, hardware_info).await?;
        }
        None => {
            // Default to showing hardware info
            println!("Burn ML Framework Testing Tool");
            println!("================================");
            println!();
            println!("Hardware Information:");
            println!("{:#?}", hardware_info);
            println!();
            println!("Use --help to see available commands");
        }
    }

    Ok(())
}