//! `cru plugin update` — pull latest changes for installed plugins.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Only update this plugin (by name). Omit to update all.
    pub name: Option<String>,
}

pub async fn execute(args: UpdateArgs) -> Result<()> {
    let plugins_toml = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("crucible")
        .join("plugins.toml");

    if !plugins_toml.exists() {
        anyhow::bail!("No plugins.toml found at {}", plugins_toml.display());
    }

    let content = std::fs::read_to_string(&plugins_toml)?;
    let config: crucible_config::PluginsConfig = toml::from_str(&content)?;

    let plugins_dir = dirs::config_dir()
        .expect("already resolved config dir")
        .join("crucible")
        .join("plugins");

    let mut updated = 0;
    for entry in &config.plugin {
        if !entry.enabled {
            continue;
        }

        let name = entry
            .url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".git");

        if name.is_empty() || name == "." || name == ".." {
            eprintln!("Skipping plugin with unparseable URL: '{}'", entry.url);
            continue;
        }

        if let Some(ref filter) = args.name {
            if name != filter.as_str() {
                continue;
            }
        }

        let dest = plugins_dir.join(name);
        if !dest.exists() {
            println!("Plugin '{}' not cloned yet, skipping", name);
            continue;
        }

        // Pinned plugins should not be updated via pull
        if entry.pin.is_some() {
            println!("Plugin '{}' is pinned, skipping", name);
            continue;
        }

        println!("Updating '{}'...", name);
        let output = tokio::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&dest)
            .output()
            .await?;

        if output.status.success() {
            println!("  Updated '{}'", name);
            updated += 1;
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("  Failed to update '{}': {}", name, stderr.trim());
        }
    }

    if let Some(ref filter) = args.name {
        if updated == 0 {
            println!("Plugin '{}' was not updated", filter);
        }
    } else {
        println!("Updated {} plugin(s)", updated);
    }

    Ok(())
}
