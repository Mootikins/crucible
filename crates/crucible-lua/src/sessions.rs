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
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

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
    fn create_session(
        &self,
        session_type: String,
        kiln: String,
        workspace: Option<String>,
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

    stub_async!("create", lua, sessions, (String, String, Option<String>));
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

    // create(session_type, kiln, workspace?)
    let a = Arc::clone(&api);
    let create_fn = lua.create_async_function(
        move |lua, (session_type, kiln, workspace): (String, String, Option<String>)| {
            let a = Arc::clone(&a);
            async move {
                match a.create_session(session_type, kiln, workspace).await {
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
        },
    )?;
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
                    let next_fn = lua.create_async_function(move |lua, (): ()| {
                        let rx = Arc::clone(&rx);
                        async move {
                            let mut guard = rx.lock().await;
                            match guard.recv().await {
                                Some(event) => {
                                    let lua_val = lua.to_value(&event)?;
                                    Ok((lua_val, Value::Nil))
                                }
                                None => Ok((Value::Nil, Value::Nil)),
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
            .load(r#"return cru.sessions.create("chat", "/tmp/kiln")"#)
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
            kiln: String,
            workspace: Option<String>,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>> {
            self.create_called.store(true, Ordering::SeqCst);
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
                    "data": { "text": "Hello" }
                }));
                let _ = tx.send(serde_json::json!({
                    "type": "text_delta",
                    "session_id": "test-session",
                    "data": { "text": " World" }
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
                local session, err = cru.sessions.create("chat", "/tmp/kiln")
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
                    first_text = events[1] and events[1].data and events[1].data.text or "none",
                    second_text = events[2] and events[2].data and events[2].data.text or "none",
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
}
