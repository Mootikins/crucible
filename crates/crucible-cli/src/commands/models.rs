use crate::config::CliConfig;
use anyhow::{Context, Result};
use crucible_rpc::DaemonClient;

pub async fn execute(config: CliConfig) -> Result<()> {
    eprintln!("Fetching models from daemon...");

    let client = DaemonClient::connect_or_start()
        .await
        .context("Failed to connect to daemon. Is it running? Try: cru daemon start")?;

    let kiln_path = &config.kiln_path;
    let models = client
        .list_all_models(Some(kiln_path.as_path()))
        .await
        .context("Failed to list models from daemon")?;

    if models.is_empty() {
        eprintln!("No models available.");
        eprintln!("\nTroubleshooting:");
        eprintln!("  - Check if the provider is running/accessible");
        eprintln!("  - Verify endpoint in config: cru config show");
        return Ok(());
    }

    println!("\nAvailable models ({}):\n", models.len());
    for model in &models {
        println!("  {}", model);
    }

    println!("\nSwitch model in chat with: :model <name>");
    println!("Or start chat with: cru chat --model <name>");

    Ok(())
}
