use super::DaemonSessionApi;
use crate::error::LuaError;
use crate::lua_util::{gate_module_keys, get_or_create_module, install_sessions_alias};
use crate::session_api::{CurrentSession, Session};
use mlua::{Lua, LuaSerdeExt, Value};
use std::sync::Arc;

/// Every function in `cru.session`, with its Luau type, in registration
/// order.
///
/// The stub path and the daemon-backed path both read this list, so a new
/// session function cannot land in one path only: the stub loop registers
/// every name here, and [`register_sessions_module_with_api`] refuses a table
/// whose key set differs from it.
///
/// The TYPE lives here rather than beside one closure because every name has
/// TWO closures — a stub and a daemon-backed body — and one declaration for
/// both is the point. [`crate::host_registry::Ns`] holds each string to the
/// daemon-backed closure's Rust types; the stub's own type
/// (`MultiValue -> (Value, Value)`) describes no single function, so the stub
/// path declares without checking. See [`register_sessions_module`].
///
/// Every function answers with the `(value, err)` pair the Lua error
/// convention uses, so the first return is nil on failure and the second is
/// `string?` throughout. A session HANDLE is a userdata
/// ([`crate::session_api::Session`]), which the declarations have no name
/// for, so the functions that answer with one say `any`.
///
/// `current` is deliberately absent: it is registered by
/// [`crate::session_api::register_session_module`], which owns the
/// `CurrentSession` instance the daemon binds to. The sessions registrations
/// merge their functions into the same table rather than replacing it, so
/// registration order cannot drop it.
pub(crate) const SESSION_FNS: &[(&str, &str)] = &[
    // The options table crosses to the daemon as ONE object (`create_params`),
    // so its fields are not narrowed: a plugin reaches every create-time field
    // the daemon's request type has, and narrowing here would reject them.
    // `delegate` is the one key this crate reads itself. The string arm is the
    // legacy positional form, `create("chat")`.
    (
        "create",
        "(options: { [string]: any } | string) -> (any, string?)",
    ),
    // `(nil, nil)` when no session has that id: absent is not an error.
    ("get", "(session_id: string) -> (any, string?)"),
    ("list", "() -> ({ any }?, string?)"),
    (
        "configure_agent",
        "(session_id: string, config: { [string]: any }) -> (boolean?, string?)",
    ),
    // The string is the new response's id, not the reply text.
    (
        "send_message",
        "(session_id: string, content: string) -> (string?, string?)",
    ),
    // The boolean says whether anything WAS cancelled, so `false` is a real
    // answer and not a failure.
    ("cancel", "(session_id: string) -> (boolean?, string?)"),
    ("pause", "(session_id: string) -> (boolean?, string?)"),
    ("resume", "(session_id: string) -> (boolean?, string?)"),
    ("end_session", "(session_id: string) -> (boolean?, string?)"),
    // The mode the session runs its TURNS in, persisted on its agent — not a
    // property of one send. An id the session does not offer is an error
    // naming the ids it does.
    (
        "set_mode",
        "(session_id: string, mode_id: string) -> (boolean?, string?)",
    ),
    (
        "set_title",
        "(session_id: string, title: string) -> (boolean?, string?)",
    ),
    (
        "interaction_respond",
        "(session_id: string, request_id: string, response: any) -> (boolean?, string?)",
    ),
    // The first return is the ITERATOR, not an event: call it for each event,
    // and it answers nil once the stream ends. It answers TWO values — the
    // second is always nil today — and the declaration has to say so: Luau
    // counts returned values, not useful ones, so `() -> any?` rejects
    // `local event, err = next_event()`, which is the shape every other
    // `cru.session` function trains an author to write.
    (
        "subscribe",
        "(session_id: string) -> ((() -> (any?, string?))?, string?)",
    ),
    ("unsubscribe", "(session_id: string) -> (boolean?, string?)"),
    // `timeout` is in SECONDS. A bare number is that same timeout — the
    // number arm of the options argument. The first return is an iterator
    // over the response parts, written the way `subscribe`'s is and for the
    // same reason.
    (
        "send_and_collect",
        "(session_id: string, content: string, options: ({ timeout: number?, \
         max_tool_result_len: number?, interactive: boolean? } | number)?) \
         -> ((() -> (any?, string?))?, string?)",
    ),
    (
        "messages",
        "(session_id: string, options: { role: string?, limit: number?, tools: boolean? }?) \
         -> ({ any }?, string?)",
    ),
    (
        "inject",
        "(session_id: string, role: string, content: string) -> (boolean?, string?)",
    ),
    // Job ids, not session ids, and the timeout is in SECONDS.
    (
        "collect_subagents",
        "(job_ids: { string }, timeout_seconds: number?) -> ({ any }?, string?)",
    ),
    // A fork is a new session, so it answers with a handle like `create`. A
    // bare number is `up_to`.
    (
        "fork",
        "(session_id: string, options: ({ up_to: number? } | number)?) -> (any, string?)",
    ),
    ("cache_stats", "(session_id: string) -> (any, string?)"),
    // Like `create`, the options table crosses whole; the string arm is the
    // prompt on its own.
    (
        "complete",
        "(session_id: string, options: { [string]: any } | string) -> (string?, string?)",
    ),
    // The number is the count of turns UNDONE. `count` defaults to 1 and
    // clamps up to 1; a bare number is that same count.
    (
        "undo",
        "(session_id: string, count: ({ count: number? } | number)?) -> (number?, string?)",
    ),
    ("can_undo", "(session_id: string) -> (boolean?, string?)"),
    ("undo_depth", "(session_id: string) -> (number?, string?)"),
    // Oldest to newest.
    (
        "undo_history",
        "(session_id: string) -> ({ any }?, string?)",
    ),
    (
        "review_list_hunks",
        "(session_id: string) -> ({ any }?, string?)",
    ),
    (
        "review_set_state",
        "(session_id: string, hunk_id: string, state: string) -> (boolean?, string?)",
    ),
    // The spec deserializes into the daemon's `ReviewCommentRequest`, minus
    // the `session_id` the bridge stamps, so these are its fields exactly.
    (
        "review_comment",
        "(session_id: string, spec: { path: string, body: string, line_start: number, \
         line_end: number?, root: string?, author: string? }) -> (any, string?)",
    ),
    (
        "review_resolve_comment",
        "(session_id: string, comment_id: string) -> (boolean?, string?)",
    ),
];

/// The names in [`SESSION_FNS`], in registration order.
fn session_fn_names() -> Vec<&'static str> {
    SESSION_FNS.iter().map(|(name, _)| *name).collect()
}

/// The declared type of one `cru.session` function.
///
/// A miss is a mistake in this file — a closure registered under a name
/// [`SESSION_FNS`] does not list — and the key-set gate would refuse the
/// table for it anyway, one step later.
fn decl(name: &str) -> Result<&'static str, LuaError> {
    SESSION_FNS
        .iter()
        .find(|(fn_name, _)| *fn_name == name)
        .map(|(_, decl)| *decl)
        .ok_or_else(|| {
            LuaError::Runtime(format!("cru.session.{name} is not listed in SESSION_FNS"))
        })
}

// ── Shared operation bodies ─────────────────────────────────────────────
//
// Each `_op` is the single implementation of one verb. The free function
// `cru.session.<verb>(session_id, …)` and the handle method `s:<verb>(…)`
// both call it, so the two surfaces cannot drift apart. Every `_op` returns
// the `(value, nil) | (nil, err)` pair the Lua error convention uses.

/// `(nil, err)` from a `String`, the shape every error arm builds.
fn err_pair(lua: &Lua, e: String) -> mlua::Result<(Value, Value)> {
    Ok((Value::Nil, Value::String(lua.create_string(&e)?)))
}

/// Wrap a daemon session record into a [`Session`] handle.
///
/// The record rides along: fixed identity (`id`, `workspace`, `isolation`)
/// comes from the handle's own fields, live knobs from its bound rpc when it
/// has one, and everything else (`session_type`, `state`, `kilns`, …) from
/// the record, so a field read that worked on the old plain table keeps
/// working on the handle. A response with no `id` is handed back untouched —
/// inventing an identity for it would be worse than a plain record.
pub(crate) fn wrap_session_record(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    val: serde_json::Value,
) -> mlua::Result<Value> {
    match val.get("id").and_then(|v| v.as_str()).map(str::to_string) {
        Some(id) => {
            let session = Session::new(id).with_record(val).with_api(Arc::clone(api));
            Ok(Value::UserData(lua.create_userdata(session)?))
        }
        None => lua.to_value(&val),
    }
}

pub(crate) async fn create_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    args: Value,
    current: Option<&CurrentSession>,
) -> mlua::Result<(Value, Value)> {
    let mut params = match create_params(&args) {
        Ok(params) => params,
        Err(message) => return err_pair(lua, message),
    };
    // Parentage is stamped here or not at all. A caller-supplied
    // `parent_session_id` is removed unconditionally — including on a plain
    // create — so borrowing another session's delegation allowlist is not
    // sayable from Lua, and `delegate = true` is the one switch that turns
    // the create path into the delegation path.
    if let Some(obj) = params.as_object_mut() {
        obj.remove("parent_session_id");
        if let Some(delegate) = obj.remove("delegate") {
            match delegate {
                serde_json::Value::Bool(true) => {
                    let parent = current
                        .and_then(CurrentSession::get_current)
                        .map(|session| session.id());
                    let Some(parent) = parent else {
                        return err_pair(
                            lua,
                            "delegate = true requires a current session: this Lua VM has no \
                             session bound (delegate from a session's own Lua, or spawn a plain \
                             session instead)"
                                .to_string(),
                        );
                    };
                    obj.insert(
                        "parent_session_id".to_string(),
                        serde_json::Value::String(parent),
                    );
                }
                serde_json::Value::Bool(false) => {}
                other => {
                    return err_pair(
                        lua,
                        format!(
                            "delegate must be a boolean, got {}",
                            serde_json::Value::as_str(&other)
                                .map_or_else(|| other.to_string(), str::to_string)
                        ),
                    );
                }
            }
        }
    }
    match api.create_session(params).await {
        Ok(val) => Ok((wrap_session_record(lua, api, val)?, Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn get_session_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.get_session(sid.to_string()).await {
        Ok(Some(val)) => Ok((wrap_session_record(lua, api, val)?, Value::Nil)),
        Ok(None) => Ok((Value::Nil, Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn list_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
) -> mlua::Result<(Value, Value)> {
    match api.list_sessions().await {
        Ok(vals) => {
            let table = lua.create_table()?;
            for (i, val) in vals.iter().enumerate() {
                table.set(i + 1, wrap_session_record(lua, api, val.clone())?)?;
            }
            Ok((Value::Table(table), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn configure_agent_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    config: Value,
) -> mlua::Result<(Value, Value)> {
    let json_config: serde_json::Value =
        serde_json::to_value(&config).map_err(mlua::Error::external)?;
    match api.configure_agent(sid.to_string(), json_config).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn send_message_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    content: String,
) -> mlua::Result<(Value, Value)> {
    match api.send_message(sid.to_string(), content).await {
        Ok(response_id) => {
            let s = lua.create_string(&response_id)?;
            Ok((Value::String(s), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn cancel_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.cancel(sid.to_string()).await {
        Ok(cancelled) => Ok((Value::Boolean(cancelled), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn pause_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.pause(sid.to_string()).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn resume_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.resume(sid.to_string()).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn end_session_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.end_session(sid.to_string()).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// set_mode(session_id, mode_id) -> (true, nil) | (nil, err)
pub(crate) async fn set_mode_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    mode_id: String,
) -> mlua::Result<(Value, Value)> {
    match api.set_mode(sid.to_string(), mode_id).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// set_title(session_id, title) -> (true, nil) | (nil, err)
pub(crate) async fn set_title_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    title: String,
) -> mlua::Result<(Value, Value)> {
    match api.set_title(sid.to_string(), title).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn interaction_respond_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    request_id: String,
    response: Value,
) -> mlua::Result<(Value, Value)> {
    let json_response: serde_json::Value =
        serde_json::to_value(&response).map_err(mlua::Error::external)?;
    match api
        .respond_to_permission(sid.to_string(), request_id, json_response)
        .await
    {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// subscribe(session_id) -> returns (next_event_fn, nil) or (nil, err)
/// next_event_fn() -> returns (event_table, nil) or (nil, nil) if stream ended
pub(crate) async fn subscribe_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.subscribe(sid.to_string()).await {
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
                            tracing::debug!(call = n, event_type, "next_event: received event");
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
        Err(e) => err_pair(lua, e),
    }
}

pub(crate) async fn unsubscribe_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.unsubscribe(sid.to_string()).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// send_and_collect(session_id, content, opts?) -> (next_part, nil) or (nil, err)
/// next_part() yields { type = "text"|"tool_call"|"tool_result"|"thinking", ... } or nil
pub(crate) async fn send_and_collect_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    content: String,
    opts: Value,
) -> mlua::Result<(Value, Value)> {
    let (timeout_secs, max_tool_result_len, interactive) = match opts {
        Value::Table(ref t) => (
            t.get::<f64>("timeout").ok(),
            t.get::<usize>("max_tool_result_len").ok(),
            // Absent means false: a caller that has not thought
            // about who may answer must not get a prompt.
            t.get::<bool>("interactive").unwrap_or(false),
        ),
        Value::Number(n) => (Some(n), None, false),
        _ => (None, None, false),
    };
    match api
        .send_and_collect(
            sid.to_string(),
            content,
            timeout_secs,
            max_tool_result_len,
            interactive,
        )
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
        Err(e) => err_pair(lua, e),
    }
}

/// messages(session_id, opts?) -> (messages_table, nil) or (nil, err)
/// opts: { role = "user"|"assistant"|"system", limit = N, tools = true }
/// `tools` adds the `tool_call` and `tool_result` rows; it is off by default.
pub(crate) async fn messages_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    opts: Value,
) -> mlua::Result<(Value, Value)> {
    let (role_filter, limit, include_tools) = match opts {
        Value::Table(ref t) => (
            t.get::<String>("role").ok(),
            t.get::<usize>("limit").ok(),
            t.get::<bool>("tools").unwrap_or(false),
        ),
        _ => (None, None, false),
    };
    match api
        .load_messages(sid.to_string(), role_filter, limit, include_tools)
        .await
    {
        Ok(messages) => {
            let table = lua.create_table()?;
            for (i, msg) in messages.iter().enumerate() {
                let lua_val = lua.to_value(msg)?;
                table.set(i + 1, lua_val)?;
            }
            Ok((Value::Table(table), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

/// inject(session_id, role, content) -> (true, nil) or (nil, err)
pub(crate) async fn inject_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    role: String,
    content: String,
) -> mlua::Result<(Value, Value)> {
    match api.inject_context(sid.to_string(), role, content).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// collect_subagents(job_ids, timeout_secs?) -> (results_table, nil) or (nil, err)
///
/// Not a handle method: it collects *jobs*, which are not the session.
pub(crate) async fn collect_subagents_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    job_ids: Vec<String>,
    timeout: Value,
) -> mlua::Result<(Value, Value)> {
    let timeout_secs = match timeout {
        Value::Number(n) => Some(n),
        Value::Integer(n) => Some(n as f64),
        _ => None,
    };
    match api.collect_subagents(job_ids, timeout_secs).await {
        Ok(results) => {
            let table = lua.create_table()?;
            for (i, val) in results.iter().enumerate() {
                let lua_val = lua.to_value(val)?;
                table.set(i + 1, lua_val)?;
            }
            Ok((Value::Table(table), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

/// fork(session_id, opts?) -> ({ id, parent_id, messages_copied }, nil) or (nil, err)
/// opts can be a table { up_to = N } or an integer N
///
/// A fork is a new session, so it comes back as a handle like `create`.
pub(crate) async fn fork_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    opts: Value,
) -> mlua::Result<(Value, Value)> {
    let up_to = match opts {
        Value::Table(ref t) => t.get::<u64>("up_to").ok(),
        Value::Integer(n) => Some(n as u64),
        _ => None,
    };
    match api.fork_session(sid.to_string(), up_to).await {
        Ok(val) => Ok((wrap_session_record(lua, api, val)?, Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// cache_stats(session_id) -> (table, nil) or (nil, err)
pub(crate) async fn cache_stats_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.cache_stats(sid.to_string()).await {
        Ok(val) => {
            let lua_val = lua.to_value(&val)?;
            Ok((lua_val, Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

/// complete(session_id, { prompt = ..., system = ..., timeout = ... })
///   -> (string, nil) or (nil, err)
///
/// One exchange against the session's own model. The options table crosses
/// as one object rather than plucked keys, for the reason `create`'s does:
/// the daemon owns what an option means, and two sides naming fields
/// separately is how they drift.
pub(crate) async fn complete_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    opts: Value,
) -> mlua::Result<(Value, Value)> {
    let params = match &opts {
        Value::Table(t) => serde_json::to_value(t)
            .map_err(|e| format!("complete() options are not serializable: {e}")),
        // A bare string is the prompt — the common case, and the one
        // where naming the key adds nothing.
        Value::String(s) => Ok(serde_json::json!({ "prompt": s.to_str()?.to_string() })),
        _ => Err(
            "complete() expects a prompt string or a table, e.g. { prompt = \"…\" }".to_string(),
        ),
    };
    let params = match params {
        Ok(params) => params,
        Err(e) => return err_pair(lua, e),
    };
    match api.complete(sid.to_string(), params).await {
        Ok(text) => Ok((Value::String(lua.create_string(&text)?), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// undo(session_id, count?) -> (turns_undone, nil) | (nil, err)
/// `count` defaults to 1; non-positive values clamp to 1. The trait
/// boundary takes a `usize`; the binding accepts integers and tables
/// for forward-compatibility (`{ count = N }`).
pub(crate) async fn undo_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    opts: Value,
) -> mlua::Result<(Value, Value)> {
    let count = match opts {
        Value::Nil => 1,
        Value::Integer(n) => n.max(1) as usize,
        Value::Number(n) => (n.max(1.0) as i64).max(1) as usize,
        Value::Table(ref t) => t.get::<usize>("count").unwrap_or(1).max(1),
        _ => 1,
    };
    match api.undo(sid.to_string(), count).await {
        Ok(turns) => Ok((Value::Integer(turns as i64), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// can_undo(session_id) -> (bool, nil) | (nil, err)
pub(crate) async fn can_undo_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.can_undo(sid.to_string()).await {
        Ok(v) => Ok((Value::Boolean(v), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// undo_depth(session_id) -> (int, nil) | (nil, err)
pub(crate) async fn undo_depth_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.undo_depth(sid.to_string()).await {
        Ok(v) => Ok((Value::Integer(v as i64), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// undo_history(session_id) -> (list_of_summary, nil) | (nil, err)
/// Each summary is a table; oldest-to-newest order.
pub(crate) async fn undo_history_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.undo_history(sid.to_string()).await {
        Ok(entries) => {
            let table = lua.create_table()?;
            for (i, entry) in entries.iter().enumerate() {
                let lua_val = lua.to_value(entry)?;
                table.set(i + 1, lua_val)?;
            }
            Ok((Value::Table(table), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

// ── Attributed-diff review ─────────────────────────────────────────────
//
// `session_id` is a parameter rather than implicit context on purpose: the
// caller a plugin tool most often has is a *delegating* agent reviewing
// the child session it spawned, and an implicit "current session" would
// make that the one thing the API cannot express.

/// review_list_hunks(session_id) -> (hunks, nil) | (nil, err)
pub(crate) async fn review_list_hunks_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
) -> mlua::Result<(Value, Value)> {
    match api.review_list_hunks(sid.to_string()).await {
        Ok(hunks) => {
            let table = lua.create_table()?;
            for (i, hunk) in hunks.iter().enumerate() {
                table.set(i + 1, lua.to_value(hunk)?)?;
            }
            Ok((Value::Table(table), Value::Nil))
        }
        Err(e) => err_pair(lua, e),
    }
}

/// review_set_state(session_id, hunk_id, state) -> (true, nil) | (nil, err)
pub(crate) async fn review_set_state_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    hunk: String,
    state: String,
) -> mlua::Result<(Value, Value)> {
    match api.review_set_state(sid.to_string(), hunk, state).await {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// review_comment(session_id, { path, line_start, line_end?, body, root?,
/// author? }) -> (comment, nil) | (nil, err)
pub(crate) async fn review_comment_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    spec: Value,
) -> mlua::Result<(Value, Value)> {
    let spec: serde_json::Value = match lua.from_value(spec) {
        Ok(v) => v,
        Err(e) => {
            let err = lua.create_string(format!("invalid comment spec: {e}"))?;
            return Ok((Value::Nil, Value::String(err)));
        }
    };
    match api.review_comment(sid.to_string(), spec).await {
        Ok(comment) => Ok((lua.to_value(&comment)?, Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// review_resolve_comment(session_id, comment_id) -> (true, nil) | (nil, err)
pub(crate) async fn review_resolve_comment_op(
    lua: &Lua,
    api: &Arc<dyn DaemonSessionApi>,
    sid: &str,
    comment_id: String,
) -> mlua::Result<(Value, Value)> {
    match api
        .review_resolve_comment(sid.to_string(), comment_id)
        .await
    {
        Ok(()) => Ok((Value::Boolean(true), Value::Nil)),
        Err(e) => err_pair(lua, e),
    }
}

/// Register the sessions module with stub functions.
///
/// Creates the `cru.session` namespace with functions that return
/// `(nil, "no daemon connected")`. Call [`register_sessions_module_with_api`]
/// to replace stubs with real daemon-backed implementations.
pub fn register_sessions_module(lua: &Lua) -> Result<(), LuaError> {
    let sessions = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.session", sessions.clone());

    // A stub ignores its arguments, but it TAKES the same ones as the
    // daemon-backed body, so `Ns` checks this path's declaration too — arity,
    // primitives and optionality on both halves of the pair. A stub that took
    // `MultiValue` would describe no single function and could only be
    // declared, never checked, and this is the path the plugin VM actually
    // loads.
    macro_rules! stub_async {
        ($name:expr, $args:ty) => {
            ns.async_func($name, decl($name)?, |lua, _args: $args| async move {
                let err = lua.create_string("no daemon connected")?;
                Ok((Value::Nil, Value::String(err)))
            })?;
        };
    }

    stub_async!("create", Value);
    stub_async!("get", String);
    stub_async!("list", ());
    stub_async!("configure_agent", (String, Value));
    stub_async!("send_message", (String, String));
    stub_async!("cancel", String);
    stub_async!("pause", String);
    stub_async!("resume", String);
    stub_async!("end_session", String);
    stub_async!("set_mode", (String, String));
    stub_async!("set_title", (String, String));
    stub_async!("interaction_respond", (String, String, Value));
    stub_async!("subscribe", String);
    stub_async!("unsubscribe", String);
    stub_async!("send_and_collect", (String, String, Value));
    stub_async!("messages", (String, Value));
    stub_async!("inject", (String, String, String));
    stub_async!("collect_subagents", (Vec<String>, Value));
    stub_async!("fork", (String, Value));
    stub_async!("cache_stats", String);
    stub_async!("complete", (String, Value));
    stub_async!("undo", (String, Value));
    stub_async!("can_undo", String);
    stub_async!("undo_depth", String);
    stub_async!("undo_history", String);
    stub_async!("review_list_hunks", String);
    stub_async!("review_set_state", (String, String, String));
    stub_async!("review_comment", (String, Value));
    stub_async!("review_resolve_comment", (String, String));

    // Two-way: a name added to SESSION_FNS and forgotten above fails here,
    // and so does a stub with no entry there.
    gate_module_keys("session", &sessions, &session_fn_names())?;
    merge_session_fns(lua, &sessions)?;
    install_sessions_alias(lua)?;

    Ok(())
}

/// Fields that name or override the session's agent.
///
/// `agent_type` is deliberately absent: on its own it selects an
/// implementation, not an agent, and `agent_type = "acp"` with no name is an
/// error on the create path where today it is a plain agent-less session.
const AGENT_FIELDS: [&str; 7] = [
    "agent_card",
    "agent_name",
    "provider",
    "provider_key",
    "model",
    "endpoint",
    "tool_policy",
];

/// Build the daemon's `session.create` params from a `cru.session.create`
/// argument.
///
/// The whole options table crosses as one object instead of four plucked keys:
/// the daemon deserializes it into the same request type its RPC handler uses,
/// so a plugin reaches every create-time field (agent cards, isolation,
/// recording) and the two sides cannot drift on a field name.
fn create_params(args: &Value) -> Result<serde_json::Value, String> {
    let mut params = match args {
        Value::Table(t) => serde_json::to_value(t)
            .map_err(|e| format!("create() options are not serializable: {e}"))?,
        // Legacy positional: create("chat") — session type only.
        Value::String(s) => {
            let session_type = s.to_str().map(|s| s.to_string()).unwrap_or_default();
            serde_json::json!({ "type": session_type })
        }
        _ => {
            return Err("create() expects a table argument, e.g. { type = \"chat\" }".to_string());
        }
    };
    let Some(obj) = params.as_object_mut() else {
        return Err(
            "create() expects named options, e.g. { type = \"chat\" }, not a list".to_string(),
        );
    };

    // mlua encodes an empty Lua table as a JSON *object*, which the request's
    // `Option<Vec<String>>` rejects. `kilns = {}` was tolerated by the
    // hand-plucked `unwrap_or_default()` this replaced, so it stays tolerated.
    if obj
        .get("kilns")
        .and_then(serde_json::Value::as_object)
        .is_some_and(serde_json::Map::is_empty)
    {
        obj.insert("kilns".to_string(), serde_json::Value::Array(Vec::new()));
    }

    // The daemon only reads the agent fields when `configure_agent` is set, so
    // without this a plugin that asked for an agent card would be silently
    // ignored. Implied here rather than in the RPC contract: the CLI and the
    // web layer both set the flag explicitly, and only Lua has a table where
    // the caller's intent is this unambiguous. An explicit `configure_agent`
    // in the table still wins.
    if !obj.contains_key("configure_agent") && AGENT_FIELDS.iter().any(|f| obj.contains_key(*f)) {
        obj.insert("configure_agent".to_string(), serde_json::Value::Bool(true));
    }

    Ok(params)
}

/// Copy the gated functions into `cru.session`.
///
/// Merging rather than replacing is what keeps `cru.session.current` —
/// registered by the executor's `register_session_module`, possibly before
/// this runs — alive across both the stub and the upgrade path, in any
/// registration order.
fn merge_session_fns(lua: &Lua, table: &mlua::Table) -> Result<(), LuaError> {
    let target = get_or_create_module(lua, "session")?;
    for (name, _) in SESSION_FNS {
        target.set(*name, table.get::<mlua::Function>(*name)?)?;
    }
    Ok(())
}

/// Register the sessions module with a real daemon API implementation.
///
/// This replaces the stub functions registered by [`register_sessions_module`]
/// with implementations that delegate to the provided [`DaemonSessionApi`].
/// `delegate = true` in a create options table is refused: with no
/// per-VM current session to stamp parentage from, a delegation could not be
/// attributed (and therefore gated) honestly.
pub fn register_sessions_module_with_api(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
) -> Result<(), LuaError> {
    register_sessions_inner(lua, api, None)
}

/// [`register_sessions_module_with_api`] bound to one VM's current session:
/// a `delegate = true` create is stamped with that session's id as the
/// delegation parent. The daemon — not Lua — remains the only writer of
/// `parent_session_id`, and the parent's own delegation policy is enforced
/// against the stamped id server-side.
pub fn register_sessions_module_with_api_and_current(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
    current: CurrentSession,
) -> Result<(), LuaError> {
    register_sessions_inner(lua, api, Some(current))
}

fn register_sessions_inner(
    lua: &Lua,
    api: Arc<dyn DaemonSessionApi>,
    current: Option<CurrentSession>,
) -> Result<(), LuaError> {
    // Build a fresh table. The gate at the end compares its keys against
    // SESSION_FNS, so a name with no daemon-backed body cannot hide
    // behind a stub.
    let sessions = lua.create_table()?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.session", sessions.clone());

    // create({ type = "chat", kilns = {"..."}, workspace = "...",
    //          agent_card = "..." })
    // Also supports legacy positional: create("chat")
    let a = Arc::clone(&api);
    ns.async_func("create", decl("create")?, move |lua, args: Value| {
        let a = Arc::clone(&a);
        let current = current.clone();
        async move { create_op(&lua, &a, args, current.as_ref()).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func("get", decl("get")?, move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move { get_session_op(&lua, &a, &session_id).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func("list", decl("list")?, move |lua, (): ()| {
        let a = Arc::clone(&a);
        async move { list_op(&lua, &a).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func(
        "configure_agent",
        decl("configure_agent")?,
        move |lua, (session_id, config): (String, Value)| {
            let a = Arc::clone(&a);
            async move { configure_agent_op(&lua, &a, &session_id, config).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "send_message",
        decl("send_message")?,
        move |lua, (session_id, content): (String, String)| {
            let a = Arc::clone(&a);
            async move { send_message_op(&lua, &a, &session_id, content).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func("cancel", decl("cancel")?, move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move { cancel_op(&lua, &a, &session_id).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func("pause", decl("pause")?, move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move { pause_op(&lua, &a, &session_id).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func("resume", decl("resume")?, move |lua, session_id: String| {
        let a = Arc::clone(&a);
        async move { resume_op(&lua, &a, &session_id).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func(
        "end_session",
        decl("end_session")?,
        move |lua, session_id: String| {
            let a = Arc::clone(&a);
            async move { end_session_op(&lua, &a, &session_id).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "set_mode",
        decl("set_mode")?,
        move |lua, (session_id, mode_id): (String, String)| {
            let a = Arc::clone(&a);
            async move { set_mode_op(&lua, &a, &session_id, mode_id).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "set_title",
        decl("set_title")?,
        move |lua, (session_id, title): (String, String)| {
            let a = Arc::clone(&a);
            async move { set_title_op(&lua, &a, &session_id, title).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "interaction_respond",
        decl("interaction_respond")?,
        move |lua, (session_id, request_id, response): (String, String, Value)| {
            let a = Arc::clone(&a);
            async move { interaction_respond_op(&lua, &a, &session_id, request_id, response).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "subscribe",
        decl("subscribe")?,
        move |lua, session_id: String| {
            let a = Arc::clone(&a);
            async move { subscribe_op(&lua, &a, &session_id).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "unsubscribe",
        decl("unsubscribe")?,
        move |lua, session_id: String| {
            let a = Arc::clone(&a);
            async move { unsubscribe_op(&lua, &a, &session_id).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "send_and_collect",
        decl("send_and_collect")?,
        move |lua, (session_id, content, opts): (String, String, Value)| {
            let a = Arc::clone(&a);
            async move { send_and_collect_op(&lua, &a, &session_id, content, opts).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "messages",
        decl("messages")?,
        move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move { messages_op(&lua, &a, &session_id, opts).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "inject",
        decl("inject")?,
        move |lua, (session_id, role, content): (String, String, String)| {
            let a = Arc::clone(&a);
            async move { inject_op(&lua, &a, &session_id, role, content).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "collect_subagents",
        decl("collect_subagents")?,
        move |lua, (job_ids, timeout): (Vec<String>, Value)| {
            let a = Arc::clone(&a);
            async move { collect_subagents_op(&lua, &a, job_ids, timeout).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "fork",
        decl("fork")?,
        move |lua, (session_id, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move { fork_op(&lua, &a, &session_id, opts).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "cache_stats",
        decl("cache_stats")?,
        move |lua, session_id: String| {
            let a = Arc::clone(&a);
            async move { cache_stats_op(&lua, &a, &session_id).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "complete",
        decl("complete")?,
        move |lua, (sid, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move { complete_op(&lua, &a, &sid, opts).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "undo",
        decl("undo")?,
        move |lua, (sid, opts): (String, Value)| {
            let a = Arc::clone(&a);
            async move { undo_op(&lua, &a, &sid, opts).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func("can_undo", decl("can_undo")?, move |lua, sid: String| {
        let a = Arc::clone(&a);
        async move { can_undo_op(&lua, &a, &sid).await }
    })?;

    let a = Arc::clone(&api);
    ns.async_func(
        "undo_depth",
        decl("undo_depth")?,
        move |lua, sid: String| {
            let a = Arc::clone(&a);
            async move { undo_depth_op(&lua, &a, &sid).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "undo_history",
        decl("undo_history")?,
        move |lua, sid: String| {
            let a = Arc::clone(&a);
            async move { undo_history_op(&lua, &a, &sid).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "review_list_hunks",
        decl("review_list_hunks")?,
        move |lua, sid: String| {
            let a = Arc::clone(&a);
            async move { review_list_hunks_op(&lua, &a, &sid).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "review_set_state",
        decl("review_set_state")?,
        move |lua, (sid, hunk, state): (String, String, String)| {
            let a = Arc::clone(&a);
            async move { review_set_state_op(&lua, &a, &sid, hunk, state).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "review_comment",
        decl("review_comment")?,
        move |lua, (sid, spec): (String, Value)| {
            let a = Arc::clone(&a);
            async move { review_comment_op(&lua, &a, &sid, spec).await }
        },
    )?;

    let a = Arc::clone(&api);
    ns.async_func(
        "review_resolve_comment",
        decl("review_resolve_comment")?,
        move |lua, (sid, comment_id): (String, String)| {
            let a = Arc::clone(&a);
            async move { review_resolve_comment_op(&lua, &a, &sid, comment_id).await }
        },
    )?;

    gate_module_keys("session", &sessions, &session_fn_names())?;

    merge_session_fns(lua, &sessions)?;
    install_sessions_alias(lua)?;

    Ok(())
}
