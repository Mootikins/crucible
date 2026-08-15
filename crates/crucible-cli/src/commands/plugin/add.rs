//! `cru plugin add` / `cru install` — clone a plugin from a git URL,
//! declare it in plugins.toml, and load it into the running daemon.

use std::fmt::Write as _;

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
    // Route through the daemon so the plugin activates without a restart —
    // the in-process path only edits TOML and clones. Fall back to that
    // offline path only when no daemon can be reached at all; an RPC error
    // from a reachable daemon is a refusal, not a cue to bypass it.
    match crucible_daemon::DaemonClient::connect_or_start().await {
        Ok(client) => {
            let resp = client
                .plugin_install(&args.url, args.branch.as_deref(), args.pin.as_deref())
                .await?;
            let (output, load_error) = render_install_response(&resp);
            print!("{output}");
            if let Some(err) = load_error {
                // Non-zero exit: the clone + TOML entry landed (printed
                // above), but "installed" must not read as success while the
                // plugin sits broken in the daemon.
                anyhow::bail!(
                    "plugin '{}' installed but failed to load: {err}\n\
                     (it stays declared; the next daemon start retries it)",
                    resp["name"].as_str().unwrap_or(&args.url)
                );
            }
            Ok(())
        }
        Err(e) => {
            println!("Daemon unreachable ({e}); installing offline.");
            install_offline(args).await
        }
    }
}

/// Split the daemon's `plugin.install` response into terminal output and,
/// when the plugin installed but failed to load, the load error. Split
/// rather than bailed inside so the caller can print what DID happen (clone,
/// TOML entry) before failing the exit code.
fn render_install_response(resp: &serde_json::Value) -> (String, Option<String>) {
    let name = resp["name"].as_str().unwrap_or("?");
    let mut out = String::new();
    match resp["outcome"]["kind"].as_str() {
        Some("cloned") => {
            let dest = resp["outcome"]["dest"].as_str().unwrap_or("?");
            let _ = writeln!(out, "Cloned '{name}' to {dest}");
        }
        Some("already_present") => {
            let _ = writeln!(
                out,
                "Plugin '{name}' is already cloned; declaring in plugins.toml"
            );
        }
        _ => {}
    }
    let toml = resp["plugins_toml"].as_str().unwrap_or("plugins.toml");
    let _ = writeln!(out, "Declared '{name}' in {toml}");

    if resp["loaded"].as_bool().unwrap_or(false) {
        let _ = writeln!(
            out,
            "Loaded in daemon: {} tool(s), {} command(s), {} service(s).",
            resp["tools"].as_u64().unwrap_or(0),
            resp["commands"].as_u64().unwrap_or(0),
            resp["services"].as_u64().unwrap_or(0),
        );
        // Design decision: runtime-installed plugins are not added to the
        // file watcher until the next daemon start.
        let _ = writeln!(
            out,
            "(Edits are not hot-watched until the daemon restarts.)"
        );
        (out, None)
    } else {
        let err = resp["error"]
            .as_str()
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "plugin did not load; see `cru plugin list`".to_string());
        (out, Some(err))
    }
}

/// The pre-daemon-routing behavior: clone + TOML edit in-process, nothing
/// loaded anywhere. Only reachable when connect_or_start failed.
async fn install_offline(args: AddArgs) -> Result<()> {
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
    // Only a restart loads daemon plugins — a new session does not.
    println!("Restart the daemon to load it.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_install_response;
    use serde_json::json;

    #[test]
    fn a_loaded_install_reports_counts_and_no_error() {
        let resp = json!({
            "name": "greeter",
            "installed": true,
            "loaded": true,
            "tools": 2,
            "commands": 1,
            "services": 0,
            "error": null,
            "watch": "not hot-watched until restart",
            "outcome": { "kind": "cloned", "dest": "/plugins/greeter" },
            "plugins_toml": "/cfg/plugins.toml",
        });
        let (out, load_error) = render_install_response(&resp);
        assert!(
            load_error.is_none(),
            "loaded install must not error: {load_error:?}"
        );
        assert!(
            out.contains("Cloned 'greeter' to /plugins/greeter"),
            "got: {out}"
        );
        assert!(
            out.contains("Declared 'greeter' in /cfg/plugins.toml"),
            "got: {out}"
        );
        assert!(
            out.contains("2 tool(s), 1 command(s), 0 service(s)"),
            "got: {out}"
        );
        assert!(
            !out.contains("Restart the daemon"),
            "daemon-routed install needs no restart: {out}"
        );
    }

    #[test]
    fn an_install_that_failed_to_load_surfaces_the_error_instead_of_success() {
        let resp = json!({
            "name": "broken",
            "installed": true,
            "loaded": false,
            "tools": 0,
            "commands": 0,
            "services": 0,
            "error": "boom inside setup",
            "watch": "not hot-watched until restart",
            "outcome": { "kind": "already_present" },
            "plugins_toml": "/cfg/plugins.toml",
        });
        let (out, load_error) = render_install_response(&resp);
        // The install DID happen on disk — say so before failing.
        assert!(
            out.contains("Declared 'broken' in /cfg/plugins.toml"),
            "got: {out}"
        );
        let err = load_error.expect("loaded: false must produce an error for the exit code");
        assert!(err.contains("boom inside setup"), "got: {err}");
    }

    #[test]
    fn a_load_failure_without_a_reason_still_fails_with_a_pointer() {
        let resp = json!({
            "name": "mute",
            "installed": true,
            "loaded": false,
            "error": null,
            "outcome": { "kind": "already_present" },
            "plugins_toml": "/cfg/plugins.toml",
        });
        let (_, load_error) = render_install_response(&resp);
        let err = load_error.expect("loaded: false must produce an error even without a reason");
        assert!(
            err.contains("plugin list"),
            "point at the diagnostic surface, got: {err}"
        );
    }
}
