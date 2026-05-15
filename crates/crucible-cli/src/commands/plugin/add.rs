//! `cru plugin add` — declare a plugin from a git URL in plugins.toml.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct AddArgs {
    /// Plugin URL (e.g. "user/repo" or full git URL)
    pub url: String,
    /// Branch to track
    #[arg(long)]
    pub branch: Option<String>,
    /// Pin to a specific tag or commit
    #[arg(long)]
    pub pin: Option<String>,
}

pub async fn execute(args: AddArgs) -> Result<()> {
    let plugins_toml = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins.toml");

    let mut config: crucible_core::config::PluginsConfig = if plugins_toml.exists() {
        let content = std::fs::read_to_string(&plugins_toml)?;
        toml::from_str(&content)?
    } else {
        crucible_core::config::PluginsConfig::default()
    };

    let name = args
        .url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".git");

    if name.is_empty() || name == "." || name == ".." {
        anyhow::bail!("Cannot derive a valid plugin name from URL '{}'", args.url);
    }

    if config.plugin.iter().any(|p| {
        let n = p
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".git");
        n == name
    }) {
        anyhow::bail!("Plugin '{}' already declared in plugins.toml", name);
    }

    let entry = crucible_core::config::PluginEntry {
        url: args.url.clone(),
        branch: args.branch,
        pin: args.pin,
        enabled: true,
    };
    config.plugin.push(entry.clone());

    if let Some(parent) = plugins_toml.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&plugins_toml, toml::to_string_pretty(&config)?)?;
    println!("Added plugin '{}' to {}", name, plugins_toml.display());

    // Clone synchronously so the user sees clone success/failure before
    // the command returns, instead of waiting for the next daemon start.
    // bootstrap_plugins is idempotent — skips dirs that already exist.
    crucible_daemon::bootstrap_plugins(std::slice::from_ref(&entry)).await?;
    println!(
        "Cloned plugin '{}' to ~/.config/crucible/plugins/{}",
        name, name
    );
    println!("Restart the daemon or start a new session to load it.");

    Ok(())
}
