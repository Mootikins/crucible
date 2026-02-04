//! Multi-session management API for Lua scripts
//!
//! Provides `cru.sessions.*` functions for managing daemon sessions from Lua plugins.
//! This module defines a [`DaemonSessionApi`] trait that the daemon crate implements,
//! avoiding a circular dependency (crucible-lua cannot depend on crucible-daemon).
//!
//! ## Architecture
//!
//! ```text
//! crucible-lua (this crate)         crucible-daemon
//! ┌──────────────────────┐          ┌──────────────────────┐
//! │ DaemonSessionApi     │◄─────────│ impl DaemonSessionApi│
//! │   (trait)            │          │  using SessionManager│
//! │                      │          │  AgentManager        │
//! │ register_sessions_*  │          │  broadcast::Sender   │
//! │   (module setup)     │          └──────────────────────┘
//! └──────────────────────┘
//! ```
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Create a new session
//! local session, err = cru.sessions.create("chat", "/path/to/kiln")
//! if session then
//!     print(session.id, session.state)
//! end
//!
//! -- List all sessions
//! local sessions, err = cru.sessions.list()
//! for _, s in ipairs(sessions) do
//!     print(s.id, s.session_type, s.state)
//! end
//!
//! -- Send a message to a session
//! local response_id, err = cru.sessions.send_message("chat-2025-...", "Hello!")
//!
//! -- Subscribe to events
//! local next_event, err = cru.sessions.subscribe("chat-2025-...")
//! if next_event then
//!     local event = next_event()  -- blocks until next event
//!     print(event.type, event.data)
//! end
//!
//! -- End a session
//! cru.sessions.end_session("chat-2025-...")
//! ```

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A structured part of an agent response.
///
/// `send_and_collect` returns a `Vec<ResponsePart>` so callers (e.g. the Discord
/// plugin) can render each segment independently — sending tool calls as separate
/// messages, folding thinking blocks, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsePart {
    /// Prose / markdown text from the LLM.
    Text { content: String },
    /// The LLM requested a tool invocation.
    ToolCall {
        tool: String,
        /// Truncated JSON preview of the arguments.
        args_brief: String,
    },
    /// A tool finished executing.
    ToolResult {
        tool: String,
        /// Truncated preview of the result.
        result_brief: String,
        is_error: bool,
    },
    /// Chain-of-thought / thinking block.
    Thinking { content: String },
    /// The agent needs permission to proceed (e.g. run a command).
    PermissionRequest {
        request_id: String,
        tool: String,
        description: String,
    },
}

/// Trait abstracting daemon session operations for Lua plugins.
///
/// The daemon crate implements this using its `SessionManager`, `AgentManager`,
/// and `broadcast::Sender<SessionEventMessage>`. All methods use `serde_json::Value`
/// as the interchange format to avoid coupling to concrete daemon types.
///
/// # Error Convention
///
/// Methods return `Result<T, String>` where the error string is surfaced to Lua
/// as the second return value: `local result, err = cru.sessions.create(...)`.
pub trait DaemonSessionApi: Send + Sync + 'static {
    /// Create a new session.
    ///
    /// Returns a JSON object with at least `{ id, session_type, state, kiln, workspace }`.
    /// `kiln` defaults to `crucible_home()` when None.
    /// `connected_kilns` are additional kilns the session can query for knowledge.
    fn create_session(
        &self,
        session_type: String,
        kiln: Option<String>,
        workspace: Option<String>,
        connected_kilns: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Get a session by ID.
    ///
    /// Returns `Ok(None)` if the session doesn't exist.
    fn get_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>;

    /// List all sessions.
    ///
    /// Returns an array of session summary objects.
    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Configure the agent for a session.
    ///
    /// `agent_config` is a JSON object matching `SessionAgent` fields.
    fn configure_agent(
        &self,
        session_id: String,
        agent_config: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Send a user message to a session, triggering agent processing.
    ///
    /// Returns a request/response ID for tracking.
    fn send_message(
        &self,
        session_id: String,
        content: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Cancel the current operation in a session.
    ///
    /// Returns `true` if something was cancelled.
    fn cancel(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>>;

    /// Pause a session.
    fn pause(&self, session_id: String)
        -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Resume a paused session.
    fn resume(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// End a session permanently.
    fn end_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Respond to a permission/interaction request.
    fn respond_to_permission(
        &self,
        session_id: String,
        request_id: String,
        response: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Subscribe to session events.
    ///
    /// Returns a receiver that yields JSON event objects. Each call to `recv()`
    /// returns the next event or `None` if the subscription ended.
    fn subscribe(
        &self,
        session_id: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                        String,
                    >,
                > + Send,
        >,
    >;

    /// Unsubscribe from session events.
    fn unsubscribe(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Send a message and stream structured response parts.
    ///
    /// Subscribes, sends the message, then returns a receiver that yields
    /// [`ResponsePart`]s as they become available. Text deltas are accumulated
    /// and flushed as a single `Text` part at each boundary (tool call, tool
    /// result, thinking, or completion). `timeout_secs` defaults to 120.
    /// `max_tool_result_len` caps tool-result previews (default 500).
    fn send_and_collect(
        &self,
        session_id: String,
        content: String,
        timeout_secs: Option<f64>,
        max_tool_result_len: Option<usize>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>>
                + Send,
        >,
    >;
}

/// Register the sessions module with stub functions.
///
/// Creates the `cru.sessions` and `crucible.sessions` namespaces with functions
/// that return `(nil, "no daemon connected")`. Call [`register_sessions_module_with_api`]
/// to replace stubs with real daemon-backed implementations.
pub fn register_sessions_module(lua: &Lua) -> Result<(), LuaError> {
    let sessions = lua.create_table()?;

    // Helper: all stubs return (nil, error_string)
    macro_rules! stub_async {
        ($name:expr, $lua:expr, $sessions:expr, $args:ty) => {
            let f = $lua.create_async_function(|lua, _args: $args| async move {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
            $sessions.set($name, f)?;
        };
    }

    stub_async!("create", lua, sessions, mlua::Value);
    stub_async!("get", lua, sessions, String);
    stub_async!("list", lua, sessions, ());
    stub_async!("configure_agent", lua, sessions, (String, mlua::Value));
    stub_async!("send_message", lua, sessions, (String, String));
    stub_async!("cancel", lua, sessions, String);
    stub_async!("pause", lua, sessions, String);
    stub_async!("resume", lua, sessions, String);
    stub_async!("end_session", lua, sessions, String);
    stub_async!(
        "interaction_respond",
        lua,
        sessions,
        (String, String, mlua::Value)
    );
    stub_async!("subscribe", lua, sessions, String);
    stub_async!("unsubscribe", lua, sessions, String);
    stub_async!(
        "send_and_collect",
        lua,
        sessions,
        (String, String, mlua::Value)
    );

    register_in_namespaces(lua, "sessions", sessions)?;

    Ok(())
}

/// Register the sessions module with a real daemon API implementation.
///
/// This replaces the stub functions registered by [`register_sessions_module`]
/// with implementations that delegate to the provided [`DaemonSessionApi`].
pub fn register_sessions_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    // First register stubs to create the table structure
    register_sessions_module(lua)?;

    // Now get the table and replace stubs with real implementations
    let globals = lua.globals();
    let cru: Table = globals.get("cru")?;
    let sessions: Table = cru.get("sessions")?;

    // create({ type = "chat", kiln = "...", workspace = "...", kilns = {"..."} })
    // Also supports legacy positional: create("chat", "/path/to/kiln")
    let a = Arc::clone(&api);
    let create_fn = lua.create_async_function(move |lua, args: Value| {
        let a = Arc::clone(&a);
        async move {
            let (session_type, kiln, workspace, connected_kilns) = match args {
                Value::Table(ref t) => {
                    let st: String = t
                        .get::<String>("type")
                        .unwrap_or_else(|_| "chat".to_string());
                    let k: Option<String> = t.get("kiln").ok();
                    let ws: Option<String> = t.get("workspace").ok();
                    let kilns: Vec<String> = t.get::<Vec<String>>("kilns").unwrap_or_default();
                    (st, k, ws, kilns)
                }
                Value::String(ref s) => {
                    // Legacy positional: create("chat") — type only, no kiln
                    let st = s
                        .to_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| "chat".to_string());
                    (st, None, None, vec![])
                }
                _ => {
                    let err = lua.create_string(
                        "create() expects a table argument, e.g. { type = \"chat\" }",
                    )?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };
            match a
                .create_session(session_type, kiln, workspace, connected_kilns)
                .await
            {
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
    sessions.set("create", create_fn)?;

    // get(session_id)
    let a = Arc::clone(&api);
    let get_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.get_session(session_id).await {
                Ok(Some(val)) => {
                    let lua_val = lua.to_value(&val)?;
                    Ok((lua_val, Value::Nil))
                }
                Ok(None) => Ok((Value::Nil, Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("get", get_fn)?;

    // list()
    let a = Arc::clone(&api);
    let list_fn = lua.create_async_function(move |lua, (): ()| {
        let a = Arc::clone(&a);
        async move {
            match a.list_sessions().await {
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
    sessions.set("list", list_fn)?;

    // configure_agent(session_id, agent_config_table)
    let a = Arc::clone(&api);
    let configure_fn =
        lua.create_async_function(move |lua, (session_id, config): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let json_config: serde_json::Value =
                    serde_json::to_value(&config).map_err(mlua::Error::external)?;
                match a.configure_agent(session_id, json_config).await {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
    sessions.set("configure_agent", configure_fn)?;

    // send_message(session_id, content)
    let a = Arc::clone(&api);
    let send_fn =
        lua.create_async_function(move |lua, (session_id, content): (String, String)| {
            let a = Arc::clone(&a);
            async move {
                match a.send_message(session_id, content).await {
                    Ok(response_id) => {
                        let s = lua.create_string(&response_id)?;
                        Ok((Value::String(s), Value::Nil))
                    }
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
    sessions.set("send_message", send_fn)?;

    // cancel(session_id)
    let a = Arc::clone(&api);
    let cancel_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.cancel(session_id).await {
                Ok(cancelled) => Ok((Value::Boolean(cancelled), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("cancel", cancel_fn)?;

    // pause(session_id)
    let a = Arc::clone(&api);
    let pause_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.pause(session_id).await {
                Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("pause", pause_fn)?;

    // resume(session_id)
    let a = Arc::clone(&api);
    let resume_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.resume(session_id).await {
                Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("resume", resume_fn)?;

    // end_session(session_id)
    let a = Arc::clone(&api);
    let end_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.end_session(session_id).await {
                Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("end_session", end_fn)?;

    // interaction_respond(session_id, request_id, response_table)
    let a = Arc::clone(&api);
    let respond_fn = lua.create_async_function(
        move |lua, (session_id, request_id, response): (String, String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let json_response: serde_json::Value =
                    serde_json::to_value(&response).map_err(mlua::Error::external)?;
                match a
                    .respond_to_permission(session_id, request_id, json_response)
                    .await
                {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        },
    )?;
    sessions.set("interaction_respond", respond_fn)?;

    // subscribe(session_id) -> returns (next_event_fn, nil) or (nil, err)
    // next_event_fn() -> returns (event_table, nil) or (nil, nil) if stream ended
    let a = Arc::clone(&api);
    let subscribe_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.subscribe(session_id).await {
                Ok(rx) => {
                    // Wrap the receiver in Arc<Mutex> so the closure can own it
                    let rx = Arc::new(tokio::sync::Mutex::new(rx));
                    let call_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let next_fn = lua.create_async_function(move |lua, (): ()| {
                        let rx = Arc::clone(&rx);
                        let call_count = Arc::clone(&call_count);
                        async move {
                            let n = call_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            tracing::debug!(call = n, "next_event: acquiring lock");
                            let mut guard = rx.lock().await;
                            tracing::debug!(call = n, "next_event: lock acquired, awaiting recv");
                            match guard.recv().await {
                                Some(event) => {
                                    let event_type = event
                                        .get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown");
                                    tracing::debug!(
                                        call = n,
                                        event_type,
                                        "next_event: received event"
                                    );
                                    let lua_val = lua.to_value(&event)?;
                                    Ok((lua_val, Value::Nil))
                                }
                                None => {
                                    tracing::debug!(call = n, "next_event: channel closed (None)");
                                    Ok((Value::Nil, Value::Nil))
                                }
                            }
                        }
                    })?;
                    Ok((Value::Function(next_fn), Value::Nil))
                }
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("subscribe", subscribe_fn)?;

    // unsubscribe(session_id)
    let a = Arc::clone(&api);
    let unsubscribe_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.unsubscribe(session_id).await {
                Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("unsubscribe", unsubscribe_fn)?;

    // send_and_collect(session_id, content, opts?) -> (next_part, nil) or (nil, err)
    // next_part() yields { type = "text"|"tool_call"|"tool_result"|"thinking", ... } or nil
    let a = Arc::clone(&api);
    let collect_fn = lua.create_async_function(
        move |lua, (session_id, content, opts): (String, String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let (timeout_secs, max_tool_result_len) = match opts {
                    Value::Table(ref t) => (
                        t.get::<f64>("timeout").ok(),
                        t.get::<usize>("max_tool_result_len").ok(),
                    ),
                    Value::Number(n) => (Some(n), None),
                    _ => (None, None),
                };
                match a
                    .send_and_collect(session_id, content, timeout_secs, max_tool_result_len)
                    .await
                {
                    Ok(rx) => {
                        let rx = Arc::new(tokio::sync::Mutex::new(rx));
                        let next_part = lua.create_async_function(move |lua, ()| {
                            let rx = Arc::clone(&rx);
                            async move {
                                let mut guard = rx.lock().await;
                                match guard.recv().await {
                                    Some(part) => {
                                        let val = lua.to_value(&part)?;
                                        Ok((val, Value::Nil))
                                    }
                                    None => Ok((Value::Nil, Value::Nil)),
                                }
                            }
                        })?;
                        Ok((Value::Function(next_part), Value::Nil))
                    }
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        },
    )?;
    sessions.set("send_and_collect", collect_fn)?;

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
        register_sessions_module(&lua).expect("Should register sessions module");
        lua
    }

    #[test]
    fn sessions_module_registers_in_namespace() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let sessions: Table = cru.get("sessions").expect("cru.sessions should exist");

        assert!(sessions.contains_key("create").unwrap());
        assert!(sessions.contains_key("get").unwrap());
        assert!(sessions.contains_key("list").unwrap());
        assert!(sessions.contains_key("configure_agent").unwrap());
        assert!(sessions.contains_key("send_message").unwrap());
        assert!(sessions.contains_key("cancel").unwrap());
        assert!(sessions.contains_key("subscribe").unwrap());
        assert!(sessions.contains_key("unsubscribe").unwrap());
        assert!(sessions.contains_key("interaction_respond").unwrap());
        assert!(sessions.contains_key("pause").unwrap());
        assert!(sessions.contains_key("resume").unwrap());
        assert!(sessions.contains_key("end_session").unwrap());
        assert!(sessions.contains_key("send_and_collect").unwrap());

        // Also registered under crucible.*
        let crucible: Table = lua
            .globals()
            .get("crucible")
            .expect("crucible should exist");
        let sessions2: Table = crucible
            .get("sessions")
            .expect("crucible.sessions should exist");
        assert!(sessions2.contains_key("create").unwrap());
    }

    #[tokio::test]
    async fn sessions_stub_create_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(r#"return cru.sessions.create({ type = "chat", kiln = "/tmp/kiln" })"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        // Second return value should be the error string
        match result.1 {
            Value::String(s) => assert_eq!(s.to_str().unwrap(), "no daemon connected"),
            _ => panic!("Expected error string, got {:?}", result.1),
        }
    }

    #[tokio::test]
    async fn sessions_stub_list_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(r#"return cru.sessions.list()"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
    }

    #[tokio::test]
    async fn sessions_stub_get_returns_nil() {
        let lua = setup_lua();

        let result: (Value, Value) = lua
            .load(r#"return cru.sessions.get("some-id")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
    }
}

#[cfg(test)]
mod api_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock implementation of DaemonSessionApi for testing.
    struct MockDaemonApi {
        create_called: AtomicBool,
    }

    impl MockDaemonApi {
        fn new() -> Self {
            Self {
                create_called: AtomicBool::new(false),
            }
        }
    }

    impl DaemonSessionApi for MockDaemonApi {
        fn create_session(
            &self,
            session_type: String,
            kiln: Option<String>,
            workspace: Option<String>,
            _connected_kilns: Vec<String>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            self.create_called.store(true, Ordering::SeqCst);
            let kiln = kiln.unwrap_or_else(|| "/default/crucible".to_string());
            let ws = workspace.unwrap_or_else(|| kiln.clone());
            Box::pin(async move {
                Ok(serde_json::json!({
                    "id": format!("{}-2025-01-01T0000-abc123", session_type),
                    "session_type": session_type,
                    "state": "active",
                    "kiln": kiln,
                    "workspace": ws,
                }))
            })
        }

        fn get_session(
            &self,
            session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>
        {
            Box::pin(async move {
                if session_id == "exists-123" {
                    Ok(Some(serde_json::json!({
                        "id": "exists-123",
                        "session_type": "chat",
                        "state": "active",
                    })))
                } else {
                    Ok(None)
                }
            })
        }

        fn list_sessions(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            Box::pin(async {
                Ok(vec![
                    serde_json::json!({
                        "id": "chat-001",
                        "session_type": "chat",
                        "state": "active",
                    }),
                    serde_json::json!({
                        "id": "agent-002",
                        "session_type": "agent",
                        "state": "paused",
                    }),
                ])
            })
        }

        fn configure_agent(
            &self,
            _session_id: String,
            _agent_config: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn send_message(
            &self,
            _session_id: String,
            _content: String,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            Box::pin(async { Ok("msg-response-001".to_string()) })
        }

        fn cancel(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
            Box::pin(async { Ok(true) })
        }

        fn pause(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn resume(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn end_session(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn respond_to_permission(
            &self,
            _session_id: String,
            _request_id: String,
            _response: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn subscribe(
            &self,
            _session_id: String,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                            String,
                        >,
                    > + Send,
            >,
        > {
            Box::pin(async {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                // Send a couple of test events then drop the sender
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "session_id": "test-session",
                    "data": { "content": "Hello" }
                }));
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "session_id": "test-session",
                    "data": { "content": " World" }
                }));
                // tx is dropped here, so after reading 2 events, recv() returns None
                Ok(rx)
            })
        }

        fn unsubscribe(
            &self,
            _session_id: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn send_and_collect(
            &self,
            _session_id: String,
            _content: String,
            _timeout_secs: Option<f64>,
            _max_tool_result_len: Option<usize>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>,
                    > + Send,
            >,
        > {
            Box::pin(async {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let _ = tx.send(ResponsePart::Text {
                    content: "Hello World".to_string(),
                });
                drop(tx);
                Ok(rx)
            })
        }
    }

    fn setup_lua_with_api(api: Arc<dyn DaemonSessionApi>) -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        lua.globals().set("cru", cru).unwrap();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_sessions_module_with_api(&lua, api).expect("Should register sessions with API");
        lua
    }

    #[tokio::test]
    async fn sessions_with_mock_api_create_returns_id() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local session, err = cru.sessions.create({ type = "chat", kiln = "/tmp/kiln" })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return session
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let id: String = result.get("id").unwrap();
        assert!(
            id.starts_with("chat-"),
            "id should start with 'chat-': {}",
            id
        );
        assert_eq!(result.get::<String>("state").unwrap(), "active");
        assert_eq!(result.get::<String>("kiln").unwrap(), "/tmp/kiln");
    }

    #[tokio::test]
    async fn sessions_with_mock_api_create_no_kiln_uses_default() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local session, err = cru.sessions.create({ type = "chat" })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return session
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let id: String = result.get("id").unwrap();
        assert!(id.starts_with("chat-"));
        // kiln should be the mock default
        assert_eq!(result.get::<String>("kiln").unwrap(), "/default/crucible");
    }

    #[tokio::test]
    async fn sessions_with_mock_api_create_with_kilns() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local session, err = cru.sessions.create({
                    type = "chat",
                    kilns = { "/tmp/notes", "/tmp/docs" },
                })
                assert(err == nil, "unexpected error: " .. tostring(err))
                return session
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let id: String = result.get("id").unwrap();
        assert!(id.starts_with("chat-"));
        // No explicit kiln → uses mock default
        assert_eq!(result.get::<String>("kiln").unwrap(), "/default/crucible");
    }

    #[tokio::test]
    async fn sessions_create_with_invalid_arg_returns_error() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: (Value, Value) = lua
            .load(r#"return cru.sessions.create(42)"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        match result.1 {
            Value::String(s) => assert!(s.to_str().unwrap().contains("expects a table")),
            _ => panic!("Expected error string"),
        }
    }

    #[tokio::test]
    async fn sessions_with_mock_api_list_returns_array() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local sessions, err = cru.sessions.list()
                assert(err == nil, "unexpected error: " .. tostring(err))
                return sessions
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.len().unwrap(), 2);

        let first: Table = result.get(1).unwrap();
        assert_eq!(first.get::<String>("id").unwrap(), "chat-001");

        let second: Table = result.get(2).unwrap();
        assert_eq!(second.get::<String>("id").unwrap(), "agent-002");
        assert_eq!(second.get::<String>("state").unwrap(), "paused");
    }

    #[tokio::test]
    async fn sessions_with_mock_api_get_existing() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: Table = lua
            .load(
                r#"
                local session, err = cru.sessions.get("exists-123")
                assert(err == nil)
                return session
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("id").unwrap(), "exists-123");
    }

    #[tokio::test]
    async fn sessions_with_mock_api_get_missing_returns_nil() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: (Value, Value) = lua
            .load(r#"return cru.sessions.get("nonexistent")"#)
            .eval_async()
            .await
            .unwrap();

        assert!(matches!(result.0, Value::Nil));
        // No error — just not found
        assert!(matches!(result.1, Value::Nil));
    }

    #[tokio::test]
    async fn sessions_subscribe_returns_iterator() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        // Subscribe and read events
        let result: Table = lua
            .load(
                r#"
                local next_event, err = cru.sessions.subscribe("test-session")
                assert(err == nil, "subscribe error: " .. tostring(err))
                assert(type(next_event) == "function", "expected function iterator")

                local events = {}
                -- Read the two events the mock sends
                local e1 = next_event()
                if e1 then events[#events + 1] = e1 end
                local e2 = next_event()
                if e2 then events[#events + 1] = e2 end

                return {
                    count = #events,
                    first_type = events[1] and events[1].type or "none",
                    first_text = events[1] and events[1].data and events[1].data.content or "none",
                    second_text = events[2] and events[2].data and events[2].data.content or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<i64>("count").unwrap(), 2);
        assert_eq!(result.get::<String>("first_type").unwrap(), "text_delta");
        assert_eq!(result.get::<String>("first_text").unwrap(), "Hello");
        assert_eq!(result.get::<String>("second_text").unwrap(), " World");
    }

    #[tokio::test]
    async fn sessions_send_message_returns_response_id() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: String = lua
            .load(
                r#"
                local id, err = cru.sessions.send_message("session-1", "Hello agent")
                assert(err == nil)
                return id
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result, "msg-response-001");
    }

    #[tokio::test]
    async fn sessions_cancel_returns_bool() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: bool = lua
            .load(
                r#"
                local ok, err = cru.sessions.cancel("session-1")
                assert(err == nil)
                return ok
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(result);
    }

    #[tokio::test]
    async fn sessions_end_session_succeeds() {
        let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
        let lua = setup_lua_with_api(api);

        let result: bool = lua
            .load(
                r#"
                local ok, err = cru.sessions.end_session("session-1")
                assert(err == nil)
                return ok
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert!(result);
    }

    // -----------------------------------------------------------------------
    // Diagnostic tests for async subscribe/next_event channel delivery
    // -----------------------------------------------------------------------
    //
    // These tests reproduce the Discord plugin scenario where:
    //   1. Lua calls subscribe() -> gets next_event function
    //   2. Lua calls send_message() -> triggers agent processing
    //   3. A background task sends events into the mpsc channel
    //   4. Lua calls next_event() -> should receive events
    //
    // The existing MockDaemonApi sends events synchronously (before subscribe
    // returns), so the receiver already has buffered data. The real daemon
    // sends events asynchronously AFTER subscribe returns. These tests use
    // a mock that delays event delivery to surface timing/async issues.

    /// Mock that holds onto the mpsc sender so events can be sent asynchronously
    /// after subscribe() returns.
    struct AsyncMockDaemonApi {
        /// Shared sender — tests inject events after subscribe returns.
        event_tx: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>>,
        /// Notify when subscribe() has been called and the sender is available.
        subscribe_barrier: Arc<tokio::sync::Notify>,
    }

    impl AsyncMockDaemonApi {
        fn new() -> Self {
            Self {
                event_tx: std::sync::Mutex::new(None),
                subscribe_barrier: Arc::new(tokio::sync::Notify::new()),
            }
        }

        /// Get a clone of the event sender (waits until subscribe is called).
        fn get_sender(&self) -> Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>> {
            self.event_tx.lock().unwrap().clone()
        }
    }

    impl DaemonSessionApi for AsyncMockDaemonApi {
        fn create_session(
            &self,
            _: String,
            _: Option<String>,
            _: Option<String>,
            _: Vec<String>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            Box::pin(async { Ok(serde_json::json!({"id": "s1"})) })
        }
        fn get_session(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>
        {
            Box::pin(async { Ok(None) })
        }
        fn list_sessions(
            &self,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn configure_agent(
            &self,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }
        fn send_message(
            &self,
            _: String,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            Box::pin(async { Ok("msg-001".to_string()) })
        }
        fn cancel(&self, _: String) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>> {
            Box::pin(async { Ok(true) })
        }
        fn pause(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }
        fn resume(&self, _: String) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }
        fn end_session(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }
        fn respond_to_permission(
            &self,
            _: String,
            _: String,
            _: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn subscribe(
            &self,
            _session_id: String,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                            String,
                        >,
                    > + Send,
            >,
        > {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            // Store the sender so the test can inject events later
            *self.event_tx.lock().unwrap() = Some(tx);
            self.subscribe_barrier.notify_one();
            Box::pin(async { Ok(rx) })
        }

        fn unsubscribe(
            &self,
            _: String,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> {
            Box::pin(async { Ok(()) })
        }

        fn send_and_collect(
            &self,
            _session_id: String,
            _content: String,
            _timeout_secs: Option<f64>,
            _max_tool_result_len: Option<usize>,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>,
                    > + Send,
            >,
        > {
            Box::pin(async {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let _ = tx.send(ResponsePart::Text {
                    content: "mock response".to_string(),
                });
                drop(tx);
                Ok(rx)
            })
        }
    }

    /// Test 1: subscribe + next_event with events sent AFTER subscribe returns.
    ///
    /// This is the core scenario: Lua calls subscribe(), gets a next_event
    /// function, then calls next_event() which must await on the mpsc receiver.
    /// A Rust task sends events into the channel after a delay.
    ///
    /// If this test fails, the issue is in mlua's create_async_function + recv.
    #[tokio::test]
    async fn subscribe_next_event_receives_async_events() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);

        // Spawn a Rust task that waits for subscribe, then sends events
        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            // Wait until Lua calls subscribe()
            barrier.notified().await;
            // Small delay to ensure next_event() is already awaiting
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            if let Some(tx) = api_clone.get_sender() {
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "session_id": "test-session",
                    "data": { "text": "async-hello" }
                }));
                // Drop the sender so next_event eventually returns nil
                drop(tx);
            }
        });

        let result: Table = lua
            .load(
                r#"
                local next_event, err = cru.sessions.subscribe("test-session")
                assert(err == nil, "subscribe error: " .. tostring(err))
                assert(type(next_event) == "function", "expected function, got " .. type(next_event))

                -- This should block/yield until the Rust task sends the event
                local event, event_err = next_event()
                assert(event ~= nil, "expected event, got nil (event_err=" .. tostring(event_err) .. ")")

                return {
                    event_type = event.type,
                    text = event.data and event.data.text or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(
            result.get::<String>("event_type").unwrap(),
            "text_delta",
            "next_event() should have received the async event"
        );
        assert_eq!(
            result.get::<String>("text").unwrap(),
            "async-hello",
            "Event data should match what was sent"
        );
    }

    /// Test 2: Full Discord plugin flow — subscribe, send_message, then
    /// next_event in the same Lua execution context.
    ///
    /// This reproduces the exact sequence from responder.lua:
    ///   local next_event = cru.sessions.subscribe(session_id)
    ///   cru.sessions.send_message(session_id, content)
    ///   local event = next_event()
    #[tokio::test]
    async fn subscribe_send_message_then_next_event() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);

        // Spawn a Rust task that sends events after subscribe + send_message
        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            barrier.notified().await;
            // Simulate agent processing delay
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            if let Some(tx) = api_clone.get_sender() {
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "session_id": "test-session",
                    "data": { "text": "response chunk" }
                }));
                let _ = tx.send(serde_json::json!({
                    "type": "stream_end",
                    "session_id": "test-session",
                    "data": {}
                }));
                drop(tx);
            }
            // Clear the stored sender so ALL senders are dropped and recv() returns None
            *api_clone.event_tx.lock().unwrap() = None;
        });

        let result: Table = lua
            .load(
                r#"
                -- Step 1: Subscribe
                local next_event, sub_err = cru.sessions.subscribe("test-session")
                assert(sub_err == nil, "subscribe error: " .. tostring(sub_err))

                -- Step 2: Send message (triggers agent processing)
                local msg_id, msg_err = cru.sessions.send_message("test-session", "Hello!")
                assert(msg_err == nil, "send_message error: " .. tostring(msg_err))

                -- Step 3: Read events (should yield until events arrive)
                local events = {}
                while true do
                    local event = next_event()
                    if event == nil then break end
                    events[#events + 1] = event
                end

                return {
                    msg_id = msg_id,
                    event_count = #events,
                    first_type = events[1] and events[1].type or "none",
                    last_type = events[#events] and events[#events].type or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        assert_eq!(result.get::<String>("msg_id").unwrap(), "msg-001");
        assert_eq!(
            result.get::<i64>("event_count").unwrap(),
            2,
            "Should have received 2 events"
        );
        assert_eq!(result.get::<String>("first_type").unwrap(), "text_delta");
        assert_eq!(result.get::<String>("last_type").unwrap(), "stream_end");
    }

    /// Test 3: next_event called from within a timer.timeout wrapper.
    ///
    /// Tests that create_async_function works when nested inside another
    /// async Lua call (timeout wraps the function in tokio::time::timeout).
    #[tokio::test]
    async fn subscribe_next_event_inside_timeout() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);
        crate::timer::register_timer_module(&lua).unwrap();

        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            barrier.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            if let Some(tx) = api_clone.get_sender() {
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "data": { "text": "timed" }
                }));
                drop(tx);
            }
        });

        let result: Table = lua
            .load(
                r#"
                local next_event, err = cru.sessions.subscribe("test-session")
                assert(err == nil, "subscribe error: " .. tostring(err))

                -- Wrap next_event in a timeout to avoid hanging forever if broken
                local ok, event = cru.timer.timeout(5.0, function()
                    return next_event()
                end)

                return {
                    timed_out = not ok,
                    text = ok and event and event.data and event.data.text or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let timed_out: bool = result.get("timed_out").unwrap();
        assert!(
            !timed_out,
            "next_event() should not have timed out — event was sent"
        );
        assert_eq!(result.get::<String>("text").unwrap(), "timed");
    }

    /// Test 4: Verify that events are NOT lost due to the channel being
    /// dropped prematurely.
    ///
    /// This specifically tests that the UnboundedReceiver returned by
    /// subscribe() stays alive as long as the Lua next_event closure exists.
    #[tokio::test]
    async fn subscribe_receiver_not_dropped_prematurely() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);
        crate::timer::register_timer_module(&lua).unwrap();

        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            barrier.notified().await;
            // Wait longer — the Lua side will sleep before calling next_event
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            if let Some(tx) = api_clone.get_sender() {
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "data": { "text": "delayed-event" }
                }));
                drop(tx);
            }
        });

        let result: Table = lua
            .load(
                r#"
                local next_event, err = cru.sessions.subscribe("test-session")
                assert(err == nil, "subscribe error: " .. tostring(err))

                -- Simulate some work between subscribe and reading events
                -- (like send_message + other setup in the Discord plugin)
                cru.timer.sleep(0.1)

                -- Now read the event
                local ok, event = cru.timer.timeout(5.0, function()
                    return next_event()
                end)

                return {
                    timed_out = not ok,
                    text = ok and event and event.data and event.data.text or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let timed_out: bool = result.get("timed_out").unwrap();
        assert!(
            !timed_out,
            "next_event() timed out — receiver may have been dropped"
        );
        assert_eq!(result.get::<String>("text").unwrap(), "delayed-event");
    }

    /// Test 5: The cru.spawn pattern — subscribe + next_event from a spawned
    /// Lua task, mimicking the Discord plugin's responder flow.
    ///
    /// This is the exact pattern that was failing in production:
    ///   cru.spawn(function()
    ///     local next_event = cru.sessions.subscribe(session_id)
    ///     cru.sessions.send_message(session_id, content)
    ///     local event = next_event()  -- THIS was hanging
    ///   end)
    ///
    /// NOTE: cru.spawn uses tokio::spawn which requires the mlua `send`
    /// feature. Without it, this test will not compile.
    #[cfg(feature = "send")]
    #[tokio::test]
    async fn subscribe_next_event_via_cru_spawn() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);
        crate::timer::register_timer_module(&lua).unwrap();

        // Spawn a Rust task that sends events after subscribe is called
        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            barrier.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            if let Some(tx) = api_clone.get_sender() {
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "data": { "text": "spawned-event" }
                }));
                drop(tx);
            }
            *api_clone.event_tx.lock().unwrap() = None;
        });

        // Use a shared table to capture results from the spawned task.
        // cru.spawn is fire-and-forget, so we use a global to communicate.
        let result = lua
            .load(
                r#"
                -- Shared result table
                _G.spawn_result = { done = false, text = "not-set" }

                cru.spawn(function()
                    local next_event, err = cru.sessions.subscribe("test-session")
                    if err then
                        _G.spawn_result.text = "subscribe error: " .. err
                        _G.spawn_result.done = true
                        return
                    end

                    local event = next_event()
                    if event then
                        _G.spawn_result.text = event.data and event.data.text or "no-text"
                    else
                        _G.spawn_result.text = "nil-event"
                    end
                    _G.spawn_result.done = true
                end)

                -- Wait for the spawned task to complete (poll with sleep)
                local waited = 0
                while not _G.spawn_result.done and waited < 5 do
                    cru.timer.sleep(0.05)
                    waited = waited + 0.05
                end

                return _G.spawn_result
                "#,
            )
            .eval_async::<Table>()
            .await;

        match result {
            Ok(table) => {
                let done: bool = table.get("done").unwrap();
                let text: String = table.get("text").unwrap();
                assert!(done, "Spawned task should have completed");
                assert_eq!(
                    text, "spawned-event",
                    "Event should have been received in the spawned task"
                );
            }
            Err(e) => {
                panic!(
                    "Lua execution failed: {}. This likely means cru.spawn \
                     cannot call async functions — check if mlua 'send' feature is enabled",
                    e
                );
            }
        }
    }

    /// Test 6: Multiple sequential next_event calls receive events in order.
    ///
    /// Verifies that the Arc<Mutex<Receiver>> pattern works correctly across
    /// multiple invocations of the same next_event closure.
    #[tokio::test]
    async fn subscribe_multiple_next_event_calls_receive_in_order() {
        let api = Arc::new(AsyncMockDaemonApi::new());
        let barrier = Arc::clone(&api.subscribe_barrier);

        let lua = setup_lua_with_api(Arc::clone(&api) as Arc<dyn DaemonSessionApi>);
        crate::timer::register_timer_module(&lua).unwrap();

        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            barrier.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;

            if let Some(tx) = api_clone.get_sender() {
                for i in 1..=5 {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    let _ = tx.send(serde_json::json!({
                        "type": "text_delta",
                        "data": { "text": format!("chunk-{}", i) }
                    }));
                }
                drop(tx);
            }
            // Clear stored sender so recv() returns None
            *api_clone.event_tx.lock().unwrap() = None;
        });

        let result: Table = lua
            .load(
                r#"
                local next_event, err = cru.sessions.subscribe("test-session")
                assert(err == nil, "subscribe error: " .. tostring(err))

                local texts = {}
                local ok, res = cru.timer.timeout(5.0, function()
                    while true do
                        local event = next_event()
                        if event == nil then break end
                        texts[#texts + 1] = event.data.text
                    end
                end)

                return {
                    timed_out = not ok,
                    count = #texts,
                    first = texts[1] or "none",
                    last = texts[#texts] or "none",
                }
                "#,
            )
            .eval_async()
            .await
            .unwrap();

        let timed_out: bool = result.get("timed_out").unwrap();
        assert!(!timed_out, "Should not time out");
        assert_eq!(
            result.get::<i64>("count").unwrap(),
            5,
            "Should receive all 5 events"
        );
        assert_eq!(result.get::<String>("first").unwrap(), "chunk-1");
        assert_eq!(result.get::<String>("last").unwrap(), "chunk-5");
    }
}
