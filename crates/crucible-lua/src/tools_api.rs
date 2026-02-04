//! Direct tool invocation API for Lua scripts
//!
//! Provides `cru.tools.*` functions for calling workspace tools directly from
//! Lua plugins, without going through a session/agent round-trip.
//!
//! ## Architecture
//!
//! ```text
//! crucible-lua (this crate)         crucible-daemon
//! ┌──────────────────────┐          ┌──────────────────────┐
//! │ DaemonToolsApi       │◄─────────│ impl DaemonToolsApi  │
//! │   (trait)            │          │  using WorkspaceTools │
//! │                      │          │                       │
//! │ register_tools_*     │          └───────────────────────┘
//! │   (module setup)     │
//! └──────────────────────┘
//! ```
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Call a single tool
//! local result, err = cru.tools.call("read_file", { path = "src/main.rs" })
//! if result then
//!     print(result.result)
//! end
//!
//! -- List available tools
//! local tools, err = cru.tools.list()
//! for _, t in ipairs(tools) do
//!     print(t.name, t.description)
//! end
//!
//! -- Call multiple tools in parallel
//! local results, err = cru.tools.batch({
//!     { tool = "read_file", args = { path = "Cargo.toml" } },
//!     { tool = "glob", args = { pattern = "**/*.rs" } },
//! })
//! -- results[1] = { result = "...", err = nil }
//! -- results[2] = { result = "...", err = nil }
//! ```

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Trait abstracting daemon tool operations for Lua plugins.
///
/// The daemon crate implements this using its `WorkspaceTools`. All methods
/// use `serde_json::Value` as the interchange format to avoid coupling to
/// concrete daemon types.
///
/// # Error Convention
///
/// Methods return `Result<T, String>` where the error string is surfaced to Lua
/// as the second return value: `local result, err = cru.tools.call(...)`.
pub trait DaemonToolsApi: Send + Sync + 'static {
    /// Call a single tool by name with the given arguments.
    ///
    /// Returns the tool's result as a JSON value.
    fn call_tool(
        &self,
        name: String,
        args: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// List available tools.
    ///
    /// Returns an array of tool definition objects with `name`, `description`,
    /// and `parameters` fields.
    fn list_tools(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;
}

/// Register the tools module with stub functions.
///
/// Creates the `cru.tools` and `crucible.tools` namespaces with functions
/// that return `(nil, "no daemon connected")`. Call [`register_tools_module_with_api`]
/// to replace stubs with real daemon-backed implementations.
pub fn register_tools_module(lua: &Lua) -> Result<(), LuaError> {
    let tools = lua.create_table()?;

    // Helper: all stubs return (nil, error_string)
    macro_rules! stub_async {
        ($name:expr, $lua:expr, $tools:expr, $args:ty) => {
            let f = $lua.create_async_function(|lua, _args: $args| async move {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
            $tools.set($name, f)?;
        };
    }

    stub_async!("call", lua, tools, (String, mlua::Value));
    stub_async!("list", lua, tools, ());
    stub_async!("batch", lua, tools, mlua::Value);

    register_in_namespaces(lua, "tools", tools)?;

    Ok(())
}

/// Register the tools module with a real daemon API implementation.
///
/// This replaces the stub functions registered by [`register_tools_module`]
/// with implementations that delegate to the provided [`DaemonToolsApi`].
pub fn register_tools_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonToolsApi>,
) -> Result<(), LuaError> {
    // First register stubs to create the table structure
    register_tools_module(lua)?;

    // Now get the table and replace stubs with real implementations
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let tools: Table = cru.get("tools")?;

    // call(tool_name, args_table) -> (result, nil) or (nil, err)
    let a = Arc::clone(&api);
    let call_fn = lua.create_async_function(move |lua, (name, args): (String, Value)| {
        let a = Arc::clone(&a);
        async move {
            let json_args: serde_json::Value = match args {
                Value::Table(_) => serde_json::to_value(&args).map_err(mlua::Error::external)?,
                Value::Nil => serde_json::Value::Object(serde_json::Map::new()),
                _ => {
                    let err = lua.create_string("call() args must be a table or nil")?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };
            match a.call_tool(name, json_args).await {
                Ok(val) => {
                    let lua_val = lua.to_value(&val)?;
                    Ok((lua_val, Value::Nil))
                }
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    tools.set("call", call_fn)?;

    // list() -> (tools_array, nil) or (nil, err)
    let a = Arc::clone(&api);
    let list_fn = lua.create_async_function(move |lua, (): ()| {
        let a = Arc::clone(&a);
        async move {
            match a.list_tools().await {
                Ok(vals) => {
                    let table = lua.create_table()?;
                    for (i, val) in vals.iter().enumerate() {
                        let lua_val = lua.to_value(val)?;
                        table.set(i + 1, lua_val)?;
                    }
                    Ok((Value::Table(table), Value::Nil))
                }
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    tools.set("list", list_fn)?;

    // batch(calls_array) -> (results_array, nil) or (nil, err)
    //
    // calls_array = { { tool = "read_file", args = { path = "..." } }, ... }
    // results_array = { { result = ..., err = nil }, { result = nil, err = "..." }, ... }
    //
    // Calls are executed concurrently via futures::join_all.
    let a = Arc::clone(&api);
    let batch_fn = lua.create_async_function(move |lua, calls: Value| {
        let a = Arc::clone(&a);
        async move {
            let calls_table = match calls {
                Value::Table(t) => t,
                _ => {
                    let err =
                        lua.create_string("batch() expects an array of {tool, args} tables")?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };

            // Parse all call specs from the Lua table
            let mut call_specs: Vec<(String, serde_json::Value)> = Vec::new();
            for pair in calls_table.sequence_values::<Table>() {
                let entry = match pair {
                    Ok(t) => t,
                    Err(e) => {
                        let err = lua.create_string(format!("invalid batch entry: {e}"))?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                let tool_name: String = match entry.get("tool") {
                    Ok(n) => n,
                    Err(_) => {
                        let err = lua.create_string("each batch entry requires a 'tool' field")?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                let args_val: Value = entry.get("args").unwrap_or(Value::Nil);
                let json_args: serde_json::Value = match args_val {
                    Value::Table(_) => {
                        serde_json::to_value(&args_val).map_err(mlua::Error::external)?
                    }
                    Value::Nil => serde_json::Value::Object(serde_json::Map::new()),
                    _ => {
                        let err = lua.create_string(format!(
                            "args for tool '{}' must be a table",
                            tool_name
                        ))?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                call_specs.push((tool_name, json_args));
            }

            if call_specs.is_empty() {
                let result = lua.create_table()?;
                return Ok((Value::Table(result), Value::Nil));
            }

            // Execute all calls concurrently
            let futures: Vec<_> = call_specs
                .into_iter()
                .map(|(name, args)| {
                    let a = Arc::clone(&a);
                    async move {
                        let result = a.call_tool(name.clone(), args).await;
                        (name, result)
                    }
                })
                .collect();

            let results = futures_util::future::join_all(futures).await;

            // Build results table
            let result_table = lua.create_table()?;
            for (i, (_name, result)) in results.into_iter().enumerate() {
                let entry = lua.create_table()?;
                match result {
                    Ok(val) => {
                        let lua_val = lua.to_value(&val)?;
                        entry.set("result", lua_val)?;
                    }
                    Err(e) => {
                        let err_str = lua.create_string(&e)?;
                        entry.set("err", Value::String(err_str))?;
                    }
                }
                result_table.set(i + 1, entry)?;
            }

            Ok((Value::Table(result_table), Value::Nil))
        }
    })?;
    tools.set("batch", batch_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_tools_module(&lua).expect("Should register tools module");
        lua
    }

    #[test]
    fn tools_module_registers_in_namespace() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let tools: Table = cru.get("tools").expect("cru.tools should exist");

        assert!(tools.contains_key("call").unwrap());
        assert!(tools.contains_key("list").unwrap());
        assert!(tools.contains_key("batch").unwrap());

        // Also registered under crucible.*
        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let tools2: Table = crucible.get("tools").expect("crucible.tools should exist");
        assert!(tools2.contains_key("call").unwrap());
    }

    #[tokio::test]
    async fn tools_stub_call_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.call("read_file", { path = "test.txt" })"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        match result.1 {
            Value::String(s) => assert_eq!(s.to_str().unwrap(), "no daemon connected"),
            _ => panic!("Expected error string, got {:?}", result.1),
        }
    }

    #[tokio::test]
    async fn tools_stub_list_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
    }

    #[tokio::test]
    async fn tools_stub_batch_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(
                r#"return cru.tools.batch({
                    { tool = "read_file", args = { path = "test.txt" } },
                })"#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock implementation of DaemonToolsApi for testing.
    struct MockToolsApi {
        call_count: AtomicUsize,
    }

    impl MockToolsApi {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }
    }

    impl DaemonToolsApi for MockToolsApi {
        fn call_tool(
            &self,
            name: String,
            args: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                match name.as_str() {
                    "read_file" => {
                        let path = args
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        Ok(serde_json::json!({
                            "result": format!("contents of {}", path)
                        }))
                    }
                    "glob" => Ok(serde_json::json!({
                        "result": "file1.rs\nfile2.rs\n\n[2 files]"
                    })),
                    "bash" => {
                        let cmd = args
                            .get("command")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        Ok(serde_json::json!({
                            "result": format!("output of: {}", cmd)
                        }))
                    }
                    _ => Err(format!("Unknown tool: {name}")),
                }
            })
        }

        fn list_tools(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            Box::pin(async {
                Ok(vec![
                    serde_json::json!({
                        "name": "read_file",
                        "description": "Read file contents",
                    }),
                    serde_json::json!({
                        "name": "bash",
                        "description": "Execute bash command",
                    }),
                    serde_json::json!({
                        "name": "glob",
                        "description": "Find files by pattern",
                    }),
                ])
            })
        }
    }

    fn setup_lua_with_api(api: Arc<dyn DaemonToolsApi>) -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_tools_module_with_api(&lua, api).expect("Should register tools with API");
        lua
    }

    #[tokio::test]
    async fn tools_call_returns_result() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local result, err = cru.tools.call("read_file", { path = "src/main.rs" })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return result
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let text: String = result.get("result").unwrap();
        assert!(text.contains("contents of src/main.rs"));
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_error() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.call("nonexistent", {})"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        match result.1 {
            Value::String(s) => assert!(s.to_str().unwrap().contains("Unknown tool")),
            _ => panic!("Expected error string"),
        }
    }

    #[tokio::test]
    async fn tools_call_with_nil_args() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local result, err = cru.tools.call("glob")
                assert(err == nil, "unexpected error: " .. tostring(err))
                return result
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let text: String = result.get("result").unwrap();
        assert!(text.contains("file1.rs"));
    }

    #[tokio::test]
    async fn tools_list_returns_definitions() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local tools, err = cru.tools.list()
                assert(err == nil, "unexpected error: " .. tostring(err))
                return tools
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 3);

        let first: Table = result.get(1).unwrap();
        assert_eq!(first.get::<String>("name").unwrap(), "read_file");
    }

    #[tokio::test]
    async fn tools_batch_returns_all_results() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local results, err = cru.tools.batch({
                    { tool = "read_file", args = { path = "Cargo.toml" } },
                    { tool = "bash", args = { command = "echo hi" } },
                })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return results
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);

        let first: Table = result.get(1).unwrap();
        let first_result: Table = first.get("result").unwrap();
        let text: String = first_result.get("result").unwrap();
        assert!(text.contains("Cargo.toml"));

        let second: Table = result.get(2).unwrap();
        let second_result: Table = second.get("result").unwrap();
        let text2: String = second_result.get("result").unwrap();
        assert!(text2.contains("echo hi"));
    }

    #[tokio::test]
    async fn tools_batch_handles_mixed_success_and_error() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local results, err = cru.tools.batch({
                    { tool = "read_file", args = { path = "test.rs" } },
                    { tool = "nonexistent", args = {} },
                })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return results
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);

        // First should succeed
        let first: Table = result.get(1).unwrap();
        assert!(first.contains_key("result").unwrap());

        // Second should have error
        let second: Table = result.get(2).unwrap();
        let err_str: String = second.get("err").unwrap();
        assert!(err_str.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn tools_batch_empty_array_returns_empty() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local results, err = cru.tools.batch({})
                assert(err == nil, "unexpected error: " .. tostring(err))
                return results
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 0);
    }
}
