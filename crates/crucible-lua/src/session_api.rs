//! Session configuration API for Lua scripts
//!
//! Provides typed session objects with property-style access:
//!
//! ```lua
//! local s = crucible.get_session()
//! s.temperature = 0.7
//! s.max_tokens = 4096
//! s.thinking_budget = 1024
//! print(s.model)  -- read-only
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
//! - `crucible.get_session(id)` for cross-session access
//! - Session multiplexing for parallel agent orchestration
//!
//! ## Disabled Features
//!
//! Model switching (`s.model = "..."`) is disabled in Lua - use TUI `:model`
//! command instead. This prevents plugins from unexpectedly changing models.

use crate::error::LuaError;
use mlua::{Lua, LuaSerdeExt, MetaMethod, UserData, UserDataMethods, Value};
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
/// `session.thinking_budget = 4096` was told it worked and nothing
/// happened. A backing that supports nothing now says so by name:
/// [`UnsupportedSessionRpc`]. A backing that supports some knobs delegates
/// the rest to it, so the compiler lists each knob it does not answer.
///
/// **Setters report an error, not `Ok(())`, when a knob is unsupported.**
/// Getters return `None`, because an absent value is honestly `nil` in Lua,
/// and an error on a read would break `session.x or fallback`.
pub trait SessionConfigRpc: Send + Sync {
    fn get_temperature(&self) -> Option<f64>;
    fn set_temperature(&self, temp: f64) -> Result<(), String>;
    fn get_max_tokens(&self) -> Option<u32>;
    fn set_max_tokens(&self, tokens: Option<u32>) -> Result<(), String>;
    fn get_thinking_budget(&self) -> Option<i64>;
    fn set_thinking_budget(&self, budget: i64) -> Result<(), String>;
    fn get_model(&self) -> Option<String>;
    fn switch_model(&self, model: &str) -> Result<(), String>;
    fn list_models(&self) -> Vec<String>;
    fn get_mode(&self) -> String;
    fn set_mode(&self, mode: &str) -> Result<(), String>;
    fn get_system_prompt(&self) -> Option<String>;
    fn set_system_prompt(&self, prompt: &str) -> Result<(), String>;
    fn mark_first_message_sent(&self);
    fn set_variable(&self, key: &str, value: serde_json::Value);
    fn get_variable(&self, key: &str) -> Option<serde_json::Value>;
    fn notify(&self, notification: crucible_core::types::Notification);
    fn toggle_messages(&self);
    fn show_messages(&self);
    fn hide_messages(&self);
    fn clear_messages(&self);
}

/// A [`SessionConfigRpc`] that supports no knob.
///
/// The daemon binds it where a session only needs identity (plugin
/// lifecycle hooks, `lua.init_session`). Every setter reports that the knob
/// is unsupported; every getter returns the absent value. A partial backing
/// delegates the knobs it does not answer to this type.
pub struct UnsupportedSessionRpc;

impl SessionConfigRpc for UnsupportedSessionRpc {
    fn get_temperature(&self) -> Option<f64> {
        None
    }
    fn set_temperature(&self, _temp: f64) -> Result<(), String> {
        Err(unsupported("temperature"))
    }
    fn get_max_tokens(&self) -> Option<u32> {
        None
    }
    fn set_max_tokens(&self, _tokens: Option<u32>) -> Result<(), String> {
        Err(unsupported("max_tokens"))
    }
    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }
    fn set_thinking_budget(&self, _budget: i64) -> Result<(), String> {
        Err(unsupported("thinking_budget"))
    }
    fn get_model(&self) -> Option<String> {
        None
    }
    fn switch_model(&self, _model: &str) -> Result<(), String> {
        Err(unsupported("model"))
    }
    fn list_models(&self) -> Vec<String> {
        Vec::new()
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
    fn mark_first_message_sent(&self) {}
    fn set_variable(&self, _key: &str, _value: serde_json::Value) {}
    fn get_variable(&self, _key: &str) -> Option<serde_json::Value> {
        None
    }
    fn notify(&self, _notification: crucible_core::types::Notification) {}
    fn toggle_messages(&self) {}
    fn show_messages(&self) {}
    fn hide_messages(&self) {}
    fn clear_messages(&self) {}
}

/// Session object with property access (returned by get_session())
#[derive(Clone)]
pub struct Session {
    rpc: Arc<Mutex<Option<Box<dyn SessionConfigRpc>>>>,
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
}

impl Session {
    pub fn new(id: String) -> Self {
        Self {
            rpc: Arc::new(Mutex::new(None)),
            id,
            workspace: None,
            isolation: None,
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

    pub fn bind(&self, rpc: Box<dyn SessionConfigRpc>) {
        *self
            .rpc
            .lock()
            .expect("session_config_rpc: poisoned while binding RPC client") = Some(rpc);
    }

    fn with_rpc<F, T>(&self, f: F) -> mlua::Result<T>
    where
        F: FnOnce(&dyn SessionConfigRpc) -> Result<T, String>,
    {
        self.rpc
            .lock()
            .map_err(|e| mlua::Error::runtime(e.to_string()))?
            .as_ref()
            .ok_or_else(|| mlua::Error::runtime("Session not connected"))
            .and_then(|rpc| f(rpc.as_ref()).map_err(mlua::Error::runtime))
    }
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
                "temperature" => this
                    .with_rpc(|r| Ok(r.get_temperature()))
                    .map(|v| v.map(Value::Number).unwrap_or(Value::Nil)),
                "max_tokens" => this
                    .with_rpc(|r| Ok(r.get_max_tokens()))
                    .map(|v| v.map(|n| Value::Integer(n as i64)).unwrap_or(Value::Nil)),
                "thinking_budget" => this
                    .with_rpc(|r| Ok(r.get_thinking_budget()))
                    .map(|v| v.map(Value::Integer).unwrap_or(Value::Nil)),
                "model" => this.with_rpc(|r| Ok(r.get_model())).and_then(|v| match v {
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
                _ => Err(mlua::Error::runtime(format!("unknown property: {}", key))),
            }
        });

        methods.add_meta_method(
            MetaMethod::NewIndex,
            |lua, this, (key, val): (String, Value)| match key.as_str() {
                "id" | "model" | "workspace" | "isolation" => {
                    Err(mlua::Error::runtime(format!("{} is read-only", key)))
                }
                "temperature" => {
                    let temp: f64 = lua.unpack(val)?;
                    if !(0.0..=2.0).contains(&temp) {
                        return Err(mlua::Error::runtime("temperature must be 0.0-2.0"));
                    }
                    this.with_rpc(|r| r.set_temperature(temp))
                }
                "max_tokens" => {
                    let tokens = match val {
                        Value::Nil => None,
                        Value::Integer(n) if n > 0 => Some(n as u32),
                        Value::Number(n) if n > 0.0 => Some(n as u32),
                        _ => {
                            return Err(mlua::Error::runtime("max_tokens must be positive or nil"))
                        }
                    };
                    this.with_rpc(|r| r.set_max_tokens(tokens))
                }
                "thinking_budget" => {
                    let budget: i64 = lua.unpack(val)?;
                    this.with_rpc(|r| r.set_thinking_budget(budget))
                }
                "mode" => {
                    let mode: String = lua.unpack(val)?;
                    this.with_rpc(|r| r.set_mode(&mode))
                }
                "system_prompt" => {
                    let prompt: String = lua.unpack(val)?;
                    this.with_rpc(|r| r.set_system_prompt(&prompt))
                }
                _ => Err(mlua::Error::runtime(format!("cannot set session.{}", key))),
            },
        );

        methods.add_method("set_variable", |lua, this, (key, val): (String, Value)| {
            let json_val: serde_json::Value = lua.from_value(val).map_err(|_| {
                mlua::Error::runtime("session variables must be JSON-serializable (cannot store functions, userdata, or recursive tables)")
            })?;
            this.with_rpc(|r| {
                r.set_variable(&key, json_val);
                Ok(())
            })
        });

        methods.add_method("get_variable", |lua, this, key: String| {
            let maybe_val = this.with_rpc(|r| Ok(r.get_variable(&key)))?;
            match maybe_val {
                None => Ok(Value::Nil),
                Some(json) => lua.to_value(&json).map_err(mlua::Error::runtime),
            }
        });

        methods.add_method("mark_first_message_sent", |_lua, this, ()| {
            this.with_rpc(|r| {
                r.mark_first_message_sent();
                Ok(())
            })
        });
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
    let globals = lua.globals();

    for name in ["crucible", "cru"] {
        let table: mlua::Table = globals.get(name).or_else(|_| {
            let t = lua.create_table()?;
            globals.set(name, t.clone())?;
            Ok::<_, mlua::Error>(t)
        })?;

        let mgr = manager.clone();
        table.set(
            "get_session",
            lua.create_function(move |_, ()| {
                mgr.get_current()
                    .ok_or_else(|| mlua::Error::runtime("No active session"))
            })?,
        )?;
    }

    Ok(manager)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[derive(Clone)]
    pub struct MockRpc {
        temperature: Arc<std::sync::RwLock<Option<f64>>>,
        model: Arc<std::sync::RwLock<Option<String>>>,
        system_prompt: Arc<std::sync::RwLock<String>>,
        first_message_sent: Arc<std::sync::RwLock<bool>>,
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
                temperature: Arc::new(std::sync::RwLock::new(Some(0.7))),
                model: Arc::new(std::sync::RwLock::new(Some("test-model".to_string()))),
                system_prompt: Arc::new(std::sync::RwLock::new(
                    crucible_core::prompts::DEFAULT_SYSTEM_PROMPT.to_string(),
                )),
                first_message_sent: Arc::new(std::sync::RwLock::new(false)),
                variables: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            }
        }
    }

    impl SessionConfigRpc for MockRpc {
        fn get_temperature(&self) -> Option<f64> {
            *self.temperature.read().unwrap()
        }
        fn set_temperature(&self, temp: f64) -> Result<(), String> {
            *self.temperature.write().unwrap() = Some(temp);
            Ok(())
        }
        fn get_model(&self) -> Option<String> {
            self.model.read().unwrap().clone()
        }
        fn switch_model(&self, model: &str) -> Result<(), String> {
            *self.model.write().unwrap() = Some(model.to_string());
            Ok(())
        }
        fn list_models(&self) -> Vec<String> {
            vec!["model-a".to_string(), "model-b".to_string()]
        }
        fn get_mode(&self) -> String {
            "act".to_string()
        }
        fn get_system_prompt(&self) -> Option<String> {
            Some(self.system_prompt.read().unwrap().clone())
        }
        fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
            if *self.first_message_sent.read().unwrap() {
                return Err("system_prompt is locked after first message".to_string());
            }
            *self.system_prompt.write().unwrap() = prompt.to_string();
            Ok(())
        }
        fn mark_first_message_sent(&self) {
            *self.first_message_sent.write().unwrap() = true;
        }
        fn set_variable(&self, key: &str, value: serde_json::Value) {
            self.variables
                .write()
                .unwrap()
                .insert(key.to_string(), value);
        }
        fn get_variable(&self, key: &str) -> Option<serde_json::Value> {
            self.variables.read().unwrap().get(key).cloned()
        }
        fn get_max_tokens(&self) -> Option<u32> {
            UnsupportedSessionRpc.get_max_tokens()
        }
        fn set_max_tokens(&self, tokens: Option<u32>) -> Result<(), String> {
            UnsupportedSessionRpc.set_max_tokens(tokens)
        }
        fn get_thinking_budget(&self) -> Option<i64> {
            UnsupportedSessionRpc.get_thinking_budget()
        }
        fn set_thinking_budget(&self, budget: i64) -> Result<(), String> {
            UnsupportedSessionRpc.set_thinking_budget(budget)
        }
        fn set_mode(&self, mode: &str) -> Result<(), String> {
            UnsupportedSessionRpc.set_mode(mode)
        }
        fn notify(&self, _notification: crucible_core::types::Notification) {}
        fn toggle_messages(&self) {}
        fn show_messages(&self) {}
        fn hide_messages(&self) {}
        fn clear_messages(&self) {}
    }

    #[test]
    fn test_get_session_returns_current() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let id: String = lua.load("return crucible.get_session().id").eval().unwrap();
        assert_eq!(id, "test-123");
    }

    #[test]
    fn test_session_property_access() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let temp: f64 = lua
            .load("return crucible.get_session().temperature")
            .eval()
            .unwrap();
        assert!((temp - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_session_property_write() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("local s = crucible.get_session(); s.temperature = 0.3")
            .exec()
            .unwrap();

        let temp: f64 = lua
            .load("return crucible.get_session().temperature")
            .eval()
            .unwrap();
        assert!((temp - 0.3).abs() < 0.001);
    }

    #[test]
    fn test_model_is_read_only() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Result<()> = lua
            .load("crucible.get_session().model = 'new-model'")
            .exec();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("read-only"));
    }

    #[test]
    fn test_no_session_error() {
        let (lua, _mgr) = TestLuaBuilder::new().build_with_current_session();

        let result: mlua::Result<String> = lua.load("return crucible.get_session().id").eval();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active session"));
    }

    #[test]
    fn test_temperature_validation() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Result<()> = lua.load("crucible.get_session().temperature = 3.0").exec();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0.0-2.0"));
    }

    #[test]
    fn test_session_variable_string() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("crucible.get_session():set_variable('key', 'value')")
            .exec()
            .unwrap();

        let result: String = lua
            .load("return crucible.get_session():get_variable('key')")
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

        lua.load("crucible.get_session():set_variable('config', {nested = true, count = 42})")
            .exec()
            .unwrap();

        let result: mlua::Table = lua
            .load("return crucible.get_session():get_variable('config')")
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
            .load("return crucible.get_session():get_variable('nonexistent')")
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
            .load("crucible.get_session():set_variable('fn', function() end)")
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
            .load("return crucible.get_session().system_prompt")
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

        lua.load("local s = crucible.get_session(); s.system_prompt = 'custom prompt'")
            .exec()
            .unwrap();

        let prompt: String = lua
            .load("return crucible.get_session().system_prompt")
            .eval()
            .unwrap();
        assert_eq!(prompt, "custom prompt");
    }

    #[test]
    fn test_session_system_prompt_locked_after_send() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        lua.load("crucible.get_session():mark_first_message_sent()")
            .exec()
            .unwrap();

        let result: mlua::Result<()> = lua
            .load("crucible.get_session().system_prompt = 'new prompt'")
            .exec();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("locked"));
    }
}

#[cfg(test)]
mod unsupported_rpc_tests {
    use super::*;

    /// An unsupported setter must fail, not succeed silently.
    ///
    /// The trait once defaulted every setter to `Ok(())`, and the daemon
    /// bound that empty impl at every site, so a plugin that wrote
    /// `session.thinking_budget = 4096` was told it worked and nothing
    /// happened. The methods are required now; the one backing that
    /// supports nothing must still say so.
    #[test]
    fn an_unsupported_setter_reports_that_it_is_unsupported() {
        let rpc = UnsupportedSessionRpc;

        for (name, result) in [
            ("temperature", rpc.set_temperature(0.5)),
            ("max_tokens", rpc.set_max_tokens(Some(128))),
            ("thinking_budget", rpc.set_thinking_budget(4096)),
            ("model", rpc.switch_model("gpt-4o")),
            ("mode", rpc.set_mode("plan")),
            ("system_prompt", rpc.set_system_prompt("hi")),
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
        assert_eq!(rpc.get_temperature(), None);
        assert_eq!(rpc.get_max_tokens(), None);
        assert_eq!(rpc.get_model(), None);
        assert_eq!(rpc.get_thinking_budget(), None);
    }
}
