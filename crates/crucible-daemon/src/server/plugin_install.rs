//! `plugin.install` / `plugin.remove` — the runtime install/remove handlers.
//!
//! Split from `plugins.rs` (which keeps reload/list/options and the shared
//! `spawn_plugin_services`) purely for module size; the two files share the
//! same `super::*` scope.

use super::plugins::spawn_plugin_services;
use super::*;
/// activation `load_plugins` pass.
#[derive(Debug)]
pub(crate) struct InstallLoadReport {
    pub loaded: bool,
    pub tools: u64,
    pub commands: u64,
    pub services: u64,
    pub error: Option<String>,
}

impl InstallLoadReport {
    fn not_loaded(error: String) -> Self {
        Self {
            loaded: false,
            tools: 0,
            commands: 0,
            services: 0,
            error: Some(error),
        }
    }
}

/// Judge the install by the loader's post-load state (`loaded_plugin_info`),
/// never by the activation pass's return value: a plugin that was ALREADY
/// Active — manually cloned into the user plugins dir and loaded at boot,
/// now being declared — is skipped by `load_all` as `AlreadyLoaded` and is
/// absent from that pass's specs, but it is loaded, not broken. Judging by
/// the pass result reported `loaded: false` with a fabricated error for a
/// healthy plugin (and failed `cru plugin add`'s exit code).
pub(crate) fn install_load_report(
    loader: &DaemonPluginLoader,
    name: &str,
    clone_dir: &std::path::Path,
) -> InstallLoadReport {
    let info = loader.loaded_plugin_info();
    // Resolve by the clone DIRECTORY first: the manager keys plugins by
    // manifest name, which legitimately differs from the URL-derived `name`
    // (repo `crucible-greeter`, manifest `name: greeter`). Matching by name
    // alone reported such a healthy plugin as broken.
    let dir_str = clone_dir.to_string_lossy();
    match info
        .iter()
        .find(|p| p["dir"] == dir_str.as_ref())
        .or_else(|| info.iter().find(|p| p["name"] == name))
    {
        Some(entry) if entry["state"] == "Active" => InstallLoadReport {
            loaded: true,
            tools: entry["tools"].as_u64().unwrap_or(0),
            commands: entry["commands"].as_u64().unwrap_or(0),
            services: entry["services"].as_u64().unwrap_or(0),
            error: None,
        },
        // Per-plugin fail-open: the pass succeeded but this plugin's own
        // execution failed — its entry carries the reason.
        Some(entry) => InstallLoadReport::not_loaded(
            entry["last_error"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| "plugin did not load; see plugin.list".to_string()),
        ),
        None => InstallLoadReport::not_loaded("plugin did not load; see plugin.list".to_string()),
    }
}

pub(crate) async fn handle_plugin_install(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match crate::rpc_helpers::typed_params::<crate::rpc_client::PluginInstallRequest>(
        &req,
    ) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    let entry = crucible_core::config::PluginEntry {
        url: params.url,
        branch: params.branch,
        pin: params.pin,
        enabled: true,
    };

    let result = match crate::plugin_ops::install(entry).await {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    // Activate on the running daemon: a second `load_plugins` pass over the
    // user plugins dir is incremental — Active plugins are skipped
    // (`AlreadyLoaded`) and `loaded_specs` merges. If activation fails, the
    // install still happened on disk: the next boot loads it, and a broken
    // plugin is visible in `plugin.list` as `state: Error` — so report
    // `installed: true, loaded: false` with the reason, not an opaque
    // failure. No TOML rollback.
    let report = match crate::plugin_ops::plugins_dir() {
        Ok(plugins_dir) => {
            let mut loader_guard = plugin_loader.lock().await;
            match loader_guard.as_mut() {
                Some(loader) => {
                    let clone_dir = plugins_dir.join(&result.name);
                    match loader
                        .load_plugins(&[(plugins_dir, crucible_lua::PluginSource::User)])
                        .await
                    {
                        Ok(_) => {
                            let report = install_load_report(loader, &result.name, &clone_dir);
                            spawn_plugin_services(loader);
                            report
                        }
                        Err(e) => InstallLoadReport::not_loaded(e.to_string()),
                    }
                }
                None => InstallLoadReport::not_loaded("plugin loader not initialized".to_string()),
            }
        }
        Err(e) => InstallLoadReport::not_loaded(e.to_string()),
    };
    let InstallLoadReport {
        loaded,
        tools,
        commands,
        services,
        error: load_error,
    } = report;

    Response::success(
        req.id,
        serde_json::json!({
            "name": result.name,
            "installed": true,
            "loaded": loaded,
            "tools": tools,
            "commands": commands,
            "services": services,
            "error": load_error,
            // The watcher's watch list is a boot-time snapshot; a plugin
            // installed at runtime works but is not hot-reloaded on edit.
            "watch": "not hot-watched until restart",
            "outcome": match result.outcome {
                crate::BootstrapOutcome::Cloned { ref dest } => serde_json::json!({
                    "kind": "cloned",
                    "dest": dest.to_string_lossy(),
                }),
                crate::BootstrapOutcome::AlreadyPresent => serde_json::json!({
                    "kind": "already_present",
                }),
                crate::BootstrapOutcome::Disabled => serde_json::json!({
                    "kind": "disabled",
                }),
            },
            "plugins_toml": result.plugins_toml.to_string_lossy(),
        }),
    )
}

pub(crate) async fn handle_plugin_remove(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match crate::rpc_helpers::typed_params::<crate::rpc_client::PluginRemoveRequest>(
        &req,
    ) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let name = params.name;
    let purge = params.purge;

    // Precondition FIRST: only plugins declared in plugins.toml are
    // removable, and every bundled `runtime/plugins/*` plugin is not.
    // Checking after deactivation instead would unload e.g. `oci` and THEN
    // fail the TOML step — a silently unloaded isolation plugin, reachable
    // from the web UI's remove button.
    let toml_path = match crate::plugin_ops::plugins_toml_path() {
        Ok(p) => p,
        Err(e) => return internal_error(req.id, e),
    };
    match crate::plugin_ops::declared_at(&toml_path, &name) {
        Ok(true) => {}
        Ok(false) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "plugin '{name}' is not declared in plugins.toml, so there is nothing to \
                     remove; to turn off a bundled plugin, set `[plugins.{name}] enabled = false` \
                     in config.toml"
                ),
            )
        }
        Err(e) => return internal_error(req.id, e),
    }

    // Deactivate + forget on the running daemon. A dependent-refusal here
    // leaves plugins.toml untouched and reports why nothing happened.
    // The manager keys plugins by manifest name, which can differ from the
    // URL-derived `name` the TOML uses; resolve through the clone directory
    // so remove reaches the actual plugin instead of no-oping on the URL
    // name (unload's NotFound tolerance would swallow the miss).
    {
        let mut loader_guard = plugin_loader.lock().await;
        if let Some(loader) = loader_guard.as_mut() {
            let manager_name = crate::plugin_ops::plugins_dir()
                .ok()
                .and_then(|d| loader.plugin_name_for_dir(&d.join(&name)))
                .unwrap_or_else(|| name.clone());
            if let Err(e) = loader.deactivate_and_forget_plugin(&manager_name).await {
                return internal_error(req.id, e);
            }
        }
    }

    // TOML commit; `remove` purges the clone dir only after the TOML write
    // succeeded. Run on spawn_blocking because it does fs writes.
    let result = {
        let name = name.clone();
        tokio::task::spawn_blocking(move || crate::plugin_ops::remove(&name, purge)).await
    };

    match result {
        Ok(Ok(outcome)) => {
            // Without --purge the clone directory stays in a permanent search
            // path: the next daemon restart (or any plugin install's load
            // pass) discovers and loads it again. Say so, rather than letting
            // "removed" read as gone-for-good.
            let kept_dir = if !purge {
                crate::plugin_ops::plugins_dir()
                    .ok()
                    .map(|d| d.join(&outcome.name))
                    .filter(|d| d.exists())
                    .map(|d| d.to_string_lossy().to_string())
            } else {
                None
            };
            Response::success(
                req.id,
                serde_json::json!({
                    "name": outcome.name,
                    "plugins_toml": outcome.plugins_toml.to_string_lossy(),
                    "purged_dir": outcome.purged_dir.map(|p| p.to_string_lossy().to_string()),
                    "purge_error": outcome.purge_error,
                    "kept_dir": kept_dir,
                }),
            )
        }
        // A concurrent plugins.toml write since the precondition check can
        // land here: the plugin is deactivated but still declared, which the
        // next daemon boot recovers by loading it again.
        Ok(Err(e)) => internal_error(
            req.id,
            format!(
                "plugin '{name}' was deactivated, but removing its plugins.toml entry failed \
                 (still declared; the next daemon start will load it again): {e}"
            ),
        ),
        Err(e) => internal_error(req.id, e),
    }
}
