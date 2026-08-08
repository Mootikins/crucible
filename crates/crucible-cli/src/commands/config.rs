use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::ConfigCommands;
use crate::config::CliConfig;
use crate::output;

/// Execute config subcommand
///
/// `config` takes the already-resolved config rather than re-loading it: the
/// resolution in `async_main` is what honours `--config/-C`, the embedding
/// overrides, and the first-run wizard's rewrite.
pub async fn execute(config: CliConfig, cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { path, force } => init(path, force).await,
        ConfigCommands::Show { format, sources } => {
            println!("{}", render(&config, &format, sources)?);
            Ok(())
        }
        ConfigCommands::Dump { format } => dump(format).await,
    }
}

/// Initialize a new config file
async fn init(path: Option<PathBuf>, force: bool) -> Result<()> {
    let config_path = path.unwrap_or_else(CliConfig::default_config_path);

    // Check if file already exists
    if config_path.exists() && !force {
        output::warning(&format!(
            "Config file already exists at: {}",
            config_path.display()
        ));
        println!(
            "  {} Try: `cru config init --force` to overwrite",
            "→".cyan()
        );
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

/// Render the effective configuration for `config show`
fn render(config: &CliConfig, format: &str, sources: bool) -> Result<String> {
    Ok(match (format, sources) {
        ("json", true) => config.display_as_json_with_sources()?,
        ("json", false) => config.display_as_json()?,
        (_, true) => config.display_as_toml_with_sources()?,
        (_, false) => config.display_as_toml()?,
    })
}

/// Dump default configuration to stdout
async fn dump(format: String) -> Result<()> {
    println!("{}", render(&CliConfig::default(), &format, false)?);
    Ok(())
}
