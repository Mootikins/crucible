//! Session configuration API for Lua scripts
//!
//! Provides typed session objects with property-style access:
//!
//! ```lua
//! local s = cru.get_session()
//! s.system_prompt = "Answer in one sentence."
//! s.model = "claude-sonnet-4"  -- in an on_session_start hook
//! ```
//!
//! ## Design Notes
//!
//! **Why explicit sessions instead of `vim.o`-style globals?**
//!
//! Neovim's `vim.o`/`vim.bo` pattern assumes a single "current" context. This
//! breaks with multiplexing (multiple concurrent sessions) and cross-session
//! access patterns. Explicit session objects avoid implicit state.
//!
//! **Future considerations:**
//! - `cru.get_session(id)` for cross-session access
//! - Session multiplexing for parallel agent orchestration
//!
//! ## Disabled Features
//!
//! Model switching (`s.model = "..."`) is disabled in Lua - use TUI `:model`
//! command instead. This prevents plugins from unexpectedly changing models.

use crate::error::LuaError;
use crate::host_hook::HostHook;
use crate::sessions::register::{
    cache_stats_op, can_undo_op, cancel_op, complete_op, configure_agent_op, end_session_op,
    fork_op, inject_op, interaction_respond_op, messages_op, pause_op, resume_op,
    review_comment_op, review_list_hunks_op, review_resolve_comment_op, review_set_state_op,
    send_and_collect_op, send_message_op, set_mode_op, set_title_op, subscribe_op, undo_depth_op,
    undo_history_op, undo_op, unsubscribe_op,
};
use crate::sessions::DaemonSessionApi;
use mlua::{Lua, LuaSerdeExt, MetaMethod, UserData, UserDataMethods, Value};
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
    fn model(&self) -> mlua::Result<Option<String>> {
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
                "id" | "workspace" | "isolation" => {
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
        session_method!(
            methods,
            "review_set_state",
            review_set_state_op,
            hunk: String,
            state: String
        );
        session_method!(methods, "review_comment", review_comment_op, spec: Value);
        session_method!(
            methods,
            "review_resolve_comment",
            review_resolve_comment_op,
            comment_id: String
        );
    }
}

/// The one session a Lua VM is currently executing for.
///
/// It was called `SessionManager`, which it is not: it holds an
/// `Option<Session>` and has `set`, `get` and `clear`. It manages nothing, and
/// it shared a name with the daemon's real `SessionManager` — 20 files, session
/// creation, resume, child sessions, storage — and with an ACP trait in
/// `crucible-core`. The three had no method in common.
/// Session manager - holds current session
///
/// TODO: Add config hierarchy (TOML < global Lua < session Lua) once
/// kiln/workspace/session/project nomenclature is clarified.
#[derive(Clone)]
pub struct CurrentSession {
    current: Arc<Mutex<Option<Session>>>,
}

impl CurrentSession {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_current(&self, session: Session) {
        *self
            .current
            .lock()
            .expect("current_session: poisoned while setting current session") = Some(session);
    }

    pub fn get_current(&self) -> Option<Session> {
        self.current.lock().ok()?.clone()
    }
}

impl Default for CurrentSession {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_session_module(lua: &Lua) -> Result<CurrentSession, LuaError> {
    let manager = CurrentSession::new();
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;

    // `current` is registered here, not with the lifecycle module, because the
    // closure must close over *this* CurrentSession — the same instance the
    // daemon later binds a session into. The lifecycle registrations merge
    // their functions into the same table, so whichever runs first, both
    // surfaces end up on `cru.session`.
    let session_mod = crate::lua_util::get_or_create_module(lua, "session")?;
    let mut session = crate::host_registry::Ns::over(lua, "cru.session", session_mod);

    // It takes NO arguments, and it RAISES when no session is bound — it does
    // not answer nil, and it does not answer the `(value, err)` pair the rest
    // of `cru.session` uses. The handle is userdata, which a declaration has
    // no name for, so the return says `any`; `cru.session.get(id)` says the
    // same for the same reason.
    let mgr = manager.clone();
    session.func("current", "() -> any", move |lua, ()| {
        let session = mgr
            .get_current()
            .ok_or_else(|| mlua::Error::runtime("No active session"))?;
        Ok(Value::UserData(lua.create_userdata(session)?))
    })?;

    // Deprecated spelling of `current`, kept for one release. Zero production
    // callers when it was deprecated; it stays only so a user's local plugin
    // keeps working. Same declaration, because it is the same closure under
    // an older name.
    let mgr = manager.clone();
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru);
    root.func("get_session", "() -> any", move |lua, ()| {
        let session = mgr
            .get_current()
            .ok_or_else(|| mlua::Error::runtime("No active session"))?;
        Ok(Value::UserData(lua.create_userdata(session)?))
    })?;

    crate::lua_util::install_sessions_alias(lua)?;

    Ok(manager)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[derive(Clone)]
    pub struct MockRpc {
        model: Arc<std::sync::RwLock<Option<String>>>,
        system_prompt: Arc<std::sync::RwLock<String>>,
        variables: Arc<std::sync::RwLock<std::collections::HashMap<String, serde_json::Value>>>,
    }

    impl Default for MockRpc {
        fn default() -> Self {
            Self::new()
        }
    }

    impl MockRpc {
        pub fn new() -> Self {
            Self {
                model: Arc::new(std::sync::RwLock::new(Some("test-model".to_string()))),
                system_prompt: Arc::new(std::sync::RwLock::new(
                    crucible_core::prompts::DEFAULT_SYSTEM_PROMPT.to_string(),
                )),
                variables: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            }
        }
    }

    impl SessionConfigRpc for MockRpc {
        fn get_model(&self) -> Option<String> {
            self.model.read().unwrap().clone()
        }
        fn switch_model(&self, model: &str) -> Result<(), String> {
            *self.model.write().unwrap() = Some(model.to_string());
            Ok(())
        }
        fn get_mode(&self) -> String {
            "act".to_string()
        }
        fn get_system_prompt(&self) -> Option<String> {
            Some(self.system_prompt.read().unwrap().clone())
        }
        fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
            *self.system_prompt.write().unwrap() = prompt.to_string();
            Ok(())
        }
        fn set_variable(&self, key: &str, value: serde_json::Value) -> Result<(), String> {
            self.variables
                .write()
                .unwrap()
                .insert(key.to_string(), value);
            Ok(())
        }
        fn get_variable(&self, key: &str) -> Option<serde_json::Value> {
            self.variables.read().unwrap().get(key).cloned()
        }
        fn set_mode(&self, mode: &str) -> Result<(), String> {
            UnsupportedSessionRpc.set_mode(mode)
        }
    }

    /// A handle is built fresh per fire site and bound once, so a second bind
    /// is a double-wired fire site. It is refused, and the FIRST binding stays.
    ///
    /// `Mutex<Option<Box<_>>>` let the second replace the first in silence, and
    /// a handle is `Clone` with a shared slot — so a late rebind changed what
    /// every already-cloned handle read, with nothing logged.
    #[test]
    fn a_second_bind_is_refused_and_the_first_rpc_stays() {
        let session = Session::new("s-bind".to_string());
        let first = MockRpc::new();
        first.switch_model("first-model").unwrap();
        session.bind(Box::new(first));

        let clone = session.clone();

        let second = MockRpc::new();
        second.switch_model("second-model").unwrap();
        session.bind(Box::new(second));

        assert_eq!(
            session.model().unwrap().as_deref(),
            Some("first-model"),
            "the first binding must survive the refused second"
        );
        assert_eq!(
            clone.model().unwrap().as_deref(),
            Some("first-model"),
            "and a clone made before the second bind must read the same"
        );
    }

    #[test]
    fn test_get_session_returns_current() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let id: String = lua.load("return cru.get_session().id").eval().unwrap();
        assert_eq!(id, "test-123");
    }

    /// `current()` and the deprecated `get_session()` read the same binding —
    /// the daemon sets one current session per VM, and both spellings of the
    /// getter must see it.
    #[test]
    fn current_and_the_deprecated_getter_read_the_same_session() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s-current".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let (current_id, deprecated_id, model): (String, String, String) = lua
            .load(
                r#"
                local cur = cru.session.current()
                return cur.id, cru.get_session().id, cur.model
                "#,
            )
            .eval()
            .unwrap();
        assert_eq!(current_id, "s-current");
        assert_eq!(deprecated_id, "s-current");
        assert_eq!(model, "test-model");
    }

    #[test]
    fn test_session_property_access() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let model: String = lua.load("return cru.get_session().model").eval().unwrap();
        assert_eq!(model, "test-model");
    }

    #[test]
    fn test_session_property_write() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load(r#"local s = cru.get_session(); s.system_prompt = "rewritten""#)
            .exec()
            .unwrap();

        let prompt: String = lua
            .load("return cru.get_session().system_prompt")
            .eval()
            .unwrap();
        assert_eq!(prompt, "rewritten");
    }

    /// `session.model = "x"` is how a hook picks the model; it lands in
    /// `switch_model` and reads back.
    #[test]
    fn assigning_model_switches_it() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let model: String = lua
            .load(
                r#"local s = cru.get_session()
                   s.model = "new-model"
                   return s.model"#,
            )
            .eval()
            .unwrap();
        assert_eq!(model, "new-model");
    }

    #[test]
    fn test_no_session_error() {
        let (lua, _mgr) = TestLuaBuilder::new().build_with_current_session();

        let result: mlua::Result<String> = lua.load("return cru.get_session().id").eval();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active session"));
    }

    #[test]
    fn test_session_variable_string() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("cru.get_session():set_variable('key', 'value')")
            .exec()
            .unwrap();

        let result: String = lua
            .load("return cru.get_session():get_variable('key')")
            .eval()
            .unwrap();
        assert_eq!(result, "value");
    }

    #[test]
    fn test_session_variable_table() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("cru.get_session():set_variable('config', {nested = true, count = 42})")
            .exec()
            .unwrap();

        let result: mlua::Table = lua
            .load("return cru.get_session():get_variable('config')")
            .eval()
            .unwrap();
        let nested: bool = result.get("nested").unwrap();
        let count: i64 = result.get("count").unwrap();
        assert!(nested);
        assert_eq!(count, 42);
    }

    #[test]
    fn test_session_variable_nil_for_missing() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Value = lua
            .load("return cru.get_session():get_variable('nonexistent')")
            .eval()
            .unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_session_variable_reject_function() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Result<()> = lua
            .load("cru.get_session():set_variable('fn', function() end)")
            .exec();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("JSON-serializable"));
    }

    #[test]
    fn test_session_system_prompt_read() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let prompt: String = lua
            .load("return cru.get_session().system_prompt")
            .eval()
            .unwrap();
        assert_eq!(prompt, crucible_core::prompts::DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn test_session_system_prompt_write() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("local s = cru.get_session(); s.system_prompt = 'custom prompt'")
            .exec()
            .unwrap();

        let prompt: String = lua
            .load("return cru.get_session().system_prompt")
            .eval()
            .unwrap();
        assert_eq!(prompt, "custom prompt");
    }
}

#[cfg(test)]
mod unsupported_rpc_tests {
    use super::*;

    /// An unsupported setter must fail, not succeed silently.
    ///
    /// The trait once defaulted every setter to `Ok(())`, and the daemon
    /// bound that empty impl at every site, so a plugin that wrote
    /// `session.system_prompt = "..."` was told it worked and nothing
    /// happened. The methods are required now; the one backing that
    /// supports nothing must still say so.
    #[test]
    fn an_unsupported_setter_reports_that_it_is_unsupported() {
        let rpc = UnsupportedSessionRpc;

        for (name, result) in [
            ("model", rpc.switch_model("gpt-4o")),
            ("mode", rpc.set_mode("plan")),
            ("system_prompt", rpc.set_system_prompt("hi")),
            ("variables", rpc.set_variable("k", serde_json::json!(1))),
        ] {
            let err = result.expect_err("{name}: a no-op setter must not report success");
            assert!(
                err.contains("not supported"),
                "{name}: the error should say why, got: {err}"
            );
        }
    }

    /// Getters stay silent. The absence of a value is honestly `nil` in
    /// Lua, and an error on a read would break `session.x or fallback`.
    #[test]
    fn unsupported_getters_stay_silent() {
        let rpc = UnsupportedSessionRpc;
        assert_eq!(rpc.get_model(), None);
        assert_eq!(rpc.get_system_prompt(), None);
    }
}

#[cfg(test)]
mod unknown_property_tests {
    use super::*;

    /// A session handle carrying the record `session_json` builds.
    fn handle_with_record() -> Session {
        Session::new("chat-test".to_string()).with_record(serde_json::json!({
            "id": "chat-test",
            "session_type": "chat",
            "kilns": ["Crucible Help"],
            "state": "Active",
            "title": "A session",
            "model": "claude-sonnet-5",
            "started_at": "2026-09-16T00:00:00Z",
            "event_count": 3,
        }))
    }

    /// Reading a name the record does not carry must come back as an
    /// `mlua::Error`, not as a panic and not as a dead process.
    ///
    /// This is the shape that took the daemon down. `session-board` read
    /// `s.agent_model`, the record spells it `model`, the `Index` metamethod
    /// answered `Err(mlua::Error::runtime(..))` exactly as it should — and the
    /// release build died, because Luau raises that `Err` by throwing out of
    /// this very callback and the profile said `panic = "abort"`. The `Err`
    /// was never the bug. The profile was, and `lib.rs` now refuses to
    /// compile under it.
    ///
    /// `catch_unwind` is the point of the test, not decoration: it is what
    /// separates "returned an error" from "unwound out of the callback", and
    /// the two read identically to `is_err()`.
    #[test]
    fn an_unknown_property_is_an_error_and_not_a_panic() {
        let outcome = std::panic::catch_unwind(|| {
            let lua = Lua::new();
            let ud = lua.create_userdata(handle_with_record()).unwrap();
            lua.globals().set("s", ud).unwrap();
            lua.load("return s.agent_model").eval::<mlua::Value>()
        });

        let result = outcome.expect("reading an unknown property must not panic");
        let err = result.expect_err("an unknown property must not read as nil");
        assert!(
            err.to_string().contains("unknown property: agent_model"),
            "the error must name the property that was not found, got: {err}"
        );
    }

    /// The names the record does carry still read, so the gate above is a
    /// gate and not a wall.
    #[test]
    fn every_name_the_record_carries_reads_back() {
        let lua = Lua::new();
        let ud = lua.create_userdata(handle_with_record()).unwrap();
        lua.globals().set("s", ud).unwrap();

        for (expr, expected) in [
            ("s.id", "chat-test"),
            ("s.session_type", "chat"),
            ("s.state", "Active"),
            ("s.title", "A session"),
            ("s.model", "claude-sonnet-5"),
        ] {
            let got: String = lua
                .load(format!("return {expr}"))
                .eval()
                .unwrap_or_else(|e| panic!("{expr} must read: {e}"));
            assert_eq!(got, expected, "{expr}");
        }
    }
}
