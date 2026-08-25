//! Lua script executor
//!
//! Executes Lua (and Fennel) scripts with async support and
//! optional thread safety via the `send` feature.

use crate::error::LuaError;
#[cfg(feature = "fennel")]
use crate::fennel::FennelCompiler;
use crate::fs::register_fs_module;
use crate::hooks::register_hooks_module;
use crate::http::register_http_module;
use crate::oil::register_oil_module;
use crate::session_api::{register_session_module, CurrentSession, Session};
#[cfg(any(test, feature = "test-utils"))]
use crate::types::LuaExecutionResult;
use mlua::{Function, Lua, LuaOptions, LuaSerdeExt, RegistryKey, StdLib, Table, Value};
use serde_json::Value as JsonValue;
use std::path::Path;
#[cfg(any(test, feature = "test-utils"))]
use std::time::Instant;

/// Lua script executor
///
/// With the `send` feature enabled, this can be wrapped in Arc<Mutex<>>
/// for multi-threaded use.
pub struct LuaExecutor {
    lua: Lua,
    #[cfg(feature = "fennel")]
    fennel: Option<FennelCompiler>,
    current_session: CurrentSession,
    on_session_start_hooks: Vec<RegistryKey>,
    /// Parallel to `on_session_start_hooks`: whether each opted into refusing
    /// the session on failure via `{ required = true }`.
    on_session_start_required: Vec<bool>,
    on_session_end_hooks: Vec<RegistryKey>,
}

impl LuaExecutor {
    /// Create a new Lua executor
    pub fn new() -> Result<Self, LuaError> {
        // Fennel needs PACKAGE (require/modules) and DEBUG (stack traces).
        // DEBUG is not in ALL_SAFE, so the VM is built with unsafe_new_with.
        //
        // That constructor skips mlua's own `disable_c_modules`, which is the
        // only thing that keeps `package.loadlib` and the two C searchers out
        // of reach. This VM runs every plugin, not just Fennel we shipped, so
        // `disable_c_modules_after_unsafe_new` below puts them back out of
        // reach. Without it a plugin loads a `.so` and leaves Lua entirely.
        #[cfg(feature = "fennel")]
        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
        };

        #[cfg(not(feature = "fennel"))]
        let lua = Lua::new();

        #[cfg(feature = "fennel")]
        disable_c_modules_after_unsafe_new(&lua)?;

        // Every handler budget is enforced from inside this hook, so it has to
        // be installed before any plugin code can run. See `handler_budget`.
        crate::handler_budget::install_deadline_hook(&lua)?;

        // Set up safe globals and Crucible API
        Self::setup_globals(&lua)?;

        // Try to load Fennel - it's optional (may not have vendor/fennel.lua)
        #[cfg(feature = "fennel")]
        let fennel = match FennelCompiler::new(&lua) {
            Ok(compiler) => Some(compiler),
            Err(e) => {
                tracing::debug!("Fennel compiler initialization failed: {}", e);
                None
            }
        };

        let current_session = register_session_module(&lua)?;

        Ok(Self {
            lua,
            #[cfg(feature = "fennel")]
            fennel,
            current_session,
            on_session_start_hooks: Vec::new(),
            on_session_start_required: Vec::new(),
            on_session_end_hooks: Vec::new(),
        })
    }

    /// Check if Fennel compiler is available
    #[cfg(test)]
    pub fn fennel_available(&self) -> bool {
        #[cfg(feature = "fennel")]
        {
            self.fennel.is_some()
        }
        #[cfg(not(feature = "fennel"))]
        {
            false
        }
    }

    pub fn current_session(&self) -> &CurrentSession {
        &self.current_session
    }

    /// Get all session start hooks
    #[cfg(test)]
    pub fn session_start_hooks(&self) -> &[RegistryKey] {
        &self.on_session_start_hooks
    }

    /// Sync session start hooks from Lua environment
    pub fn sync_session_start_hooks(&mut self) -> Result<(), LuaError> {
        use crate::hooks::{get_session_start_hooks, get_session_start_required_flags};
        self.on_session_start_hooks = get_session_start_hooks(&self.lua)?;
        self.on_session_start_required = get_session_start_required_flags(&self.lua)?;
        Ok(())
    }

    /// Fire all registered session start hooks.
    ///
    /// Async because hooks routinely call async `cru.*` APIs. `cru.shell.exec`,
    /// `cru.http.*` and `cru.timer.sleep` are all `create_async_function`s, which
    /// must be driven from a coroutine — a plain `Function::call` cannot suspend
    /// them. Firing these synchronously meant a hook that starts a container
    /// (the `oci` plugin's entire purpose) could never work.
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
        for (i, key) in self.on_session_start_hooks.iter().enumerate() {
            // Absent flag => not required, so a hook registered by older code
            // stays non-fatal.
            let required = self
                .on_session_start_required
                .get(i)
                .copied()
                .unwrap_or(false);
            match self.lua.registry_value::<Function>(key) {
                Ok(func) => {
                    if let Err(e) = self
                        .call_lifecycle_hook(&func, session, "session_start")
                        .await
                    {
                        tracing::error!(required, "Session start hook failed: {}", e);
                        if required {
                            failures.push(e.to_string());
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(
                        required,
                        "Failed to retrieve session start hook from registry: {}",
                        e
                    );
                    if required {
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

    /// Sync session end hooks from Lua environment
    pub fn sync_session_end_hooks(&mut self) -> Result<(), LuaError> {
        use crate::hooks::get_session_end_hooks;
        let hooks = get_session_end_hooks(&self.lua)?;
        self.on_session_end_hooks = hooks;
        Ok(())
    }

    /// Fire all registered session end hooks.
    ///
    /// Errors are logged per-hook and do not propagate — unlike the start
    /// hooks. Async for the same reason as [`Self::fire_session_start_hooks`] — and it
    /// matters more here: teardown hooks stop containers and release resources
    /// via async `cru.shell.exec`, so a synchronous call leaks whatever the
    /// start hook acquired.
    pub async fn fire_session_end_hooks(&self, session: &Session) -> Result<(), LuaError> {
        for key in &self.on_session_end_hooks {
            match self.lua.registry_value::<Function>(key) {
                Ok(func) => {
                    if let Err(e) = self
                        .call_lifecycle_hook(&func, session, "session_end")
                        .await
                    {
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

        // Create the cru namespace — the one Lua root Crucible owns.
        lua.load("cru = cru or {}").exec()?;

        let cru_ns: mlua::Table = globals.get("cru")?;

        // cru.log(level, message) — the base function.
        // `register_notify_module` wraps it into the callable log table that
        // carries `levels`, `notify`, `notify_once` and `messages`.
        let log_fn = lua.create_function(|_, (level, msg): (String, String)| {
            match level.as_str() {
                "debug" => tracing::debug!("{}", msg),
                "info" => tracing::info!("{}", msg),
                "warn" => tracing::warn!("{}", msg),
                "error" => tracing::error!("{}", msg),
                _ => tracing::info!("{}", msg),
            }
            Ok(())
        })?;
        cru_ns.set("log", log_fn)?;

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
        json_table.set(
            "array",
            lua.create_function(|lua, table: mlua::Table| {
                crate::json_query::mark_json_array(lua, table)
            })?,
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
        crate::lua_stdlib::register_lua_stdlib(lua)?;

        Ok(())
    }

    /// Install the plugin test harness in this executor's VM.
    ///
    /// Only the plugin test runner calls this. A production VM must not carry
    /// `describe`, `it`, `run_tests`, or the harness `assert` table.
    pub fn install_test_harness(&self) -> Result<(), LuaError> {
        crate::lua_stdlib::register_test_harness(&self.lua).map_err(LuaError::from)
    }

    /// Compile Fennel source to Lua with this executor's compiler.
    ///
    /// Public so callers that `lua().load()` sources directly (the plugin
    /// test runner) can handle `.fnl` files the same way `execute_source`
    /// does — those files used to be discovered, loaded raw, and counted as
    /// a load failure with a parse error that never said why.
    pub fn compile_fennel_source(&self, source: &str) -> Result<String, LuaError> {
        #[cfg(feature = "fennel")]
        {
            match &self.fennel {
                Some(fennel) => fennel.compile_with_lua(&self.lua, source),
                None => Err(LuaError::FennelCompile(
                    "Fennel compiler not available. Download fennel.lua from \
                    https://fennel-lang.org/downloads and place in \
                    crates/crucible-lua/vendor/fennel.lua"
                        .into(),
                )),
            }
        }
        #[cfg(not(feature = "fennel"))]
        {
            let _ = source;
            Err(LuaError::FennelCompile(
                "Fennel support not enabled (compile with 'fennel' feature)".into(),
            ))
        }
    }

    /// Execute Lua or Fennel source code
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn execute_source(
        &self,
        source: &str,
        is_fennel: bool,
        args: JsonValue,
    ) -> Result<LuaExecutionResult, LuaError> {
        let start = Instant::now();

        // Compile Fennel to Lua if needed
        #[cfg(feature = "fennel")]
        let lua_source = if is_fennel {
            self.compile_fennel_source(source)?
        } else {
            source.to_string()
        };

        #[cfg(not(feature = "fennel"))]
        let lua_source = if is_fennel {
            return Err(LuaError::FennelCompile(
                "Fennel support not enabled (compile with 'fennel' feature)".into(),
            ));
        } else {
            source.to_string()
        };

        // Execute the script
        let result = self.execute_lua(&lua_source, args);

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
        let result = executor.execute_source(source, false, args).await.unwrap();

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
            .execute_source(source, false, serde_json::json!({}))
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

        let result = executor
            .execute_source(source, false, args.clone())
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.content, Some(args));
    }

    #[test]
    fn test_fennel_available() {
        let executor = LuaExecutor::new().unwrap();
        // Should be available when fennel feature is enabled (default)
        #[cfg(feature = "fennel")]
        assert!(
            executor.fennel_available(),
            "Fennel should be available with fennel feature"
        );
        #[cfg(not(feature = "fennel"))]
        assert!(!executor.fennel_available());
    }

    #[test]
    fn test_hook_storage_empty_by_default() {
        let executor = LuaExecutor::new().unwrap();
        assert!(executor.session_start_hooks().is_empty());
    }

    #[test]
    fn test_on_session_start_registers_hook() {
        let mut executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(
                r#"
            cru.on_session_start(function(s) end)
        "#,
            )
            .exec()
            .unwrap();
        executor.sync_session_start_hooks().unwrap();
        assert_eq!(executor.session_start_hooks().len(), 1);
    }

    #[tokio::test]
    async fn test_fire_hooks_calls_registered_hooks() {
        use crate::session_api::Session;

        let mut executor = LuaExecutor::new().unwrap();
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
        executor.sync_session_start_hooks().unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_start_hooks(&session).await.unwrap();

        let called: bool = executor.lua().load("return test_called").eval().unwrap();
        assert!(called);
    }

    #[test]
    fn test_on_session_end_registers_hook() {
        let mut executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(r#"cru.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();
        executor.sync_session_end_hooks().unwrap();
        assert_eq!(executor.on_session_end_hooks.len(), 1);
    }

    #[tokio::test]
    async fn test_fire_session_end_hooks_calls_registered_hooks() {
        use crate::session_api::Session;

        let mut executor = LuaExecutor::new().unwrap();
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
        executor.sync_session_end_hooks().unwrap();

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

/// Put `package.loadlib` and the C searchers out of reach.
///
/// `Lua::new_with` does this itself; `unsafe_new_with` does not, and Fennel
/// forces the unsafe constructor because it needs the DEBUG library. This is
/// the same work mlua does in safe mode: replace `loadlib`, replace the third
/// searcher, drop the fourth (the all-in-one C loader).
#[cfg(feature = "fennel")]
fn disable_c_modules_after_unsafe_new(lua: &Lua) -> Result<(), LuaError> {
    let package: mlua::Table = lua.globals().get("package")?;
    package.set(
        "loadlib",
        lua.create_function(|_, ()| -> mlua::Result<()> {
            Err(mlua::Error::runtime(
                "package.loadlib is disabled: a plugin may not load a C module",
            ))
        })?,
    )?;
    let searchers: mlua::Table = package.get("searchers")?;
    let refuse = lua.create_function(|_, ()| Ok("\n\tC modules are disabled"))?;
    searchers.raw_set(3, refuse)?;
    if searchers.raw_len() >= 4 {
        searchers.raw_remove(4)?;
    }
    Ok(())
}

#[cfg(test)]
mod vm_safety_tests {
    use super::*;

    /// A plugin must not reach a C module. The VM is built with
    /// `unsafe_new_with` for Fennel's DEBUG library, which skips mlua's own
    /// safety pass, so the pass is redone by hand.
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
