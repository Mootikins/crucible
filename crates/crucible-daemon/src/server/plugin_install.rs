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

/// The config-declared plugin names, read from the live effective config.
/// Empty when the daemon has no app config.
fn declared_plugin_names(ctx: &crate::rpc::RpcContext) -> Vec<String> {
    ctx.effective_config()
        .as_ref()
        .and_then(|cfg| cfg.get("plugins"))
        .and_then(|v| {
            serde_json::from_value::<std::collections::BTreeMap<String, serde_json::Value>>(
                v.clone(),
            )
            .ok()
        })
        .map(|map| {
            crucible_core::config::declared_plugins(&map)
                .0
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        })
        .unwrap_or_default()
}

/// Where the declaration was written — `lua (init.lua:12)` — from the boot
/// store's provenance. The refusal that names this is what lets the user
/// find the line to edit.
fn declaration_site(name: &str) -> Option<String> {
    let provenance = crucible_lua::get_app_config_provenance()?;
    let leaf = format!(
        "plugins.{}.{name}",
        crucible_core::config::PLUGINS_DECLARE_KEY
    );
    let child_prefix = format!("{leaf}.");
    provenance
        .get(&leaf)
        .cloned()
        .or_else(|| {
            provenance
                .iter()
                .find(|(path, _)| path.starts_with(&child_prefix))
                .map(|(_, tag)| tag.clone())
        })
        .map(|tag| tag.detail())
}

/// The refusal both handlers give for a config-declared plugin: the machine
/// must not edit the user's config file, so it names the declaration site
/// and stops.
fn declared_refusal(name: &str, action: &str) -> String {
    let site = declaration_site(name).unwrap_or_else(|| "your init.lua".to_string());
    format!(
        "plugin '{name}' is declared in your config ({site}); {action}. Edit the          `plugins.{}.{name}` entry there — Crucible never edits your config file",
        crucible_core::config::PLUGINS_DECLARE_KEY
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

    let entry = crucible_core::config::PluginEntry {
        url: params.url,
        branch: params.branch,
        pin: params.pin,
        enabled: true,
    };

    // A declared plugin already bootstraps at every boot; recording it in
    // the manifest too would create a permanent shadow pair.
    if let Some(name) = entry.name() {
        if declared_plugin_names(ctx).contains(&name) {
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
        match crate::plugin_ops::install_at(entry, &manifest_path, &plugins_dir).await {
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

    // A config-declared plugin is refused with its declaration site: the
    // user removes it by editing their own file, never the machine.
    if declared_plugin_names(ctx).contains(&name) {
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
    use crucible_core::config::ConfigSource;

    /// The refusal names the `file:line` of the declaration — that is the
    /// whole value of the refusal: the user knows which line to edit.
    #[test]
    fn the_declared_refusal_names_the_declaration_site() {
        // Process-per-test (nextest): the global store starts empty here.
        crucible_lua::begin_boot_store();
        crucible_lua::merge_app_config_tagged(
            serde_json::json!({
                "plugins": { "declare": { "greeter": "user/greeter" } }
            }),
            ConfigSource::Lua {
                last_set: crucible_core::config::LastSet::new(
                    crucible_core::lua_source::LuaSource::UserLua,
                    "init.lua",
                    Some(12),
                ),
            },
        );

        let site = declaration_site("greeter").expect("a declared plugin has a site");
        assert_eq!(site, "lua (init.lua:12)");

        let refusal = declared_refusal("greeter", "cannot remove");
        assert!(
            refusal.contains("init.lua:12") && refusal.contains("plugins.declare.greeter"),
            "the refusal must name the line and the entry to edit: {refusal}"
        );

        assert!(
            declaration_site("ghost").is_none(),
            "an undeclared plugin has no site"
        );
    }
}
