use super::*;

pub(crate) async fn handle_plugin_reload(
    req: Request,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let name = require_param!(req, "name", as_str);

    let mut loader_guard = plugin_loader.lock().await;
    let loader = match loader_guard.as_mut() {
        Some(l) => l,
        None => return internal_error(req.id, "Plugin loader not initialized"),
    };

    match loader.reload_plugin(name).await {
        Ok(spec) => {
            let service_fns = loader.take_service_fns();
            for (svc_name, func) in service_fns {
                info!("Re-spawning service after reload: {}", svc_name);
                tokio::spawn(async move {
                    match func.call_async::<()>(()).await {
                        Ok(()) => info!("Service '{}' completed", svc_name),
                        Err(e) => warn!("Service '{}' failed: {}", svc_name, e),
                    }
                });
            }

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
                }),
            )
        }
        None => Response::success(
            req.id,
            serde_json::json!({
                "plugins": [],
                "plugin_info": [],
            }),
        ),
    }
}

// --- Project handlers ---

pub(crate) async fn handle_project_register(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match pm.register(Path::new(path)) {
        Ok(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

pub(crate) async fn handle_project_unregister(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match pm.unregister(Path::new(path)) {
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
    let path = require_param!(req, "path", as_str);

    match pm.get(Path::new(path)) {
        Some(project) => match serde_json::to_value(project) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        None => Response::success(req.id, serde_json::Value::Null),
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
                            let service_fns = loader.take_service_fns();
                            drop(guard);
                            for (svc_name, func) in service_fns {
                                info!("Re-spawning service after auto-reload: {}", svc_name);
                                tokio::spawn(async move {
                                    match func.call_async::<()>(()).await {
                                        Ok(()) => info!("Service '{}' completed", svc_name),
                                        Err(e) => warn!("Service '{}' failed: {}", svc_name, e),
                                    }
                                });
                            }
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
