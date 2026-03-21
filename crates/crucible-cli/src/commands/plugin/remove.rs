//! `cru plugin remove` — remove a plugin declaration from plugins.toml.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct RemoveArgs {
    /// Plugin name (last path segment of the URL, without .git)
    pub name: String,
    /// Also delete the plugin directory from disk
    #[arg(long)]
    pub purge: bool,
}

pub async fn execute(args: RemoveArgs) -> Result<()> {
    let plugins_toml = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins.toml");

    if !plugins_toml.exists() {
        anyhow::bail!("No plugins.toml found at {}", plugins_toml.display());
    }

    let content = std::fs::read_to_string(&plugins_toml)?;
    let mut config: crucible_config::PluginsConfig = toml::from_str(&content)?;

    let before = config.plugin.len();
    config.plugin.retain(|p| {
        let entry_name = p
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".git");
        entry_name != args.name
    });

    if config.plugin.len() == before {
        anyhow::bail!(
            "Plugin '{}' not found in {}",
            args.name,
            plugins_toml.display()
        );
    }

    std::fs::write(&plugins_toml, toml::to_string_pretty(&config)?)?;
    println!(
        "Removed plugin '{}' from {}",
        args.name,
        plugins_toml.display()
    );

    if args.purge {
        let plugins_dir = dirs::config_dir()
            .expect("already resolved config dir")
            .join("crucible")
            .join("plugins");
        let plugin_dir = plugins_dir.join(&args.name);
        if plugin_dir.exists() {
            std::fs::remove_dir_all(&plugin_dir)?;
            println!("Deleted {}", plugin_dir.display());
        } else {
            println!(
                "Directory {} does not exist, nothing to delete",
                plugin_dir.display()
            );
        }
    }

    Ok(())
}
