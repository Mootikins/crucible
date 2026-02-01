//! Lua script executor
//!
//! Executes Lua (and Fennel) scripts with async support and
//! optional thread safety via the `send` feature.

use crate::ask::register_ask_module;
use crate::error::LuaError;
#[cfg(feature = "fennel")]
use crate::fennel::FennelCompiler;
use crate::fs::register_fs_module;
use crate::hooks::register_hooks_module;
use crate::http::register_http_module;
use crate::interaction::register_interaction_module;
use crate::oil::register_oil_module;
use crate::session_api::{register_session_module, Session, SessionManager};
use crate::types::{LuaExecutionResult, LuaTool, ToolResult};
use mlua::{Function, Lua, LuaOptions, LuaSerdeExt, RegistryKey, StdLib, Value};
use serde_json::Value as JsonValue;
use std::path::Path;
use std::time::Instant;
use tracing::instrument;

/// Lua script executor
///
/// With the `send` feature enabled, this can be wrapped in Arc<Mutex<>>
/// for multi-threaded use.
pub struct LuaExecutor {
    lua: Lua,
    #[cfg(feature = "fennel")]
    fennel: Option<FennelCompiler>,
    session_manager: SessionManager,
    on_session_start_hooks: Vec<RegistryKey>,
}

impl LuaExecutor {
    /// Create a new Lua executor
    pub fn new() -> Result<Self, LuaError> {
        // Fennel requires both PACKAGE (for require/modules) and DEBUG (for stack traces)
        // DEBUG is not in ALL_SAFE, so we use unsafe_new_with for Fennel support.
        // This is safe because we're running controlled Fennel code, not arbitrary C modules.
        #[cfg(feature = "fennel")]
        let lua = unsafe {
            Lua::unsafe_new_with(StdLib::ALL_SAFE | StdLib::DEBUG, LuaOptions::default())
        };

        #[cfg(not(feature = "fennel"))]
        let lua = Lua::new();

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

        let session_manager = register_session_module(&lua)?;

        Ok(Self {
            lua,
            #[cfg(feature = "fennel")]
            fennel,
            session_manager,
            on_session_start_hooks: Vec::new(),
        })
    }

    /// Check if Fennel compiler is available
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

    pub fn session_manager(&self) -> &SessionManager {
        &self.session_manager
    }

    /// Add a session start hook
    pub fn add_session_start_hook(&mut self, key: RegistryKey) {
        self.on_session_start_hooks.push(key);
    }

    /// Get all session start hooks
    pub fn session_start_hooks(&self) -> &[RegistryKey] {
        &self.on_session_start_hooks
    }

    /// Sync session start hooks from Lua environment
    pub fn sync_session_start_hooks(&mut self) -> Result<(), LuaError> {
        use crate::hooks::get_session_start_hooks;
        let hooks = get_session_start_hooks(&self.lua)?;
        self.on_session_start_hooks = hooks;
        Ok(())
    }

    /// Fire all registered session start hooks
    ///
    /// Calls each hook with the session object. Logs errors but continues
    /// to next hook (error isolation). Returns Ok even if some hooks fail.
    pub fn fire_session_start_hooks(&self, session: &Session) -> Result<(), LuaError> {
        for key in &self.on_session_start_hooks {
            match self.lua.registry_value::<Function>(key) {
                Ok(func) => {
                    if let Err(e) = func.call::<()>(session.clone()) {
                        tracing::error!("Session start hook failed: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to retrieve session start hook from registry: {}", e);
                }
            }
        }
        Ok(())
    }

    /// Load user configuration from init.lua
    ///
    /// This registers crucible.statusline and other config modules,
    /// then loads init.lua from the config directory if it exists.
    pub fn load_config(&self, kiln_path: Option<&Path>) -> Result<(), LuaError> {
        use crate::config::ConfigLoader;
        let loader = ConfigLoader::with_defaults(kiln_path);
        loader.load(&self.lua)
    }

    /// Set up global functions available to scripts
    fn setup_globals(lua: &Lua) -> Result<(), LuaError> {
        let globals = lua.globals();

        // Create cru namespace and define fmt function
        lua.load(
            r#"
cru = cru or {}
function cru.fmt(template, vars)
    vars = vars or {}
    return (template:gsub("{(%w+)}", function(key)
        local val = vars[key]
        if val ~= nil then
            return tostring(val)
        end
        return "{" .. key .. "}"
    end))
end
"#,
        )
        .exec()?;

        // Get the cru table that was just created
        let _cru: mlua::Table = globals.get("cru")?;

        // Create crucible namespace
        let crucible = lua.create_table()?;

        // crucible.log(level, message)
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
        crucible.set("log", log_fn)?;

        // crucible.json_encode(value) -> string
        let json_encode = lua.create_function(|_lua, value: Value| {
            serde_json::to_string(&value).map_err(mlua::Error::external)
        })?;
        crucible.set("json_encode", json_encode)?;

        // crucible.json_decode(string) -> value
        let json_decode = lua.create_function(|lua, s: String| {
            let json: JsonValue = serde_json::from_str(&s).map_err(mlua::Error::external)?;
            lua.to_value(&json)
        })?;
        crucible.set("json_decode", json_decode)?;

        register_hooks_module(lua, &crucible)?;
        crate::notify::register_notify_module(lua, &crucible)?;

        globals.set("crucible", crucible)?;

        // Register ask module for user interaction
        register_ask_module(lua)?;

        // Register oil module for UI building
        register_oil_module(lua)?;

        // Register interaction module for unified interaction bindings
        register_interaction_module(lua)?;

        // Register stateless utility modules
        register_http_module(lua)?;
        register_fs_module(lua)?;

        Ok(())
    }

    /// Execute a Lua or Fennel file
    #[instrument(skip(self, args), fields(path = %path.as_ref().display()))]
    pub async fn execute_file(
        &self,
        path: impl AsRef<Path>,
        args: JsonValue,
    ) -> Result<LuaExecutionResult, LuaError> {
        let path = path.as_ref();
        let source = tokio::fs::read_to_string(path).await?;

        let is_fennel = path.extension().map(|e| e == "fnl").unwrap_or(false);

        self.execute_source(&source, is_fennel, args).await
    }

    /// Execute Lua or Fennel source code
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
            match &self.fennel {
                Some(fennel) => fennel.compile_with_lua(&self.lua, source)?,
                None => {
                    return Err(LuaError::FennelCompile(
                        "Fennel compiler not available. Download fennel.lua from \
                        https://fennel-lang.org/downloads and place in \
                        crates/crucible-lua/vendor/fennel.lua"
                            .into(),
                    ));
                }
            }
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

    /// Execute a tool by name from the registry
    pub async fn execute_tool(
        &self,
        tool: &LuaTool,
        args: JsonValue,
    ) -> Result<ToolResult, LuaError> {
        let result = self.execute_file(&tool.source_path, args).await?;

        if result.success {
            Ok(ToolResult::ok(result.content.unwrap_or(JsonValue::Null)))
        } else {
            Ok(ToolResult::err(
                result.error.unwrap_or_else(|| "Unknown error".into()),
            ))
        }
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
                crucible.log("info", "Hello from Lua!")
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
                local encoded = crucible.json_encode(args)
                local decoded = crucible.json_decode(encoded)
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
            crucible.on_session_start(function(s) end)
        "#,
            )
            .exec()
            .unwrap();
        executor.sync_session_start_hooks().unwrap();
        assert_eq!(executor.session_start_hooks().len(), 1);
    }

    #[test]
    fn test_fire_hooks_calls_registered_hooks() {
        use crate::session_api::Session;

        let mut executor = LuaExecutor::new().unwrap();
        executor
            .lua()
            .load(
                r#"
            test_called = false
            crucible.on_session_start(function(s) 
                test_called = true
            end)
        "#,
            )
            .exec()
            .unwrap();
        executor.sync_session_start_hooks().unwrap();

        let session = Session::new("test".to_string());
        session.bind(Box::new(crate::session_api::tests::MockRpc::new()));
        executor.fire_session_start_hooks(&session).unwrap();

        let called: bool = executor.lua().load("return test_called").eval().unwrap();
        assert!(called);
    }

    #[test]
    fn test_cru_fmt_basic_substitution() {
        let executor = LuaExecutor::new().unwrap();

        let result: String = executor
            .lua()
            .load(r#"return cru.fmt("Hello {name}", {name="world"})"#)
            .eval()
            .unwrap();

        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_cru_fmt_missing_key_preserved() {
        let executor = LuaExecutor::new().unwrap();

        let result: String = executor
            .lua()
            .load(r#"return cru.fmt("Missing {key}", {})"#)
            .eval()
            .unwrap();

        assert_eq!(result, "Missing {key}");
    }

    #[test]
    fn test_cru_fmt_number_conversion() {
        let executor = LuaExecutor::new().unwrap();

        let result: String = executor
            .lua()
            .load(r#"return cru.fmt("Count: {n}", {n=42})"#)
            .eval()
            .unwrap();

        assert_eq!(result, "Count: 42");
    }

    #[test]
    fn test_cru_fmt_multiple_placeholders() {
        let executor = LuaExecutor::new().unwrap();

        let result: String = executor
            .lua()
            .load(r#"return cru.fmt("{greeting} {name}!", {greeting="Hello", name="Alice"})"#)
            .eval()
            .unwrap();

        assert_eq!(result, "Hello Alice!");
    }

    #[test]
    fn test_cru_fmt_empty_vars() {
        let executor = LuaExecutor::new().unwrap();

        let result: String = executor
            .lua()
            .load(r#"return cru.fmt("No placeholders", {})"#)
            .eval()
            .unwrap();

        assert_eq!(result, "No placeholders");
    }

    #[test]
    fn test_http_module_available() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(r#"return http ~= nil and type(http.get) == "function""#)
            .eval()
            .unwrap();

        assert!(result, "http module should be available with get function");
    }

    #[test]
    fn test_fs_module_available() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(r#"return fs ~= nil and type(fs.read) == "function""#)
            .eval()
            .unwrap();

        assert!(result, "fs module should be available with read function");
    }

    #[test]
    fn test_http_and_fs_modules_in_production() {
        let executor = LuaExecutor::new().unwrap();

        let result: bool = executor
            .lua()
            .load(
                r#"
                local has_http = http ~= nil and type(http.get) == "function" and type(http.post) == "function"
                local has_fs = fs ~= nil and type(fs.read) == "function" and type(fs.write) == "function"
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
