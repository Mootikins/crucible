use super::*;
use crate::rpc_helpers::typed_params;
use crate::daemon_plugins::PluginServiceFn;

/// Drain extracted service functions, spawn each, and record the handle
/// against its owning plugin so reload/disable/remove can abort it. The
/// three call sites (boot, reload RPC, file watcher) previously each
/// open-coded a bare `tokio::spawn` — which is exactly how the discord
/// gateway got duplicated on reload.
pub(crate) fn spawn_plugin_services(loader: &mut DaemonPluginLoader) {
    for PluginServiceFn {
        plugin,
        service,
        func,
    } in loader.take_service_fns()
    {
        info!("Spawning service '{}' from plugin '{}'", service, plugin);
        let handle = tokio::spawn(async move {
            match func.call_async::<()>(()).await {
                Ok(()) => info!("Service '{}' completed", service),
                Err(e) => warn!("Service '{}' failed: {}", service, e),
            }
        });
        loader.record_service_task(&plugin, handle);
    }
}

pub(crate) async fn handle_plugin_reload(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::NameRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let name = params.name.as_str();

    let mut loader_guard = plugin_loader.lock().await;
    let loader = match loader_guard.as_mut() {
        Some(l) => l,
        None => return internal_error(req.id, "Plugin loader not initialized"),
    };

    match loader.reload_plugin(name).await {
        Ok(spec) => {
            spawn_plugin_services(loader);

            Response::success(
                req.id,
                serde_json::json!({
                    "name": name,
                    "reloaded": true,
                    "tools": spec.tools.len(),
                    "commands": spec.commands.len(),
                    "handlers": spec.handlers.len(),
                    "services": spec.services.len(),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_plugin_list(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let loader_guard = plugin_loader.lock().await;
    match loader_guard.as_ref() {
        Some(l) => {
            let plugins = l.loaded_plugin_info();
            let names: Vec<String> = l.loaded_plugin_names();
            Response::success(
                req.id,
                serde_json::json!({
                    "plugins": names,
                    "plugin_info": plugins,
                    // Directories that never became plugins at all — a manifest
                    // that doesn't parse has no `plugin_info` entry to carry its
                    // error, so it would otherwise reach no client.
                    "errors": l.discovery_errors(),
                }),
            )
        }
        None => Response::success(
            req.id,
            serde_json::json!({
                "plugins": [],
                "plugin_info": [],
                "errors": [],
            }),
        ),
    }
}

/// List the commands loaded plugins declared.
///
/// Commands are an agent-level concern, not a TUI-local one — the web client
/// gets slash commands from the same source — so they are served from the
/// daemon's plugin loader rather than from a per-client Lua session.
/// `session.status` — the status slots plugins published for a session.
///
/// Read by TUI and web so a plugin's durable state (e.g. "sandboxed:
/// alpine:latest") is visible. Keyed and sorted, so the chrome owner renders
/// any plugin's slots without knowing which plugins exist.
pub(crate) async fn handle_session_status(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let loader_guard = plugin_loader.lock().await;
    let slots = loader_guard
        .as_ref()
        .map(|l| l.status().get(session_id))
        .unwrap_or_default();
    let status: Vec<_> = slots
        .into_iter()
        .map(|(key, e)| {
            // Progress goes out as a fraction or the string "indeterminate",
            // and is absent when the slot describes a state rather than work.
            // Absent must stay distinguishable from 0.0: a bar pinned at zero
            // reads as stalled, which "sandboxed: alpine" is not.
            let progress = match e.progress {
                Some(crucible_lua::Progress::Indeterminate) => {
                    serde_json::json!("indeterminate")
                }
                Some(crucible_lua::Progress::Fraction(f)) => serde_json::json!(f),
                None => serde_json::Value::Null,
            };
            serde_json::json!({
                "key": key,
                "plugin": e.plugin,
                "text": e.text,
                "level": e.level,
                "progress": progress,
            })
        })
        .collect();
    Response::success(req.id, serde_json::json!({ "status": status }))
}

/// `plugin.publications` — what plugins published about themselves.
///
/// Generic by construction: values are whatever the plugin published, and this
/// handler never inspects them. `key` filters to one contribution kind; without
/// it, everything is returned.
///
/// This is the seam that keeps plugin config out of clients. The web learned
/// which isolation profiles a box offered by matching on the shape of raw
/// `[plugins.*]` TOML, which put one plugin's config schema in the rendering
/// layer and made a second plugin answering the same question invisible.
pub(crate) async fn handle_plugin_publications(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::PluginPublicationsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let key = params.key.as_deref();
    let loader_guard = plugin_loader.lock().await;
    let Some(loader) = loader_guard.as_ref() else {
        return Response::success(req.id, serde_json::json!({ "publications": {} }));
    };
    let registry = loader.publications();

    let publications = match key {
        Some(key) => {
            let by_plugin: serde_json::Map<String, serde_json::Value> =
                registry.get(key).into_iter().collect();
            serde_json::json!({ key: by_plugin })
        }
        None => serde_json::to_value(registry.all()).unwrap_or_else(|_| serde_json::json!({})),
    };
    Response::success(req.id, serde_json::json!({ "publications": publications }))
}

/// `plugin.options` — the settings tree a plugin declared, rendered for `ui`.
///
/// Every function-valued field is evaluated per request, so `values` lists what
/// is true of this box now rather than at plugin load. Without `plugin`, every
/// plugin's tree is returned keyed by name.
pub(crate) async fn handle_plugin_options(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::PluginOptionsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let ui = params.ui.as_deref().unwrap_or("web");
    let loader_guard = plugin_loader.lock().await;
    let Some(loader) = loader_guard.as_ref() else {
        return Response::success(req.id, serde_json::json!({ "options": {} }));
    };
    let registry = loader.options();

    let mut out = serde_json::Map::new();
    match params.plugin.as_deref() {
        Some(name) => {
            if let Some(tree) = registry.describe(name, ui) {
                out.insert(name.to_string(), tree);
            }
        }
        None => {
            for name in registry.plugins() {
                if let Some(tree) = registry.describe(&name, ui) {
                    out.insert(name, tree);
                }
            }
        }
    }
    Response::success(req.id, serde_json::json!({ "options": out }))
}

/// Which callback an option RPC reaches.
///
/// An enum rather than a `&str` because the dispatcher's METHODS-drift test
/// reads string literals out of the dispatch arms, and bare `"get"`/`"set"`
/// there read as method names that do not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OptionAction {
    Get,
    Set,
    Execute,
}

/// `plugin.option_get` / `plugin.option_set` / `plugin.option_execute`.
///
/// One handler because the three differ only in which Lua callback they reach;
/// splitting them would triple the parameter plumbing for no gain.
pub(crate) async fn handle_plugin_option_call(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
    action: OptionAction,
) -> Response {
    let params = match typed_params::<crate::rpc_client::PluginOptionCallRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let plugin = params.plugin;
    let ui = params.ui.as_deref().unwrap_or("web");
    let path = params.path;
    if path.is_empty() {
        return Response::error(req.id, INVALID_PARAMS, "`path` is required".to_string());
    }

    let loader_guard = plugin_loader.lock().await;
    let Some(loader) = loader_guard.as_ref() else {
        return Response::error(req.id, INTERNAL_ERROR, "no plugins loaded".to_string());
    };
    let registry = loader.options();

    let outcome = match action {
        OptionAction::Get => registry
            .get(&plugin, &path, ui)
            .map(|v| serde_json::json!({ "value": v })),
        OptionAction::Set => {
            let value = params.value;
            registry.set(&plugin, &path, value.clone(), ui).map(|()| {
                // Only after the plugin accepted it. Recording first would
                // persist a value the plugin rejected, and replay it into a
                // refusal on every subsequent boot.
                //
                // The store lives under the loader's bound data root — the
                // daemon's resolved `data_home`, not `crucible_home()`, so an
                // injected root is honored instead of the process's real one.
                if let Some(dir) = loader.option_store_dir() {
                    crate::daemon_plugins::option_store::record(dir, &plugin, &path, value);
                }
                serde_json::json!({ "ok": true })
            })
        }
        OptionAction::Execute => registry
            .execute(&plugin, &path, ui)
            .map(|()| serde_json::json!({ "ok": true })),
    };

    match outcome {
        Ok(value) => Response::success(req.id, value),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e),
    }
}

pub(crate) async fn handle_plugin_commands(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let loader_guard = plugin_loader.lock().await;
    let commands = loader_guard
        .as_ref()
        .map(|l| l.plugin_registry().commands_json())
        .unwrap_or_default();
    Response::success(req.id, serde_json::json!({ "commands": commands }))
}

/// Invoke a plugin command by name.
pub(crate) async fn handle_plugin_run_command(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::PluginRunCommandRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let name = params.name;
    let args = params.args;

    // Clone the registry Arc out of the guard: a command handler can call back
    // into daemon APIs, and holding the loader mutex across that awaits a lock
    // we may already own.
    let registry = {
        let loader_guard = plugin_loader.lock().await;
        loader_guard.as_ref().map(|l| l.plugin_registry())
    };
    let Some(registry) = registry else {
        return internal_error(req.id, "Plugin loader not initialized");
    };

    match registry.run_command(&name, args).await {
        Ok(Some(result)) => Response::success(
            req.id,
            serde_json::json!({ "name": name, "result": result }),
        ),
        Ok(None) => internal_error(req.id, format!("Unknown plugin command: {name}")),
        Err(e) => internal_error(req.id, e),
    }
}

// --- Project handlers ---

pub(crate) async fn handle_project_register(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let params = match typed_params::<crate::rpc_client::PathRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match pm.register(Path::new(&params.path)) {
        Ok(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

pub(crate) async fn handle_project_unregister(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let params = match typed_params::<crate::rpc_client::PathRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match pm.unregister(Path::new(&params.path)) {
        Ok(()) => Response::success(req.id, serde_json::json!({"status": "ok"})),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

pub(crate) async fn handle_project_list(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let projects = pm.list();
    match serde_json::to_value(projects) {
        Ok(v) => Response::success(req.id, v),
        Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

pub(crate) async fn handle_project_get(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let params = match typed_params::<crate::rpc_client::PathRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    match pm.get(Path::new(&params.path)) {
        Some(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        None => Response::success(req.id, serde_json::Value::Null),
    }
}

// --- SCM (git) handlers ---

/// `scm.clone`: clone a remote git repo to a destination and register it as a
/// project. URL is validated/normalized before git runs; `dest` (if given)
/// must be absolute and must not exist, otherwise the clone lands in
/// `<projects_dir>/<repo-name>`.
pub(crate) async fn handle_scm_clone(
    req: Request,
    pm: &Arc<ProjectManager>,
    projects_dir: Option<&str>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::ScmCloneRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let dest_param = params.dest.as_deref();
    let name_param = params.name.as_deref();

    // Validate + normalize the URL before git ever sees it.
    let url = match crate::scm::normalize_clone_url(&params.url) {
        Ok(u) => u,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
    };

    // Resolve the destination path. BOTH forms are contained to the
    // projects dir — an explicit dest is validated against it (canonicalized,
    // no '..', symlink-hop safe), matching the containment every other write
    // endpoint enforces.
    let base = crate::scm::resolve_projects_dir(projects_dir, dirs::home_dir().as_deref());
    if let Err(e) = tokio::fs::create_dir_all(&base).await {
        return internal_error(req.id, format!("failed to create projects dir: {e}"));
    }
    let dest = if let Some(dest) = dest_param {
        let dest = Path::new(dest);
        if !dest.is_absolute() {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("dest must be an absolute path: {dest:?}"),
            );
        }
        if let Err(e) = crate::scm::validate_clone_dest(dest, &base) {
            return Response::error(req.id, INVALID_PARAMS, e.to_string());
        }
        dest.to_path_buf()
    } else {
        // repo-name comes from `name` (if given) else the URL's last segment.
        let repo_name = match name_param {
            Some(n) => crate::scm::sanitize_repo_name(n),
            None => crate::scm::derive_repo_name(&url),
        };
        let repo_name = match repo_name {
            Ok(n) => n,
            Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
        };
        base.join(repo_name)
    };

    if dest.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            crate::scm::ScmError::DestExists(dest.to_string_lossy().to_string()).to_string(),
        );
    }

    if let Err(e) = crate::scm::clone_repo(&url, &dest).await {
        return match e {
            crate::scm::ScmError::DestExists(_) => {
                Response::error(req.id, INVALID_PARAMS, e.to_string())
            }
            _ => internal_error(req.id, e),
        };
    }

    let project = match pm.register(&dest) {
        Ok(project) => project,
        Err(e) => return internal_error(req.id, e),
    };

    let response = crate::scm::ScmCloneResponse {
        path: dest.to_string_lossy().to_string(),
        project,
    };
    match serde_json::to_value(response) {
        Ok(v) => Response::success(req.id, v),
        Err(e) => internal_error(req.id, e),
    }
}

pub(super) fn spawn_plugin_watcher(
    plugin_dirs: Vec<(String, PathBuf)>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
) {
    use notify::{RecursiveMode, Watcher};

    let dir_to_plugin: std::collections::HashMap<PathBuf, String> = plugin_dirs
        .iter()
        .map(|(name, dir)| (dir.clone(), name.clone()))
        .collect();

    let watch_dirs: Vec<PathBuf> = plugin_dirs.into_iter().map(|(_, dir)| dir).collect();

    let (sync_tx, sync_rx) = std::sync::mpsc::channel::<PathBuf>();

    let mut watcher = match notify::recommended_watcher(
        move |res: std::result::Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                if !event.kind.is_modify() && !event.kind.is_create() {
                    return;
                }
                for path in &event.paths {
                    let ext = path.extension().and_then(|e| e.to_str());
                    if matches!(ext, Some("lua") | Some("fnl")) {
                        let _ = sync_tx.send(path.clone());
                    }
                }
            }
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            warn!("Failed to create plugin file watcher: {}", e);
            return;
        }
    };

    for dir in &watch_dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
            warn!("Failed to watch plugin dir {}: {}", dir.display(), e);
        }
    }

    info!(
        "Plugin file watcher active for {} director(ies)",
        watch_dirs.len()
    );

    tokio::spawn(async move {
        let _watcher_guard = watcher;
        let debounce = tokio::time::Duration::from_millis(500);
        let mut pending: std::collections::HashMap<String, tokio::time::Instant> =
            std::collections::HashMap::new();

        loop {
            let next_fire = pending.values().copied().min();

            let timeout = match next_fire {
                Some(t) => t.saturating_duration_since(tokio::time::Instant::now()),
                None => tokio::time::Duration::from_millis(100),
            };

            tokio::time::sleep(timeout).await;

            while let Ok(changed_path) = sync_rx.try_recv() {
                if let Some(plugin_name) = find_owning_plugin(&changed_path, &dir_to_plugin) {
                    pending.insert(plugin_name, tokio::time::Instant::now() + debounce);
                }
            }

            let now = tokio::time::Instant::now();
            let ready: Vec<String> = pending
                .iter()
                .filter(|(_, &t)| t <= now)
                .map(|(name, _)| name.clone())
                .collect();

            for name in ready {
                pending.remove(&name);
                let mut guard = plugin_loader.lock().await;
                if let Some(ref mut loader) = *guard {
                    match loader.reload_plugin(&name).await {
                        Ok(_spec) => {
                            info!("Plugin '{}' auto-reloaded due to file change", name);
                            // Spawning inside the guard is safe — tokio::spawn
                            // is not an await point — and required: recording
                            // the handles needs the loader.
                            spawn_plugin_services(loader);
                        }
                        Err(e) => {
                            warn!("Auto-reload failed for plugin '{}': {}", name, e);
                        }
                    }
                }
            }
        }
    });
}

pub(super) fn find_owning_plugin(
    path: &Path,
    dir_to_plugin: &std::collections::HashMap<PathBuf, String>,
) -> Option<String> {
    for (dir, name) in dir_to_plugin {
        if path.starts_with(dir) {
            return Some(name.clone());
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Session observe RPC handlers (load_events, list_persisted, render_markdown,
//                                export_to_file, cleanup, reindex)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod plugin_command_rpc_tests {
    use super::*;
    use crate::protocol::RequestId;

    fn request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "test".to_string(),
            params,
        }
    }

    /// A daemon started without plugins is not an error state — clients ask for
    /// the command list unconditionally at startup.
    #[tokio::test]
    async fn commands_without_a_loader_is_an_empty_list_not_an_error() {
        let loader: Arc<Mutex<Option<DaemonPluginLoader>>> = Arc::new(Mutex::new(None));
        let resp = handle_plugin_commands(request(serde_json::Value::Null), &loader).await;

        let result = resp.result.expect("should succeed");
        assert_eq!(result["commands"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn run_command_without_a_loader_reports_an_error() {
        let loader: Arc<Mutex<Option<DaemonPluginLoader>>> = Arc::new(Mutex::new(None));
        let resp =
            handle_plugin_run_command(request(serde_json::json!({ "name": "greet" })), &loader)
                .await;

        assert!(resp.error.is_some(), "expected an error response");
    }
}

#[cfg(test)]
mod project_rpc_param_tests {
    use super::*;
    use crate::protocol::{RequestId, INVALID_PARAMS};

    fn request(params: serde_json::Value) -> Request {
        Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "test".to_string(),
            params,
        }
    }

    /// A hermetic `ProjectManager` — a temp `projects.json`, never the
    /// developer's real `~/.crucible` registry.
    fn manager(store: &std::path::Path) -> Arc<ProjectManager> {
        Arc::new(ProjectManager::new(store.join("projects.json")))
    }

    #[tokio::test]
    async fn project_register_reads_the_path_field() {
        let store = tempfile::TempDir::new().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let pm = manager(store.path());

        let resp = handle_project_register(
            request(serde_json::json!({ "path": dir.path().to_string_lossy() })),
            &pm,
        )
        .await;

        assert!(resp.error.is_none(), "register failed: {:?}", resp.error);
        assert!(
            pm.get(dir.path()).is_some(),
            "the handler registered some other path than the one it was sent"
        );
    }

    #[tokio::test]
    async fn project_get_answers_null_for_an_unregistered_path() {
        let store = tempfile::TempDir::new().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let pm = manager(store.path());

        let resp = handle_project_get(
            request(serde_json::json!({ "path": dir.path().to_string_lossy() })),
            &pm,
        )
        .await;

        assert_eq!(resp.result, Some(serde_json::Value::Null));
    }

    #[tokio::test]
    async fn project_register_without_a_path_is_invalid_params() {
        let store = tempfile::TempDir::new().unwrap();
        let pm = manager(store.path());

        let resp = handle_project_register(request(serde_json::json!({})), &pm).await;

        let error = resp.error.expect("a request with no `path` must fail");
        assert_eq!(error.code, INVALID_PARAMS);
        assert!(
            error.message.contains("path"),
            "the message must name the field: {}",
            error.message
        );
    }

    /// The absent-`path` answer is the handler's own sentence, not serde's:
    /// `path` defaults so that this message survives the typed request.
    #[tokio::test]
    async fn option_call_without_a_path_still_says_path_is_required() {
        let loader: Arc<Mutex<Option<DaemonPluginLoader>>> = Arc::new(Mutex::new(None));

        let resp = handle_plugin_option_call(
            request(serde_json::json!({ "plugin": "oci" })),
            &loader,
            OptionAction::Get,
        )
        .await;

        let error = resp.error.expect("a request with no `path` must fail");
        assert_eq!(error.code, INVALID_PARAMS);
        assert_eq!(error.message, "`path` is required");
    }
}

/// `plugin.list` is the only window a client has onto plugin health. Every
/// failure mode below used to be invisible there: the plugin was either
/// filtered out of the response entirely or reported `Active`.
#[cfg(test)]
mod plugin_health_visibility_tests {
    use super::*;
    use crate::protocol::RequestId;
    use crucible_lua::PluginSource;
    use std::fs;
    use tempfile::TempDir;

    fn write_plugin(root: &Path, name: &str, manifest: &str, init_lua: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).expect("plugin dir");
        fs::write(dir.join("plugin.yaml"), manifest).expect("manifest");
        fs::write(dir.join("init.lua"), init_lua).expect("init.lua");
    }

    /// Load `root` as a plugin search path and return the `plugin.list` result.
    async fn list_after_loading(root: &Path) -> serde_json::Value {
        let mut loader = DaemonPluginLoader::new(std::collections::HashMap::new()).expect("loader");
        loader
            .load_plugins(&[(root.to_path_buf(), PluginSource::Runtime)])
            .await
            .expect("load_plugins");

        let loader = Arc::new(Mutex::new(Some(loader)));
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: Some(RequestId::Number(1)),
            method: "plugin.list".to_string(),
            params: serde_json::Value::Null,
        };
        handle_plugin_list(req, &loader)
            .await
            .result
            .expect("plugin.list result")
    }

    fn entry<'a>(result: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
        result["plugin_info"]
            .as_array()
            .expect("plugin_info array")
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("plugin '{name}' missing from {result:#}"))
    }

    /// A plugin that raises while loading is dropped from `plugin.list`
    /// altogether, because `loaded_plugin_info` filtered on
    /// `PluginState::Active`. "Not listed" is indistinguishable from
    /// "not installed".
    #[tokio::test]
    async fn a_plugin_that_raises_at_load_is_listed_with_its_error() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "raiser",
            "name: raiser\nversion: \"0.1.0\"\nmain: init.lua\n",
            "error('kaboom')\n",
        );

        let result = list_after_loading(tmp.path()).await;
        let plugin = entry(&result, "raiser");

        assert_ne!(
            plugin["state"], "Active",
            "a plugin that never executed must not report Active: {result:#}"
        );
        let last_error = plugin["last_error"].as_str().unwrap_or("");
        assert!(
            last_error.contains("kaboom"),
            "last_error should carry the Lua error, got {last_error:?} in {result:#}"
        );
    }

    /// `setup()` runs only in the daemon's real VM — the spec sandbox never
    /// calls it — so a raising `setup()` was downgraded to `warn!` while
    /// `load_plugin_spec` still returned `Ok`, leaving the plugin `Active`.
    #[tokio::test]
    async fn a_plugin_whose_setup_raises_is_not_active() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "badsetup",
            "name: badsetup\nversion: \"0.1.0\"\nmain: init.lua\n",
            "return { name = 'badsetup', setup = function() error('setup exploded') end }\n",
        );

        let result = list_after_loading(tmp.path()).await;
        let plugin = entry(&result, "badsetup");

        assert_ne!(
            plugin["state"], "Active",
            "a plugin whose setup() raised must not report Active: {result:#}"
        );
        let last_error = plugin["last_error"].as_str().unwrap_or("");
        assert!(
            last_error.contains("setup exploded"),
            "last_error should carry the setup failure, got {last_error:?} in {result:#}"
        );
    }

    /// A manifest that doesn't parse keeps the plugin out of
    /// `PluginManager::plugins` entirely — there is no entry to mark broken.
    /// `reflection` shipped for months in exactly this state, with the only
    /// trace a `warn!` in the daemon's log.
    #[tokio::test]
    async fn a_plugin_with_an_unparseable_manifest_reaches_the_client() {
        let tmp = TempDir::new().unwrap();
        write_plugin(
            tmp.path(),
            "bogus-caps",
            "name: bogus-caps\nversion: \"0.1.0\"\nmain: init.lua\ncapabilities:\n  - teleportation\n",
            "return { name = 'bogus-caps' }\n",
        );

        let result = list_after_loading(tmp.path()).await;
        let errors = result["errors"]
            .as_array()
            .unwrap_or_else(|| panic!("plugin.list must report discovery errors: {result:#}"));

        assert!(
            errors.iter().any(|e| {
                e["path"].as_str().is_some_and(|p| p.contains("bogus-caps"))
                    && e["error"]
                        .as_str()
                        .is_some_and(|m| m.contains("teleportation"))
            }),
            "discovery failure for 'bogus-caps' should be reported: {result:#}"
        );
    }
}
