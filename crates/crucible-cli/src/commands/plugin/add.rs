//! `cru plugin add` / `cru install` — clone a plugin from a git URL and
//! declare it in plugins.toml.

use anyhow::{Context, Result};
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

    let name = crucible_core::config::plugin_name_from_url(&args.url).ok_or_else(|| {
        anyhow::anyhow!("Cannot derive a valid plugin name from URL '{}'", args.url)
    })?;

    if config
        .plugin
        .iter()
        .any(|p| p.name().as_deref() == Some(name.as_str()))
    {
        anyhow::bail!("Plugin '{}' already declared in plugins.toml", name);
    }

    let entry = crucible_core::config::PluginEntry {
        url: args.url.clone(),
        branch: args.branch,
        pin: args.pin,
        enabled: true,
    };

    // Clone first; only persist to plugins.toml on success. This way a
    // failed clone (bad URL, no network, unreachable pin) doesn't leave
    // a phantom declaration behind.
    let outcome = crucible_daemon::bootstrap_plugin_entry(&entry)
        .await
        .with_context(|| format!("failed to install plugin '{}'", name))?;

    match outcome {
        crucible_daemon::BootstrapOutcome::Cloned { dest } => {
            println!("Cloned '{}' to {}", name, dest.display());
        }
        crucible_daemon::BootstrapOutcome::AlreadyPresent => {
            println!("Plugin '{}' is already cloned; declaring in plugins.toml", name);
        }
        crucible_daemon::BootstrapOutcome::Disabled => {
            // Won't happen — we construct entry with enabled = true.
            unreachable!("freshly-constructed entry should not be disabled");
        }
    }

    config.plugin.push(entry);

    if let Some(parent) = plugins_toml.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&plugins_toml, toml::to_string_pretty(&config)?)?;
    println!("Declared '{}' in {}", name, plugins_toml.display());
    println!("Restart the daemon or start a new session to load it.");

    Ok(())
}
