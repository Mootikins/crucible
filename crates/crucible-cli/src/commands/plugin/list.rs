//! `cru plugin list` — the configured git-hosted plugins (a `cru.plugin.setup`
//! entry in init.lua, or a record in the installed manifest) plus the
//! daemon's runtime view: what actually loaded, and *why* the broken ones
//! broke.
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
/// reachable. Best-effort by design: the installed section must work with
/// the daemon down, and spawning one just to list plugins would be a
/// surprise.
async fn runtime_plugins(
    client: Option<&crucible_daemon::DaemonClient>,
) -> Option<Vec<serde_json::Value>> {
    client?.plugin_list_info().await.ok()
}

pub async fn execute(args: ListArgs) -> Result<()> {
    let client = crate::common::daemon_client_if_running().await;
    let (entries, notes) = super::configured_plugin_entries(client.as_ref()).await?;
    let plugins_dir = crucible_daemon::plugin_ops::plugins_dir()?;

    let runtime = runtime_plugins(client.as_ref()).await;

    if args.json {
        let configured: Vec<_> = entries
            .iter()
            .map(|entry| {
                let cloned = plugins_dir.join(&entry.name).exists();
                serde_json::json!({
                    "name": entry.name,
                    "url": entry.url,
                    "branch": entry.branch,
                    "pin": entry.pin,
                    "enabled": entry.enabled,
                    "cloned": cloned,
                    "source": entry.source.as_str(),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "configured": configured,
                "notes": notes,
                "runtime": runtime,
            }))?
        );
        return Ok(());
    }

    for note in &notes {
        eprintln!("note: {note}");
    }
    if entries.is_empty() {
        println!("No git-hosted plugins configured.");
        println!("Add one with: cru install <user/repo>");
    } else {
        println!(
            "{:<24} {:<10} {:<10} {:<10} URL",
            "NAME", "SOURCE", "STATE", "PIN"
        );
        for entry in &entries {
            let cloned = plugins_dir.join(&entry.name).exists();
            let state = match (entry.enabled, cloned) {
                (false, _) => "disabled",
                (true, true) => "cloned",
                (true, false) => "pending",
            };
            let pin = entry.pin.as_deref().unwrap_or("-");
            println!(
                "{:<24} {:<10} {:<10} {:<10} {}",
                entry.name,
                entry.source.as_str(),
                state,
                pin,
                entry.url
            );
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
