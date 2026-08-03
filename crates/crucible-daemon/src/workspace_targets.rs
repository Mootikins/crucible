//! The workspace axis: *where do a session's files live?*
//!
//! Distinct from the runtime axis (*where does the process run?*), which is
//! [`crate::session_lifecycle`]'s isolation claim. The two compose — a session
//! can run in a container against a worktree — and keeping them apart is what
//! lets a worktree provider exist without colliding with `oci`, whose resolver
//! raises on an isolation profile name it does not recognise.
//!
//! A client asks for `workspace_target = "worktree:feat/x"`. The provider
//! (`worktree`) is a plugin that published itself on the `targets` channel and
//! named a command that materialises the target and answers with a path. The
//! session is then created against *that* path.
//!
//! Resolution happens here, before `session.create` runs, because everything
//! that path feeds is decided inside create: the ACP agent's working directory
//! (`resolve_create_agent`), project registration (`register_if_missing`) and
//! the persisted workspace itself. A plugin rewriting the workspace from
//! `on_session_start` — the only plugin entry point that existed — would be
//! three steps too late, leaving the project registered at the old path and the
//! agent already pointed at it.
//!
//! Everything here is fail-closed. A target that was asked for and not
//! delivered refuses the session rather than falling back to the main checkout,
//! for the reason [`crate::session_lifecycle`] refuses an unenforceable
//! isolation claim: silently getting the *other* thing is worse than getting
//! nothing, because nobody looks.

use crate::daemon_plugins::DaemonPluginLoader;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// The publication channel a target provider declares itself on.
const TARGETS_KEY: &str = "targets";

/// Split `"worktree:feat/x"` into its provider and the target within it.
///
/// Only the FIRST colon separates — a target is free to contain more (`ssh`
/// hosts and branch names both do). A spec with no colon is a provider with an
/// empty target, which is how a provider offering exactly one thing is named.
pub fn split_target(spec: &str) -> (&str, &str) {
    match spec.split_once(':') {
        Some((provider, target)) => (provider, target),
        None => (spec, ""),
    }
}

/// The path a provider's resolve command answered with.
///
/// Accepts `{ path = "…" }` or a bare string, and insists the result is
/// absolute: a relative path would be resolved against the daemon's working
/// directory, which is not the client's and not the project's.
pub fn path_from_result(value: &serde_json::Value) -> Result<PathBuf> {
    let raw = value
        .get("path")
        .and_then(|v| v.as_str())
        .or_else(|| value.as_str())
        .filter(|s| !s.is_empty())
        .context("the provider answered with no path")?;

    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        bail!("the provider answered with a relative path ('{raw}')");
    }
    Ok(path)
}

/// Resolves workspace targets against the plugins that provide them.
///
/// Holds the loader rather than a snapshot of it so a plugin reloaded between
/// sessions is seen — the same reason [`crate::session_lifecycle`] reaches
/// through the loader for the isolation registry.
pub struct WorkspaceTargets {
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
}

impl WorkspaceTargets {
    pub fn new(plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>) -> Self {
        Self { plugin_loader }
    }

    /// Materialise `spec` and answer with the workspace path to use.
    ///
    /// `workspace` is what the client selected before choosing a target — the
    /// repository a worktree is cut from, the machine-local path an `ssh`
    /// target reinterprets. Providers that need it and did not get it say so
    /// themselves; this only forwards.
    pub async fn resolve(&self, spec: &str, workspace: Option<&str>) -> Result<PathBuf> {
        let (provider, target) = split_target(spec);
        if provider.is_empty() {
            bail!("workspace_target names no provider");
        }

        // Both the publication and the registry come from one acquisition: a
        // reload between the two would resolve a command against a provider
        // that no longer declares it.
        let (declaration, registry) = {
            let guard = self.plugin_loader.lock().await;
            let loader = guard
                .as_ref()
                .context("plugins are not loaded, so no workspace target can be resolved")?;
            let declaration = loader
                .publications()
                .all()
                .remove(TARGETS_KEY)
                .and_then(|by_plugin| by_plugin.get(provider).cloned());
            (declaration, loader.plugin_registry())
        };

        let declaration = declaration
            .with_context(|| format!("no plugin provides workspace targets named '{provider}'"))?;

        // A runtime provider answering a workspace request would resolve
        // something, and it would be the wrong axis. Checked rather than
        // assumed: the two share one publication channel.
        let axis = declaration.get("axis").and_then(|v| v.as_str());
        if axis != Some("workspace") {
            bail!(
                "plugin '{provider}' provides '{}' targets, not workspace targets",
                axis.unwrap_or("unknown")
            );
        }

        let command = declaration
            .get("resolve_command")
            .and_then(|v| v.as_str())
            .with_context(|| {
                format!("plugin '{provider}' publishes workspace targets but no resolve_command")
            })?;

        let args = serde_json::json!({ "target": target, "workspace": workspace });
        let answer = registry
            .run_command(command, args)
            .await
            .with_context(|| format!("resolving workspace target '{spec}'"))?
            .with_context(|| {
                format!("plugin '{provider}' names resolve_command '{command}', which it does not declare")
            })?;

        path_from_result(&answer).with_context(|| format!("resolving workspace target '{spec}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spec_splits_on_its_first_colon_only() {
        assert_eq!(split_target("worktree:feat/x"), ("worktree", "feat/x"));
        // Branch names and ssh destinations both contain colons; splitting on
        // the last (or on every) colon truncates them.
        assert_eq!(
            split_target("ssh:user@host:/srv/repo"),
            ("ssh", "user@host:/srv/repo")
        );
    }

    #[test]
    fn a_spec_with_no_colon_is_a_provider_with_no_target() {
        assert_eq!(split_target("worktree"), ("worktree", ""));
    }

    #[test]
    fn a_path_is_read_from_either_shape() {
        let object = serde_json::json!({ "path": "/repo/tree/feat-x" });
        let bare = serde_json::json!("/repo/tree/feat-x");
        let expected = PathBuf::from("/repo/tree/feat-x");
        assert_eq!(path_from_result(&object).unwrap(), expected);
        assert_eq!(path_from_result(&bare).unwrap(), expected);
    }

    /// The failure this guards is silent: a relative path resolves against the
    /// daemon's working directory, so the session starts somewhere real and
    /// entirely unrelated to the project.
    #[test]
    fn a_relative_path_is_refused() {
        let answer = serde_json::json!({ "path": "tree/feat-x" });
        let err = path_from_result(&answer).unwrap_err().to_string();
        assert!(err.contains("relative"), "unexpected message: {err}");
    }

    #[test]
    fn an_empty_or_absent_path_is_refused() {
        assert!(path_from_result(&serde_json::json!({})).is_err());
        assert!(path_from_result(&serde_json::json!({ "path": "" })).is_err());
        assert!(path_from_result(&serde_json::json!(null)).is_err());
    }

    /// Fail-closed with no plugins at all: a target asked for and not delivered
    /// must refuse, never quietly become "the workspace you already had".
    #[tokio::test]
    async fn a_target_is_refused_when_no_plugin_provides_it() {
        let targets = WorkspaceTargets::new(Arc::new(Mutex::new(None)));
        assert!(targets
            .resolve("worktree:feat/x", Some("/repo"))
            .await
            .is_err());
    }
}
