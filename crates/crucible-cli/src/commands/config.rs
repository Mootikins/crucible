use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;

use crate::cli::ConfigCommands;
use crate::config::CliConfig;

/// Execute config subcommand
pub async fn execute(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { path, force } => init(path, force).await,
        ConfigCommands::Show { format } => show(format).await,
        ConfigCommands::MigrateEnvVars { output, dry_run } => {
            migrate_env_vars(output, dry_run).await
        }
    }
}

/// Initialize a new config file
async fn init(path: Option<PathBuf>, force: bool) -> Result<()> {
    let config_path = path
        .or_else(|| CliConfig::default_config_path().ok())
        .context("Could not determine config file path")?;

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
async fn show(format: String) -> Result<()> {
    // Load the current config (with all precedence applied)
    let config = CliConfig::load(None, None, None)?;

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

/// Migrate environment variable configuration to config file
async fn migrate_env_vars(output: Option<PathBuf>, dry_run: bool) -> Result<()> {
    println!(
        "{}",
        "🔍 Scanning for environment variables...".cyan().bold()
    );
    println!();

    let mut found_vars = Vec::new();
    let mut config = CliConfig::builder();

    // Check for kiln path
    if let Ok(path) = std::env::var("OBSIDIAN_KILN_PATH") {
        println!("  {} OBSIDIAN_KILN_PATH = {}", "✓".green(), path.yellow());
        config = config.kiln_path(&path);
        found_vars.push(("OBSIDIAN_KILN_PATH", path));
    }

    // Check for embedding configuration
    if let Ok(endpoint) = std::env::var("EMBEDDING_ENDPOINT") {
        println!(
            "  {} EMBEDDING_ENDPOINT = {}",
            "✓".green(),
            endpoint.yellow()
        );
        config = config.embedding_url(&endpoint);
        found_vars.push(("EMBEDDING_ENDPOINT", endpoint));
    }
    if let Ok(model) = std::env::var("EMBEDDING_MODEL") {
        println!("  {} EMBEDDING_MODEL = {}", "✓".green(), model.yellow());
        config = config.embedding_model(&model);
        found_vars.push(("EMBEDDING_MODEL", model));
    }

    // Check for LLM configuration
    if let Ok(model) = std::env::var("CRUCIBLE_CHAT_MODEL") {
        println!("  {} CRUCIBLE_CHAT_MODEL = {}", "✓".green(), model.yellow());
        config = config.chat_model(&model);
        found_vars.push(("CRUCIBLE_CHAT_MODEL", model));
    }
    if let Ok(temp) = std::env::var("CRUCIBLE_TEMPERATURE") {
        if let Ok(temp_f32) = temp.parse::<f32>() {
            println!("  {} CRUCIBLE_TEMPERATURE = {}", "✓".green(), temp.yellow());
            config = config.temperature(temp_f32);
            found_vars.push(("CRUCIBLE_TEMPERATURE", temp));
        }
    }
    if let Ok(tokens) = std::env::var("CRUCIBLE_MAX_TOKENS") {
        if let Ok(tokens_u32) = tokens.parse::<u32>() {
            println!(
                "  {} CRUCIBLE_MAX_TOKENS = {}",
                "✓".green(),
                tokens.yellow()
            );
            config = config.max_tokens(tokens_u32);
            found_vars.push(("CRUCIBLE_MAX_TOKENS", tokens));
        }
    }
    if let Ok(prompt) = std::env::var("CRUCIBLE_SYSTEM_PROMPT") {
        println!(
            "  {} CRUCIBLE_SYSTEM_PROMPT = {}",
            "✓".green(),
            prompt.yellow()
        );
        config = config.system_prompt(&prompt);
        found_vars.push(("CRUCIBLE_SYSTEM_PROMPT", prompt));
    }

    // Check for backend configuration
    if let Ok(endpoint) = std::env::var("OLLAMA_ENDPOINT") {
        println!("  {} OLLAMA_ENDPOINT = {}", "✓".green(), endpoint.yellow());
        config = config.ollama_endpoint(&endpoint);
        found_vars.push(("OLLAMA_ENDPOINT", endpoint));
    }

    // Check for network configuration
    if let Ok(timeout) = std::env::var("CRUCIBLE_TIMEOUT") {
        if let Ok(timeout_u64) = timeout.parse::<u64>() {
            println!("  {} CRUCIBLE_TIMEOUT = {}", "✓".green(), timeout.yellow());
            config = config.timeout_secs(timeout_u64);
            found_vars.push(("CRUCIBLE_TIMEOUT", timeout));
        }
    }

    // Check for database path
    if let Ok(db_path) = std::env::var("CRUCIBLE_DB_PATH") {
        println!("  {} CRUCIBLE_DB_PATH = {}", "✓".green(), db_path.yellow());
        config = config.database_path(&db_path);
        found_vars.push(("CRUCIBLE_DB_PATH", db_path));
    }

    println!();

    if found_vars.is_empty() {
        println!("{}", "No environment variables found to migrate.".dimmed());
        println!("{}", "Your configuration is already file-based! ✨".green());
        return Ok(());
    }

    println!(
        "{} Found {} environment variable(s) to migrate",
        "📋".bold(),
        found_vars.len().to_string().cyan().bold()
    );
    println!();

    // Build the config
    let migrated_config = config.build()?;

    if dry_run {
        println!(
            "{}",
            "Dry run mode - no files will be written".yellow().bold()
        );
        println!();
        println!("{}", "Config that would be written:".bold());
        println!("{}", "─".repeat(50).dimmed());
        println!("{}", migrated_config.display_as_toml()?);
        println!("{}", "─".repeat(50).dimmed());
        println!();
        println!("{}", "To actually migrate, run without --dry-run".dimmed());
    } else {
        let config_path = output
            .or_else(|| CliConfig::default_config_path().ok())
            .context("Could not determine config file path")?;

        // Check if config already exists
        if config_path.exists() {
            println!(
                "{} Config file already exists at: {}",
                "⚠️ Warning:".yellow().bold(),
                config_path.display()
            );
            println!(
                "{}",
                "The migration will be merged with your existing config.".dimmed()
            );
            println!();
        } else {
            // Create parent directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Write the config file
        let toml_content = migrated_config.display_as_toml()?;
        std::fs::write(&config_path, toml_content)?;

        println!(
            "{} Migrated configuration saved to: {}",
            "✅ Success:".green().bold(),
            config_path.display()
        );
        println!();
        println!(
            "{}",
            "You can now remove these environment variables:".bold()
        );
        for (var_name, _) in &found_vars {
            println!("  unset {}", var_name.yellow());
        }
        println!();
        println!(
            "{}",
            "Or add to your shell config (~/.bashrc, ~/.zshrc, etc.):".dimmed()
        );
        for (var_name, _) in &found_vars {
            println!("  # unset {}", var_name.dimmed());
        }
    }

    Ok(())
}
