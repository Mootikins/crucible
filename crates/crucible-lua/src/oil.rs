//! Lua oil module for building TUI nodes
//!
//! Provides `cru.oil.*` functions for constructing UI nodes from Lua scripts.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_oil::template::{html_to_node, parse_color, spec_to_node, NodeSpec};
use crucible_oil::{
    badge, bullet_list, divider, fragment, horizontal_rule, if_else, key_value, numbered_list,
    popup, popup_item, progress_bar, scrollback, spacer, spinner, styled, text, text_input, when,
    AlignItems, Border, BoxNode, Direction, Gap, JustifyContent, Node, Padding, Style,
};
use mlua::{
    FromLua, Function, Lua, MultiValue, Result as LuaResult, Table, UserData, UserDataMethods,
    Value,
};
use std::collections::HashMap;

fn parse_border(s: &str) -> Border {
    match s {
        "double" => Border::Double,
        "rounded" => Border::Rounded,
        "heavy" => Border::Heavy,
        _ => Border::Single,
    }
}

fn parse_justify(s: &str) -> JustifyContent {
    match s.to_lowercase().replace('-', "_").as_str() {
        "end" => JustifyContent::End,
        "center" => JustifyContent::Center,
        "space_between" => JustifyContent::SpaceBetween,
        "space_around" => JustifyContent::SpaceAround,
        "space_evenly" => JustifyContent::SpaceEvenly,
        _ => JustifyContent::Start,
    }
}

fn parse_align(s: &str) -> AlignItems {
    match s.to_lowercase().as_str() {
        "end" => AlignItems::End,
        "center" => AlignItems::Center,
        "stretch" => AlignItems::Stretch,
        _ => AlignItems::Start,
    }
}

#[derive(Debug, Clone)]
pub struct LuaNode(pub Node);

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("with_style", |_lua, this, style_table: Table| {
            let style = parse_style_from_table(&style_table)?;
            Ok(LuaNode(this.0.clone().with_style(style)))
        });

        methods.add_method("with_padding", |_, this, padding: u16| {
            Ok(LuaNode(this.0.clone().with_padding(Padding::all(padding))))
        });

        methods.add_method("with_border", |_, this, border_type: Option<String>| {
            let border = border_type.as_deref().map_or(Border::Single, parse_border);
            Ok(LuaNode(this.0.clone().with_border(border)))
        });

        methods.add_method("with_margin", |_, this, margin: u16| {
            Ok(LuaNode(this.0.clone().with_margin(Padding::all(margin))))
        });

        methods.add_method("gap", |_, this, gap_val: u16| {
            Ok(LuaNode(this.0.clone().gap(Gap::all(gap_val))))
        });

        methods.add_method("justify", |_, this, justify_str: String| {
            Ok(LuaNode(this.0.clone().justify(parse_justify(&justify_str))))
        });

        methods.add_method("align", |_, this, align_str: String| {
            Ok(LuaNode(this.0.clone().align(parse_align(&align_str))))
        });
    }
}

impl FromLua for LuaNode {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaNode>().map(|n| n.clone()),
            Value::Nil => Ok(LuaNode(Node::Empty)),
            Value::String(s) => Ok(LuaNode(text(s.to_str()?.to_string()))),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaNode".to_string(),
                message: Some("expected LuaNode userdata, nil, or string".to_string()),
            }),
        }
    }
}

pub fn register_oil_module(lua: &Lua) -> Result<(), LuaError> {
    let oil = lua.create_table()?;

    // cru.oil.text(content, opts?)
    let text_fn = lua.create_function(|_lua, args: MultiValue| {
        let mut args_iter = args.into_iter();

        let content = match args_iter.next() {
            Some(Value::String(s)) => s.to_str()?.to_string(),
            Some(Value::Integer(n)) => n.to_string(),
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::Nil) | None => String::new(),
            Some(other) => {
                return Err(mlua::Error::FromLuaConversionError {
                    from: other.type_name(),
                    to: "string".to_string(),
                    message: Some("text content must be a string".to_string()),
                });
            }
        };

        let style = match args_iter.next() {
            Some(Value::Table(t)) => parse_style_from_table(&t)?,
            _ => Style::default(),
        };

        if style == Style::default() {
            Ok(LuaNode(text(content)))
        } else {
            Ok(LuaNode(styled(content, style)))
        }
    })?;
    oil.set("text", text_fn)?;

    // cru.oil.col(opts_or_children...)
    let col_fn = lua.create_function(|lua, args: MultiValue| {
        let (opts, children) = parse_container_args(lua, args)?;
        let node = create_box_node(Direction::Column, opts, children);
        Ok(LuaNode(node))
    })?;
    oil.set("col", col_fn)?;

    // cru.oil.row(opts_or_children...)
    let row_fn = lua.create_function(|lua, args: MultiValue| {
        let (opts, children) = parse_container_args(lua, args)?;
        let node = create_box_node(Direction::Row, opts, children);
        Ok(LuaNode(node))
    })?;
    oil.set("row", row_fn)?;

    // cru.oil.spacer()
    let spacer_fn = lua.create_function(|_, ()| Ok(LuaNode(spacer())))?;
    oil.set("spacer", spacer_fn)?;

    // cru.oil.spinner(label?)
    let spinner_fn =
        lua.create_function(|_, label: Option<String>| Ok(LuaNode(spinner(label, 0))))?;
    oil.set("spinner", spinner_fn)?;

    // cru.oil.fragment(children...)
    let fragment_fn = lua.create_function(|_, children: MultiValue| {
        let child_nodes: Vec<Node> = children
            .into_iter()
            .filter_map(|v| {
                if let Value::UserData(ud) = v {
                    ud.borrow::<LuaNode>().ok().map(|n| n.0.clone())
                } else if let Value::String(s) = v {
                    s.to_str().ok().map(|s| text(s.to_string()))
                } else {
                    None
                }
            })
            .collect();
        Ok(LuaNode(fragment(child_nodes)))
    })?;
    oil.set("fragment", fragment_fn)?;

    // cru.oil.when(condition, node)
    let when_fn = lua.create_function(|_, (condition, node): (bool, LuaNode)| {
        Ok(LuaNode(when(condition, node.0)))
    })?;
    oil.set("when", when_fn)?;

    // cru.oil.either(condition, true_node, false_node)
    let either_fn = lua.create_function(|_, (cond, t, f): (bool, LuaNode, LuaNode)| {
        Ok(LuaNode(if_else(cond, t.0, f.0)))
    })?;
    oil.set("either", either_fn)?;

    // cru.oil.each(items, fn)
    let each_fn = lua.create_function(|_, (items, func): (Table, Function)| {
        let mut children = Vec::new();
        for pair in items.pairs::<i64, Value>() {
            let (idx, item) = pair?;
            let result: LuaNode = func.call((item, idx))?;
            children.push(result.0);
        }
        Ok(LuaNode(fragment(children)))
    })?;
    oil.set("each", each_fn)?;

    // cru.oil.input(opts)
    let input_fn = lua.create_function(|_, opts: Option<Table>| {
        let mut value = String::new();
        let mut cursor = 0;
        let mut placeholder = None;
        let mut focused = true;

        if let Some(t) = opts {
            if let Ok(v) = t.get::<String>("value") {
                value = v;
            }
            if let Ok(c) = t.get::<usize>("cursor") {
                cursor = c;
            }
            if let Ok(p) = t.get::<String>("placeholder") {
                placeholder = Some(p);
            }
            if let Ok(f) = t.get::<bool>("focused") {
                focused = f;
            }
        }

        let mut input = text_input(&value, cursor);
        if let Node::Input(ref mut i) = input {
            i.placeholder = placeholder;
            i.focused = focused;
        }
        Ok(LuaNode(input))
    })?;
    oil.set("input", input_fn)?;

    // cru.oil.popup(items, selected?, max_visible?)
    let popup_fn = lua.create_function(
        |_, (items, selected, max_visible): (Table, Option<usize>, Option<usize>)| {
            let mut popup_items = Vec::new();
            for pair in items.pairs::<i64, Value>() {
                let (_, item) = pair?;
                match item {
                    Value::String(s) => {
                        popup_items.push(popup_item(s.to_str()?.to_string()));
                    }
                    Value::Table(t) => {
                        let label: String = t.get("label").unwrap_or_default();
                        let mut pi = popup_item(label);
                        if let Ok(desc) = t.get::<String>("desc") {
                            pi = pi.desc(desc);
                        }
                        if let Ok(kind) = t.get::<String>("kind") {
                            pi = pi.kind(kind);
                        }
                        popup_items.push(pi);
                    }
                    _ => {}
                }
            }
            Ok(LuaNode(popup(
                popup_items,
                selected.unwrap_or(0),
                max_visible.unwrap_or(10),
            )))
        },
    )?;
    oil.set("popup", popup_fn)?;

    // cru.oil.divider(char?, width?)
    let divider_fn = lua.create_function(|_, (ch, width): (Option<String>, Option<u16>)| {
        let char = ch.and_then(|s| s.chars().next()).unwrap_or('─');
        Ok(LuaNode(divider(char, width.unwrap_or(80))))
    })?;
    oil.set("divider", divider_fn)?;

    // cru.oil.hr()
    let hr_fn = lua.create_function(|_, ()| Ok(LuaNode(horizontal_rule())))?;
    oil.set("hr", hr_fn)?;

    // cru.oil.progress(value, width?)
    let progress_fn = lua.create_function(|_, (value, width): (f64, Option<u16>)| {
        Ok(LuaNode(progress_bar(value as f32, width.unwrap_or(20))))
    })?;
    oil.set("progress", progress_fn)?;

    // cru.oil.badge(label, opts?)
    let badge_fn = lua.create_function(|_, (label, opts): (String, Option<Table>)| {
        let style = opts
            .map(|t| parse_style_from_table(&t))
            .transpose()?
            .unwrap_or_default();
        Ok(LuaNode(badge(label, style)))
    })?;
    oil.set("badge", badge_fn)?;

    // cru.oil.bullet_list(items)
    let bullet_list_fn = lua.create_function(|_, items: Table| {
        let list: Vec<String> = items
            .pairs::<i64, String>()
            .filter_map(|r| r.ok().map(|(_, v)| v))
            .collect();
        Ok(LuaNode(bullet_list(list)))
    })?;
    oil.set("bullet_list", bullet_list_fn)?;

    // cru.oil.numbered_list(items)
    let numbered_list_fn = lua.create_function(|_, items: Table| {
        let list: Vec<String> = items
            .pairs::<i64, String>()
            .filter_map(|r| r.ok().map(|(_, v)| v))
            .collect();
        Ok(LuaNode(numbered_list(list)))
    })?;
    oil.set("numbered_list", numbered_list_fn)?;

    // cru.oil.kv(key, value)
    let kv_fn = lua
        .create_function(|_, (key, value): (String, String)| Ok(LuaNode(key_value(key, value))))?;
    oil.set("kv", kv_fn)?;

    // cru.oil.build(table) - Build a node from a spec table
    let build_fn = lua.create_function(|lua, table: Table| {
        let spec_value = lua_table_to_spec(lua, &table)?;
        let node =
            spec_to_node(&spec_value).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        Ok(LuaNode(node))
    })?;
    oil.set("build", build_fn)?;

    // cru.oil.html(html_string)
    let html_fn = lua.create_function(|_, html: String| {
        let node = html_to_node(&html).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        Ok(LuaNode(node))
    })?;
    oil.set("html", html_fn)?;

    // cru.oil.scrollback(key, children...)
    let scrollback_fn = lua.create_function(|_, args: MultiValue| {
        let mut args_iter = args.into_iter();
        let key = match args_iter.next() {
            Some(Value::String(s)) => s.to_str()?.to_string(),
            _ => {
                return Err(mlua::Error::RuntimeError(
                    "scrollback requires a key".into(),
                ))
            }
        };

        let children: Vec<Node> = args_iter
            .filter_map(|v| {
                if let Value::UserData(ud) = v {
                    ud.borrow::<LuaNode>().ok().map(|n| n.0.clone())
                } else if let Value::String(s) = v {
                    s.to_str().ok().map(|s| text(s.to_string()))
                } else {
                    None
                }
            })
            .collect();

        Ok(LuaNode(scrollback(key, children)))
    })?;
    oil.set("scrollback", scrollback_fn)?;

    register_in_namespaces(lua, "oil", oil)?;

    Ok(())
}

fn parse_style_from_table(table: &Table) -> LuaResult<Style> {
    let mut style = Style::default();

    if let Ok(fg) = table.get::<String>("fg") {
        style.fg = Some(parse_color(&fg).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?);
    }
    if let Ok(bg) = table.get::<String>("bg") {
        style.bg = Some(parse_color(&bg).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?);
    }
    if table.get::<bool>("bold").unwrap_or(false) {
        style.bold = true;
    }
    if table.get::<bool>("dim").unwrap_or(false) {
        style.dim = true;
    }
    if table.get::<bool>("italic").unwrap_or(false) {
        style.italic = true;
    }
    if table.get::<bool>("underline").unwrap_or(false) {
        style.underline = true;
    }

    Ok(style)
}

fn parse_container_args(lua: &Lua, args: MultiValue) -> LuaResult<(Option<Table>, Vec<Node>)> {
    let args_vec: Vec<Value> = args.into_iter().collect();
    let mut children = Vec::new();
    let mut opts = None;

    for (i, arg) in args_vec.into_iter().enumerate() {
        match arg {
            Value::Table(t) if i == 0 => {
                // First table might be opts if it has style-like keys
                if t.contains_key("gap")?
                    || t.contains_key("padding")?
                    || t.contains_key("border")?
                    || t.contains_key("justify")?
                    || t.contains_key("align")?
                    || t.contains_key("fg")?
                    || t.contains_key("bg")?
                    || t.contains_key("bold")?
                {
                    opts = Some(t);
                } else {
                    // It's a spec-style array, convert to node
                    let spec_value = lua_table_to_spec(lua, &t)?;
                    let node = spec_to_node(&spec_value)
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                    children.push(node);
                }
            }
            Value::Table(t) => {
                let spec_value = lua_table_to_spec(lua, &t)?;
                let node = spec_to_node(&spec_value)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                children.push(node);
            }
            Value::UserData(ud) => {
                if let Ok(n) = ud.borrow::<LuaNode>() {
                    children.push(n.0.clone());
                }
            }
            Value::String(s) => {
                children.push(text(s.to_str()?.to_string()));
            }
            _ => {}
        }
    }

    Ok((opts, children))
}

fn create_box_node(direction: Direction, opts: Option<Table>, children: Vec<Node>) -> Node {
    let mut node = BoxNode {
        children,
        direction,
        ..Default::default()
    };

    if let Some(t) = opts {
        if let Ok(gap) = t.get::<u16>("gap") {
            node.gap = Gap::all(gap);
        }
        if let Ok(padding) = t.get::<u16>("padding") {
            node.padding = Padding::all(padding);
        }
        if let Ok(margin) = t.get::<u16>("margin") {
            node.margin = Padding::all(margin);
        }
        if let Ok(border_str) = t.get::<String>("border") {
            node.border = Some(parse_border(&border_str));
        } else if t.get::<bool>("border").unwrap_or(false) {
            node.border = Some(Border::Single);
        }
        if let Ok(justify) = t.get::<String>("justify") {
            node.justify = parse_justify(&justify);
        }
        if let Ok(align) = t.get::<String>("align") {
            node.align = parse_align(&align);
        }
        if let Ok(style) = parse_style_from_table(&t) {
            node.style = style;
        }
    }

    Node::Box(node)
}

fn lua_table_to_spec(lua: &Lua, table: &Table) -> LuaResult<NodeSpec> {
    let len = table.raw_len();

    if len > 0 {
        let mut arr = Vec::new();
        for i in 1..=len {
            let val: Value = table.get(i)?;
            arr.push(lua_value_to_spec(lua, val)?);
        }
        Ok(NodeSpec::Array(arr))
    } else {
        let mut map = HashMap::new();
        for pair in table.pairs::<String, Value>() {
            let (k, v) = pair?;
            map.insert(k, lua_value_to_spec(lua, v)?);
        }
        Ok(NodeSpec::Object(map))
    }
}

fn lua_value_to_spec(lua: &Lua, value: Value) -> LuaResult<NodeSpec> {
    match value {
        Value::Nil => Ok(NodeSpec::Null),
        Value::Boolean(b) => Ok(NodeSpec::Bool(b)),
        Value::Integer(i) => Ok(NodeSpec::Int(i)),
        Value::Number(n) => Ok(NodeSpec::Float(n)),
        Value::String(s) => Ok(NodeSpec::String(s.to_str()?.to_string())),
        Value::Table(t) => lua_table_to_spec(lua, &t),
        _ => Ok(NodeSpec::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::{Color, Size};

    fn setup_lua() -> Lua {
        let lua = Lua::new();

        // Create crucible table first (mimics executor setup)
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();

        register_oil_module(&lua).expect("Should register oil module");
        lua
    }

    #[test]
    fn test_register_oil_module() {
        let lua = setup_lua();

        let cru: Table = lua.globals().get("cru").expect("cru should exist");
        let oil: Table = cru.get("oil").expect("cru.oil should exist");

        assert!(oil.contains_key("text").unwrap());
        assert!(oil.contains_key("col").unwrap());
        assert!(oil.contains_key("row").unwrap());
        assert!(oil.contains_key("spacer").unwrap());
        assert!(oil.contains_key("spinner").unwrap());
        assert!(oil.contains_key("when").unwrap());
        assert!(oil.contains_key("either").unwrap());
        assert!(oil.contains_key("each").unwrap());
        assert!(oil.contains_key("build").unwrap());
    }

    #[test]
    fn test_oil_text() {
        let lua = setup_lua();

        let result: LuaNode = lua.load(r#"return cru.oil.text("hello")"#).eval().unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "hello");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_text_with_style() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.text("styled", {bold = true, fg = "red"})"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "styled");
            assert!(t.style.bold);
            assert_eq!(t.style.fg, Some(Color::Red));
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_col_with_children() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.col({gap = 1},
                    cru.oil.text("a"),
                    cru.oil.text("b")
                )
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.children.len(), 2);
            assert_eq!(b.gap, Gap::all(1));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_row_with_children() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.row(
                    cru.oil.text("left"),
                    cru.oil.spacer(),
                    cru.oil.text("right")
                )
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.direction, Direction::Row);
            assert_eq!(b.children.len(), 3);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_when_true() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.when(true, cru.oil.text("visible"))"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Text(_)));
    }

    #[test]
    fn test_oil_when_false() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.when(false, cru.oil.text("hidden"))"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Empty));
    }

    #[test]
    fn test_oil_either() {
        let lua = setup_lua();

        let result_true: LuaNode = lua
            .load(r#"return cru.oil.either(true, cru.oil.text("yes"), cru.oil.text("no"))"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result_true.0 {
            assert_eq!(t.content, "yes");
        } else {
            panic!("Expected Text node");
        }

        let result_false: LuaNode = lua
            .load(r#"return cru.oil.either(false, cru.oil.text("yes"), cru.oil.text("no"))"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result_false.0 {
            assert_eq!(t.content, "no");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_each() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                local items = {"a", "b", "c"}
                return cru.oil.each(items, function(item)
                    return cru.oil.text(item)
                end)
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Fragment(children) = result.0 {
            assert_eq!(children.len(), 3);
        } else {
            panic!("Expected Fragment node");
        }
    }

    #[test]
    fn test_oil_spacer() {
        let lua = setup_lua();

        let result: LuaNode = lua.load(r#"return cru.oil.spacer()"#).eval().unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.size, Size::Flex(1));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_spinner() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.spinner("Loading...")"#)
            .eval()
            .unwrap();

        if let Node::Spinner(s) = result.0 {
            assert_eq!(s.label, Some("Loading...".to_string()));
        } else {
            panic!("Expected Spinner node");
        }
    }

    #[test]
    fn test_oil_build_simple() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.build({"text", "Hello"})"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "Hello");
        } else {
            panic!("Expected Text node, got {:?}", result.0);
        }
    }

    #[test]
    fn test_oil_build_col_with_children() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.build({"col", {gap = 1},
                    {"text", "a"},
                    {"text", "b"}
                })
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.gap, Gap::all(1));
            assert_eq!(b.children.len(), 2);
        } else {
            panic!("Expected Box node, got {:?}", result.0);
        }
    }

    #[test]
    fn test_oil_progress() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.progress(0.5, 10)"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Text(_)));
    }

    #[test]
    fn test_oil_divider() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.divider("-", 5)"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "-----");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_bullet_list() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.bullet_list({"item 1", "item 2"})"#)
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.children.len(), 2);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_kv() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.kv("Name", "Value")"#)
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.direction, Direction::Row);
        } else {
            panic!("Expected Box node (row)");
        }
    }

    #[test]
    fn test_oil_node_chaining() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.text("hello")
                    :with_style({bold = true, fg = "green"})
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Text(t) = result.0 {
            assert!(t.style.bold);
            assert_eq!(t.style.fg, Some(Color::Green));
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_input() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.input({
                    value = "hello",
                    cursor = 5,
                    placeholder = "Type here..."
                })
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Input(i) = result.0 {
            assert_eq!(i.value, "hello");
            assert_eq!(i.cursor, 5);
            assert_eq!(i.placeholder, Some("Type here...".to_string()));
        } else {
            panic!("Expected Input node");
        }
    }

    #[test]
    fn test_oil_popup() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                return cru.oil.popup({
                    "Item 1",
                    "Item 2",
                    {label = "Item 3", desc = "Description"}
                }, 1, 5)
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Popup(p) = result.0 {
            assert_eq!(p.items.len(), 3);
            assert_eq!(p.selected, 1);
            assert_eq!(p.max_visible, 5);
            assert_eq!(p.items[2].description, Some("Description".to_string()));
        } else {
            panic!("Expected Popup node");
        }
    }

    #[test]
    fn test_oil_html() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.html('<div gap="2"><p>Hello</p><p>World</p></div>')"#)
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.direction, Direction::Column);
            assert_eq!(b.gap, Gap::all(2));
            assert_eq!(b.children.len(), 2);
        } else {
            panic!("Expected Box node from div, got {:?}", result.0);
        }
    }
}
