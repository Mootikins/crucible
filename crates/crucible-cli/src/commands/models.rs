use crate::config::CliConfig;
use crate::formatting::OutputFormat;
use anyhow::{Context, Result};
use serde::Serialize;

use crate::common::daemon_client;

#[derive(Debug, Serialize)]
pub struct ModelOutput {
    pub name: String,
    pub provider: Option<String>,
    pub parameter_count: Option<u64>,
}

pub async fn execute(config: CliConfig, format: OutputFormat) -> Result<()> {
    eprintln!("Fetching models from daemon...");

    let client = daemon_client().await?;

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

    /// Both human formats end with this. `table` became the default when the
    /// format vocabulary was typed, and the hint lived only in the plain arm —
    /// so for a moment the default output stopped telling anyone how to use it.
    fn print_switch_hint() {
        println!("\nSwitch model in chat with: :model <name>");
        println!("Or start chat with: cru chat --model <name>");
    }

    match format {
        OutputFormat::Json => {
            let output: Vec<ModelOutput> = models
                .iter()
                .map(|m| ModelOutput {
                    name: m.clone(),
                    provider: None,
                    parameter_count: None,
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        OutputFormat::Table => {
            let rows: Vec<Vec<String>> = models.iter().map(|m| vec![m.clone()]).collect();
            println!("{}", crate::output::records_table(&["Model"], &rows));
            print_switch_hint();
        }
        OutputFormat::Plain => {
            println!("\nAvailable models ({}):\n", models.len());
            for model in &models {
                println!("  {}", model);
            }
            print_switch_hint();
        }
    }

    Ok(())
}
