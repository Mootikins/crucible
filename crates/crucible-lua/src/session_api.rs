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
use tokio::sync::{mpsc, oneshot};

/// Extension trait to convert channel send errors to String.
trait SendExt<T> {
    fn or_closed(self) -> Result<T, String>;
}

impl<T, E> SendExt<T> for Result<T, tokio::sync::mpsc::error::SendError<E>> {
    fn or_closed(self) -> Result<T, String> {
        self.map_err(|_| "Channel closed".to_string())
    }
}

/// Thin RPC interface for session configuration.
/// Does NOT expose message sending or other sensitive operations.
///
/// All methods have no-op defaults so that stub implementations (e.g. daemon-side
/// NoopSessionRpc, test mocks) don't need 16 boilerplate methods.
pub trait SessionConfigRpc: Send + Sync {
    fn get_temperature(&self) -> Option<f64> {
        None
    }
    fn set_temperature(&self, _temp: f64) -> Result<(), String> {
        Ok(())
    }
    fn get_max_tokens(&self) -> Option<u32> {
        None
    }
    fn set_max_tokens(&self, _tokens: Option<u32>) -> Result<(), String> {
        Ok(())
    }
    fn get_thinking_budget(&self) -> Option<i64> {
        None
    }
    fn set_thinking_budget(&self, _budget: i64) -> Result<(), String> {
        Ok(())
    }
    fn get_model(&self) -> Option<String> {
        None
    }
    fn switch_model(&self, _model: &str) -> Result<(), String> {
        Ok(())
    }
    fn list_models(&self) -> Vec<String> {
        Vec::new()
    }
    fn get_mode(&self) -> String {
        "chat".to_string()
    }
    fn set_mode(&self, _mode: &str) -> Result<(), String> {
        Ok(())
    }
    fn get_system_prompt(&self) -> Option<String> {
        None
    }
    fn set_system_prompt(&self, _prompt: &str) -> Result<(), String> {
        Err("system_prompt: not supported".to_string())
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

/// Commands sent from Lua to the event loop for async execution.
#[derive(Debug)]
pub enum SessionCommand {
    SetTemperature(f64, oneshot::Sender<Result<(), String>>),
    GetTemperature(oneshot::Sender<Option<f64>>),
    SetMaxTokens(Option<u32>, oneshot::Sender<Result<(), String>>),
    GetMaxTokens(oneshot::Sender<Option<u32>>),
    SetThinkingBudget(i64, oneshot::Sender<Result<(), String>>),
    GetThinkingBudget(oneshot::Sender<Option<i64>>),
    SwitchModel(String, oneshot::Sender<Result<(), String>>),
    GetModel(oneshot::Sender<Option<String>>),
    ListModels(oneshot::Sender<Vec<String>>),
    SetMode(String, oneshot::Sender<Result<(), String>>),
    GetMode(oneshot::Sender<String>),
    GetSystemPrompt(oneshot::Sender<Option<String>>),
    SetSystemPrompt(String, oneshot::Sender<Result<(), String>>),
    MarkFirstMessageSent,
    SetVariable {
        key: String,
        value: serde_json::Value,
    },
    GetVariable {
        key: String,
        response: oneshot::Sender<Option<serde_json::Value>>,
    },
    Notify(crucible_core::types::Notification),
    ToggleMessages,
    ShowMessages,
    HideMessages,
    ClearMessages,
}

/// Channel-based adapter that sends commands to the event loop.
pub struct ChannelSessionRpc {
    tx: mpsc::UnboundedSender<SessionCommand>,
}

impl ChannelSessionRpc {
    pub fn new(tx: mpsc::UnboundedSender<SessionCommand>) -> Self {
        Self { tx }
    }

    /// Send a query command and return the response, or None on channel failure.
    fn query<T>(
        &self,
        cmd: impl FnOnce(oneshot::Sender<Option<T>>) -> SessionCommand,
    ) -> Option<T> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(cmd(tx)).ok()?;
        rx.blocking_recv().ok().flatten()
    }

    /// Send a command that returns Result, propagating channel errors.
    fn command(
        &self,
        cmd: impl FnOnce(oneshot::Sender<Result<(), String>>) -> SessionCommand,
    ) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(cmd(tx)).or_closed()?;
        rx.blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
    }
}

impl SessionConfigRpc for ChannelSessionRpc {
    fn get_temperature(&self) -> Option<f64> {
        self.query(SessionCommand::GetTemperature)
    }

    fn set_temperature(&self, temp: f64) -> Result<(), String> {
        self.command(|tx| SessionCommand::SetTemperature(temp, tx))
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.query(SessionCommand::GetMaxTokens)
    }

    fn set_max_tokens(&self, tokens: Option<u32>) -> Result<(), String> {
        self.command(|tx| SessionCommand::SetMaxTokens(tokens, tx))
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        self.query(SessionCommand::GetThinkingBudget)
    }

    fn set_thinking_budget(&self, budget: i64) -> Result<(), String> {
        self.command(|tx| SessionCommand::SetThinkingBudget(budget, tx))
    }

    fn get_model(&self) -> Option<String> {
        self.query(SessionCommand::GetModel)
    }

    fn switch_model(&self, model: &str) -> Result<(), String> {
        self.command(|tx| SessionCommand::SwitchModel(model.to_string(), tx))
    }

    fn list_models(&self) -> Vec<String> {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(SessionCommand::ListModels(tx)).is_err() {
            return Vec::new();
        }
        rx.blocking_recv().unwrap_or_default()
    }

    fn get_mode(&self) -> String {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(SessionCommand::GetMode(tx)).is_err() {
            return "unknown".to_string();
        }
        rx.blocking_recv().unwrap_or_else(|_| "unknown".to_string())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        self.command(|tx| SessionCommand::SetMode(mode.to_string(), tx))
    }

    fn get_system_prompt(&self) -> Option<String> {
        self.query(SessionCommand::GetSystemPrompt)
    }

    fn set_system_prompt(&self, prompt: &str) -> Result<(), String> {
        self.command(|tx| SessionCommand::SetSystemPrompt(prompt.to_string(), tx))
    }

    fn mark_first_message_sent(&self) {
        let _ = self.tx.send(SessionCommand::MarkFirstMessageSent);
    }

    fn set_variable(&self, key: &str, value: serde_json::Value) {
        let _ = self.tx.send(SessionCommand::SetVariable {
            key: key.to_string(),
            value,
        });
    }

    fn get_variable(&self, key: &str) -> Option<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        if self
            .tx
            .send(SessionCommand::GetVariable {
                key: key.to_string(),
                response: tx,
            })
            .is_err()
        {
            return None;
        }
        rx.blocking_recv().ok().flatten()
    }

    fn notify(&self, notification: crucible_core::types::Notification) {
        let _ = self.tx.send(SessionCommand::Notify(notification));
    }

    fn toggle_messages(&self) {
        let _ = self.tx.send(SessionCommand::ToggleMessages);
    }

    fn show_messages(&self) {
        let _ = self.tx.send(SessionCommand::ShowMessages);
    }

    fn hide_messages(&self) {
        let _ = self.tx.send(SessionCommand::HideMessages);
    }

    fn clear_messages(&self) {
        let _ = self.tx.send(SessionCommand::ClearMessages);
    }
}

/// Session object with property access (returned by get_session())
#[derive(Clone)]
pub struct Session {
    rpc: Arc<Mutex<Option<Box<dyn SessionConfigRpc>>>>,
    id: String,
}

impl Session {
    pub fn new(id: String) -> Self {
        Self {
            rpc: Arc::new(Mutex::new(None)),
            id,
        }
    }

    pub fn bind(&self, rpc: Box<dyn SessionConfigRpc>) {
        *self
            .rpc
            .lock()
            .expect("session_config_rpc: poisoned while binding RPC client") = Some(rpc);
    }

    pub fn unbind(&self) {
        *self
            .rpc
            .lock()
            .expect("session_config_rpc: poisoned while unbinding RPC client") = None;
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
                "id" | "model" => Err(mlua::Error::runtime(format!("{} is read-only", key))),
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

/// Session manager - holds current session
///
/// TODO: Add config hierarchy (TOML < global Lua < session Lua) once
/// kiln/workspace/session/project nomenclature is clarified.
#[derive(Clone)]
pub struct SessionManager {
    current: Arc<Mutex<Option<Session>>>,
}

impl SessionManager {
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

    pub fn clear_current(&self) {
        *self
            .current
            .lock()
            .expect("current_session: poisoned while clearing current session") = None;
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_session_module(lua: &Lua) -> Result<SessionManager, LuaError> {
    let manager = SessionManager::new();
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

    impl MockRpc {
        pub fn new() -> Self {
            Self {
                temperature: Arc::new(std::sync::RwLock::new(Some(0.7))),
                model: Arc::new(std::sync::RwLock::new(Some("test-model".to_string()))),
                system_prompt: Arc::new(std::sync::RwLock::new(
                    crucible_core::prompts::LARGE_MODEL_PROMPT.to_string(),
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
    }

    #[test]
    fn test_get_session_returns_current() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let id: String = lua.load("return crucible.get_session().id").eval().unwrap();
        assert_eq!(id, "test-123");
    }

    #[test]
    fn test_session_property_access() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, _mgr) = TestLuaBuilder::new().build_with_session_manager();

        let result: mlua::Result<String> = lua.load("return crucible.get_session().id").eval();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active session"));
    }

    #[test]
    fn test_temperature_validation() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Result<()> = lua.load("crucible.get_session().temperature = 3.0").exec();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0.0-2.0"));
    }

    #[test]
    fn test_session_variable_string() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let prompt: String = lua
            .load("return crucible.get_session().system_prompt")
            .eval()
            .unwrap();
        assert_eq!(prompt, crucible_core::prompts::LARGE_MODEL_PROMPT);
    }

    #[test]
    fn test_session_system_prompt_write() {
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
        let (lua, mgr) = TestLuaBuilder::new().build_with_session_manager();

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
