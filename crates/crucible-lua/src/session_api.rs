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
use mlua::{Lua, MetaMethod, UserData, UserDataMethods, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// Thin RPC interface for session configuration.
/// Does NOT expose message sending or other sensitive operations.
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
}

/// Channel-based adapter that sends commands to the event loop.
pub struct ChannelSessionRpc {
    tx: mpsc::UnboundedSender<SessionCommand>,
}

impl ChannelSessionRpc {
    pub fn new(tx: mpsc::UnboundedSender<SessionCommand>) -> Self {
        Self { tx }
    }
}

impl SessionConfigRpc for ChannelSessionRpc {
    fn get_temperature(&self) -> Option<f64> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SessionCommand::GetTemperature(reply_tx))
            .is_err()
        {
            return None;
        }
        reply_rx.blocking_recv().ok().flatten()
    }

    fn set_temperature(&self, temp: f64) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SetTemperature(temp, reply_tx))
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
    }

    fn get_max_tokens(&self) -> Option<u32> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SessionCommand::GetMaxTokens(reply_tx))
            .is_err()
        {
            return None;
        }
        reply_rx.blocking_recv().ok().flatten()
    }

    fn set_max_tokens(&self, tokens: Option<u32>) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SetMaxTokens(tokens, reply_tx))
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self
            .tx
            .send(SessionCommand::GetThinkingBudget(reply_tx))
            .is_err()
        {
            return None;
        }
        reply_rx.blocking_recv().ok().flatten()
    }

    fn set_thinking_budget(&self, budget: i64) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SetThinkingBudget(budget, reply_tx))
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
    }

    fn get_model(&self) -> Option<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(SessionCommand::GetModel(reply_tx)).is_err() {
            return None;
        }
        reply_rx.blocking_recv().ok().flatten()
    }

    fn switch_model(&self, model: &str) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SwitchModel(model.to_string(), reply_tx))
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
    }

    fn list_models(&self) -> Vec<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(SessionCommand::ListModels(reply_tx)).is_err() {
            return Vec::new();
        }
        reply_rx.blocking_recv().unwrap_or_default()
    }

    fn get_mode(&self) -> String {
        let (reply_tx, reply_rx) = oneshot::channel();
        if self.tx.send(SessionCommand::GetMode(reply_tx)).is_err() {
            return "unknown".to_string();
        }
        reply_rx
            .blocking_recv()
            .unwrap_or_else(|_| "unknown".to_string())
    }

    fn set_mode(&self, mode: &str) -> Result<(), String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SessionCommand::SetMode(mode.to_string(), reply_tx))
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx
            .blocking_recv()
            .map_err(|_| "Reply channel closed".to_string())?
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
        *self.rpc.lock().unwrap() = Some(rpc);
    }

    pub fn unbind(&self) {
        *self.rpc.lock().unwrap() = None;
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
                _ => Err(mlua::Error::runtime(format!("cannot set session.{}", key))),
            },
        );
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
        *self.current.lock().unwrap() = Some(session);
    }

    pub fn get_current(&self) -> Option<Session> {
        self.current.lock().ok()?.clone()
    }

    pub fn clear_current(&self) {
        *self.current.lock().unwrap() = None;
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

    pub struct MockRpc {
        temperature: std::sync::RwLock<Option<f64>>,
        model: std::sync::RwLock<Option<String>>,
    }

    impl MockRpc {
        pub fn new() -> Self {
            Self {
                temperature: std::sync::RwLock::new(Some(0.7)),
                model: std::sync::RwLock::new(Some("test-model".to_string())),
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
        fn get_max_tokens(&self) -> Option<u32> {
            None
        }
        fn set_max_tokens(&self, _: Option<u32>) -> Result<(), String> {
            Ok(())
        }
        fn get_thinking_budget(&self) -> Option<i64> {
            None
        }
        fn set_thinking_budget(&self, _: i64) -> Result<(), String> {
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
        fn set_mode(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    fn setup_lua() -> (Lua, SessionManager) {
        let lua = Lua::new();
        lua.globals()
            .set("crucible", lua.create_table().unwrap())
            .unwrap();
        lua.globals()
            .set("cru", lua.create_table().unwrap())
            .unwrap();
        let mgr = register_session_module(&lua).unwrap();
        (lua, mgr)
    }

    #[test]
    fn test_get_session_returns_current() {
        let (lua, mgr) = setup_lua();

        let session = Session::new("test-123".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let id: String = lua.load("return crucible.get_session().id").eval().unwrap();
        assert_eq!(id, "test-123");
    }

    #[test]
    fn test_session_property_access() {
        let (lua, mgr) = setup_lua();

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
        let (lua, mgr) = setup_lua();

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
        let (lua, mgr) = setup_lua();

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
        let (lua, _mgr) = setup_lua();

        let result: mlua::Result<String> = lua.load("return crucible.get_session().id").eval();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No active session"));
    }

    #[test]
    fn test_temperature_validation() {
        let (lua, mgr) = setup_lua();

        let session = Session::new("s1".to_string());
        session.bind(Box::new(MockRpc::new()));
        mgr.set_current(session);

        let result: mlua::Result<()> = lua.load("crucible.get_session().temperature = 3.0").exec();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("0.0-2.0"));
    }
}
