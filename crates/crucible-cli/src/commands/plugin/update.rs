//! `cru plugin update` — pull latest changes for installed plugins.

use anyhow::Result;
use clap::Args;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Only update this plugin (by name). Omit to update all.
    pub name: Option<String>,
}

pub async fn execute(args: UpdateArgs) -> Result<()> {
    let (entries, notes) = super::configured_plugin_entries().await?;
    for note in &notes {
        eprintln!("note: {note}");
    }
    if entries.is_empty() {
        anyhow::bail!("no git-hosted plugins configured (declared or installed)");
    }

    let plugins_dir = crucible_daemon::plugin_ops::plugins_dir()?;

    let mut updated = 0;
    for (name, entry, _source) in &entries {
        if !entry.enabled {
            continue;
        }
        let name = name.as_str();

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
