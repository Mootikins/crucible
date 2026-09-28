//! The Lua session handle: the `Session` userdata and its config contract.
//!
//! A plugin gets a handle from `cru.session.current()`, from
//! `cru.session.create/get/list/fork`, or as the argument of a session hook:
//!
//! ```lua
//! local s = cru.session.current()
//! s.system_prompt = "Answer in one sentence."
//! s.model = "claude-sonnet-4"  -- in an on_session_start hook
//! ```
//!
//! ## Design notes
//!
//! The API uses explicit session objects, not `vim.o`-style globals. The
//! Neovim pattern assumes one "current" context. That pattern fails when
//! one VM serves many sessions, and when a plugin reads another session.
//!
//! A property read or write goes through [`SessionConfigRpc`]. A method
//! call (`s:send_message(...)`) goes through the same `_op` body in
//! `register` that the free function `cru.session.<name>` calls.

use super::register::{
    cache_stats_op, can_undo_op, cancel_op, clear_op, complete_op, configure_agent_op,
    end_session_op, fork_op, inject_op, interaction_respond_op, messages_op, pause_op, resume_op,
    review_list_hunks_op, send_and_collect_op, send_message_op, set_mode_op, set_title_op,
    subscribe_op, undo_depth_op, undo_history_op, undo_op, unsubscribe_op,
};
use super::DaemonSessionApi;
use crate::host_hook::HostHook;
use mlua::{LuaSerdeExt, MetaMethod, UserData, UserDataMethods, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// The message a setter with no backing gives back, naming the field so a
/// plugin author can see which assignment reached nothing.
fn unsupported(field: &str) -> String {
    format!("{field}: not supported on this session")
}

/// Thin RPC interface for session configuration.
/// Does NOT expose message sending or other sensitive operations.
///
/// Every method is required. The trait used to default every method, so a
/// backing that forgot one still compiled, and a Lua knob could be half
/// wired: `NoopSessionRpc` was `impl SessionConfigRpc for NoopSessionRpc {}`
/// and was bound at every daemon site, so a plugin that wrote
/// `session.system_prompt = "..."` was told it worked and nothing
/// happened. A backing that supports nothing now says so by name:
/// [`UnsupportedSessionRpc`]. A backing that supports some knobs delegates
/// the rest to it, so the compiler lists each knob it does not answer.
///
/// **Setters report an error, not `Ok(())`, when a knob is unsupported.**
/// Getters return `None`, because an absent value is honestly `nil` in Lua,
/// and an error on a read would break `session.x or fallback`.
pub trait SessionConfigRpc: Send + Sync {
    fn get_model(&self) -> Option<String>;
    fn switch_model(&self, model: &str) -> Result<(), String>;
    fn get_mode(&self) -> String;
    fn set_mode(&self, mode: &str) -> Result<(), String>;
    fn get_system_prompt(&self) -> Option<String>;
    fn set_system_prompt(&self, prompt: &str) -> Result<(), String>;
    fn set_variable(&self, key: &str, value: serde_json::Value) -> Result<(), String>;
    fn get_variable(&self, key: &str) -> Option<serde_json::Value>;
}

/// A [`SessionConfigRpc`] that supports no knob.
///
/// The daemon binds it where a session only needs identity (plugin
/// lifecycle hooks, `lua.init_session`). Every setter reports that the knob
/// is unsupported; every getter returns the absent value. A partial backing
/// delegates the knobs it does not answer to this type.
pub struct UnsupportedSessionRpc;

impl SessionConfigRpc for UnsupportedSessionRpc {
    fn get_model(&self) -> Option<String> {
        None
    }
    fn switch_model(&self, _model: &str) -> Result<(), String> {
        Err(unsupported("model"))
    }
    fn get_mode(&self) -> String {
        "chat".to_string()
    }
    fn set_mode(&self, _mode: &str) -> Result<(), String> {
        Err(unsupported("mode"))
    }
    fn get_system_prompt(&self) -> Option<String> {
        None
    }
    fn set_system_prompt(&self, _prompt: &str) -> Result<(), String> {
        Err(unsupported("system_prompt"))
    }
    fn set_variable(&self, _key: &str, _value: serde_json::Value) -> Result<(), String> {
        Err(unsupported("variables"))
    }
    fn get_variable(&self, _key: &str) -> Option<serde_json::Value> {
        None
    }
}

/// The per-session key/value map behind `session:set_variable` and
/// `session:get_variable`.
///
/// Cheap to clone: every clone shares one map, so the daemon keeps a clone on
/// the session slot while the Lua session object holds another. The daemon
/// seeds it from the persisted session and writes it back, which is what makes
/// a variable survive a resume. The keys mean nothing to the daemon.
#[derive(Debug, Clone, Default)]
pub struct SessionVariables {
    inner: Arc<Mutex<BTreeMap<String, serde_json::Value>>>,
}

impl SessionVariables {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, serde_json::Value>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        self.lock().get(key).cloned()
    }

    pub fn set(&self, key: &str, value: serde_json::Value) {
        self.lock().insert(key.to_string(), value);
    }

    /// A copy of every pair, in key order.
    pub fn snapshot(&self) -> BTreeMap<String, serde_json::Value> {
        self.lock().clone()
    }

    /// Replace every pair at once. The daemon calls this with the persisted
    /// map before the session's hooks run.
    pub fn replace(&self, values: BTreeMap<String, serde_json::Value>) {
        *self.lock() = values;
    }
}

/// Session object with property access (returned by get_session())
#[derive(Clone)]
pub struct Session {
    /// The live config RPC, installed once by whichever fire site built this
    /// handle. See [`HostHook`]: `Mutex<Option<Box<_>>>` paid a lock on every
    /// property read and let a second `bind` replace the first in silence.
    rpc: HostHook<Box<dyn SessionConfigRpc>>,
    /// The lifecycle API a handle's methods delegate to. Present on handles
    /// that came from `cru.session.create/get/list/fork` (they were made
    /// *through* an API, so they carry it) and on the current-session handle
    /// where the daemon wired it; absent otherwise, and methods then report
    /// that they are not connected rather than silently doing nothing.
    api: Option<Arc<dyn DaemonSessionApi>>,
    id: String,
    /// The session's working directory. Identity, not mutable config, so it
    /// lives here rather than behind `SessionConfigRpc` — a plugin that needs
    /// to mount or scope the workspace (`oci`) must be able to read it during
    /// `on_session_start`, before any agent is configured.
    workspace: Option<String>,
    /// The session's isolation override, as `session.create` received it.
    ///
    /// Identity like `workspace`, and for the same reason: the isolating
    /// plugin has to read it during `on_session_start`, before any agent
    /// exists. The daemon never interprets it — `false`, a profile name and an
    /// environment table are the *plugin's* vocabulary, which is why this is a
    /// field on the object the plugin already receives rather than a new Lua
    /// API. Lua sees `nil` when the caller said nothing, which is distinct from
    /// `false` ("no container even if the project has one").
    isolation: Option<serde_json::Value>,
    /// Why the session stops, on the handle that the end hooks receive:
    /// `paused`, `ended`, `archived`, `auto_archived`, `deleted`, `refused` or
    /// `child_done`. `nil` everywhere else. A hook that reviews a finished
    /// session reads it to skip a pause.
    end_reason: Option<String>,
    /// The daemon's own response object for this session (`create`/`get`/
    /// `list` results). Property reads that name no fixed field and no live
    /// knob fall back to it, so `session.state` and friends keep working on
    /// a handle exactly as they did on the plain table the API used to return.
    record: Option<serde_json::Value>,
}

impl Session {
    pub fn new(id: String) -> Self {
        Self {
            rpc: HostHook::new(),
            api: None,
            id,
            workspace: None,
            isolation: None,
            end_reason: None,
            record: None,
        }
    }

    /// Attach the session's working directory. `None` when the caller has no
    /// workspace to give (`lua.init_session`, tests), in which case Lua sees
    /// `session.workspace == nil` rather than a wrong path.
    #[must_use]
    pub fn with_workspace(mut self, workspace: impl Into<String>) -> Self {
        self.workspace = Some(workspace.into());
        self
    }

    /// Attach the session's isolation override (see [`Session::isolation`]).
    #[must_use]
    pub fn with_isolation(mut self, isolation: serde_json::Value) -> Self {
        self.isolation = Some(isolation);
        self
    }

    /// Attach why the session stops (the `end_reason` field).
    #[must_use]
    pub fn with_end_reason(mut self, reason: impl Into<String>) -> Self {
        self.end_reason = Some(reason.into());
        self
    }

    /// Attach the daemon record this handle was built from.
    #[must_use]
    pub fn with_record(mut self, record: serde_json::Value) -> Self {
        self.record = Some(record);
        self
    }

    /// Attach the lifecycle API the handle's methods delegate to.
    #[must_use]
    pub fn with_api(mut self, api: Arc<dyn DaemonSessionApi>) -> Self {
        self.api = Some(api);
        self
    }

    /// The `(api, id)` pair a lifecycle method runs against, or why it cannot.
    fn op_ctx(&self, op: &str) -> mlua::Result<(Arc<dyn DaemonSessionApi>, String)> {
        match &self.api {
            Some(api) => Ok((Arc::clone(api), self.id.clone())),
            None => Err(mlua::Error::runtime(format!(
                "session method '{op}' is not connected to the daemon"
            ))),
        }
    }

    /// Install this handle's config RPC. Every fire site builds a fresh
    /// handle and binds it once, so a second bind is a double-wired fire site:
    /// it is refused and logged rather than replacing the first, because a
    /// replacement would silently change what every already-cloned handle
    /// reads.
    pub fn bind(&self, rpc: Box<dyn SessionConfigRpc>) {
        if !self.rpc.install(rpc) {
            tracing::warn!(
                session_id = %self.id,
                "session config RPC was already bound; keeping the first"
            );
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    /// The session's model: the live value when an RPC is bound, else the
    /// `model` field of the daemon record the handle was built from.
    pub(super) fn model(&self) -> mlua::Result<Option<String>> {
        Ok(match self.rpc.get() {
            Some(rpc) => rpc.get_model(),
            None => self
                .record
                .as_ref()
                .and_then(|record| record.get("model"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        })
    }

    fn with_rpc<F, T>(&self, f: F) -> mlua::Result<T>
    where
        F: FnOnce(&dyn SessionConfigRpc) -> Result<T, String>,
    {
        self.rpc
            .get()
            .ok_or_else(|| mlua::Error::runtime("Session not connected"))
            .and_then(|rpc| f(rpc.as_ref()).map_err(mlua::Error::runtime))
    }
}

/// Wire one lifecycle verb onto the handle.
///
/// The free function `cru.session.<name>(id, …)` and the method `s:<name>(…)`
/// call the same `_op`, so the two surfaces cannot drift. The session id
/// comes from the handle, which is the whole point of a handle.
macro_rules! session_method {
    ($m:expr, $name:literal, $op:path) => {
        $m.add_async_method($name, |lua, this, (): ()| {
            let this = this.clone();
            async move {
                let (api, sid) = match this.op_ctx($name) {
                    Ok(pair) => pair,
                    // The free functions answer `(nil, err)`; a method that
                    // raised instead would be the one caller in the module
                    // with a different error convention.
                    Err(mlua::Error::RuntimeError(msg)) => {
                        let err = lua.create_string(msg)?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                    Err(e) => return Err(e),
                };
                $op(&lua, &api, &sid).await
            }
        });
    };
    ($m:expr, $name:literal, $op:path, $a:ident: $ta:ty) => {
        $m.add_async_method($name, |lua, this, $a: $ta| {
            let this = this.clone();
            async move {
                let (api, sid) = match this.op_ctx($name) {
                    Ok(pair) => pair,
                    Err(mlua::Error::RuntimeError(msg)) => {
                        let err = lua.create_string(msg)?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                    Err(e) => return Err(e),
                };
                $op(&lua, &api, &sid, $a).await
            }
        });
    };
    ($m:expr, $name:literal, $op:path, $a:ident: $ta:ty, $b:ident: $tb:ty) => {
        $m.add_async_method($name, |lua, this, ($a, $b): ($ta, $tb)| {
            let this = this.clone();
            async move {
                let (api, sid) = match this.op_ctx($name) {
                    Ok(pair) => pair,
                    Err(mlua::Error::RuntimeError(msg)) => {
                        let err = lua.create_string(msg)?;
                        return Ok((Value::Nil, Value::String(err)));
                    }
                    Err(e) => return Err(e),
                };
                $op(&lua, &api, &sid, $a, $b).await
            }
        });
    };
}

impl UserData for Session {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(MetaMethod::Index, |lua, this, key: String| {
            match key.as_str() {
                "id" => lua.create_string(&this.id).map(Value::String),
                "workspace" => match &this.workspace {
                    Some(w) => lua.create_string(w).map(Value::String),
                    None => Ok(Value::Nil),
                },
                "isolation" => match &this.isolation {
                    Some(v) => lua.to_value(v),
                    None => Ok(Value::Nil),
                },
                "end_reason" => match &this.end_reason {
                    Some(reason) => lua.create_string(reason).map(Value::String),
                    None => Ok(Value::Nil),
                },
                // The plugin that created the session, from the record. Nil
                // for a session that no plugin created, not an unknown
                // property: the reflection pass reads it on every session.
                "plugin" => match this
                    .record
                    .as_ref()
                    .and_then(|record| record.get("plugin"))
                    .and_then(|v| v.as_str())
                {
                    Some(name) => lua.create_string(name).map(Value::String),
                    None => Ok(Value::Nil),
                },
                // A handle from `get`/`list` binds no RPC, so the daemon's
                // record is the only place the model can come from. A bound
                // handle still answers with the live value.
                "model" => this.model().and_then(|v| match v {
                    Some(s) => lua.create_string(&s).map(Value::String),
                    None => Ok(Value::Nil),
                }),
                "mode" => this
                    .with_rpc(|r| Ok(r.get_mode()))
                    .and_then(|s| lua.create_string(&s).map(Value::String)),
                "system_prompt" => {
                    this.with_rpc(|r| Ok(r.get_system_prompt()))
                        .and_then(|v| match v {
                            Some(s) => lua.create_string(&s).map(Value::String),
                            None => Ok(Value::Nil),
                        })
                }
                // A handle from create/get/list carries the daemon's own
                // response object; its fields (`session_type`, `state`,
                // `kilns`, …) read exactly as they did on the plain table
                // the API used to return. Live state above always wins over
                // a stale record.
                key => {
                    let from_record = this
                        .record
                        .as_ref()
                        .and_then(|record| record.get(key))
                        .map(|v| lua.to_value(v))
                        .transpose()
                        .map_err(mlua::Error::runtime)?;
                    match from_record {
                        Some(v) => Ok(v),
                        None => Err(mlua::Error::runtime(format!("unknown property: {key}"))),
                    }
                }
            }
        });

        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, val): (String, Value)| match key.as_str() {
                "id" | "workspace" | "isolation" | "end_reason" => {
                    Err(mlua::Error::runtime(format!("{} is read-only", key)))
                }
                "model" => {
                    let model: String = lua.unpack(val)?;
                    this.with_rpc(|r| r.switch_model(&model))
                }
                "mode" => {
                    let mode: String = lua.unpack(val)?;
                    this.with_rpc(|r| r.set_mode(&mode))
                }
                "system_prompt" => {
                    let prompt: String = lua.unpack(val)?;
                    this.with_rpc(|r| r.set_system_prompt(&prompt))
                }
                _ => Err(mlua::Error::runtime(format!(
                    "cannot set session.{key}: read-only (record fields are read-only; use \
                     configure_agent to change another session's agent)"
                ))),
            },
        );

        methods.add_method("set_variable", |lua, this, (key, val): (String, Value)| {
            let json_val: serde_json::Value = lua.from_value(val).map_err(|_| {
                mlua::Error::runtime("session variables must be JSON-serializable (cannot store functions, userdata, or recursive tables)")
            })?;
            this.with_rpc(|r| r.set_variable(&key, json_val))
        });

        methods.add_method("get_variable", |lua, this, key: String| {
            let maybe_val = this.with_rpc(|r| Ok(r.get_variable(&key)))?;
            match maybe_val {
                None => Ok(Value::Nil),
                Some(json) => lua.to_value(&json).map_err(mlua::Error::runtime),
            }
        });

        // ── Lifecycle verbs ─────────────────────────────────────────────
        session_method!(methods, "configure_agent", configure_agent_op, config: Value);
        session_method!(methods, "send_message", send_message_op, content: String);
        session_method!(methods, "clear", clear_op, options: Value);
        session_method!(methods, "cancel", cancel_op);
        session_method!(methods, "pause", pause_op);
        session_method!(methods, "resume", resume_op);
        session_method!(methods, "end_session", end_session_op);
        // A verb, not `s.mode = "auto"`: a handle from `create` binds no
        // `SessionConfigRpc`, so the NewIndex arm would answer "Session not
        // connected" on the one handle a plugin actually holds.
        session_method!(methods, "set_mode", set_mode_op, mode_id: String);
        session_method!(methods, "set_title", set_title_op, title: String);
        session_method!(
            methods,
            "interaction_respond",
            interaction_respond_op,
            request_id: String,
            response: Value
        );
        session_method!(methods, "subscribe", subscribe_op);
        session_method!(methods, "unsubscribe", unsubscribe_op);
        session_method!(
            methods,
            "send_and_collect",
            send_and_collect_op,
            content: String,
            opts: Value
        );
        session_method!(methods, "messages", messages_op, opts: Value);
        session_method!(methods, "inject", inject_op, role: String, content: String);
        session_method!(methods, "fork", fork_op, opts: Value);
        session_method!(methods, "cache_stats", cache_stats_op);
        session_method!(methods, "complete", complete_op, opts: Value);
        session_method!(methods, "undo", undo_op, opts: Value);
        session_method!(methods, "can_undo", can_undo_op);
        session_method!(methods, "undo_depth", undo_depth_op);
        session_method!(methods, "undo_history", undo_history_op);
        session_method!(methods, "review_list_hunks", review_list_hunks_op);
    }
}
