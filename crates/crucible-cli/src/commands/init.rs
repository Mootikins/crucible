use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task;

use crate::config::CliConfig;

/// Execute init command
pub async fn execute(path: Option<PathBuf>, force: bool) -> Result<()> {
    // Determine the target path
    let target_path = path.unwrap_or_else(|| PathBuf::from("."));
    let crucible_dir = target_path.join(".crucible");

    // Check if .crucible directory already exists
    if crucible_dir.exists() && !force {
        println!(
            "{} Kiln already initialized at: {}",
            "Error:".red().bold(),
            crucible_dir.display()
        );
        println!("Use {} to reinitialize", "--force".yellow());
        return Ok(());
    }

    // Create the kiln directories and files
    task::spawn_blocking(move || create_kiln(&crucible_dir, force)).await??;

    println!(
        "{} Kiln initialized at: {}",
        "Success:".green().bold(),
        target_path.display()
    );

    Ok(())
}

/// Create kiln directories and config file in a blocking context
fn create_kiln(crucible_dir: &Path, force: bool) -> Result<()> {
    // Create .crucible directory if force flag is set or if it doesn't exist
    if force {
        // Remove existing directory if it exists
        if crucible_dir.exists() {
            fs::remove_dir_all(crucible_dir)?;
        }
    }

    // Create .crucible directory
    fs::create_dir_all(crucible_dir)?;

    // Create sessions directory
    let sessions_dir = crucible_dir.join("sessions");
    fs::create_dir_all(&sessions_dir)?;

    // Create plugins directory
    let plugins_dir = crucible_dir.join("plugins");
    fs::create_dir_all(&plugins_dir)?;

    // Create config.toml with minimal defaults
    let config_path = crucible_dir.join("config.toml");
    let config_content = generate_minimal_config();
    fs::write(&config_path, config_content)?;

    Ok(())
}

/// Generate minimal config.toml content
fn generate_minimal_config() -> String {
    r#"# Crucible kiln configuration
# See https://github.com/mootless/crucible for options

[kiln]
# Kiln path is automatically set to current directory

[storage]
# Storage backend (sqlite recommended)
backend = "sqlite"

[llm]
# LLM provider configuration
# Run `cru config dump` to see all available options
"#
    .to_string()
}