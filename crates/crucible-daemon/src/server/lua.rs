use super::*;

pub(super) async fn handle_lua_init_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str).to_string();
    let kiln_root = optional_param!(req, "kiln_path", as_str)
        .or_else(|| optional_param!(req, "kiln", as_str))
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

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
    if let Err(e) = executor.fire_session_start_hooks(&session) {
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
        Arc::new(Mutex::new(LuaSessionState { executor, registry })),
    );

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "commands": [],
            "views": [],
        }),
    )
}

pub(super) async fn handle_lua_register_hooks(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let hooks = require_param!(req, "hooks", as_array);

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let initial_count = state
        .registry
        .runtime_handlers()
        .lock()
        .map(|handlers| handlers.len())
        .unwrap_or(0);

    for hook in hooks {
        let source = if let Some(source) = hook.as_str() {
            Some(source)
        } else if let Some(obj) = hook.as_object() {
            obj.get("source")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("code").and_then(|v| v.as_str()))
        } else {
            None
        };

        if let Some(source) = source {
            if let Err(e) = state.executor.lua().load(source).exec() {
                warn!(session_id = %session_id, error = %e, "Failed to register Lua hook source");
            }
        }
    }

    let final_count = state
        .registry
        .runtime_handlers()
        .lock()
        .map(|handlers| handlers.len())
        .unwrap_or(initial_count);
    let registered = final_count.saturating_sub(initial_count);
    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "registered": registered,
        }),
    )
}

pub(super) async fn handle_lua_execute_hook(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let hook_name = require_param!(req, "hook_name", as_str);
    let context = req
        .params
        .get("context")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    let Some(state) = lua_sessions.get(session_id) else {
        return session_not_found(req.id, session_id);
    };

    let state = state.value().clone();
    let state = state.lock().await;
    let handlers = state.registry.runtime_handlers_for(hook_name);
    let mut results = Vec::new();

    for handler in handlers {
        let event = SessionEvent::Custom {
            name: hook_name.to_string(),
            payload: context.clone(),
        };

        let result = match state.registry.execute_runtime_handler(
            state.executor.lua(),
            &handler.name,
            &event,
        ) {
            Ok(ScriptHandlerResult::Transform(payload)) => {
                serde_json::json!({"handler": handler.name, "type": "transform", "payload": payload})
            }
            Ok(ScriptHandlerResult::PassThrough) => {
                serde_json::json!({"handler": handler.name, "type": "pass_through"})
            }
            Ok(ScriptHandlerResult::Cancel { reason }) => {
                serde_json::json!({"handler": handler.name, "type": "cancel", "reason": reason})
            }
            Ok(ScriptHandlerResult::Inject { content, position }) => serde_json::json!({
                "handler": handler.name,
                "type": "inject",
                "content": content,
                "position": position,
            }),
            Err(e) => {
                serde_json::json!({"handler": handler.name, "type": "error", "error": e.to_string()})
            }
        };
        results.push(result);
    }

    Response::success(
        req.id,
        serde_json::json!({
            "executed": results.len(),
            "results": results,
        }),
    )
}

pub(super) async fn handle_lua_shutdown_session(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
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

pub(super) async fn handle_lua_discover_plugins(req: Request) -> Response {
    let kiln_path = require_param!(req, "kiln_path", as_str).to_string();

    match PluginManager::initialize(Some(Path::new(&kiln_path))) {
        Ok(manager) => {
            let plugins: Vec<serde_json::Value> = manager
                .list()
                .map(|p| {
                    serde_json::json!({
                        "name": p.name(),
                        "version": p.version(),
                        "state": p.state.to_string(),
                        "error": p.last_error,
                    })
                })
                .collect();
            Response::success(req.id, serde_json::json!({ "plugins": plugins }))
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(super) async fn handle_lua_plugin_health(req: Request) -> Response {
    let plugin_path_str = require_param!(req, "plugin_path", as_str).to_string();
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

pub(super) async fn handle_lua_generate_stubs(req: Request) -> Response {
    let output_dir = require_param!(req, "output_dir", as_str).to_string();
    let verify = optional_param!(req, "verify", as_bool).unwrap_or(false);

    if verify {
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

pub(super) async fn handle_lua_run_plugin_tests(req: Request) -> Response {
    let test_path_str = require_param!(req, "test_path", as_str).to_string();
    let filter = optional_param!(req, "filter", as_str).map(|s| s.to_string());
    let test_path = PathBuf::from(&test_path_str);

    if !test_path.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Test path does not exist: {}", test_path.display()),
        );
    }

    // Discover test files
    let test_files = match discover_plugin_test_files(&test_path) {
        Ok(files) => files,
        Err(e) => return internal_error(req.id, e),
    };

    if test_files.is_empty() {
        return Response::success(
            req.id,
            serde_json::json!({ "passed": 0, "failed": 0, "load_failures": 0, "message": "No test files found" }),
        );
    }

    let executor = match LuaExecutor::new() {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };

    // Set package.path to include the plugin root
    let plugin_root = test_path
        .canonicalize()
        .unwrap_or_else(|_| test_path.clone());
    let plugin_root_str = plugin_root.to_string_lossy();
    if let Err(e) = executor
        .lua()
        .load(format!(
            r#"
local plugin_root = {plugin_root_str:?}
local entries = {{
    plugin_root .. "/?.lua",
    plugin_root .. "/?/init.lua",
}}
for _, entry in ipairs(entries) do
    if not package.path:find(entry, 1, true) then
        package.path = entry .. ";" .. package.path
    end
end
"#
        ))
        .set_name("plugin_package_path")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Setup test mocks
    if let Err(e) = executor
        .lua()
        .load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Apply test filter if provided
    if let Some(ref filter_str) = filter {
        if let Err(e) = executor
            .lua()
            .globals()
            .set("__cru_plugin_test_filter", filter_str.clone())
        {
            return internal_error(req.id, e);
        }
        if let Err(e) = executor
            .lua()
            .load(
                r#"
                local _orig_it = it
                local _orig_pending = pending
                local filter = _G.__cru_plugin_test_filter

                it = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_it(name, fn)
                    end
                end

                pending = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_pending(name, fn)
                    end
                end
                "#,
            )
            .set_name("test_filter")
            .exec()
        {
            return internal_error(req.id, e);
        }
    }

    // Load test files
    let mut load_failures: usize = 0;
    for file in &test_files {
        let file_contents = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(_) => {
                load_failures += 1;
                continue;
            }
        };

        let chunk_name = file.to_string_lossy();
        if executor
            .lua()
            .load(&file_contents)
            .set_name(chunk_name.as_ref())
            .exec()
            .is_err()
        {
            load_failures += 1;
        }
    }

    // Run tests
    let results: mlua::Table = match executor
        .lua()
        .load("return run_tests()")
        .set_name("plugin_test_runner")
        .eval()
    {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    let passed: usize = results.get("passed").unwrap_or(0);
    let failed: usize = results.get("failed").unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "passed": passed,
            "failed": failed,
            "load_failures": load_failures,
        }),
    )
}

pub(super) async fn handle_lua_register_commands(
    req: Request,
    lua_sessions: &Arc<DashMap<String, Arc<Mutex<LuaSessionState>>>>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let commands = require_param!(req, "commands", as_array);

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

/// Discover test files in a plugin directory (files ending with _test.lua or _test.fnl)
pub(super) fn discover_plugin_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();

    // Check tests/ subdirectory
    let tests_dir = path.join("tests");
    if tests_dir.is_dir() {
        collect_plugin_test_files(&tests_dir, &mut files)?;
    }

    // Check root directory
    collect_plugin_test_files(path, &mut files)?;

    files.sort();
    files.dedup();
    Ok(files)
}

pub(super) fn collect_plugin_test_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            let stem = path.file_stem().and_then(|name| name.to_str());
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!((stem, ext), (Some(s), Some("lua" | "fnl")) if s.ends_with("_test")) {
                out.push(path);
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage maintenance RPC stubs
// ─────────────────────────────────────────────────────────────────────────────
