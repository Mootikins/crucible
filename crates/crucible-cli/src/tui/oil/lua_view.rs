//! Lua-backed custom views
//!
//! Renders views defined in Lua scripts with `@view` annotations.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::node::{self as cli_node, text, Node};
use crate::tui::oil::style::{self as cli_style, Style};
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

        Ok(convert_ink_node(result.0))
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
                    fg: Some(cli_style::Color::Red),
                    ..Default::default()
                };
                text(format!("View error: {}", e)).with_style(error_style)
            }
        }
    }
}

fn convert_ink_node(ink_node: crucible_oil::Node) -> Node {
    match ink_node {
        crucible_oil::Node::Empty => Node::Empty,
        crucible_oil::Node::Text(t) => Node::Text(cli_node::TextNode {
            content: t.content,
            style: convert_style(t.style),
        }),
        crucible_oil::Node::Box(b) => Node::Box(cli_node::BoxNode {
            children: b.children.into_iter().map(convert_ink_node).collect(),
            direction: convert_direction(b.direction),
            size: convert_size(b.size),
            padding: convert_padding(b.padding),
            margin: convert_padding(b.margin),
            border: b.border.map(convert_border),
            style: convert_style(b.style),
            justify: convert_justify(b.justify),
            align: convert_align(b.align),
            gap: convert_gap(b.gap),
        }),
        crucible_oil::Node::Static(s) => Node::Static(cli_node::StaticNode {
            key: s.key,
            children: s.children.into_iter().map(convert_ink_node).collect(),
            newline: s.newline,
        }),
        crucible_oil::Node::Input(i) => Node::Input(cli_node::InputNode {
            value: i.value,
            cursor: i.cursor,
            placeholder: i.placeholder,
            style: convert_style(i.style),
            focused: i.focused,
        }),
        crucible_oil::Node::Spinner(s) => Node::Spinner(cli_node::SpinnerNode {
            label: s.label,
            style: convert_style(s.style),
            frame: s.frame,
            frames: None,
        }),
        crucible_oil::Node::Popup(p) => Node::Popup(cli_node::PopupNode {
            items: p
                .items
                .into_iter()
                .map(|i| cli_node::PopupItemNode {
                    label: i.label,
                    description: i.description,
                    kind: i.kind,
                })
                .collect(),
            selected: p.selected,
            viewport_offset: p.viewport_offset,
            max_visible: p.max_visible,
        }),
        crucible_oil::Node::Fragment(f) => {
            Node::Fragment(f.into_iter().map(convert_ink_node).collect())
        }
        crucible_oil::Node::Focusable(f) => Node::Focusable(cli_node::FocusableNode {
            id: crate::tui::oil::focus::FocusId(f.id.0),
            child: Box::new(convert_ink_node(*f.child)),
            auto_focus: f.auto_focus,
        }),
        crucible_oil::Node::ErrorBoundary(e) => Node::ErrorBoundary(cli_node::ErrorBoundaryNode {
            child: Box::new(convert_ink_node(*e.child)),
            fallback: Box::new(convert_ink_node(*e.fallback)),
        }),
        crucible_oil::Node::Overlay(o) => Node::Overlay(cli_node::OverlayNode {
            child: Box::new(convert_ink_node(*o.child)),
            anchor: convert_overlay_anchor(o.anchor),
        }),
    }
}

fn convert_style(s: crucible_oil::Style) -> Style {
    Style {
        fg: s.fg.map(convert_color),
        bg: s.bg.map(convert_color),
        bold: s.bold,
        dim: s.dim,
        italic: s.italic,
        underline: s.underline,
    }
}

fn convert_color(c: crucible_oil::Color) -> cli_style::Color {
    match c {
        crucible_oil::Color::Black => cli_style::Color::Black,
        crucible_oil::Color::Red => cli_style::Color::Red,
        crucible_oil::Color::Green => cli_style::Color::Green,
        crucible_oil::Color::Yellow => cli_style::Color::Yellow,
        crucible_oil::Color::Blue => cli_style::Color::Blue,
        crucible_oil::Color::Magenta => cli_style::Color::Magenta,
        crucible_oil::Color::Cyan => cli_style::Color::Cyan,
        crucible_oil::Color::White => cli_style::Color::White,
        crucible_oil::Color::Gray => cli_style::Color::Gray,
        crucible_oil::Color::DarkGray => cli_style::Color::DarkGray,
        crucible_oil::Color::Rgb(r, g, b) => cli_style::Color::Rgb(r, g, b),
        crucible_oil::Color::Reset => cli_style::Color::Reset,
    }
}

fn convert_direction(d: crucible_oil::Direction) -> cli_node::Direction {
    match d {
        crucible_oil::Direction::Row => cli_node::Direction::Row,
        crucible_oil::Direction::Column => cli_node::Direction::Column,
    }
}

fn convert_size(s: crucible_oil::Size) -> cli_node::Size {
    match s {
        crucible_oil::Size::Fixed(n) => cli_node::Size::Fixed(n),
        crucible_oil::Size::Flex(f) => cli_node::Size::Flex(f),
        crucible_oil::Size::Content => cli_node::Size::Content,
    }
}

fn convert_padding(p: crucible_oil::Padding) -> cli_style::Padding {
    cli_style::Padding {
        top: p.top,
        right: p.right,
        bottom: p.bottom,
        left: p.left,
    }
}

fn convert_border(b: crucible_oil::Border) -> cli_style::Border {
    match b {
        crucible_oil::Border::Single => cli_style::Border::Single,
        crucible_oil::Border::Double => cli_style::Border::Double,
        crucible_oil::Border::Rounded => cli_style::Border::Rounded,
        crucible_oil::Border::Heavy => cli_style::Border::Heavy,
    }
}

fn convert_justify(j: crucible_oil::JustifyContent) -> cli_style::JustifyContent {
    match j {
        crucible_oil::JustifyContent::Start => cli_style::JustifyContent::Start,
        crucible_oil::JustifyContent::End => cli_style::JustifyContent::End,
        crucible_oil::JustifyContent::Center => cli_style::JustifyContent::Center,
        crucible_oil::JustifyContent::SpaceBetween => cli_style::JustifyContent::SpaceBetween,
        crucible_oil::JustifyContent::SpaceAround => cli_style::JustifyContent::SpaceAround,
        crucible_oil::JustifyContent::SpaceEvenly => cli_style::JustifyContent::SpaceEvenly,
    }
}

fn convert_align(a: crucible_oil::AlignItems) -> cli_style::AlignItems {
    match a {
        crucible_oil::AlignItems::Start => cli_style::AlignItems::Start,
        crucible_oil::AlignItems::End => cli_style::AlignItems::End,
        crucible_oil::AlignItems::Center => cli_style::AlignItems::Center,
        crucible_oil::AlignItems::Stretch => cli_style::AlignItems::Stretch,
    }
}

fn convert_gap(g: crucible_oil::Gap) -> cli_style::Gap {
    cli_style::Gap {
        row: g.row,
        column: g.column,
    }
}

fn convert_overlay_anchor(
    a: crucible_oil::OverlayAnchor,
) -> crate::tui::oil::overlay::OverlayAnchor {
    match a {
        crucible_oil::OverlayAnchor::FromBottom(n) => {
            crate::tui::oil::overlay::OverlayAnchor::FromBottom(n)
        }
    }
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
