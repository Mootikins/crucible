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
//! -- Call multiple tools in parallel. `opts.session` states which session the
//! -- calls are for, exactly as `call` does — an isolating daemon needs it to
//! -- decide whether that session is sandboxed.
//! local results, err = cru.tools.batch({
//!     { tool = "read_file", args = { path = "Cargo.toml" } },
//!     { tool = "glob", args = { pattern = "**/*.rs" } },
//! }, { session = ctx.session_id })
//! -- results[1] = { result = "...", err = nil }
//! -- results[2] = { result = "...", err = nil }
//!
//! -- Narrow what one session offers its model. Glob patterns, the same
//! -- language a mode's `tools` selector speaks.
//! cru.tools.set_active(ctx.session_id, { "read_*", "grep_notes" })
//! local names = cru.tools.get_active(ctx.session_id)  -- the patterns above
//! cru.tools.set_active(ctx.session_id, nil)           -- back to automatic
//! ```
//!
//! ## What an active set does, exactly
//!
//! It **narrows and only narrows** the session's visible tool set, applied
//! after the session's mode filter and before progressive tool disclosure
//! decides what to defer. So:
//!
//! - it cannot re-add a tool the mode removed — `set_active` in plan mode
//!   still gets no write tools;
//! - narrowing shrinks the attached tool schemas, which usually takes the
//!   session under the disclosure budget so nothing is deferred at all;
//! - if what remains is still over that budget, deferral still happens. An
//!   active set is not an override of the context budget, and deferral costs
//!   no capability — deferred tools stay callable through `discover_tools` /
//!   `invoke_tool`.
//!
//! The set is enforced at dispatch as well as in the advertisement, so a
//! model that names an excluded tool anyway is refused rather than obeyed.
//! `discover_tools` and `get_tool_schema` are the exception: they enumerate
//! the whole catalog, so an excluded tool can still be *found* there — it
//! just cannot be run.
//!
//! Two things it is not: it is not persistent (a daemon restart drops every
//! set, and a resumed session comes back with its automatic tool list), and
//! it is not available to a session delegated to an external ACP agent —
//! Crucible does not assemble that agent's tool list, so `set_active` returns
//! an error there rather than narrowing half of it.

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
    /// `session` is the session the call is being made *for*, when the caller
    /// knows it — a plugin inside a hook has it as `ctx.session_id`. It exists
    /// because this path executes workspace tools with no agent and no session
    /// of its own, so without it an implementation cannot tell whether the
    /// session it is acting for is sandboxed. `None` means "not stated", which
    /// an isolating implementation must treat as unproven rather than safe.
    ///
    /// Returns the tool's result as a JSON value.
    fn call_tool(
        &self,
        name: String,
        args: serde_json::Value,
        session: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// List available tools.
    ///
    /// Returns an array of tool definition objects with `name`, `description`,
    /// and `parameters` fields.
    fn list_tools(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Put an explicit active tool set in force for `session`, or clear it.
    ///
    /// `Some(patterns)` narrows the session to the tools matching one of the
    /// glob patterns; an empty vector is a real answer ("no tools"), not a
    /// clear. `None` clears the set, restoring the automatic behaviour.
    ///
    /// Synchronous: this writes a registry, it does not reach a session's
    /// agent. The next request the session makes reads it.
    ///
    /// Errors when `session` names no live session, and when it names a
    /// session delegated to an external agent whose tool list the daemon does
    /// not assemble. Both used to answer `Ok(())` and do nothing.
    fn set_active_tools(
        &self,
        session: String,
        patterns: Option<Vec<String>>,
    ) -> Result<(), String>;

    /// The patterns in force for `session`, or `None` when it has no explicit
    /// set and is therefore offering whatever its mode and budget allow.
    fn get_active_tools(&self, session: String) -> Result<Option<Vec<String>>, String>;
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

    stub_async!("call", lua, tools, (String, mlua::Value, mlua::Value));
    stub_async!("list", lua, tools, ());
    stub_async!("batch", lua, tools, (mlua::Value, mlua::Value));

    // `set_active`/`get_active` are synchronous — they read and write a
    // registry rather than executing anything — so their stubs are too.
    macro_rules! stub_sync {
        ($name:expr, $lua:expr, $tools:expr, $args:ty) => {
            let f = $lua.create_function(|lua: &Lua, _args: $args| {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
            $tools.set($name, f)?;
        };
    }

    stub_sync!("set_active", lua, tools, (String, mlua::Value));
    stub_sync!("get_active", lua, tools, String);

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

    // call(tool_name, args_table[, opts]) -> (result, nil) or (nil, err)
    //
    // `opts.session` states which session the call is for. A plugin calling
    // from inside a hook has it as `ctx.session_id`; passing it is what lets
    // the daemon side decide whether that session is sandboxed.
    let a = Arc::clone(&api);
    let call_fn =
        lua.create_async_function(move |lua, (name, args, opts): (String, Value, Value)| {
            let a = Arc::clone(&a);
            async move {
                let session = match &opts {
                    Value::Table(t) => t.get::<Option<String>>("session").ok().flatten(),
                    _ => None,
                };
                let json_args: serde_json::Value = match args {
                    Value::Table(_) => {
                        serde_json::to_value(&args).map_err(mlua::Error::external)?
                    }
                    Value::Nil => serde_json::Value::Object(serde_json::Map::new()),
                    _ => {
                        let err = lua.create_string("call() args must be a table or nil")?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                };
                match a.call_tool(name, json_args, session).await {
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

    // batch(calls_array[, opts]) -> (results_array, nil) or (nil, err)
    //
    // calls_array = { { tool = "read_file", args = { path = "..." } }, ... }
    // results_array = { { result = ..., err = nil }, { result = nil, err = "..." }, ... }
    //
    // Calls are executed concurrently via futures::join_all.
    let a = Arc::clone(&api);
    let batch_fn = lua.create_async_function(move |lua, (calls, opts): (Value, Value)| {
        let a = Arc::clone(&a);
        async move {
            // Same `opts.session` as `call`. Without it every batch was
            // "session not stated", which an isolating implementation must
            // refuse — leaving a sandboxed plugin an error telling it to pass
            // a session through a parameter that did not exist.
            let session = match &opts {
                Value::Table(t) => t.get::<Option<String>>("session").ok().flatten(),
                _ => None,
            };
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
                    let session = session.clone();
                    async move {
                        let result = a.call_tool(name.clone(), args, session).await;
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

    // set_active(session_id, names_or_nil) -> (true, nil) or (nil, err)
    //
    // `names` is an array of glob patterns. `nil` clears the set; `{}` is a
    // real answer meaning "no tools", so the two are NOT the same call.
    let a = Arc::clone(&api);
    let set_active_fn = lua.create_function(move |lua, (session, names): (String, Value)| {
        let patterns = match &names {
            Value::Nil => None,
            Value::Table(t) => {
                let mut out = Vec::new();
                for value in t.clone().sequence_values::<String>() {
                    match value {
                        Ok(name) => out.push(name),
                        Err(e) => {
                            let err = lua.create_string(format!(
                                "set_active() names must be an array of strings: {e}"
                            ))?;
                            return Ok((Value::Nil, Value::String(err)));
                        }
                    }
                }
                // Every entry has to be part of the array walk above. A
                // map-shaped table (`{ read_file = true }`) or a sparse one
                // (`{ [1] = "a", [3] = "b" }`) has entries the walk never
                // reaches, and the leftover was silently the empty set — so
                // asking for two tools by the wrong shape handed the session
                // NO tools. `{}` is still the deliberate empty set: it has
                // nothing outside the array either.
                let entries = t.clone().pairs::<Value, Value>().count();
                if entries != out.len() {
                    let err = lua.create_string(format!(
                        "set_active() names must be an array of tool patterns; {} of \
                         the {entries} entries are not array elements (a map like \
                         {{ read_file = true }}, or a gap in the indices)",
                        entries - out.len()
                    ))?;
                    return Ok((Value::Nil, Value::String(err)));
                }
                Some(out)
            }
            _ => {
                let err =
                    lua.create_string("set_active() expects an array of tool patterns, or nil")?;
                return Ok((Value::Nil, Value::String(err)));
            }
        };
        match a.set_active_tools(session, patterns) {
            Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
            Err(e) => {
                let err = lua.create_string(&e)?;
                Ok((Value::Nil, Value::String(err)))
            }
        }
    })?;
    tools.set("set_active", set_active_fn)?;

    // get_active(session_id) -> (names, nil) | (nil, nil) | (nil, err)
    //
    // Three outcomes, not two: `(nil, nil)` is the session having no explicit
    // set, which is a successful answer and the common one. Check the second
    // return before concluding anything from a nil first return.
    let a = Arc::clone(&api);
    let get_active_fn =
        lua.create_function(
            move |lua, session: String| match a.get_active_tools(session) {
                Ok(None) => Ok((Value::Nil, Value::Nil)),
                Ok(Some(patterns)) => {
                    let table = lua.create_table()?;
                    for (i, name) in patterns.iter().enumerate() {
                        table.set(i + 1, name.as_str())?;
                    }
                    Ok((Value::Table(table), Value::Nil))
                }
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            },
        )?;
    tools.set("get_active", get_active_fn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn tools_module_registers_in_namespace() {
        let lua = TestLuaBuilder::new().with_tools().build();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let tools: Table = cru.get("tools").expect("cru.tools should exist");

        assert!(tools.contains_key("call").unwrap());
        assert!(tools.contains_key("list").unwrap());
        assert!(tools.contains_key("batch").unwrap());
        assert!(tools.contains_key("set_active").unwrap());
        assert!(tools.contains_key("get_active").unwrap());

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
        let lua = TestLuaBuilder::new().with_tools().build();

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
    async fn tools_stub_set_active_returns_nil() {
        let lua = TestLuaBuilder::new().with_tools().build();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.set_active("s1", { "read_*" })"#)
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
    async fn tools_stub_get_active_returns_nil() {
        let lua = TestLuaBuilder::new().with_tools().build();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.get_active("s1")"#)
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
        let lua = TestLuaBuilder::new().with_tools().build();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
    }

    #[tokio::test]
    async fn tools_stub_batch_returns_nil() {
        let lua = TestLuaBuilder::new().with_tools().build();

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
    use crate::test_support::TestLuaBuilder;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Mock implementation of DaemonToolsApi for testing.
    struct MockToolsApi {
        call_count: AtomicUsize,
        /// What each call stated as its session, in call order.
        sessions: std::sync::Mutex<Vec<Option<String>>>,
        /// Stands in for the daemon's per-session active tool sets.
        active: std::sync::Mutex<std::collections::HashMap<String, Vec<String>>>,
    }

    impl MockToolsApi {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                sessions: std::sync::Mutex::new(Vec::new()),
                active: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl DaemonToolsApi for MockToolsApi {
        fn call_tool(
            &self,
            name: String,
            args: serde_json::Value,
            session: Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            self.sessions.lock().unwrap().push(session);
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

        fn set_active_tools(
            &self,
            session: String,
            patterns: Option<Vec<String>>,
        ) -> Result<(), String> {
            let mut active = self.active.lock().unwrap();
            match patterns {
                Some(patterns) => active.insert(session, patterns),
                None => active.remove(&session),
            };
            Ok(())
        }

        fn get_active_tools(&self, session: String) -> Result<Option<Vec<String>>, String> {
            Ok(self.active.lock().unwrap().get(&session).cloned())
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

    #[tokio::test]
    async fn tools_call_returns_result() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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

    /// `batch` hardcoded `None`, so every batched call read as "session not
    /// stated" — which an isolating daemon must refuse. A sandboxed plugin got
    /// an error telling it to pass a session through a parameter `batch` did
    /// not have, and no way to comply.
    #[tokio::test]
    async fn tools_batch_forwards_the_session_to_every_call() {
        let mock = Arc::new(MockToolsApi::new());
        let api: Arc<dyn DaemonToolsApi> = mock.clone();
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        lua.load(
            r#"
            local _, err = cru.tools.batch({
                { tool = "read_file", args = { path = "a" } },
                { tool = "glob" },
            }, { session = "s-sandboxed" })
            assert(err == nil, "unexpected error: " .. tostring(err))
            "#,
        )
        .exec_async()
        .await
        .unwrap();

        let seen = mock.sessions.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert!(
            seen.iter().all(|s| s.as_deref() == Some("s-sandboxed")),
            "every call in the batch must carry the stated session, got {seen:?}"
        );
    }

    /// ...and omitting it still means "not stated", rather than inventing one.
    #[tokio::test]
    async fn tools_batch_without_opts_states_no_session() {
        let mock = Arc::new(MockToolsApi::new());
        let api: Arc<dyn DaemonToolsApi> = mock.clone();
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        lua.load(r#"cru.tools.batch({ { tool = "glob" } })"#)
            .exec_async()
            .await
            .unwrap();

        assert_eq!(mock.sessions.lock().unwrap().clone(), vec![None]);
    }

    #[tokio::test]
    async fn set_active_and_get_active_round_trip() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let result: Table = lua
            .load(
                r#"
                local ok, err = cru.tools.set_active("s1", { "read_*", "grep_notes" })
                assert(ok, "unexpected error: " .. tostring(err))
                local names, err2 = cru.tools.get_active("s1")
                assert(err2 == nil, "unexpected error: " .. tostring(err2))
                return names
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>(1).unwrap(), "read_*");
        assert_eq!(result.get::<String>(2).unwrap(), "grep_notes");
    }

    /// A session with no set answers `(nil, nil)` — a successful "nothing in
    /// force", not an error. A plugin has to be able to tell the two apart.
    #[tokio::test]
    async fn get_active_without_a_set_is_a_successful_nil() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.get_active("never-set")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        assert!(
            matches!(result.1, Value::Nil),
            "no set in force is not an error, got {:?}",
            result.1
        );
    }

    /// `nil` clears; `{}` means "no tools". Collapsing them would make
    /// "offer nothing" unsayable.
    #[tokio::test]
    async fn nil_clears_the_set_and_an_empty_table_does_not() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let (empty_len, cleared): (usize, Value) = lua
            .load(
                r#"
                cru.tools.set_active("s1", { "read_*" })
                cru.tools.set_active("s1", {})
                local empty = cru.tools.get_active("s1")
                assert(empty ~= nil, "an empty table is a set, not a clear")
                cru.tools.set_active("s1", nil)
                return #empty, cru.tools.get_active("s1")
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(empty_len, 0);
        assert!(matches!(cleared, Value::Nil));
    }

    #[tokio::test]
    async fn set_active_rejects_a_non_table_argument() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let result: (Value, Value) = lua
            .load(r#"return cru.tools.set_active("s1", "read_file")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        match result.1 {
            Value::String(s) => assert!(s.to_str().unwrap().contains("array of tool patterns")),
            _ => panic!("Expected error string, got {:?}", result.1),
        }
    }

    /// A map-shaped table is a mistake, not "offer no tools".
    ///
    /// The array walk sees nothing in `{ read_file = true }`, so the caller
    /// got the empty set — a plugin that meant to allow two tools silently
    /// took every tool away instead. Refusing is the only reading that cannot
    /// be mistaken for the deliberate `{}`.
    #[tokio::test]
    async fn set_active_rejects_a_map_shaped_table_instead_of_offering_no_tools() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let (value, err, still_unset): (Value, Value, Value) = lua
            .load(
                r#"
                local ok, err = cru.tools.set_active("s1", { read_file = true, bash = true })
                return ok, err, cru.tools.get_active("s1")
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(value, Value::Nil));
        match err {
            Value::String(s) => {
                assert!(s.to_str().unwrap().contains("not array elements"), "{s:?}")
            }
            other => panic!("expected an error string, got {other:?}"),
        }
        assert!(
            matches!(still_unset, Value::Nil),
            "a refused call must not have written a set"
        );
    }

    /// A gap in the indices hides everything past it from the array walk, so
    /// `{ [1] = "a", [3] = "b" }` used to mean "just a" with no complaint.
    #[tokio::test]
    async fn set_active_rejects_a_sparse_array() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

        let (value, err): (Value, Value) = lua
            .load(r#"return cru.tools.set_active("s1", { [1] = "read_*", [3] = "bash" })"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(value, Value::Nil));
        match err {
            Value::String(s) => {
                assert!(s.to_str().unwrap().contains("not array elements"), "{s:?}")
            }
            other => panic!("expected an error string, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tools_batch_empty_array_returns_empty() {
        let api: Arc<dyn DaemonToolsApi> = Arc::new(MockToolsApi::new());
        let lua = TestLuaBuilder::new().with_tools_api(api).build();

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
