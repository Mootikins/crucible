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

/// List the commands loaded plugins declared.
///
/// Commands are an agent-level concern, not a TUI-local one — the web client
/// gets slash commands from the same source — so they are served from the
/// daemon's plugin loader rather than from a per-client Lua session.
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
    let name = require_param!(req, "name", as_str).to_string();
    let args = req
        .params
        .get("args")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

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

// --- Install / remove handlers ---

pub(crate) async fn handle_plugin_install(req: Request) -> Response {
    let url = require_param!(req, "url", as_str).to_string();
    let branch = optional_param!(req, "branch", as_str).map(|s| s.to_string());
    let pin = optional_param!(req, "pin", as_str).map(|s| s.to_string());

    let entry = crucible_core::config::PluginEntry {
        url,
        branch,
        pin,
        enabled: true,
    };

    match crate::plugin_ops::install(entry).await {
        Ok(result) => Response::success(
            req.id,
            serde_json::json!({
                "name": result.name,
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
        ),
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_plugin_remove(req: Request) -> Response {
    let name = require_param!(req, "name", as_str).to_string();
    let purge = optional_param!(req, "purge", as_bool).unwrap_or(false);

    // plugin_ops::remove is synchronous (no I/O off the runtime); run on
    // spawn_blocking anyway because it does fs writes.
    let result = tokio::task::spawn_blocking(move || crate::plugin_ops::remove(&name, purge)).await;

    match result {
        Ok(Ok(outcome)) => Response::success(
            req.id,
            serde_json::json!({
                "name": outcome.name,
                "plugins_toml": outcome.plugins_toml.to_string_lossy(),
                "purged_dir": outcome.purged_dir.map(|p| p.to_string_lossy().to_string()),
            }),
        ),
        Ok(Err(e)) => internal_error(req.id, e),
        Err(e) => internal_error(req.id, e),
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

// --- SCM (git) handlers ---

/// `scm.branches`: list local + remote-only branches for the repo containing
/// `path`, annotated with worktree paths and which branch is current.
pub(crate) async fn handle_scm_branches(req: Request, _pm: &Arc<ProjectManager>) -> Response {
    let path = require_param!(req, "path", as_str);

    match crate::scm::collect_branches(Path::new(path)).await {
        Ok(resp) => match serde_json::to_value(resp) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => internal_error(req.id, e),
        },
        Err(e @ crate::scm::ScmError::NotARepo(_)) => {
            Response::error(req.id, INVALID_PARAMS, e.to_string())
        }
        Err(e) => internal_error(req.id, e),
    }
}

/// `scm.worktree_add`: create a worktree for `branch` under the configured
/// `worktree_dir` template and register it as a project.
pub(crate) async fn handle_scm_worktree_add(
    req: Request,
    pm: &Arc<ProjectManager>,
    worktree_dir: Option<&str>,
) -> Response {
    let repo_root = require_param!(req, "repo_root", as_str).to_string();
    let branch = require_param!(req, "branch", as_str).to_string();
    let create_branch = optional_param!(req, "create_branch", as_bool).unwrap_or(false);

    let added =
        match crate::scm::add_worktree(Path::new(&repo_root), &branch, create_branch, worktree_dir)
            .await
        {
            Ok(added) => added,
            Err(e @ crate::scm::ScmError::InvalidBranch(_))
            | Err(e @ crate::scm::ScmError::DestExists(_))
            | Err(e @ crate::scm::ScmError::NotARepo(_)) => {
                return Response::error(req.id, INVALID_PARAMS, e.to_string());
            }
            Err(e) => return internal_error(req.id, e),
        };

    let project = match pm.register(&added.dest) {
        Ok(project) => project,
        Err(e) => return internal_error(req.id, e),
    };

    let warning = if added.not_ignored_warning {
        Some(format!(
            "{} is inside the repository but not gitignored; it will show as untracked",
            added.dest.display()
        ))
    } else {
        None
    };

    let response = crate::scm::ScmWorktreeAddResponse {
        path: added.dest.to_string_lossy().to_string(),
        project,
        warning,
    };
    match serde_json::to_value(response) {
        Ok(v) => Response::success(req.id, v),
        Err(e) => internal_error(req.id, e),
    }
}

/// `scm.clone`: clone a remote git repo to a destination and register it as a
/// project. URL is validated/normalized before git runs; `dest` (if given)
/// must be absolute and must not exist, otherwise the clone lands in
/// `<projects_dir>/<repo-name>`.
pub(crate) async fn handle_scm_clone(
    req: Request,
    pm: &Arc<ProjectManager>,
    projects_dir: Option<&str>,
) -> Response {
    let raw_url = require_param!(req, "url", as_str);
    let dest_param = optional_param!(req, "dest", as_str);
    let name_param = optional_param!(req, "name", as_str);

    // Validate + normalize the URL before git ever sees it.
    let url = match crate::scm::normalize_clone_url(raw_url) {
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
