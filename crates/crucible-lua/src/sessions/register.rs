use super::DaemonSessionApi;
use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use std::sync::Arc;

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
    stub_async!("inject", lua, sessions, (String, String, String));
    stub_async!(
        "collect_subagents",
        lua,
        sessions,
        (mlua::Value, mlua::Value)
    );
    stub_async!("messages", lua, sessions, (String, mlua::Value));
    stub_async!("fork", lua, sessions, (String, mlua::Value));
    stub_async!("cache_stats", lua, sessions, String);
    stub_async!(
        "set_output_validation",
        lua,
        sessions,
        (String, mlua::Value)
    );
    stub_async!("undo", lua, sessions, (String, mlua::Value));
    stub_async!("can_undo", lua, sessions, String);
    stub_async!("undo_depth", lua, sessions, String);
    stub_async!("undo_history", lua, sessions, String);

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

    // messages(session_id, opts?) -> (messages_table, nil) or (nil, err)
    // opts: { role = "user"|"assistant"|"system", limit = N }
    let a = Arc::clone(&api);
    let messages_fn =
        lua.create_async_function(move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let (role_filter, limit) = match opts {
                    Value::Table(ref t) => {
                        (t.get::<String>("role").ok(), t.get::<usize>("limit").ok())
                    }
                    _ => (None, None),
                };
                match a.load_messages(session_id, role_filter, limit).await {
                    Ok(messages) => {
                        let table = lua.create_table()?;
                        for (i, msg) in messages.iter().enumerate() {
                            let lua_val = lua.to_value(msg)?;
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
    sessions.set("messages", messages_fn)?;

    // inject(session_id, role, content) -> (true, nil) or (nil, err)
    let a = Arc::clone(&api);
    let inject_fn = lua.create_async_function(
        move |lua, (session_id, role, content): (String, String, String)| {
            let a = Arc::clone(&a);
            async move {
                match a.inject_context(session_id, role, content).await {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        },
    )?;
    sessions.set("inject", inject_fn)?;

    // collect_subagents(job_ids, timeout_secs?) -> (results_table, nil) or (nil, err)
    let a = Arc::clone(&api);
    let collect_fn =
        lua.create_async_function(move |lua, (job_ids, timeout): (Vec<String>, Value)| {
            let a = Arc::clone(&a);
            async move {
                let timeout_secs = match timeout {
                    Value::Number(n) => Some(n),
                    Value::Integer(n) => Some(n as f64),
                    _ => None,
                };
                match a.collect_subagents(job_ids, timeout_secs).await {
                    Ok(results) => {
                        let table = lua.create_table()?;
                        for (i, val) in results.iter().enumerate() {
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
    sessions.set("collect_subagents", collect_fn)?;

    // fork(session_id, opts?) -> ({ id, parent_id, messages_copied }, nil) or (nil, err)
    // opts can be a table { up_to = N } or an integer N
    let a = Arc::clone(&api);
    let fork_fn = lua.create_async_function(move |lua, (session_id, opts): (String, Value)| {
        let a = Arc::clone(&a);
        async move {
            let up_to = match opts {
                Value::Table(ref t) => t.get::<u64>("up_to").ok(),
                Value::Integer(n) => Some(n as u64),
                _ => None,
            };
            match a.fork_session(session_id, up_to).await {
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
    sessions.set("fork", fork_fn)?;

    // cache_stats(session_id) -> (table, nil) or (nil, err)
    let a = Arc::clone(&api);
    let cache_stats_fn = lua.create_async_function(move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move {
            match a.cache_stats(session_id).await {
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
    sessions.set("cache_stats", cache_stats_fn)?;

    // set_output_validation(session_id, spec) -> (true, nil) or (nil, err)
    //
    // `spec` accepts either:
    //   - a string: "none" | "json" | "regex:<pattern>" | "lua:<name>"
    //   - a table:  { type = "none" | "json" }
    //               { type = "regex", pattern = "..." }
    //               { type = "lua", name = "..." }
    //
    // The Lua API normalises both forms to the canonical string before
    // crossing the trait boundary. The daemon then runs it through
    // `OutputValidation::from_str` for full validation (e.g. regex
    // compile errors surface here, not at agent-turn time).
    let a = Arc::clone(&api);
    let set_validation_fn =
        lua.create_async_function(move |lua, (sid, spec): (String, Value)| {
            let a = Arc::clone(&a);
            async move {
                let serialized = match spec {
                    Value::String(s) => s.to_str()?.to_string(),
                    Value::Table(t) => {
                        let ty: String = t.get("type")?;
                        match ty.as_str() {
                            "none" => "none".to_string(),
                            "json" => "json".to_string(),
                            "regex" => {
                                let pattern: String = t.get("pattern")?;
                                format!("regex:{pattern}")
                            }
                            "lua" => {
                                let name: String = t.get("name")?;
                                format!("lua:{name}")
                            }
                            other => {
                                return Err(mlua::Error::runtime(format!(
                                    "unknown validation type '{other}'; want none|json|regex|lua"
                                )));
                            }
                        }
                    }
                    other => {
                        return Err(mlua::Error::runtime(format!(
                            "validation spec must be string or table, got {}",
                            other.type_name()
                        )));
                    }
                };
                match a.set_output_validation(sid, serialized).await {
                    Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
                    Err(e) => {
                        let err = lua.create_string(&e)?;
                        Ok((Value::Nil, Value::String(err)))
                    }
                }
            }
        })?;
    sessions.set("set_output_validation", set_validation_fn)?;

    // undo(session_id, count?) -> (turns_undone, nil) | (nil, err)
    // `count` defaults to 1; non-positive values clamp to 1. The trait
    // boundary takes a `usize`; the binding accepts integers and tables
    // for forward-compatibility (`{ count = N }`).
    let a = Arc::clone(&api);
    let undo_fn = lua.create_async_function(move |lua, (sid, opts): (String, Value)| {
        let a = Arc::clone(&a);
        async move {
            let count = match opts {
                Value::Nil => 1,
                Value::Integer(n) => n.max(1) as usize,
                Value::Number(n) => (n.max(1.0) as i64).max(1) as usize,
                Value::Table(ref t) => t.get::<usize>("count").unwrap_or(1).max(1),
                _ => 1,
            };
            match a.undo(sid, count).await {
                Ok(turns) => Ok((Value::Integer(turns as i64), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("undo", undo_fn)?;

    // can_undo(session_id) -> (bool, nil) | (nil, err)
    let a = Arc::clone(&api);
    let can_undo_fn = lua.create_async_function(move |lua, sid: String| {
        let a = Arc::clone(&a);
        async move {
            match a.can_undo(sid).await {
                Ok(v) => Ok((Value::Boolean(v), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("can_undo", can_undo_fn)?;

    // undo_depth(session_id) -> (int, nil) | (nil, err)
    let a = Arc::clone(&api);
    let undo_depth_fn = lua.create_async_function(move |lua, sid: String| {
        let a = Arc::clone(&a);
        async move {
            match a.undo_depth(sid).await {
                Ok(v) => Ok((Value::Integer(v as i64), Value::Nil)),
                Err(e) => {
                    let err = lua.create_string(&e)?;
                    Ok((Value::Nil, Value::String(err)))
                }
            }
        }
    })?;
    sessions.set("undo_depth", undo_depth_fn)?;

    // undo_history(session_id) -> (list_of_summary, nil) | (nil, err)
    // Each summary is a table; oldest-to-newest order.
    let a = Arc::clone(&api);
    let undo_history_fn = lua.create_async_function(move |lua, sid: String| {
        let a = Arc::clone(&a);
        async move {
            match a.undo_history(sid).await {
                Ok(entries) => {
                    let table = lua.create_table()?;
                    for (i, entry) in entries.iter().enumerate() {
                        let lua_val = lua.to_value(entry)?;
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
    sessions.set("undo_history", undo_history_fn)?;

    Ok(())
}
