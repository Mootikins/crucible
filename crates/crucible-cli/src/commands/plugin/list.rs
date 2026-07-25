//! `cru plugin list` — declared plugins (plugins.toml) plus the daemon's
//! runtime view: what actually loaded, and *why* the broken ones broke.
//!
//! The runtime section is the only place a user can see a load failure:
//! `plugin.list` carries `state` and `last_error`, and until this command
//! read them, a plugin dying at `require` was invisible everywhere — the
//! daemon logged one `warn!` to a stdout that auto-spawn points at /dev/null.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Output as JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

/// Runtime plugin info from the daemon, or `None` when no daemon is
/// reachable. Best-effort by design: the declared section must work with the
/// daemon down, and spawning one just to list plugins would be a surprise.
async fn runtime_plugins() -> Option<Vec<serde_json::Value>> {
    let client = crate::common::daemon_client_if_running().await?;
    client.plugin_list_info().await.ok()
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

    let runtime = runtime_plugins().await;

    if args.json {
        let declared: Vec<_> = config
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
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "declared": declared,
                "runtime": runtime,
            }))?
        );
        return Ok(());
    }

    if config.plugin.is_empty() {
        println!("No plugins declared.");
        println!("Add one with: cru install <user/repo>");
    } else {
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
    }

    match runtime {
        Some(plugins) if !plugins.is_empty() => {
            println!();
            println!("Loaded in daemon:");
            println!(
                "{:<24} {:<10} {:<12} TOOLS/CMDS/HOOKS",
                "NAME", "VERSION", "STATE"
            );
            for p in &plugins {
                let name = p["name"].as_str().unwrap_or("?");
                let version = p["version"].as_str().unwrap_or("?");
                let state = p["state"].as_str().unwrap_or("?");
                println!(
                    "{:<24} {:<10} {:<12} {}/{}/{}",
                    name,
                    version,
                    state,
                    p["tools"].as_u64().unwrap_or(0),
                    p["commands"].as_u64().unwrap_or(0),
                    p["handlers"].as_u64().unwrap_or(0),
                );
                // The reason a plugin broke, where the user is looking.
                if let Some(err) = p["last_error"].as_str().filter(|e| !e.is_empty()) {
                    println!("    error: {err}");
                }
            }
        }
        Some(_) => {}
        None => {
            println!();
            println!("(daemon not running — runtime state unavailable)");
        }
    }

    Ok(())
}
