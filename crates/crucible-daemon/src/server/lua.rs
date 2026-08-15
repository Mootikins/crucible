use super::*;
use crate::rpc_helpers::typed_params;

pub(crate) async fn handle_lua_init_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
    plugin_loader: &Arc<Mutex<Option<DaemonPluginLoader>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaInitSessionRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.clone();
    // The `kiln` spelling this used to accept as a second name is now
    // `#[serde(alias = "kiln")]` on the struct, so the alias is documented where
    // the field is rather than only here.
    let kiln_root = params
        .kiln_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crucible_core::config::crucible_home);

    let mut executor = match LuaExecutor::new() {
        Ok(executor) => executor,
        Err(e) => return internal_error(req.id, e),
    };

    if let Err(e) = executor.load_config(Some(&kiln_root)) {
        warn!(
            session_id = %session_id,
            kiln = %kiln_root.display(),
            error = %e,
            "Failed to load Lua config"
        );
    }

    let session = LuaSession::new("chat".to_string());
    session.bind(Box::new(NoopSessionRpc));
    executor.session_manager().set_current(session.clone());

    if let Err(e) = executor.sync_session_start_hooks() {
        warn!(session_id = %session_id, error = %e, "Failed to sync session_start hooks");
    }
    if let Err(e) = executor.fire_session_start_hooks(&session).await {
        warn!(session_id = %session_id, error = %e, "Failed to fire session_start hooks");
    }

    let registry = LuaScriptHandlerRegistry::new();
    if let Err(e) = register_crucible_on_api(
        executor.lua(),
        registry.runtime_handlers(),
        registry.handler_functions(),
    ) {
        warn!(session_id = %session_id, error = %e, "Failed to register crucible.on API");
    }

    lua_sessions.insert(
        session_id.clone(),
        Arc::new(Mutex::new(LuaSessionState {
            executor,
            end_hooks_fired: false,
        })),
    );

    // Commands come from the daemon's plugin loader, not this per-call
    // executor: a plugin's command handlers only exist in the loader's VM, and
    // every client (TUI, web) must see the same set.
    let commands = {
        let guard = plugin_loader.lock().await;
        guard
            .as_ref()
            .map(|l| l.plugin_registry().commands_json())
            .unwrap_or_default()
    };

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "commands": commands,
        }),
    )
}

pub(crate) async fn handle_lua_shutdown_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaShutdownSessionRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();

    // Fire on_session_end hooks before removing the Lua session.
    //
    // Why this path exists in addition to dispatch.rs::handle_session_end:
    // The CLI fires `lua.shutdown_session` whenever the chat REPL exits,
    // including paths that never called `session.end` (process kill,
    // unhandled error, daemon shutdown). For those cases this is the only
    // chance to run `on_session_end` handlers, so we tag the reason as
    // `Shutdown`.
    //
    // Daemon ensures one fire per session lifecycle: whichever of
    // `session.end` (reason=User) or `lua.shutdown_session` (reason=Shutdown)
    // arrives first sets `end_hooks_fired`; the second is a no-op. Plugins
    // do NOT need to be idempotent.
    if let Some(state) = lua_sessions.get(session_id) {
        let state = state.value().clone();
        let mut state = state.lock().await;
        if state.end_hooks_fired {
            tracing::debug!(
                session_id = %session_id,
                "on_session_end hooks already fired; skipping"
            );
        } else {
            if let Err(e) = state.executor.sync_session_end_hooks() {
                warn!(session_id = %session_id, error = %e, "Failed to sync session_end hooks");
            }
            if let Some(session) = state.executor.session_manager().get_current() {
                if let Err(e) = state.executor.fire_session_end_hooks(&session).await {
                    warn!(session_id = %session_id, error = %e, "Failed to fire session_end hooks");
                }
            }
            state.end_hooks_fired = true;
        }
    }

    let removed = lua_sessions.remove(session_id).is_some();
    Response::success(
        req.id,
        serde_json::json!({
            "shutdown": removed,
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Lua plugin management RPC handlers
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) async fn handle_lua_discover_plugins(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaDiscoverPluginsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    // `kiln_path` is accepted and ignored: the parameter predates the decision
    // that a kiln's `plugins/` is not a search path. Kept so the RPC shape does
    // not break, and because it will mean something again if per-kiln loading
    // returns behind `runtimepath`.
    let _ = &params.kiln_path;

    match discover_available_plugins() {
        Ok(entries) => Response::success(
            req.id,
            serde_json::json!({
                "plugins": entries,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

/// The plugins on the standard search paths, as status entries.
///
/// **Discovery only — nothing is executed.** This used to call
/// `PluginManager::initialize`, which loads what it finds, so answering "what
/// plugins are there" ran every one of their `init.lua` files. Two callers made
/// that reachable: this RPC (which the web UI calls, `daemon.rs:399`) and
/// `session.create`'s setup task. Name, version and state all come from
/// `plugin.yaml`; none of them needs a VM.
///
/// Shared by the RPC and `session.create` so the two cannot answer differently.
pub(crate) fn discover_available_plugins(
) -> anyhow::Result<Vec<crucible_core::types::PluginStatusEntry>> {
    let manager = PluginManager::discover_only()?;
    Ok(manager
        .list()
        .map(|p| crucible_core::types::PluginStatusEntry {
            name: p.name().to_string(),
            version: p.version().to_string(),
            state: p.state.to_string(),
            error: p.last_error.clone(),
        })
        .collect())
}

pub(crate) async fn handle_lua_plugin_health(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaPluginHealthRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let plugin_path_str = params.plugin_path.clone();
    let plugin_path = PathBuf::from(&plugin_path_str);

    if !plugin_path.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Plugin path does not exist: {}", plugin_path.display()),
        );
    }

    // Find health.lua in the plugin directory
    let health_path = if plugin_path.file_name().and_then(|n| n.to_str()) == Some("health.lua") {
        Some(plugin_path.clone())
    } else {
        let hp = plugin_path.join("health.lua");
        if hp.exists() {
            Some(hp)
        } else {
            None
        }
    };

    let Some(health_path) = health_path else {
        return Response::success(
            req.id,
            serde_json::json!({
                "name": plugin_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                "healthy": true,
                "checks": [],
                "message": "No health.lua found"
            }),
        );
    };

    let executor = match LuaExecutor::new() {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };
    let lua = executor.lua();

    // Setup test mocks (in case health checks use cru.* APIs)
    if let Err(e) = lua
        .load("test_mocks = test_mocks or {}; test_mocks.setup = function() end")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Load and execute health.lua
    let health_lua = match std::fs::read_to_string(&health_path) {
        Ok(s) => s,
        Err(e) => return internal_error(req.id, e),
    };

    let health_module: mlua::Table = match lua.load(&health_lua).eval() {
        Ok(t) => t,
        Err(e) => return internal_error(req.id, e),
    };

    let check_fn: mlua::Function = match health_module.get("check") {
        Ok(f) => f,
        Err(e) => return internal_error(req.id, e),
    };

    if let Err(e) = check_fn.call::<()>(()) {
        return internal_error(req.id, e);
    }

    // Get results from cru.health.get_results
    let get_results: mlua::Function = match lua.load("return cru.health.get_results").eval() {
        Ok(f) => f,
        Err(e) => return internal_error(req.id, e),
    };

    let results: mlua::Table = match get_results.call(()) {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    // Extract results
    let name: String = results.get("name").unwrap_or_default();
    let healthy: bool = results.get("healthy").unwrap_or(false);
    let checks_table: Option<mlua::Table> = results.get("checks").ok();

    let mut checks_vec = Vec::new();
    if let Some(table) = checks_table {
        if let Ok(len) = table.len() {
            for i in 1..=len as usize {
                if let Ok(check) = table.get::<mlua::Table>(i) {
                    let level: String = check.get("level").unwrap_or_default();
                    let msg: String = check.get("msg").unwrap_or_default();
                    let advice: Vec<String> = check
                        .get::<mlua::Table>("advice")
                        .ok()
                        .map(|t| {
                            let mut items = Vec::new();
                            if let Ok(alen) = t.len() {
                                for j in 1..=alen as usize {
                                    if let Ok(s) = t.get::<String>(j) {
                                        items.push(s);
                                    }
                                }
                            }
                            items
                        })
                        .unwrap_or_default();
                    let mut obj = serde_json::json!({ "level": level, "msg": msg });
                    if !advice.is_empty() {
                        obj["advice"] = serde_json::json!(advice);
                    }
                    checks_vec.push(obj);
                }
            }
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "name": name,
            "healthy": healthy,
            "checks": checks_vec,
        }),
    )
}

pub(crate) async fn handle_lua_generate_stubs(req: Request) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaGenerateStubsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let output_dir = params.output_dir.clone();

    if params.verify {
        match StubGenerator::verify(Path::new(&output_dir)) {
            Ok(true) => Response::success(
                req.id,
                serde_json::json!({ "status": "ok", "path": output_dir }),
            ),
            Ok(false) => Response::success(
                req.id,
                serde_json::json!({ "status": "outdated", "path": output_dir }),
            ),
            Err(e) => internal_error(req.id, e),
        }
    } else {
        match StubGenerator::generate(Path::new(&output_dir)) {
            Ok(()) => Response::success(
                req.id,
                serde_json::json!({ "status": "ok", "path": output_dir }),
            ),
            Err(e) => internal_error(req.id, e),
        }
    }
}

pub(crate) async fn handle_lua_register_commands(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::LuaRegisterCommandsRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let commands = &params.commands;

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let mut registered: usize = 0;

    for cmd in commands {
        if let Some(source) = cmd.get("source").and_then(|v| v.as_str()) {
            if state.executor.lua().load(source).exec().is_ok() {
                registered += 1;
            }
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "registered": registered,
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage maintenance RPC stubs
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod discover_plugins_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Write a plugin into `<root>/plugins/<name>/` whose `init.lua` records
    /// that it ran, by touching `<root>/EXECUTED`.
    ///
    /// The marker is the point: a plugin that merely fails to appear in a
    /// listing could still have been executed, and executing it is the actual
    /// defect.
    fn write_kiln_plugin(kiln_root: &Path, name: &str, version: &str) {
        let plugin_dir = kiln_root.join("plugins").join(name);
        fs::create_dir_all(&plugin_dir).unwrap();
        fs::write(
            plugin_dir.join("plugin.yaml"),
            format!("name: {name}\nversion: \"{version}\"\nmain: init.lua\n"),
        )
        .unwrap();
        let marker = kiln_root.join("EXECUTED");
        fs::write(
            plugin_dir.join("init.lua"),
            format!(
                "local f = io.open({:?}, 'w'); f:write('ran'); f:close()\n\
                 return {{ setup = function() end }}\n",
                marker.to_string_lossy()
            ),
        )
        .unwrap();
    }

    #[test]
    fn returns_ok_when_nothing_is_installed() {
        let result = discover_available_plugins();
        // The user's own config_dir may hold plugins; only the call contract is
        // asserted here.
        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
    }

    /// A kiln's `plugins/` is not a search path, and — the part that matters —
    /// its plugins are not executed.
    ///
    /// They were. `session.create` and this RPC both called
    /// `PluginManager::initialize(Some(kiln))`, which discovers *and loads*,
    /// so opening a session against a cloned repo ran that repo's Lua inside
    /// the daemon. The manager was then dropped, so the tools and handlers it
    /// registered reached nobody: the exposure was total and the feature was
    /// nil. A kiln's plugins load by putting the kiln on `runtimepath`.
    #[test]
    fn a_kiln_plugin_is_neither_listed_nor_executed() {
        let kiln = TempDir::new().unwrap();
        write_kiln_plugin(kiln.path(), "krohn-test-plugin", "0.1.0");

        let entries = discover_available_plugins().expect("discovery succeeds");

        assert!(
            !entries.iter().any(|e| e.name == "krohn-test-plugin"),
            "a kiln's plugins/ must not be a search path, got {:?}",
            entries.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(
            !kiln.path().join("EXECUTED").exists(),
            "the kiln plugin's init.lua RAN — this is `git clone` to arbitrary \
             code execution in the daemon"
        );
    }
}
