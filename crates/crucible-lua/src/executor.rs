//! Lua script executor
//!
//! Executes Luau scripts with async support and
//! optional thread safety via the `send` feature.

use crate::error::LuaError;
use crate::fs::register_fs_module;
use crate::handlers::Firing;
use crate::hooks::register_hooks_module;
use crate::http::register_http_module;
use crate::modules::{ModuleRegistry, PrivateRootGuard, RootKind};
use crate::oil::register_oil_module;
use crate::session_api::{register_session_module, CurrentSession, Session};
#[cfg(any(test, feature = "test-utils"))]
use crate::types::LuaExecutionResult;
use mlua::{Function, Lua, LuaSerdeExt, Table, Value};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-utils"))]
use std::time::Instant;

/// Lua script executor
///
/// With the `send` feature enabled, this can be wrapped in Arc<Mutex<>>
/// for multi-threaded use.
pub struct LuaExecutor {
    lua: Lua,
    modules: ModuleRegistry,
    current_session: CurrentSession,
}

impl LuaExecutor {
    /// Create a new Lua executor
    pub fn new() -> Result<Self, LuaError> {
        let lua = Lua::new();

        // Every handler budget is enforced from inside this hook, so it has to
        // be installed before any plugin code can run. See `handler_budget`.
        crate::handler_budget::install_deadline_hook(&lua)?;

        // Set up safe globals and Crucible API
        Self::setup_globals(&lua)?;
        let modules = ModuleRegistry::install(&lua)?;

        let current_session = register_session_module(&lua)?;

        Ok(Self {
            lua,
            modules,
            current_session,
        })
    }

    pub fn current_session(&self) -> &CurrentSession {
        &self.current_session
    }

    /// Every `session:start` hook registered on this VM.
    #[cfg(test)]
    pub fn session_start_hooks(&self) -> Vec<crate::Registration> {
        crate::hooks::session_start_hooks(&self.lua, Firing::Sessionless).unwrap_or_default()
    }

    /// Fire all registered session start hooks.
    ///
    /// Async because hooks routinely call async `cru.*` APIs. `cru.shell.exec`,
    /// `cru.http.*` and `cru.timer.sleep` are all `create_async_function`s, which
    /// must be driven from a coroutine — a plain `Function::call` cannot suspend
    /// them. Firing these synchronously meant a hook that starts a container
    /// (the `oci` plugin's entire purpose) could never work.
    ///
    /// A hook registered with `{ required = true }` is **fatal to the
    /// session**; every other hook's failure is logged and the session
    /// continues.
    ///
    /// That opt-in is the whole point. A hook owning an isolation boundary
    /// (`oci` and its container) must be able to stop a session that would
    /// otherwise run unsandboxed — but if *every* hook were fatal, a single
    /// typo in any loaded plugin would refuse every session daemon-wide.
    ///
    /// Every hook still runs even after one fails, so one plugin's failure
    /// can't skip another's setup; required failures are collected and
    /// reported together.
    pub async fn fire_session_start_hooks(&self, session: &Session) -> Result<(), LuaError> {
        let mut failures = Vec::new();
        let id = session.id();
        // The session these hooks run for, so a hook that activates its
        // plugin FOR this session resolves the id from the host rather than
        // naming one. `on_session_start` is the place a plugin author will
        // reach for, so the bracket has to be here and not only on the
        // `cru.on` dispatch path.
        let _session = crate::plugin_context::enter_session(&self.lua, Some(&id));
        for hook in crate::hooks::session_start_hooks(&self.lua, Firing::InSession(&id))? {
            match hook.take_body(&self.lua) {
                Ok(func) => {
                    // Under the source that registered it, exactly as the end
                    // path runs. Without this the hook ran with no source, so
                    // `cru.storage` refused its writes and
                    // `cru.plugin.publish` attributed them to nobody.
                    let previous =
                        crate::plugin_context::set_source(&self.lua, hook.source.clone());
                    let result = self
                        .call_lifecycle_hook(&func, session, "session_start")
                        .await;
                    crate::plugin_context::set_source(&self.lua, previous);
                    if let Err(e) = result {
                        tracing::error!(
                            required = hook.required,
                            "Session start hook failed: {}",
                            e
                        );
                        if hook.required {
                            failures.push(e.to_string());
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        required = hook.required,
                        "Failed to retrieve session start hook from registry: {}",
                        e
                    );
                    if hook.required {
                        failures.push(e.to_string());
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(LuaError::Runtime(format!(
                "session_start hook(s) failed: {}",
                failures.join("; ")
            )))
        }
    }

    /// Run one lifecycle hook under the lifecycle time budget.
    ///
    /// 120 s, not the 30 s a turn-loop stage gets: `oci` pulls container images
    /// in `on_session_start`, and a short default would break a shipped plugin.
    /// Both mechanisms apply, for the reason `handler_budget` gives — the
    /// timeout ends a hook that awaits, the VM deadline ends one that spins.
    async fn call_lifecycle_hook(
        &self,
        func: &Function,
        session: &Session,
        what: &str,
    ) -> mlua::Result<()> {
        let budget = crate::handler_budget::LIFECYCLE_BUDGET;
        let _guard = crate::handler_budget::enter(&self.lua, budget, format!("the `{what}` hook"));
        match tokio::time::timeout(budget, func.call_async::<()>(session.clone())).await {
            Ok(call) => call,
            Err(_elapsed) => Err(mlua::Error::runtime(format!(
                "the `{what}` hook exceeded its {} ms time budget and was cancelled",
                budget.as_millis()
            ))),
        }
    }

    /// Fire all registered session end hooks.
    ///
    /// Errors are logged per-hook and do not propagate — unlike the start
    /// hooks. Async for the same reason as [`Self::fire_session_start_hooks`] — and it
    /// matters more here: teardown hooks stop containers and release resources
    /// via async `cru.shell.exec`, so a synchronous call leaks whatever the
    /// start hook acquired.
    ///
    /// Each hook runs under the source that registered it, as every other
    /// registration does. `cru.storage` refuses a call from an source that
    /// names no plugin, so without this a plugin cannot read at session end
    /// what it stored during the session.
    pub async fn fire_session_end_hooks(&self, session: &Session) -> Result<(), LuaError> {
        let id = session.id();
        let _session = crate::plugin_context::enter_session(&self.lua, Some(&id));
        for hook in crate::hooks::session_end_hooks(&self.lua, Firing::InSession(&id))? {
            match hook.take_body(&self.lua) {
                Ok(func) => {
                    let previous =
                        crate::plugin_context::set_source(&self.lua, hook.source.clone());
                    let result = self
                        .call_lifecycle_hook(&func, session, "session_end")
                        .await;
                    // Restore on every path: an source left behind attributes
                    // whatever runs next to the wrong author.
                    crate::plugin_context::set_source(&self.lua, previous);
                    if let Err(e) = result {
                        tracing::error!("Session end hook failed: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to retrieve session end hook from registry: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Load user configuration from init.lua
    ///
    /// This registers cru.statusline and other config modules,
    /// then loads init.lua from the config directory if it exists.
    pub fn load_config(&self, kiln_path: Option<&Path>) -> Result<(), LuaError> {
        use crate::config::ConfigLoader;
        let loader = ConfigLoader::with_defaults(kiln_path);
        loader.load(&self.lua)
    }

    /// Set up global functions available to scripts
    fn setup_globals(lua: &Lua) -> Result<(), LuaError> {
        let globals = lua.globals();

        // `io` and the file-touching half of `os` are the host's, because
        // Luau ships neither. See `crate::luau_compat`.
        crate::luau_compat::register_stdlib_compat(lua)?;

        // Create the cru namespace — the one Lua root Crucible owns.
        lua.load("cru = cru or {}").exec()?;

        let cru_ns: mlua::Table = globals.get("cru")?;

        register_log_function(lua, &cru_ns)?;

        // `cru.json.encode(value, { pretty = true })` — the pretty form
        // replaces `oq.json_pretty`, so the option lives beside `encode`
        // rather than in a second function name.
        let json_encode = lua.create_function(|_lua, (value, opts): (Value, Option<Table>)| {
            let pretty = match opts {
                Some(opts) => opts.get::<Option<bool>>("pretty")?.unwrap_or(false),
                None => false,
            };
            if pretty {
                serde_json::to_string_pretty(&value).map_err(mlua::Error::external)
            } else {
                serde_json::to_string(&value).map_err(mlua::Error::external)
            }
        })?;
        let json_decode = lua.create_function(|lua, s: String| {
            let json: JsonValue = serde_json::from_str(&s).map_err(mlua::Error::external)?;
            lua.to_value(&json)
        })?;

        register_hooks_module(lua, &cru_ns)?;
        crate::auth_plugin::register_auth_module(lua, &cru_ns)?;
        crate::notify::register_notify_module(lua, &cru_ns)?;

        let json_table = lua.create_table()?;
        json_table.set("encode", json_encode)?;
        json_table.set("decode", json_decode)?;
        // `cru.json.array(t)` — mark a table as a JSON list. Lua cannot tell an
        // empty list from an empty map, and the encoder resolves that as a map,
        // so an unmarked empty list reaches a consumer as `{}` while a
        // populated one is `[…]`. Tools returning result lists need the type to
        // be stable across "found nothing".
        //
        // It answers with the SAME table, marked — not a copy — so the return
        // is chained (`return cru.json.array({})`) rather than discarded.
        let mut json = crate::host_registry::Ns::over(lua, "cru.json", json_table.clone());
        json.func(
            "array",
            "(list: { any }) -> { any }",
            |lua, table: mlua::Table| crate::json_query::mark_json_array(lua, table),
        )?;
        cru_ns.set("json", json_table)?;

        // Register oil module for UI building
        register_oil_module(lua)?;

        // Register cru.config.set()/get() for unified config
        crate::config::register_app_config_api(lua, &cru_ns)?;

        // Register stateless utility modules
        register_http_module(lua)?;
        register_fs_module(lua)?;
        crate::timer::register_timer_module(lua)?;
        crate::ratelimit::register_ratelimit_module(lua)?;
        crate::vec_api::register_vec_module(lua)?;
        crate::prelude::register_prelude(lua)?;

        Ok(())
    }

    /// Install the plugin test harness in this executor's VM.
    ///
    /// Only the plugin test runner calls this. A production VM must not carry
    /// `describe`, `it`, `run_tests`, or the harness `assert` table.
    pub fn install_test_harness(&self) -> Result<(), LuaError> {
        crate::prelude::register_test_harness(&self.lua).map_err(LuaError::from)
    }

    /// Execute Luau source code.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn execute_source(
        &self,
        source: &str,
        args: JsonValue,
    ) -> Result<LuaExecutionResult, LuaError> {
        let start = Instant::now();

        // Execute the script
        let result = self.execute_lua(source, args);

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(content) => Ok(LuaExecutionResult {
                success: true,
                content: Some(content),
                error: None,
                duration_ms,
            }),
            Err(e) => Ok(LuaExecutionResult {
                success: false,
                content: None,
                error: Some(e.to_string()),
                duration_ms,
            }),
        }
    }

    /// Execute Lua source and call the main/handler function
    #[cfg(any(test, feature = "test-utils"))]
    fn execute_lua(&self, source: &str, args: JsonValue) -> Result<JsonValue, LuaError> {
        // Load and execute the chunk (defines functions)
        self.lua.load(source).exec()?;

        // Look for handler or main function
        let globals = self.lua.globals();

        let handler: Function = globals
            .get("handler")
            .or_else(|_| globals.get("main"))
            .map_err(|_| LuaError::InvalidTool("No 'handler' or 'main' function found".into()))?;

        // Convert args to Lua
        let lua_args = self.lua.to_value(&args)?;

        // Call handler
        let result: Value = handler.call(lua_args)?;

        // Convert result back to JSON
        Ok(serde_json::to_value(&result)?)
    }

    /// Get a reference to the underlying Lua state
    ///
    /// Use this for advanced integration (e.g., registering custom functions).
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// The module resolver this VM's `require` reads.
    pub fn modules(&self) -> &ModuleRegistry {
        &self.modules
    }

    /// Restrict `require` to the supplied plugin roots. Luau has no mutable
    /// `package.path`; lookup belongs to the host so plugin code cannot widen
    /// its own import authority.
    pub fn configure_module_roots(&self, roots: Vec<PathBuf>) -> Result<(), LuaError> {
        self.configure_roots(
            roots
                .into_iter()
                .map(|root| (root, RootKind::Plugin))
                .collect(),
        )
    }

    /// Add plugin roots to the search roots that are already there. The
    /// boot seeds the user root and the plugin roots before `init.lua`
    /// runs; the activation pass adds its roots without dropping those.
    pub fn add_module_roots(&self, roots: Vec<PathBuf>) -> Result<(), LuaError> {
        for root in roots {
            self.modules.add_root(root, RootKind::Plugin)?;
        }
        Ok(())
    }

    /// Set the search roots, user roots included.
    pub fn configure_roots(&self, roots: Vec<(PathBuf, RootKind)>) -> Result<(), LuaError> {
        self.modules.set_roots(roots)?;
        Ok(())
    }

    /// Make one plugin's own `lua/` directory resolvable while the guard
    /// lives. The guard pops it, so the next plugin does not inherit it.
    pub fn enter_plugin_root(&self, plugin_dir: &Path) -> Result<PrivateRootGuard, LuaError> {
        Ok(self.modules.enter_plugin_root(plugin_dir)?)
    }

    /// Forget the plugin's private modules cached from under `dir`, so a
    /// reload re-reads them. The entry instance in `package.loaded` stays:
    /// activation reuses the one a boot `require` created.
    pub fn invalidate_private_modules_under(&self, dir: &Path) -> Result<(), LuaError> {
        self.modules.invalidate_private_under(&self.lua, dir)?;
        Ok(())
    }
}

/// `cru.log(level, message)` — the base function, declared on `cru` itself
/// rather than in a namespace of its own.
///
/// [`crate::notify::register_notify_module`] then wraps it into the callable
/// log table that carries `levels`, `notify`, `notify_once` and `messages`.
/// The metatable's `__call` forwards to this closure, so this declaration is
/// the CALL half of the intersection the generator renders for `cru.log`.
///
/// A level the match does not know logs at INFO rather than raising: a plugin
/// that misspells a level must still get its message out.
pub fn register_log_function(lua: &Lua, cru: &mlua::Table) -> Result<(), LuaError> {
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru.clone());
    root.func(
        "log",
        "(level: string, message: string) -> ()",
        |_, (level, msg): (String, String)| {
            match level.as_str() {
                "debug" => tracing::debug!("{}", msg),
                "info" => tracing::info!("{}", msg),
                "warn" => tracing::warn!("{}", msg),
                "error" => tracing::error!("{}", msg),
                _ => tracing::info!("{}", msg),
            }
            Ok(())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_simple_lua() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
            function handler(args)
                return { result = args.x + args.y }
            end
        "#;

        let args = serde_json::json!({ "x": 1, "y": 2 });
        let result = executor.execute_source(source, args).await.unwrap();

        assert!(result.success);
        assert_eq!(result.content, Some(serde_json::json!({ "result": 3 })));
    }

    #[tokio::test]
    async fn test_crucible_log() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
            function handler(args)
                cru.log("info", "Hello from Lua!")
                return { logged = true }
            end
        "#;

        let result = executor
            .execute_source(source, serde_json::json!({}))
            .await
            .unwrap();

        assert!(result.success);
    }

    #[tokio::test]
    async fn test_json_roundtrip() {
        let executor = LuaExecutor::new().unwrap();

        let source = r#"
            function handler(args)
                local encoded = cru.json.encode(args)
                local decoded = cru.json.decode(encoded)
                return decoded
            end
        "#;

        let args = serde_json::json!({
            "string": "hello",
            "number": 42,
            "array": [1, 2, 3],
            "nested": { "key": "value" }
        });

        let result = executor.execute_source(source, args.clone()).await.unwrap();

        assert!(result.success);
        assert_eq!(result.content, Some(args));
    }

    #[test]
    fn test_hook_storage_empty_by_default() {
        let executor = LuaExecutor::new().unwrap();
        assert!(executor.session_start_hooks().is_empty());
    }

    #[test]
    fn test_on_session_start_registers_hook() {
        let executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(
                r#"
            cru.on_session_start(function(s) end)
        "#,
            )
            .exec()
            .unwrap();
        assert_eq!(executor.session_start_hooks().len(), 1);
    }

    #[tokio::test]
    async fn test_fire_hooks_calls_registered_hooks() {
        use crate::session_api::Session;

        let executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(
                r#"
            test_called = false
            cru.on_session_start(function(s) 
                test_called = true
            end)
        "#,
            )
            .exec()
            .unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_start_hooks(&session).await.unwrap();

        let called: bool = executor.lua().load("return test_called").eval().unwrap();
        assert!(called);
    }

    #[test]
    fn test_on_session_end_registers_hook() {
        let executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(r#"cru.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();
        assert_eq!(
            crate::hooks::session_end_hooks(executor.lua(), crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn test_fire_session_end_hooks_calls_registered_hooks() {
        use crate::session_api::Session;

        let executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(
                r#"
            test_end_called = false
            cru.on_session_end(function(s)
                test_end_called = true
            end)
        "#,
            )
            .exec()
            .unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_end_hooks(&session).await.unwrap();

        let called: bool = executor
            .lua()
            .load("return test_end_called")
            .eval()
            .unwrap();
        assert!(called);
    }

    /// A session-end hook runs under the plugin that registered it, so
    /// `cru.storage` resolves that plugin's namespace. Without the context
    /// the store refuses the call, and a hook that reads what the plugin
    /// stored during the session gets nothing.
    #[tokio::test]
    async fn session_end_hook_runs_in_the_context_of_its_plugin() {
        use crate::session_api::Session;
        use crate::test_support::MemoryPropertyStore;
        use crucible_core::storage::PropertyStore;
        use std::sync::Arc;

        let executor = LuaExecutor::new().unwrap();
        let store = Arc::new(MemoryPropertyStore::new());
        crate::register_storage_module(executor.lua()).unwrap();
        crate::register_storage_module_with_store(
            executor.lua(),
            Arc::clone(&store) as Arc<dyn PropertyStore>,
        )
        .unwrap();

        // Register the hook as the `reflection` plugin does: inside its load.
        let previous = crate::plugin_context::enter_plugin(executor.lua(), "reflection");
        executor
            .lua()
            .load(
                r#"
            cru.on_session_end(function(s)
                end_hook_read = cru.storage.get(s.id, "injected_titles")
            end)
        "#,
            )
            .exec()
            .unwrap();
        crate::plugin_context::set_source(executor.lua(), previous);

        store
            .property_set(
                "test",
                "plugin:reflection",
                "injected_titles",
                "[\"Kilns\"]",
            )
            .await
            .unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_end_hooks(&session).await.unwrap();

        let read: Option<String> = executor.lua().load("return end_hook_read").eval().unwrap();
        assert_eq!(read.as_deref(), Some("[\"Kilns\"]"));
        assert!(
            crate::plugin_context::current_source(executor.lua())
                == crate::plugin_context::LuaSource::UserLua,
            "the fire path must restore the previous source"
        );
    }

    /// …and so does a session-START hook. The start path ran every hook with
    /// NO plugin context, so `cru.storage` refused the call and a plugin
    /// reading its own state at session start got nothing.
    #[tokio::test]
    async fn session_start_hook_runs_in_the_context_of_its_plugin() {
        use crate::session_api::Session;
        use crate::test_support::MemoryPropertyStore;
        use crucible_core::storage::PropertyStore;
        use std::sync::Arc;

        let executor = LuaExecutor::new().unwrap();
        let store = Arc::new(MemoryPropertyStore::new());
        crate::register_storage_module(executor.lua()).unwrap();
        crate::register_storage_module_with_store(
            executor.lua(),
            Arc::clone(&store) as Arc<dyn PropertyStore>,
        )
        .unwrap();

        let previous = crate::plugin_context::enter_plugin(executor.lua(), "reflection");
        executor
            .lua()
            .load(
                r#"
            cru.on_session_start(function(s)
                start_hook_read = cru.storage.get(s.id, "injected_titles")
            end)
        "#,
            )
            .exec()
            .unwrap();
        crate::plugin_context::set_source(executor.lua(), previous);

        store
            .property_set(
                "test",
                "plugin:reflection",
                "injected_titles",
                "[\"Kilns\"]",
            )
            .await
            .unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_start_hooks(&session).await.unwrap();

        let read: Option<String> = executor
            .lua()
            .load("return start_hook_read")
            .eval()
            .unwrap();
        assert_eq!(read.as_deref(), Some("[\"Kilns\"]"));
        assert!(
            crate::plugin_context::current_source(executor.lua())
                == crate::plugin_context::LuaSource::UserLua,
            "the fire path must restore the previous source"
        );
    }

    /// The `pretty` option is the replacement for `oq.json_pretty`, so it
    /// must produce the same text.
    #[test]
    fn json_encode_pretty_produces_pretty_json() {
        let executor = LuaExecutor::new().unwrap();

        let encoded: String = executor
            .lua()
            .load(
                r#"
                return cru.json.encode({ name = "Alice" }, { pretty = true })
                "#,
            )
            .eval()
            .unwrap();

        // serde_json::to_string_pretty output, which is what oq.json_pretty
        // produced before its removal.
        assert_eq!(encoded, "{\n  \"name\": \"Alice\"\n}");
    }

    #[test]
    fn json_encode_stays_compact_without_the_option() {
        let executor = LuaExecutor::new().unwrap();

        let (plain, explicit): (String, String) = executor
            .lua()
            .load(
                r#"
                local value = { name = "Alice" }
                return cru.json.encode(value), cru.json.encode(value, { pretty = false })
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(plain, r#"{"name":"Alice"}"#);
        assert_eq!(explicit, plain);
    }

    #[test]
    fn test_http_module_available() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(r#"return type(cru.http.get) == "function""#)
            .eval()
            .unwrap();

        assert!(result, "http module should be available with get function");
    }

    #[test]
    fn test_fs_module_available() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(r#"return type(cru.fs.exists) == "function""#)
            .eval()
            .unwrap();

        assert!(result, "fs module should be available with exists function");
    }

    #[test]
    fn test_http_and_fs_modules_in_production() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(
                r#"
                local has_http = type(cru.http.get) == "function" and type(cru.http.post) == "function"
                local has_fs = type(cru.fs.mkdir) == "function" and type(cru.fs.exists) == "function"
                return has_http and has_fs
            "#,
            )
            .eval()
            .unwrap();

        assert!(
            result,
            "Both http and fs modules should be available in production"
        );
    }
}

#[cfg(test)]
mod vm_safety_tests {
    use super::*;

    /// A plugin must not reach a C module. Luau has no package library, so
    /// the host compatibility table exposes a module cache and nothing that
    /// loads native code.
    #[cfg(feature = "luau")]
    #[test]
    fn a_script_cannot_load_a_c_module() {
        let executor = LuaExecutor::new().expect("executor");
        let package: mlua::Table = executor
            .lua()
            .globals()
            .get("package")
            .expect("compatibility package table");
        assert!(
            package.get::<mlua::Value>("path").unwrap().is_nil(),
            "Luau must not expose a mutable package.path"
        );
        assert!(
            package.get::<mlua::Value>("loadlib").unwrap().is_nil(),
            "Luau must not expose package.loadlib"
        );
        assert!(package.get::<mlua::Table>("loaded").is_ok());
    }

    /// A plugin must not reach a C module. PUC Lua keeps `loadlib` and the C
    /// searchers, so the host puts both out of reach.
    #[cfg(not(feature = "luau"))]
    #[test]
    fn a_script_cannot_load_a_c_module() {
        let executor = LuaExecutor::new().expect("executor");
        let err = executor
            .lua()
            .load("return package.loadlib('/tmp/x.so', 'luaopen_x')")
            .exec()
            .expect_err("package.loadlib must refuse");
        assert!(
            err.to_string().contains("disabled"),
            "expected a refusal, got {err}"
        );

        let searcher_msg: String = executor
            .lua()
            .load("return package.searchers[3]()")
            .eval()
            .expect("the third searcher answers");
        assert!(
            searcher_msg.contains("C modules are disabled"),
            "expected the C searcher to refuse, got {searcher_msg:?}"
        );
    }

    /// The language's own `assert` is a function. The plugin test harness
    /// replaces it with a callable table, so the harness must not load in a
    /// VM that runs plugins or user config.
    #[test]
    fn a_production_vm_keeps_the_language_assert() {
        let executor = LuaExecutor::new().expect("executor");
        let kind: String = executor
            .lua()
            .load("return type(assert)")
            .eval()
            .expect("assert exists");
        assert_eq!(
            kind, "function",
            "the harness assert leaked into a plugin VM"
        );

        for absent in ["describe", "it", "run_tests", "before_each"] {
            let kind: String = executor
                .lua()
                .load(format!("return type({absent})"))
                .eval()
                .expect("type() answers");
            assert_eq!(kind, "nil", "the test harness leaked `{absent}`");
        }
    }

    /// The harness is still available where it belongs.
    #[test]
    fn a_test_vm_gets_the_harness() {
        let executor = LuaExecutor::new().expect("executor");
        executor.install_test_harness().expect("harness installs");
        let kind: String = executor
            .lua()
            .load("return type(describe)")
            .eval()
            .expect("type() answers");
        assert_eq!(kind, "function");

        // Even here `assert` is the language's own function. Matchers live on
        // `expect`, so a script reads the same inside a test and outside one.
        let assert_kind: String = executor
            .lua()
            .load("return type(assert)")
            .eval()
            .expect("assert exists");
        assert_eq!(assert_kind, "function", "the harness shadowed assert");
        let expect_kind: String = executor
            .lua()
            .load("return type(expect)")
            .eval()
            .expect("expect exists");
        assert_eq!(expect_kind, "table");
    }
}
