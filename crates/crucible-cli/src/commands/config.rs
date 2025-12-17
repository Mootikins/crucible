use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::ConfigCommands;
use crate::config::CliConfig;

/// Execute config subcommand
pub async fn execute(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { path, force } => init(path, force).await,
        ConfigCommands::Show { format, sources } => show(format, sources).await,
        ConfigCommands::Dump { format } => dump(format).await,
    }
}

/// Initialize a new config file
async fn init(path: Option<PathBuf>, force: bool) -> Result<()> {
    let config_path = path.unwrap_or_else(|| CliConfig::default_config_path());

    // Check if file already exists
    if config_path.exists() && !force {
        println!(
            "{} Config file already exists at: {}",
            "Error:".red().bold(),
            config_path.display()
        );
        println!("Use {} to overwrite", "--force".yellow());
        return Ok(());
    }

    // Create the config file
    CliConfig::create_example(&config_path)?;

    println!(
        "{} Created config file at: {}",
        "Success:".green().bold(),
        config_path.display()
    );
    println!(
        "\n{}",
        "Edit this file to configure your kiln settings.".dimmed()
    );
    println!(
        "{}",
        "Default values will be used until you customize the config.".dimmed()
    );

    Ok(())
}

/// Show the current effective configuration
async fn show(format: String, sources: bool) -> Result<()> {
    // Load the current config (with all precedence applied)
    let config = CliConfig::load(None, None, None)?;

    if sources {
        match format.as_str() {
            "json" => {
                let json = config.display_as_json_with_sources()?;
                println!("{}", json);
            }
            "toml" | _ => {
                let toml = config.display_as_toml_with_sources()?;
                println!("{}", toml);
            }
        }
    } else {
        match format.as_str() {
            "json" => {
                let json = config.display_as_json()?;
                println!("{}", json);
            }
            "toml" | _ => {
                let toml = config.display_as_toml()?;
                println!("{}", toml);
            }
        }
    }

    Ok(())
}

/// Dump default configuration to stdout
async fn dump(format: String) -> Result<()> {
    // Create default config
    let config = CliConfig::default();

    match format.as_str() {
        "json" => {
            let json = config.display_as_json()?;
            println!("{}", json);
        }
        "toml" | _ => {
            let toml = config.display_as_toml()?;
            println!("{}", toml);
        }
    }

    Ok(())
}
