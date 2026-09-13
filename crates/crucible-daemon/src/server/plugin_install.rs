//! `plugin.install` / `plugin.remove` — the runtime install/remove handlers.
//!
//! Split from `plugins.rs` (which keeps reload/list/options and the shared
//! `spawn_plugin_services`) purely for module size; the two files share the
//! same `super::*` scope.

use super::plugins::spawn_plugin_services;
use super::*;
/// activation pass.
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

/// Whether the operator's `init.lua` declares `name` with a git source.
/// Read from the spec on the plugin VM; `false` when the daemon has no
/// loader. The lock is taken for the read alone, and dropped before the
/// handler takes it again for the load.
async fn is_declared(ctx: &crate::rpc::RpcContext, name: &str) -> bool {
    let guard = ctx.plugin_loader.lock().await;
    guard.as_ref().is_some_and(|loader| {
        crate::daemon_plugins::declared_git_entry(&crucible_lua::spec_of(loader.lua()), name)
    })
}

/// The refusal both handlers give for a declared plugin: the machine must
/// not edit the user's config file, so it names the entry to edit and
/// stops.
fn declared_refusal(name: &str, action: &str) -> String {
    format!(
        "plugin '{name}' is declared in your init.lua (a `cru.plugin.setup` entry with a git \
         source); {action}. Edit that entry there — Crucible never edits your config file"
    )
}

pub(crate) async fn handle_plugin_install(
    req: Request,
    ctx: &Arc<crate::rpc::RpcContext>,
) -> Response {
    let plugin_loader = &ctx.plugin_loader;
    let params =
        match crate::rpc_helpers::typed_params::<crate::rpc_client::PluginInstallRequest>(&req) {
            Ok(p) => p,
            Err(response) => return *response,
        };

    let entry = crate::plugin_ops::InstalledEntry::new(params.url, params.branch, params.pin);

    // A declared plugin already bootstraps at every boot; recording it in
    // the manifest too would create a permanent shadow pair.
    if let Some(name) = entry.name() {
        if is_declared(ctx, &name).await {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                declared_refusal(&name, "it is already installed at every boot"),
            );
        }
    }

    let manifest_path = crate::plugin_ops::installed_manifest_path(&ctx.data_home);
    let result = {
        let plugins_dir = match crate::plugin_ops::plugins_dir() {
            Ok(d) => d,
            Err(e) => return internal_error(req.id, e),
        };
        match crate::plugin_ops::install_at(entry.clone(), &manifest_path, &plugins_dir).await {
            Ok(r) => r,
            Err(e) => return internal_error(req.id, e),
        }
    };

    // Activate on the running daemon: the user plugins dir joins the search
    // paths, and the installed plugin is activated by name through the one
    // activation body. If activation fails, the install still happened on
    // disk: the next boot activates it, and a broken plugin is visible in
    // `plugin.list` as `state: Error` — so report `installed: true,
    // loaded: false` with the reason, not an opaque failure.
    let report = match crate::plugin_ops::plugins_dir() {
        Ok(plugins_dir) => {
            // BEFORE the load, because the load runs the plugin's `setup()`
            // and every `cru.config.set` in it is classified by the file
            // that made the call. The boot never saw this directory when it
            // did not yet exist, and without this the plugin's defaults pin
            // as if the user had written them.
            crate::daemon_plugins::boot::learn_plugin_author_root(&plugins_dir);
            let mut loader_guard = plugin_loader.lock().await;
            match loader_guard.as_mut() {
                Some(loader) => {
                    // The record joins the spec at Builtin rank now, as the
                    // next boot's merge would place it, so `plugin.list` and
                    // the `enabled` resolution see the same entry either way.
                    crucible_lua::merge_spec_entry(
                        loader.lua(),
                        entry.spec_entry(&result.name),
                        crucible_core::config::SpecRank::Builtin,
                    );
                    let clone_dir = plugins_dir.join(&result.name);
                    let activated = match loader
                        .add_plugin_paths(&[(plugins_dir, crucible_lua::PluginSource::User)])
                    {
                        Ok(()) => loader.activate_plugin(&result.name).await,
                        Err(e) => Err(e),
                    };
                    match activated {
                        Ok(()) => {
                            let report = install_load_report(loader, &result.name, &clone_dir);
                            spawn_plugin_services(loader);
                            report
                        }
                        // `activate` marked the plugin `Error`, so the report
                        // reads its `last_error`; a name discovery never saw
                        // reports the miss.
                        Err(_) => install_load_report(loader, &result.name, &clone_dir),
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
            "manifest": result.manifest.to_string_lossy(),
        }),
    )
}

pub(crate) async fn handle_plugin_remove(
    req: Request,
    ctx: &Arc<crate::rpc::RpcContext>,
) -> Response {
    let plugin_loader = &ctx.plugin_loader;
    let params =
        match crate::rpc_helpers::typed_params::<crate::rpc_client::PluginRemoveRequest>(&req) {
            Ok(p) => p,
            Err(response) => return *response,
        };
    let name = params.name;
    let purge = params.purge;

    // A declared plugin is refused: the user removes it by editing their
    // own file, never the machine.
    if is_declared(ctx, &name).await {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            declared_refusal(&name, "`cru plugin remove` removes installed plugins only"),
        );
    }

    // Precondition FIRST: only plugins recorded in the installed manifest
    // are removable, and every bundled `runtime/plugins/*` plugin is not.
    // Checking after deactivation instead would unload e.g. `oci` and THEN
    // fail the manifest step — a silently unloaded isolation plugin,
    // reachable from the web UI's remove button.
    let manifest_path = crate::plugin_ops::installed_manifest_path(&ctx.data_home);
    match crate::plugin_ops::installed_at(&manifest_path, &name) {
        Ok(true) => {}
        Ok(false) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "plugin '{name}' is not in the installed manifest, so there is nothing to \
                     remove; to turn off a bundled plugin, set \
                     `plugins = {{ {name} = {{ enabled = false }} }}` in init.lua"
                ),
            )
        }
        Err(e) => return internal_error(req.id, e),
    }

    // Deactivate + forget on the running daemon. A dependent-refusal here
    // leaves the manifest untouched and reports why nothing happened.
    // The manager keys plugins by manifest name, which can differ from the
    // URL-derived `name` the manifest uses; resolve through the clone directory
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

    // Manifest commit; the clone dir is purged only after the manifest
    // write succeeded. Run on spawn_blocking because it does fs writes.
    let result = {
        let name = name.clone();
        let manifest_path = manifest_path.clone();
        let plugins_dir = match crate::plugin_ops::plugins_dir() {
            Ok(d) => d,
            Err(e) => return internal_error(req.id, e),
        };
        tokio::task::spawn_blocking(move || {
            crate::plugin_ops::remove_at(&name, purge, &manifest_path, &plugins_dir)
        })
        .await
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
                    "manifest": outcome.manifest.to_string_lossy(),
                    "purged_dir": outcome.purged_dir.map(|p| p.to_string_lossy().to_string()),
                    "purge_error": outcome.purge_error,
                    "kept_dir": kept_dir,
                }),
            )
        }
        // A concurrent manifest write since the precondition check can
        // land here: the plugin is deactivated but still recorded, which the
        // next daemon boot recovers by loading it again.
        Ok(Err(e)) => internal_error(
            req.id,
            format!(
                "plugin '{name}' was deactivated, but removing its manifest entry failed \
                 (still recorded; the next daemon start will load it again): {e}"
            ),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

#[cfg(test)]
mod declared_refusal_tests {
    use super::*;

    /// The refusal names the entry to edit: a `cru.plugin.setup` entry in
    /// `init.lua`. That is the whole value of the refusal.
    #[test]
    fn the_declared_refusal_names_the_setup_entry() {
        let refusal = declared_refusal("greeter", "cannot remove");
        assert!(
            refusal.contains("init.lua") && refusal.contains("cru.plugin.setup"),
            "the refusal must name the file and the entry to edit: {refusal}"
        );
        assert!(refusal.contains("cannot remove"), "{refusal}");
    }

    /// Declared means: the operator's own entry has a git source. An
    /// installed plugin (Builtin rank) is not declared, and neither is an
    /// operator entry that only disables one.
    #[tokio::test]
    async fn only_an_operator_git_entry_is_declared() {
        let loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
        loader
            .eval_user_init(r#"cru.plugin.setup({ "user/greeter", { "tool", enabled = false } })"#)
            .await
            .expect("spec");
        crucible_lua::merge_spec_entry(
            loader.lua(),
            crate::plugin_ops::InstalledEntry::new("other/tool".into(), None, None)
                .spec_entry("tool"),
            crucible_core::config::SpecRank::Builtin,
        );
        let spec = crucible_lua::spec_of(loader.lua());

        assert!(crate::daemon_plugins::declared_git_entry(&spec, "greeter"));
        assert!(
            !crate::daemon_plugins::declared_git_entry(&spec, "tool"),
            "an installed plugin the operator only disabled is removable"
        );
        assert!(!crate::daemon_plugins::declared_git_entry(&spec, "ghost"));
    }
}
