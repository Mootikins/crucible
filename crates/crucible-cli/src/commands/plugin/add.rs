//! `cru plugin add` / `cru install` — clone a plugin from a git URL,
//! record it in the installed manifest, and load it into the running daemon.

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
    // the in-process path only edits the manifest and clones. Fall back to that
    // offline path only when no daemon can be reached at all; an RPC error
    // from a reachable daemon is a refusal, not a cue to bypass it.
    match crucible_daemon::DaemonClient::connect_or_start().await {
        Ok(client) => {
            let resp = client
                .plugin_install(&args.url, args.branch.as_deref(), args.pin.as_deref())
                .await?;
            let name = resp.name.clone();
            let (output, load_error) = render_install_response(&resp);
            print!("{output}");
            if let Some(err) = load_error {
                // Non-zero exit: the clone + manifest entry landed (printed
                // above), but "installed" must not read as success while the
                // plugin sits broken in the daemon.
                anyhow::bail!(
                    "plugin '{name}' installed but failed to load: {err}\n\
                     (it stays recorded; the next daemon start retries it)"
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
/// manifest entry) before failing the exit code.
fn render_install_response(
    resp: &crucible_core::types::PluginInstallReply,
) -> (String, Option<String>) {
    let name = &resp.name;
    let mut out = String::new();
    match &resp.outcome {
        crucible_core::types::PluginInstallOutcome::Cloned { dest } => {
            let _ = writeln!(out, "Cloned '{name}' to {dest}");
        }
        crucible_core::types::PluginInstallOutcome::AlreadyPresent => {
            let _ = writeln!(
                out,
                "Plugin '{name}' is already cloned; recording in the installed manifest"
            );
        }
        crucible_core::types::PluginInstallOutcome::Disabled => {}
    }
    let _ = writeln!(out, "Recorded '{name}' in {}", resp.manifest);

    if resp.loaded {
        let _ = writeln!(
            out,
            "Loaded in daemon: {} tool(s), {} command(s), {} service(s).",
            resp.tools, resp.commands, resp.services,
        );
        // Design decision: runtime-installed plugins are not added to the
        // file watcher until the next daemon start.
        let _ = writeln!(
            out,
            "(Edits are not hot-watched until the daemon restarts.)"
        );
        (out, None)
    } else {
        let err = resp
            .error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "plugin did not load; see `cru plugin list`".to_string());
        (out, Some(err))
    }
}

/// The pre-daemon-routing behavior: clone + manifest edit in-process,
/// nothing loaded anywhere. Only reachable when connect_or_start failed.
async fn install_offline(args: AddArgs) -> Result<()> {
    let entry =
        crucible_daemon::plugin_ops::InstalledEntry::new(args.url.clone(), args.branch, args.pin);

    let result = crucible_daemon::plugin_ops::install(entry).await?;

    match &result.outcome {
        crucible_daemon::BootstrapOutcome::Cloned { dest } => {
            println!("Cloned '{}' to {}", result.name, dest.display());
        }
        crucible_daemon::BootstrapOutcome::AlreadyPresent => {
            println!(
                "Plugin '{}' is already cloned; recording in the installed manifest",
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
        "Recorded '{}' in {}",
        result.name,
        result.manifest.display()
    );
    // Only a restart loads daemon plugins — a new session does not.
    println!("Restart the daemon to load it.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::render_install_response;
    use crucible_core::types::{PluginInstallOutcome, PluginInstallReply};

    fn reply(
        name: &str,
        loaded: bool,
        tools: u64,
        commands: u64,
        services: u64,
        error: Option<&str>,
        outcome: PluginInstallOutcome,
    ) -> PluginInstallReply {
        PluginInstallReply {
            name: name.to_string(),
            installed: true,
            loaded,
            tools,
            commands,
            services,
            error: error.map(str::to_string),
            watch: "not hot-watched until restart".to_string(),
            outcome,
            manifest: "/data/plugins.installed.json".to_string(),
        }
    }

    #[test]
    fn a_loaded_install_reports_counts_and_no_error() {
        let resp = reply(
            "greeter",
            true,
            2,
            1,
            0,
            None,
            PluginInstallOutcome::Cloned {
                dest: "/plugins/greeter".to_string(),
            },
        );
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
            out.contains("Recorded 'greeter' in /data/plugins.installed.json"),
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
        let resp = reply(
            "broken",
            false,
            0,
            0,
            0,
            Some("boom inside setup"),
            PluginInstallOutcome::AlreadyPresent,
        );
        let (out, load_error) = render_install_response(&resp);
        // The install DID happen on disk — say so before failing.
        assert!(
            out.contains("Recorded 'broken' in /data/plugins.installed.json"),
            "got: {out}"
        );
        let err = load_error.expect("loaded: false must produce an error for the exit code");
        assert!(err.contains("boom inside setup"), "got: {err}");
    }

    #[test]
    fn a_load_failure_without_a_reason_still_fails_with_a_pointer() {
        let resp = reply(
            "mute",
            false,
            0,
            0,
            0,
            None,
            PluginInstallOutcome::AlreadyPresent,
        );
        let (_, load_error) = render_install_response(&resp);
        let err = load_error.expect("loaded: false must produce an error even without a reason");
        assert!(
            err.contains("plugin list"),
            "point at the diagnostic surface, got: {err}"
        );
    }
}
