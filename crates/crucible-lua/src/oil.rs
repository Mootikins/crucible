//! Lua oil module for building TUI nodes
//!
//! Provides `cru.oil.*` functions for constructing UI nodes from Lua scripts.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use crucible_oil::template::{html_to_node, parse_color};
use crucible_oil::{
    badge, bullet_list, divider, fragment, horizontal_rule, if_else, key_value, numbered_list,
    popup, popup_item, progress_bar, scrollback, spacer, spinner, styled, text, text_input, when,
    AlignItems, Border, BoxNode, Direction, Gap, JustifyContent, Node, Padding, Style,
};
use mlua::{
    FromLua, Function, Lua, MultiValue, Result as LuaResult, Table, UserData, UserDataMethods,
    Value,
};

fn parse_border(s: &str) -> Border {
    match s {
        "double" => Border::Double,
        "rounded" => Border::Rounded,
        "heavy" => Border::Heavy,
        _ => Border::Single,
    }
}

fn extract_node_from_value(v: Value) -> Option<Node> {
    match v {
        Value::UserData(ud) => ud.borrow::<LuaNode>().ok().map(|n| n.0.clone()),
        Value::String(s) => s.to_str().ok().map(|s| text(s.to_string())),
        _ => None,
    }
}

fn collect_child_nodes(values: impl Iterator<Item = Value>) -> Vec<Node> {
    values.filter_map(extract_node_from_value).collect()
}

fn table_to_string_list(items: &Table) -> Vec<String> {
    items
        .pairs::<i64, String>()
        .filter_map(|r| r.ok().map(|(_, v)| v))
        .collect()
}

fn parse_color_with_error(value: &str, prop_name: &str) -> LuaResult<crucible_oil::Color> {
    parse_color(value).map_err(|_| {
        mlua::Error::RuntimeError(format!(
            "invalid color '{}' for '{}'. Use named colors (red, green, blue, yellow, \
             cyan, magenta, white, black) or hex (#ff0000)",
            value, prop_name
        ))
    })
}

const PROP_KEYS: &[&str] = &[
    "gap", "padding", "border", "justify", "align", "fg", "bg", "bold", "margin",
];

fn is_props_table(t: &Table) -> LuaResult<bool> {
    for key in PROP_KEYS {
        if t.contains_key(*key)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn child_type_error(position: usize, type_name: &str, hint: &str) -> mlua::Error {
    mlua::Error::RuntimeError(format!(
        "child at position {} has type '{}'. {}",
        position, type_name, hint
    ))
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
        Ok(LuaNode(fragment(collect_child_nodes(children.into_iter()))))
    })?;
    oil.set("fragment", fragment_fn)?;

    // cru.oil.when(condition, node)
    let when_fn = lua.create_function(|_, (condition, node): (bool, LuaNode)| {
        Ok(LuaNode(when(condition, node.0)))
    })?;
    oil.set("when", when_fn)?;

    // cru.oil.either(condition, true_node, false_node) - also aliased as if_else
    let either_fn = lua.create_function(|_, (cond, t, f): (bool, LuaNode, LuaNode)| {
        Ok(LuaNode(if_else(cond, t.0, f.0)))
    })?;
    oil.set("either", either_fn.clone())?;
    oil.set("if_else", either_fn)?;

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
        Ok(LuaNode(bullet_list(table_to_string_list(&items))))
    })?;
    oil.set("bullet_list", bullet_list_fn)?;

    // cru.oil.numbered_list(items)
    let numbered_list_fn = lua.create_function(|_, items: Table| {
        Ok(LuaNode(numbered_list(table_to_string_list(&items))))
    })?;
    oil.set("numbered_list", numbered_list_fn)?;

    // cru.oil.kv(key, value)
    let kv_fn = lua
        .create_function(|_, (key, value): (String, String)| Ok(LuaNode(key_value(key, value))))?;
    oil.set("kv", kv_fn)?;

    // cru.oil.markup(markup_string) - Parse XML-like markup into nodes
    let markup_fn = lua.create_function(|_, markup: String| {
        let node = html_to_node(&markup).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        Ok(LuaNode(node))
    })?;
    oil.set("markup", markup_fn)?;

    // cru.oil.component(base_fn, default_props) -> callable that merges props
    let component_fn = lua.create_function(|lua, (base_fn, defaults): (Function, Table)| {
        let wrapper = lua.create_function(move |lua, args: MultiValue| {
            let args_vec: Vec<Value> = args.into_iter().collect();
            let mut merged_args = Vec::new();

            let (user_props, rest) = if let Some(Value::Table(t)) = args_vec.first() {
                let merged = lua.create_table()?;
                for pair in defaults.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    merged.set(k, v)?;
                }
                for pair in t.pairs::<Value, Value>() {
                    let (k, v) = pair?;
                    merged.set(k, v)?;
                }
                (Some(merged), &args_vec[1..])
            } else {
                (Some(defaults.clone()), args_vec.as_slice())
            };

            if let Some(props) = user_props {
                merged_args.push(Value::Table(props));
            }
            merged_args.extend(rest.iter().cloned());

            base_fn.call::<LuaNode>(MultiValue::from_iter(merged_args))
        })?;
        Ok(wrapper)
    })?;
    oil.set("component", component_fn)?;

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
        Ok(LuaNode(scrollback(key, collect_child_nodes(args_iter))))
    })?;
    oil.set("scrollback", scrollback_fn)?;

    // cru.oil.decrypt(content, revealed, frame?)
    // Renders text with a movie-style decrypt/scramble animation effect
    let decrypt_fn = lua.create_function(
        |_, (content, revealed, frame): (String, usize, Option<usize>)| {
            let frame = frame.unwrap_or(0);
            Ok(LuaNode(crucible_oil::decrypt_text(
                &content, revealed, frame,
            )))
        },
    )?;
    oil.set("decrypt", decrypt_fn)?;

    register_in_namespaces(lua, "oil", oil)?;

    Ok(())
}

fn get_bool_prop(table: &Table, key: &str) -> LuaResult<bool> {
    match table.get::<Value>(key) {
        Ok(Value::Boolean(b)) => Ok(b),
        Ok(Value::Nil) | Err(_) => Ok(false),
        Ok(other) => Err(mlua::Error::RuntimeError(format!(
            "style property '{}' must be a boolean, got {}",
            key,
            other.type_name()
        ))),
    }
}

fn parse_style_from_table(table: &Table) -> LuaResult<Style> {
    let mut style = Style::default();

    if let Ok(fg) = table.get::<String>("fg") {
        style.fg = Some(parse_color_with_error(&fg, "fg")?);
    }
    if let Ok(bg) = table.get::<String>("bg") {
        style.bg = Some(parse_color_with_error(&bg, "bg")?);
    }

    style.bold = get_bool_prop(table, "bold")?;
    style.dim = get_bool_prop(table, "dim")?;
    style.italic = get_bool_prop(table, "italic")?;
    style.underline = get_bool_prop(table, "underline")?;

    Ok(style)
}

fn parse_container_args(_lua: &Lua, args: MultiValue) -> LuaResult<(Option<Table>, Vec<Node>)> {
    let args_vec: Vec<Value> = args.into_iter().collect();
    let mut children = Vec::new();
    let mut opts = None;

    for (i, arg) in args_vec.into_iter().enumerate() {
        match arg {
            Value::Table(t) if i == 0 && is_props_table(&t)? => {
                opts = Some(t);
            }
            Value::Table(_) => {
                return Err(child_type_error(
                    i + 1,
                    "table",
                    "Use oil.col(), oil.row(), etc. to create child nodes",
                ));
            }
            Value::UserData(ud) => {
                children.push(
                    ud.borrow::<LuaNode>()
                        .map_err(|_| {
                            child_type_error(
                                i + 1,
                                "userdata",
                                "Use oil.text(), oil.col(), etc. to create nodes",
                            )
                        })?
                        .0
                        .clone(),
                );
            }
            Value::String(s) => {
                children.push(text(s.to_str()?.to_string()));
            }
            Value::Nil => {}
            Value::Boolean(_) | Value::Integer(_) | Value::Number(_) => {
                return Err(child_type_error(
                    i + 1,
                    arg.type_name(),
                    "Wrap primitives with oil.text() to display them",
                ));
            }
            Value::Function(_) => {
                return Err(child_type_error(
                    i + 1,
                    "function",
                    "Did you forget to call it? Use fn() instead of fn",
                ));
            }
            _ => {
                return Err(child_type_error(i + 1, arg.type_name(), "Unsupported type"));
            }
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
        assert!(oil.contains_key("if_else").unwrap());
        assert!(oil.contains_key("each").unwrap());
        assert!(oil.contains_key("markup").unwrap());
        assert!(oil.contains_key("component").unwrap());
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
    fn test_oil_markup() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(r#"return cru.oil.markup('<div gap="2"><p>Hello</p><p>World</p></div>')"#)
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

    #[test]
    fn test_oil_if_else_alias() {
        let lua = setup_lua();

        let result_true: LuaNode = lua
            .load(r#"return cru.oil.if_else(true, cru.oil.text("yes"), cru.oil.text("no"))"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result_true.0 {
            assert_eq!(t.content, "yes");
        } else {
            panic!("Expected Text node");
        }

        let result_false: LuaNode = lua
            .load(r#"return cru.oil.if_else(false, cru.oil.text("yes"), cru.oil.text("no"))"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result_false.0 {
            assert_eq!(t.content, "no");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_component_with_defaults() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                local Card = cru.oil.component(cru.oil.col, {padding = 2, border = "rounded"})
                return Card({gap = 1}, cru.oil.text("Title"), cru.oil.text("Body"))
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.padding, Padding::all(2));
            assert_eq!(b.border, Some(Border::Rounded));
            assert_eq!(b.gap, Gap::all(1));
            assert_eq!(b.children.len(), 2);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_component_without_user_props() {
        let lua = setup_lua();

        let result: LuaNode = lua
            .load(
                r#"
                local Card = cru.oil.component(cru.oil.col, {padding = 1, gap = 2})
                return Card(cru.oil.text("Child"))
            "#,
            )
            .eval()
            .unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.padding, Padding::all(1));
            assert_eq!(b.gap, Gap::all(2));
            assert_eq!(b.children.len(), 1);
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_error_invalid_color() {
        let lua = setup_lua();

        let result = lua
            .load(r#"return cru.oil.text("hello", {fg = "invalid_color"})"#)
            .eval::<LuaNode>();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid color"),
            "Error should mention invalid color: {}",
            err
        );
        assert!(
            err.contains("invalid_color"),
            "Error should include the bad value: {}",
            err
        );
    }

    #[test]
    fn test_oil_error_primitive_child() {
        let lua = setup_lua();

        let result = lua.load(r#"return cru.oil.col(42)"#).eval::<LuaNode>();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("oil.text()"),
            "Error should suggest wrapping: {}",
            err
        );
    }

    #[test]
    fn test_oil_error_function_child() {
        let lua = setup_lua();

        let result = lua
            .load(r#"return cru.oil.col(function() end)"#)
            .eval::<LuaNode>();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("function"),
            "Error should mention function: {}",
            err
        );
        assert!(
            err.contains("call"),
            "Error should suggest calling it: {}",
            err
        );
    }

    #[test]
    fn test_oil_decrypt() {
        let lua = setup_lua();

        // Basic usage - partial reveal
        let result: LuaNode = lua
            .load(r#"return cru.oil.decrypt("hello", 2, 0)"#)
            .eval()
            .unwrap();

        // Should return a row with children (mix of revealed and scrambled)
        assert!(matches!(result.0, Node::Box(_)));
    }

    #[test]
    fn test_oil_decrypt_fully_revealed() {
        let lua = setup_lua();

        // Fully revealed - should return plain text
        let result: LuaNode = lua
            .load(r#"return cru.oil.decrypt("hello", 10, 0)"#)
            .eval()
            .unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "hello");
        } else {
            panic!("Expected Text node for fully revealed content");
        }
    }

    #[test]
    fn test_oil_decrypt_default_frame() {
        let lua = setup_lua();

        // Frame parameter is optional, defaults to 0
        let result: LuaNode = lua
            .load(r#"return cru.oil.decrypt("test", 1)"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Box(_)));
    }
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crucible_oil::render_to_string;
    use proptest::prelude::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        register_oil_module(&lua).expect("Should register oil module");
        lua
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn lua_text_nodes_render_without_panic(
            content in "[a-zA-Z0-9 ]{0,50}",
            bold in any::<bool>(),
            width in 20usize..120,
        ) {
            let lua = setup_lua();
            let bold_str = if bold { "true" } else { "false" };
            let script = format!(
                r#"return cru.oil.text("{}", {{bold = {}}})"#,
                content.replace('\\', "\\\\").replace('"', "\\\""),
                bold_str
            );

            if let Ok(node) = lua.load(&script).eval::<LuaNode>() {
                let _ = render_to_string(&node.0, width);
            }
        }

        #[test]
        fn lua_col_nodes_render_without_panic(
            texts in prop::collection::vec("[a-zA-Z0-9]{0,20}", 0..5),
            gap in 0u16..4,
            width in 20usize..120,
        ) {
            let lua = setup_lua();
            let children: Vec<String> = texts
                .iter()
                .map(|t| format!(r#"cru.oil.text("{}")"#, t))
                .collect();
            let script = format!(
                r#"return cru.oil.col({{gap = {}}}, {})"#,
                gap,
                children.join(", ")
            );

            if let Ok(node) = lua.load(&script).eval::<LuaNode>() {
                let _ = render_to_string(&node.0, width);
            }
        }

        #[test]
        fn lua_row_with_spacer_renders_without_panic(
            left in "[a-zA-Z]{0,10}",
            right in "[a-zA-Z]{0,10}",
            width in 20usize..120,
        ) {
            let lua = setup_lua();
            let script = format!(
                r#"return cru.oil.row(cru.oil.text("{}"), cru.oil.spacer(), cru.oil.text("{}"))"#,
                left, right
            );

            if let Ok(node) = lua.load(&script).eval::<LuaNode>() {
                let _ = render_to_string(&node.0, width);
            }
        }

        #[test]
        fn lua_nested_layout_renders_without_panic(
            depth in 1usize..4,
            width in 40usize..120,
        ) {
            let lua = setup_lua();

            let mut script = String::from(r#"cru.oil.text("leaf")"#);
            for i in 0..depth {
                let container = if i % 2 == 0 { "col" } else { "row" };
                script = format!(r#"cru.oil.{}({{gap = 1}}, {})"#, container, script);
            }
            script = format!("return {}", script);

            if let Ok(node) = lua.load(&script).eval::<LuaNode>() {
                let _ = render_to_string(&node.0, width);
            }
        }

        #[test]
        fn lua_conditional_nodes_render_without_panic(
            condition in any::<bool>(),
            text in "[a-zA-Z]{0,20}",
            width in 20usize..80,
        ) {
            let lua = setup_lua();
            let script = format!(
                r#"return cru.oil.when({}, cru.oil.text("{}"))"#,
                condition, text
            );

            if let Ok(node) = lua.load(&script).eval::<LuaNode>() {
                let _ = render_to_string(&node.0, width);
            }
        }
    }
}
