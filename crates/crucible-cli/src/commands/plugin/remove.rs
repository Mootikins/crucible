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
    let outcome = crucible_daemon::plugin_ops::remove(&args.name, args.purge)?;
    println!(
        "Removed plugin '{}' from {}",
        outcome.name,
        outcome.plugins_toml.display()
    );
    if let Some(dir) = outcome.purged_dir {
        println!("Deleted {}", dir.display());
    } else if args.purge {
        println!("(No plugin directory found to delete.)");
    }
    Ok(())
}
