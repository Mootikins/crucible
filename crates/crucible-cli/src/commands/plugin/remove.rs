//! `cru plugin remove` — deactivate a plugin in the running daemon and
//! remove its declaration from plugins.toml.

use std::fmt::Write as _;

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
    // Route through the daemon: the RPC deactivates + forgets the running
    // plugin before touching TOML, and refuses with the reason (bundled
    // plugin, dependents) instead of leaving a half-removed state. Daemon
    // refusals propagate as errors; only an unreachable daemon falls back to
    // the offline TOML-only path.
    match crucible_daemon::DaemonClient::connect_or_start().await {
        Ok(client) => {
            let resp = client.plugin_remove(&args.name, args.purge).await?;
            print!("{}", render_remove_response(&resp, args.purge));
            Ok(())
        }
        Err(e) => {
            println!("Daemon unreachable ({e}); removing offline.");
            remove_offline(args)
        }
    }
}

/// Terminal output for the daemon's `plugin.remove` response. Failure cases
/// never reach here — the RPC call errors instead.
fn render_remove_response(resp: &serde_json::Value, purge: bool) -> String {
    let name = resp["name"].as_str().unwrap_or("?");
    let toml = resp["plugins_toml"].as_str().unwrap_or("plugins.toml");
    let mut out = String::new();
    let _ = writeln!(out, "Removed plugin '{name}' from {toml}");
    // Distinguishes this path from the offline fallback, whose output is
    // otherwise identical — the user should know the running daemon changed.
    let _ = writeln!(out, "Unloaded from the running daemon.");
    if let Some(err) = resp["purge_error"].as_str() {
        // The removal itself succeeded; only the directory deletion failed.
        let _ = writeln!(out, "Warning: {err}");
        let _ = writeln!(
            out,
            "The directory remains and will load again until deleted."
        );
        return out;
    }
    match resp["purged_dir"].as_str() {
        Some(dir) => {
            let _ = writeln!(out, "Deleted {dir}");
        }
        None if purge => {
            let _ = writeln!(out, "(No plugin directory found to delete.)");
        }
        None => {
            // Loading is directory-driven, not declaration-driven: a kept
            // clone in the search path comes back on the next daemon restart
            // or plugin install's load pass. Don't let "removed" read as
            // gone-for-good.
            if let Some(dir) = resp["kept_dir"].as_str() {
                let _ = writeln!(
                    out,
                    "Directory kept at {dir}; it will load again on the next daemon \
                     restart or plugin install. Use --purge to delete it."
                );
            }
        }
    }
    out
}

/// TOML-only removal for when no daemon is reachable — nothing is running,
/// so there is nothing to deactivate.
fn remove_offline(args: RemoveArgs) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::render_remove_response;
    use serde_json::json;

    #[test]
    fn a_daemon_remove_reports_the_toml_and_purged_dir() {
        let resp = json!({
            "name": "greeter",
            "plugins_toml": "/cfg/plugins.toml",
            "purged_dir": "/plugins/greeter",
        });
        let out = render_remove_response(&resp, true);
        assert!(
            out.contains("Removed plugin 'greeter' from /cfg/plugins.toml"),
            "got: {out}"
        );
        assert!(out.contains("Deleted /plugins/greeter"), "got: {out}");
    }

    #[test]
    fn a_purge_with_no_directory_says_so() {
        let resp = json!({
            "name": "greeter",
            "plugins_toml": "/cfg/plugins.toml",
            "purged_dir": null,
        });
        let out = render_remove_response(&resp, true);
        assert!(out.contains("No plugin directory found"), "got: {out}");
    }

    #[test]
    fn a_plain_remove_warns_that_the_kept_directory_loads_again() {
        let resp = json!({
            "name": "greeter",
            "plugins_toml": "/cfg/plugins.toml",
            "purged_dir": null,
            "kept_dir": "/plugins/greeter",
        });
        let out = render_remove_response(&resp, false);
        assert!(
            out.contains("Directory kept at /plugins/greeter"),
            "got: {out}"
        );
        assert!(out.contains("--purge"), "got: {out}");
    }

    #[test]
    fn a_purge_failure_is_a_warning_that_does_not_claim_the_toml_survived() {
        let resp = json!({
            "name": "greeter",
            "plugins_toml": "/cfg/plugins.toml",
            "purged_dir": null,
            "purge_error": "failed to remove plugin dir /plugins/greeter: EACCES",
        });
        let out = render_remove_response(&resp, true);
        assert!(out.contains("Warning:"), "got: {out}");
        assert!(out.contains("EACCES"), "got: {out}");
        // The TOML edit succeeded — output must not claim otherwise.
        assert!(
            out.contains("Removed plugin 'greeter' from /cfg/plugins.toml"),
            "got: {out}"
        );
        assert!(!out.contains("still declared"), "got: {out}");
    }
}
