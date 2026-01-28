//! Lua-backed custom views
//!
//! Renders views defined in Lua scripts with `@view` annotations.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::node::{text, Node};
use crate::tui::oil::style::{Color, Style};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crucible_lua::{DiscoveredView, LuaExecutor};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub struct LuaView {
    name: String,
    executor: Arc<LuaExecutor>,
    source_path: PathBuf,
    view_fn: String,
    handler_fn: Option<String>,
    state: Arc<RwLock<Option<Vec<u8>>>>,
    width: u16,
    height: u16,
}

impl LuaView {
    pub fn new(
        name: String,
        executor: Arc<LuaExecutor>,
        source_path: PathBuf,
        view_fn: String,
        handler_fn: Option<String>,
    ) -> Self {
        Self {
            name,
            executor,
            source_path,
            view_fn,
            handler_fn,
            state: Arc::new(RwLock::new(None)),
            width: 80,
            height: 24,
        }
    }

    pub fn from_discovered(view: &DiscoveredView, executor: Arc<LuaExecutor>) -> Self {
        Self::new(
            view.name.clone(),
            executor,
            PathBuf::from(&view.source_path),
            view.view_fn.clone(),
            view.handler_fn.clone(),
        )
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_size(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
    }

    fn call_view(&self) -> Result<Node, String> {
        let source =
            std::fs::read_to_string(&self.source_path).map_err(|e| format!("IO error: {}", e))?;

        let is_fennel = self.source_path.extension().is_some_and(|e| e == "fnl");
        let lua = self.executor.lua();

        if is_fennel && self.executor.fennel_available() {
            let compiled = self
                .compile_fennel(&source)
                .map_err(|e| format!("Fennel compile error: {}", e))?;
            lua.load(&compiled)
                .exec()
                .map_err(|e| format!("Lua exec error: {}", e))?;
        } else if is_fennel {
            return Err("Fennel support not available".into());
        } else {
            lua.load(&source)
                .exec()
                .map_err(|e| format!("Lua exec error: {}", e))?;
        }

        let globals = lua.globals();

        let view_fn: mlua::Function = globals
            .get(self.view_fn.as_str())
            .map_err(|e| format!("View function '{}' not found: {}", self.view_fn, e))?;

        let ctx = lua
            .create_table()
            .map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.set("width", self.width)
            .map_err(|e| format!("Failed to set width: {}", e))?;
        ctx.set("height", self.height)
            .map_err(|e| format!("Failed to set height: {}", e))?;
        ctx.set("name", self.name.as_str())
            .map_err(|e| format!("Failed to set name: {}", e))?;

        if let Ok(state_guard) = self.state.read() {
            if let Some(state_bytes) = state_guard.as_ref() {
                if let Ok(state_str) = std::str::from_utf8(state_bytes) {
                    if let Ok(state_table) = lua.load(state_str).eval::<mlua::Table>() {
                        let _ = ctx.set("state", state_table);
                    }
                }
            }
        }

        let state_ref = self.state.clone();
        let set_state = lua
            .create_function(move |lua, new_state: mlua::Table| {
                let serialized = serialize_lua_table(lua, &new_state);
                if let Ok(mut guard) = state_ref.write() {
                    *guard = Some(serialized.into_bytes());
                }
                Ok(())
            })
            .map_err(|e| format!("Failed to create set_state: {}", e))?;
        ctx.set("set_state", set_state)
            .map_err(|e| format!("Failed to set set_state: {}", e))?;

        let result: crucible_lua::LuaNode = view_fn
            .call(ctx)
            .map_err(|e| format!("View call error: {}", e))?;

        Ok(convert_oil_node(result.0))
    }

    fn compile_fennel(&self, source: &str) -> Result<String, String> {
        use crucible_lua::FennelCompiler;
        let lua = self.executor.lua();
        let fennel = FennelCompiler::new(lua).map_err(|e| format!("Fennel init: {}", e))?;
        fennel
            .compile_with_lua(lua, source)
            .map_err(|e| format!("Fennel compile: {}", e))
    }

    pub fn handle_key(&self, key: KeyEvent) -> ViewAction {
        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
            return ViewAction::Close;
        }

        let Some(handler_fn) = &self.handler_fn else {
            return ViewAction::None;
        };

        let source = match std::fs::read_to_string(&self.source_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to load view source: {}", e);
                return ViewAction::None;
            }
        };

        let is_fennel = self.source_path.extension().is_some_and(|e| e == "fnl");
        let lua = self.executor.lua();

        let load_result = if is_fennel && self.executor.fennel_available() {
            self.compile_fennel(&source)
                .map(|compiled| lua.load(&compiled).exec())
        } else if is_fennel {
            Err("Fennel not available".into())
        } else {
            Ok(lua.load(&source).exec())
        };

        if let Err(e) = load_result {
            tracing::error!("Failed to load view source: {:?}", e);
            return ViewAction::None;
        }

        let globals = lua.globals();
        let handler: mlua::Function = match globals.get(handler_fn.as_str()) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Failed to get handler function '{}': {}", handler_fn, e);
                return ViewAction::None;
            }
        };

        let key_str = format_key_event(&key);

        let ctx = match lua.create_table() {
            Ok(t) => t,
            Err(_) => return ViewAction::None,
        };

        if let Ok(state_guard) = self.state.read() {
            if let Some(state_bytes) = state_guard.as_ref() {
                if let Ok(state_str) = std::str::from_utf8(state_bytes) {
                    if let Ok(state_table) = lua.load(state_str).eval::<mlua::Table>() {
                        let _ = ctx.set("state", state_table);
                    }
                }
            }
        }

        let state_ref = self.state.clone();
        if let Ok(set_state) = lua.create_function(move |lua, new_state: mlua::Table| {
            let serialized = serialize_lua_table(lua, &new_state);
            if let Ok(mut guard) = state_ref.write() {
                *guard = Some(serialized.into_bytes());
            }
            Ok(())
        }) {
            let _ = ctx.set("set_state", set_state);
        }

        let action_result = std::sync::Arc::new(std::sync::Mutex::new(ViewAction::None));
        let action_clone = action_result.clone();

        if let Ok(close_fn) = lua.create_function(move |_, ()| {
            if let Ok(mut a) = action_clone.lock() {
                *a = ViewAction::Close;
            }
            Ok(())
        }) {
            let _ = ctx.set("close", close_fn);
        }

        if let Err(e) = handler.call::<()>((key_str, ctx)) {
            tracing::error!("View handler error: {}", e);
        }

        if let Ok(a) = action_result.lock() {
            return a.clone();
        }

        ViewAction::None
    }
}

impl Component for LuaView {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        match self.call_view() {
            Ok(node) => node,
            Err(e) => {
                let error_style = Style {
                    fg: Some(Color::Red),
                    ..Default::default()
                };
                text(format!("View error: {}", e)).with_style(error_style)
            }
        }
    }
}

fn convert_oil_node(oil_node: crucible_oil::Node) -> Node {
    // CLI types are now re-exports of Oil types — identity conversion
    oil_node
}

#[derive(Debug, Clone, Default)]
pub enum ViewAction {
    #[default]
    None,
    Close,
}

fn serialize_lua_table(_lua: &mlua::Lua, table: &mlua::Table) -> String {
    fn serialize_value(value: &mlua::Value, out: &mut String) {
        match value {
            mlua::Value::Nil => out.push_str("nil"),
            mlua::Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
            mlua::Value::Integer(i) => out.push_str(&i.to_string()),
            mlua::Value::Number(n) => out.push_str(&n.to_string()),
            mlua::Value::String(s) => {
                out.push('"');
                if let Ok(str_val) = s.to_str() {
                    for c in str_val.chars() {
                        match c {
                            '"' => out.push_str("\\\""),
                            '\\' => out.push_str("\\\\"),
                            '\n' => out.push_str("\\n"),
                            '\r' => out.push_str("\\r"),
                            '\t' => out.push_str("\\t"),
                            _ => out.push(c),
                        }
                    }
                }
                out.push('"');
            }
            mlua::Value::Table(t) => {
                out.push('{');
                let mut first = true;
                if let Ok(pairs) = t
                    .clone()
                    .pairs::<mlua::Value, mlua::Value>()
                    .collect::<Result<Vec<_>, _>>()
                {
                    for (k, v) in pairs {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push('[');
                        serialize_value(&k, out);
                        out.push_str("]=");
                        serialize_value(&v, out);
                    }
                }
                out.push('}');
            }
            _ => out.push_str("nil"),
        }
    }

    let mut result = String::new();
    serialize_value(&mlua::Value::Table(table.clone()), &mut result);
    result
}

fn format_key_event(key: &KeyEvent) -> String {
    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("C");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("M");
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("S");
    }

    let key_name = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::F(n) => format!("F{}", n),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::PageUp => "PageUp".into(),
        KeyCode::PageDown => "PageDown".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::Delete => "Delete".into(),
        KeyCode::Insert => "Insert".into(),
        KeyCode::Esc => "Esc".into(),
        _ => "Unknown".into(),
    };

    if parts.is_empty() {
        key_name
    } else {
        parts.push(&key_name);
        parts.join("-")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_key_event_simple() {
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "a");
    }

    #[test]
    fn test_format_key_event_with_ctrl() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(format_key_event(&key), "C-c");
    }

    #[test]
    fn test_format_key_event_with_alt() {
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);
        assert_eq!(format_key_event(&key), "M-x");
    }

    #[test]
    fn test_format_key_event_special() {
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "Esc");

        let key = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "Enter");
    }

    #[test]
    fn test_format_key_event_function() {
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "F1");
    }

    #[test]
    fn test_format_key_event_combined_modifiers() {
        let key = KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        );
        assert_eq!(format_key_event(&key), "C-S-s");
    }
}
