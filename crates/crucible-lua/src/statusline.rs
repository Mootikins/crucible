//! Statusline configuration module for Lua scripts
//!
//! Provides a table-based API for defining statusline components and layout.
//!
//! ## Lua Usage
//!
//! ```lua
//! local statusline = require("statusline")
//!
//! -- Define the statusline layout
//! statusline.setup({
//!     left = {
//!         statusline.mode({
//!             normal = { text = " NORMAL ", bg = "green", fg = "black" },
//!             plan = { text = " PLAN ", bg = "blue", fg = "black" },
//!             auto = { text = " AUTO ", bg = "yellow", fg = "black" },
//!         }),
//!     },
//!     center = {
//!         statusline.model({ max_length = 20, fallback = "..." }),
//!     },
//!     right = {
//!         statusline.context({ format = "{percent}% ctx" }),
//!     },
//!     separator = " ",  -- Between components (default: " ")
//! })
//! ```
//!
//! ## Built-in Components
//!
//! - `mode`: Current chat mode (normal/plan/auto)
//! - `model`: Active model name
//! - `context`: Context window usage
//! - `notification`: Transient notifications
//! - `text`: Static text with optional styling
//! - `spacer`: Flexible space between sections
//!
//! ## Custom Components
//!
//! ```lua
//! -- Define a custom component
//! local my_component = statusline.component({
//!     render = function(state)
//!         if state.streaming then
//!             return { text = "⏳", fg = "yellow" }
//!         end
//!         return nil  -- Hide when not streaming
//!     end,
//! })
//! ```

use crate::error::LuaError;
use mlua::{FromLua, Lua, Result as LuaResult, Table, Value};
use serde::{Deserialize, Serialize};

/// Color specification - can be a named color or hex
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ColorSpec {
    Named(String),
    Hex(String),
}

impl Default for ColorSpec {
    fn default() -> Self {
        ColorSpec::Named("default".to_string())
    }
}

impl FromLua for ColorSpec {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::String(s) => {
                let s = s.to_str()?.to_string();
                if s.starts_with('#') {
                    Ok(ColorSpec::Hex(s))
                } else {
                    Ok(ColorSpec::Named(s))
                }
            }
            Value::Nil => Ok(ColorSpec::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ColorSpec".to_string(),
                message: Some("expected string color or nil".to_string()),
            }),
        }
    }
}

/// Style specification for a component
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleSpec {
    pub fg: Option<ColorSpec>,
    pub bg: Option<ColorSpec>,
    pub bold: bool,
}

impl FromLua for StyleSpec {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(t) => {
                let fg: Option<ColorSpec> = t.get("fg").ok();
                let bg: Option<ColorSpec> = t.get("bg").ok();
                let bold: bool = t.get("bold").unwrap_or(false);
                Ok(StyleSpec { fg, bg, bold })
            }
            Value::Nil => Ok(StyleSpec::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "StyleSpec".to_string(),
                message: Some("expected table or nil".to_string()),
            }),
        }
    }
}

/// Mode style configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeStyleSpec {
    pub text: String,
    #[serde(flatten)]
    pub style: StyleSpec,
}

impl FromLua for ModeStyleSpec {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::Table(t) => {
                let text: String = t.get("text").unwrap_or_default();
                let fg: Option<ColorSpec> = t.get("fg").ok();
                let bg: Option<ColorSpec> = t.get("bg").ok();
                let bold: bool = t.get("bold").unwrap_or(true);
                Ok(ModeStyleSpec {
                    text,
                    style: StyleSpec { fg, bg, bold },
                })
            }
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "ModeStyleSpec".to_string(),
                message: Some("expected table".to_string()),
            }),
        }
    }
}

/// Component types that can appear in the statusline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StatuslineComponent {
    /// Mode indicator (normal/plan/auto)
    Mode {
        normal: ModeStyleSpec,
        plan: ModeStyleSpec,
        auto: ModeStyleSpec,
    },
    /// Model name display
    Model {
        max_length: Option<usize>,
        fallback: Option<String>,
        #[serde(flatten)]
        style: StyleSpec,
    },
    /// Context usage display
    Context {
        format: Option<String>,
        #[serde(flatten)]
        style: StyleSpec,
    },
    /// Static text
    Text {
        content: String,
        #[serde(flatten)]
        style: StyleSpec,
    },
    /// Flexible spacer
    Spacer,
    /// Notification display (shows fallback component when no notification is active)
    Notification {
        #[serde(flatten)]
        style: StyleSpec,
        /// Component to render when no notification is active (e.g., context usage)
        fallback: Option<Box<StatuslineComponent>>,
    },
}

/// Complete statusline configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatuslineConfig {
    pub left: Vec<StatuslineComponent>,
    pub center: Vec<StatuslineComponent>,
    pub right: Vec<StatuslineComponent>,
    pub separator: Option<String>,
}

impl StatuslineConfig {
    /// Returns the built-in default layout matching the embedded Lua default.
    ///
    /// Used when Lua hasn't initialized (e.g., tests) so the status bar
    /// still renders a usable layout instead of falling back to emergency mode.
    pub fn builtin_default() -> Self {
        Self {
            left: vec![
                StatuslineComponent::Mode {
                    normal: ModeStyleSpec::default(),
                    plan: ModeStyleSpec::default(),
                    auto: ModeStyleSpec::default(),
                },
                StatuslineComponent::Model {
                    max_length: Some(25),
                    fallback: None,
                    style: StyleSpec::default(),
                },
            ],
            center: vec![],
            right: vec![StatuslineComponent::Notification {
                style: StyleSpec::default(),
                fallback: Some(Box::new(StatuslineComponent::Context {
                    format: None,
                    style: StyleSpec::default(),
                })),
            }],
            separator: Some(" ".to_string()),
        }
    }
}

fn extract_color(table: &Table, key: &str) -> Option<ColorSpec> {
    table.get::<String>(key).ok().map(|s| {
        if s.starts_with('#') {
            ColorSpec::Hex(s)
        } else {
            ColorSpec::Named(s)
        }
    })
}

fn extract_style(table: &Table) -> StyleSpec {
    StyleSpec {
        fg: extract_color(table, "fg"),
        bg: extract_color(table, "bg"),
        bold: table.get("bold").unwrap_or(false),
    }
}

fn parse_component(table: &Table) -> LuaResult<StatuslineComponent> {
    let component_type: String = table.get("type")?;

    match component_type.as_str() {
        "mode" => {
            let normal: ModeStyleSpec = table.get("normal")?;
            let plan: ModeStyleSpec = table.get("plan")?;
            let auto: ModeStyleSpec = table.get("auto")?;
            Ok(StatuslineComponent::Mode { normal, plan, auto })
        }
        "model" => {
            let max_length: Option<usize> = table.get("max_length").ok();
            let fallback: Option<String> = table.get("fallback").ok();
            let style = extract_style(table);
            Ok(StatuslineComponent::Model {
                max_length,
                fallback,
                style,
            })
        }
        "context" => {
            let format: Option<String> = table.get("format").ok();
            let style = extract_style(table);
            Ok(StatuslineComponent::Context { format, style })
        }
        "text" => {
            let content: String = table.get("content")?;
            let style = extract_style(table);
            Ok(StatuslineComponent::Text { content, style })
        }
        "spacer" => Ok(StatuslineComponent::Spacer),
        "notification" => {
            let style = extract_style(table);
            let fallback = if let Ok(fb_table) = table.get::<Table>("fallback") {
                Some(Box::new(parse_component(&fb_table)?))
            } else {
                None
            };
            Ok(StatuslineComponent::Notification { style, fallback })
        }
        _ => Err(mlua::Error::RuntimeError(format!(
            "unknown component type: {}",
            component_type
        ))),
    }
}

/// Parse a section (left/center/right) from a Lua table
fn parse_section(value: Value) -> LuaResult<Vec<StatuslineComponent>> {
    match value {
        Value::Table(t) => {
            let mut components = Vec::new();
            for pair in t.pairs::<i64, Table>() {
                let (_, component_table) = pair?;
                components.push(parse_component(&component_table)?);
            }
            Ok(components)
        }
        Value::Nil => Ok(Vec::new()),
        _ => Err(mlua::Error::FromLuaConversionError {
            from: value.type_name(),
            to: "section".to_string(),
            message: Some("expected table or nil".to_string()),
        }),
    }
}

/// Register the statusline module with a Lua state
pub fn register_statusline_module(lua: &Lua) -> Result<(), LuaError> {
    let statusline = lua.create_table()?;

    // statusline.mode({ normal = {...}, plan = {...}, auto = {...} })
    let mode_fn = lua.create_function(|lua, config: Table| {
        let component = lua.create_table()?;
        component.set("type", "mode")?;
        component.set("normal", config.get::<Table>("normal")?)?;
        component.set("plan", config.get::<Table>("plan")?)?;
        component.set("auto", config.get::<Table>("auto")?)?;
        Ok(component)
    })?;
    statusline.set("mode", mode_fn)?;

    // statusline.model({ max_length = 20, fallback = "...", fg = "cyan" })
    let model_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "model")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("max_length") {
                component.set("max_length", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fallback") {
                component.set("fallback", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("model", model_fn)?;

    // statusline.context({ format = "{percent}% ctx", fg = "gray" })
    let context_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "context")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("format") {
                component.set("format", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("context", context_fn)?;

    // statusline.text("static text", { fg = "white" })
    let text_fn = lua.create_function(|lua, (content, style): (String, Option<Table>)| {
        let component = lua.create_table()?;
        component.set("type", "text")?;
        component.set("content", content)?;
        if let Some(s) = style {
            if let Ok(v) = s.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = s.get::<Value>("bg") {
                component.set("bg", v)?;
            }
            if let Ok(v) = s.get::<Value>("bold") {
                component.set("bold", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("text", text_fn)?;

    // statusline.spacer()
    let spacer_fn = lua.create_function(|lua, ()| {
        let component = lua.create_table()?;
        component.set("type", "spacer")?;
        Ok(component)
    })?;
    statusline.set("spacer", spacer_fn)?;

    // statusline.notification({ fg = "yellow", fallback = statusline.context() })
    let notification_fn = lua.create_function(|lua, config: Option<Table>| {
        let component = lua.create_table()?;
        component.set("type", "notification")?;
        if let Some(cfg) = config {
            if let Ok(v) = cfg.get::<Value>("fg") {
                component.set("fg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("bg") {
                component.set("bg", v)?;
            }
            if let Ok(v) = cfg.get::<Value>("fallback") {
                component.set("fallback", v)?;
            }
        }
        Ok(component)
    })?;
    statusline.set("notification", notification_fn)?;

    // Register as global module
    lua.globals().set("statusline", statusline)?;

    Ok(())
}

/// Parse a complete statusline configuration from Lua
pub fn parse_statusline_config(config: &Table) -> LuaResult<StatuslineConfig> {
    let left = parse_section(config.get("left")?)?;
    let center = parse_section(config.get("center").unwrap_or(Value::Nil))?;
    let right = parse_section(config.get("right").unwrap_or(Value::Nil))?;
    let separator: Option<String> = config.get("separator").ok();

    Ok(StatuslineConfig {
        left,
        center,
        right,
        separator,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        register_statusline_module(&lua).expect("Should register statusline module");
        lua
    }

    #[test]
    fn test_mode_component() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
            return statusline.mode({
                normal = { text = " NORMAL ", bg = "green", fg = "black" },
                plan = { text = " PLAN ", bg = "blue", fg = "black" },
                auto = { text = " AUTO ", bg = "yellow", fg = "black" },
            })
        "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("type").unwrap(), "mode");
    }

    #[test]
    fn test_model_component() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
            return statusline.model({ max_length = 20, fallback = "...", fg = "cyan" })
        "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("type").unwrap(), "model");
        assert_eq!(result.get::<usize>("max_length").unwrap(), 20);
        assert_eq!(result.get::<String>("fallback").unwrap(), "...");
    }

    #[test]
    fn test_context_component() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
            return statusline.context({ format = "{percent}% ctx", fg = "gray" })
        "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("type").unwrap(), "context");
        assert_eq!(result.get::<String>("format").unwrap(), "{percent}% ctx");
    }

    #[test]
    fn test_text_component() {
        let lua = setup_lua();
        let result: Table = lua
            .load(
                r#"
            return statusline.text(" | ", { fg = "gray" })
        "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result.get::<String>("type").unwrap(), "text");
        assert_eq!(result.get::<String>("content").unwrap(), " | ");
    }

    #[test]
    fn test_spacer_component() {
        let lua = setup_lua();
        let result: Table = lua.load("return statusline.spacer()").eval().unwrap();

        assert_eq!(result.get::<String>("type").unwrap(), "spacer");
    }

    #[test]
    fn test_full_config() {
        let lua = setup_lua();
        let config: Table = lua
            .load(
                r#"
            return {
                left = {
                    statusline.mode({
                        normal = { text = " NORMAL ", bg = "green", fg = "black" },
                        plan = { text = " PLAN ", bg = "blue", fg = "black" },
                        auto = { text = " AUTO ", bg = "yellow", fg = "black" },
                    }),
                },
                center = {
                    statusline.model({ max_length = 20, fallback = "...", fg = "cyan" }),
                },
                right = {
                    statusline.context({ format = "{percent}% ctx", fg = "gray" }),
                },
                separator = " ",
            }
        "#,
            )
            .eval()
            .unwrap();

        let parsed = parse_statusline_config(&config).unwrap();
        assert_eq!(parsed.left.len(), 1);
        assert_eq!(parsed.center.len(), 1);
        assert_eq!(parsed.right.len(), 1);
        assert_eq!(parsed.separator, Some(" ".to_string()));
    }

    #[test]
    fn test_default_statusline_config() {
        let lua = setup_lua();
        let config: Table = lua
            .load(
                r#"
            -- This is the default Crucible statusline
            return {
                left = {
                    statusline.mode({
                        normal = { text = " NORMAL ", bg = "green", fg = "black", bold = true },
                        plan = { text = " PLAN ", bg = "blue", fg = "black", bold = true },
                        auto = { text = " AUTO ", bg = "yellow", fg = "black", bold = true },
                    }),
                },
                center = {
                    statusline.model({ max_length = 20, fallback = "...", fg = "cyan" }),
                },
                right = {
                    statusline.context({ format = "{percent}% ctx", fg = "darkgray" }),
                    statusline.spacer(),
                    statusline.notification({ fg = "yellow" }),
                },
                separator = " ",
            }
        "#,
            )
            .eval()
            .unwrap();

        let parsed = parse_statusline_config(&config).unwrap();

        // Verify mode component
        match &parsed.left[0] {
            StatuslineComponent::Mode { normal, plan, auto } => {
                assert_eq!(normal.text, " NORMAL ");
                assert_eq!(plan.text, " PLAN ");
                assert_eq!(auto.text, " AUTO ");
            }
            _ => panic!("Expected Mode component"),
        }
    }
}
