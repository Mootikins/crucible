//! `cru plugin add` / `cru install` — clone a plugin from a git URL and
//! declare it in plugins.toml.

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
    let entry = crucible_core::config::PluginEntry {
        url: args.url.clone(),
        branch: args.branch,
        pin: args.pin,
        enabled: true,
    };

    let result = crucible_daemon::plugin_ops::install(entry).await?;

    match &result.outcome {
        crucible_daemon::BootstrapOutcome::Cloned { dest } => {
            println!("Cloned '{}' to {}", result.name, dest.display());
        }
        crucible_daemon::BootstrapOutcome::AlreadyPresent => {
            println!(
                "Plugin '{}' is already cloned; declaring in plugins.toml",
                result.name
            );
        }
        crucible_daemon::BootstrapOutcome::Disabled => {
            anyhow::bail!(
                "internal: bootstrap_plugin_entry returned Disabled for an entry constructed with enabled=true"
            );
        }
    }
    println!(
        "Declared '{}' in {}",
        result.name,
        result.plugins_toml.display()
    );
    println!("Restart the daemon or start a new session to load it.");

    Ok(())
}
