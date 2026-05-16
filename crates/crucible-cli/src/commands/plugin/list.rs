//! `cru plugin list` — show declared plugins and their clone status.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Output as JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

pub async fn execute(args: ListArgs) -> Result<()> {
    let plugins_toml = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins.toml");

    let config: crucible_core::config::PluginsConfig = if plugins_toml.exists() {
        let content = std::fs::read_to_string(&plugins_toml)?;
        toml::from_str(&content)?
    } else {
        crucible_core::config::PluginsConfig::default()
    };

    let plugins_dir = dirs::config_dir()
        .expect("already resolved config dir")
        .join("crucible")
        .join("plugins");

    if args.json {
        let entries: Vec<_> = config
            .plugin
            .iter()
            .map(|p| {
                let name = p.name();
                let cloned = name
                    .as_ref()
                    .map(|n| plugins_dir.join(n).exists())
                    .unwrap_or(false);
                serde_json::json!({
                    "name": name,
                    "url": p.url,
                    "branch": p.branch,
                    "pin": p.pin,
                    "enabled": p.enabled,
                    "cloned": cloned,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(());
    }

    if config.plugin.is_empty() {
        println!("No plugins declared.");
        println!("Add one with: cru install <user/repo>");
        return Ok(());
    }

    println!("{:<24} {:<10} {:<10} URL", "NAME", "STATE", "PIN");
    for entry in &config.plugin {
        let name = entry.name().unwrap_or_else(|| "(invalid)".into());
        let cloned = plugins_dir.join(&name).exists();
        let state = match (entry.enabled, cloned) {
            (false, _) => "disabled",
            (true, true) => "cloned",
            (true, false) => "pending",
        };
        let pin = entry.pin.as_deref().unwrap_or("-");
        println!("{:<24} {:<10} {:<10} {}", name, state, pin, entry.url);
    }

    Ok(())
}
