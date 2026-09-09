//! Lua oil module for building TUI nodes
//!
//! Provides `cru.oil.*` functions for constructing UI nodes from Lua scripts.

use crate::error::LuaError;
use crate::host_registry::{LuauValue, Ns};
use crate::signature::LuaType;
use crucible_oil::template::html_to_node;
use crucible_oil::{
    action, badge, bullet_list, divider, fragment, if_else, key_value, numbered_list, popup,
    popup_item, progress_bar, spacer, spinner, styled, text, text_input, when, Border, BoxNode,
    Direction, Gap, Node, Padding, Style,
};
use mlua::{
    FromLua, Function, Lua, MultiValue, Result as LuaResult, Table, UserData, UserDataMethods,
    Value,
};
use std::collections::BTreeMap;

/// Pure conversions from Lua values to crucible_oil primitives.
///
/// Grouped into a submodule so the registrar body below stays focused on
/// wiring; callers use `parse::border(s)`, `parse::style_from_table(t)`, etc.
mod parse {
    use super::{LuaNode, Style};
    use crucible_oil::template::parse_color;
    use crucible_oil::{text, AlignItems, Border, JustifyContent, Node};
    use mlua::{Result as LuaResult, Table, Value};

    pub fn border(s: &str) -> Border {
        match s {
            "double" => Border::Double,
            "rounded" => Border::Rounded,
            "heavy" => Border::Heavy,
            _ => Border::Single,
        }
    }

    pub fn justify(s: &str) -> JustifyContent {
        match s.to_lowercase().replace('-', "_").as_str() {
            "end" => JustifyContent::End,
            "center" => JustifyContent::Center,
            "space_between" => JustifyContent::SpaceBetween,
            "space_around" => JustifyContent::SpaceAround,
            "space_evenly" => JustifyContent::SpaceEvenly,
            _ => JustifyContent::Start,
        }
    }

    pub fn align(s: &str) -> AlignItems {
        match s.to_lowercase().as_str() {
            "end" => AlignItems::End,
            "center" => AlignItems::Center,
            "stretch" => AlignItems::Stretch,
            _ => AlignItems::Start,
        }
    }

    pub fn color(value: &str, prop_name: &str) -> LuaResult<crucible_oil::Color> {
        parse_color(value).map_err(|_| {
            mlua::Error::RuntimeError(format!(
                "invalid color '{}' for '{}'. Use named colors (red, green, blue, yellow, \
                 cyan, magenta, white, black) or hex (#ff0000)",
                value, prop_name
            ))
        })
    }

    pub fn style_from_table(table: &Table) -> LuaResult<Style> {
        let mut style = Style::default();

        if let Ok(fg) = table.get::<String>("fg") {
            style.fg = Some(color(&fg, "fg")?);
        }
        if let Ok(bg) = table.get::<String>("bg") {
            style.bg = Some(color(&bg, "bg")?);
        }

        style.bold = bool_prop(table, "bold")?;
        style.dim = bool_prop(table, "dim")?;
        style.italic = bool_prop(table, "italic")?;
        style.underline = bool_prop(table, "underline")?;

        Ok(style)
    }

    pub fn extract_node(v: Value) -> Option<Node> {
        match v {
            Value::UserData(ud) => ud.borrow::<LuaNode>().ok().map(|n| n.0.clone()),
            Value::String(s) => s.to_str().ok().map(|s| text(s.to_string())),
            _ => None,
        }
    }

    pub fn collect_child_nodes(values: impl Iterator<Item = Value>) -> Vec<Node> {
        values.filter_map(extract_node).collect()
    }

    pub fn string_list(items: &Table) -> Vec<String> {
        items
            .pairs::<i64, String>()
            .filter_map(|r| r.ok().map(|(_, v)| v))
            .collect()
    }

    const PROP_KEYS: &[&str] = &[
        "gap", "padding", "border", "justify", "align", "fg", "bg", "bold", "margin",
    ];

    pub fn is_props_table(t: &Table) -> LuaResult<bool> {
        for key in PROP_KEYS {
            if t.contains_key(*key)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn bool_prop(table: &Table, key: &str) -> LuaResult<bool> {
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
}

fn child_type_error(position: usize, type_name: &str, hint: &str) -> mlua::Error {
    mlua::Error::RuntimeError(format!(
        "child at position {} has type '{}'. {}",
        position, type_name, hint
    ))
}

#[derive(Debug, Clone)]
pub struct LuaNode(pub Node);

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("with_style", |_lua, this, style_table: Table| {
            let style = parse::style_from_table(&style_table)?;
            Ok(LuaNode(this.0.clone().with_style(style)))
        });

        methods.add_method("with_padding", |_, this, padding: u16| {
            Ok(LuaNode(this.0.clone().with_padding(Padding::all(padding))))
        });

        methods.add_method("with_border", |_, this, border_type: Option<String>| {
            let border = border_type.as_deref().map_or(Border::Single, parse::border);
            Ok(LuaNode(this.0.clone().with_border(border)))
        });

        methods.add_method("with_margin", |_, this, margin: u16| {
            Ok(LuaNode(this.0.clone().with_margin(Padding::all(margin))))
        });

        methods.add_method("gap", |_, this, gap_val: u16| {
            Ok(LuaNode(this.0.clone().gap(Gap::all(gap_val))))
        });

        methods.add_method("justify", |_, this, justify_str: String| {
            Ok(LuaNode(
                this.0.clone().justify(parse::justify(&justify_str)),
            ))
        });

        methods.add_method("align", |_, this, align_str: String| {
            Ok(LuaNode(this.0.clone().align(parse::align(&align_str))))
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

/// A node is mlua userdata, and Luau has no name for one, so `Any` is as much
/// as the host can say in Rust. Every declaration below narrows it: to
/// `OilNode` where a function ANSWERS with a node, and to the wider
/// [`NODE_ARG`] where one is accepted, because `LuaNode::from_lua` also takes
/// a bare string and nil.
impl LuauValue for LuaNode {
    fn ty() -> LuaType {
        LuaType::Any
    }
}

/// What a node-shaped ARGUMENT really accepts, read off `LuaNode::from_lua`:
/// a node, a string it wraps in `cru.oil.text`, or nil for `Node::Empty`.
/// Wider than `OilNode`, which is what every function answers with.
const NODE_ARG: &str = "(OilNode | string)?";

/// The same set in a RETURN position, spelled with an explicit `nil`.
///
/// `-> (OilNode | string)?` is ambiguous: the trailing `?` binds to the
/// function type rather than to the return, so the callback itself becomes
/// optional. A union carrying `nil` says the same thing with no ambiguity —
/// and a callback that answers with a bare string or with nothing is
/// accepted by `LuaNode::from_lua`, so the declaration has to allow both.
const NODE_RESULT: &str = "(OilNode | string | nil)";

/// Register the oil module.
///
/// Every function declares its Luau type beside its closure, and `Ns` holds
/// the declaration to the Rust types at registration. See
/// [`crate::host_registry`]. `OilNode`, `OilStyle` and `OilProps` are declared
/// in `host_api::OIL_TYPES`, because Luau cannot name an mlua userdata and a
/// shape named once beats the same record written out twenty times.
pub fn register_oil_module(lua: &Lua) -> Result<(), LuaError> {
    let mut oil = Ns::new(lua, "cru.oil")?;

    // A number is accepted and stringified, so `cru.oil.text(count)` works
    // without a `tostring`. Anything else raises; nil and no argument alike
    // give an empty string.
    oil.func(
        "text",
        "(content: (string | number)?, style: OilStyle?) -> OilNode",
        |_lua, (content, style): (Value, Value)| {
            let content = match content {
                Value::String(s) => s.to_str()?.to_string(),
                Value::Integer(n) => n.to_string(),
                Value::Number(n) => n.to_string(),
                Value::Nil => String::new(),
                other => {
                    return Err(mlua::Error::FromLuaConversionError {
                        from: other.type_name(),
                        to: "string".to_string(),
                        message: Some("text content must be a string".to_string()),
                    });
                }
            };

            // A second argument that is not a table is IGNORED rather than
            // refused, which is why this reads a `Value` and not an
            // `Option<Table>`.
            let style = match style {
                Value::Table(t) => parse::style_from_table(&t)?,
                _ => Style::default(),
            };

            if style == Style::default() {
                Ok(LuaNode(text(content)))
            } else {
                Ok(LuaNode(styled(content, style)))
            }
        },
    )?;

    // A props table FIRST when there is one, then children. A `MultiValue`
    // closure reads its own positions, so NOTHING below is checked against
    // Rust — `parse_container_args` is the only thing that holds a caller to
    // it, and it raises with the offending position number. The declaration
    // is still the shape a plugin author reads, so it names what it can.
    //
    // Luau has no syntax for NAMING a variadic — `...children: OilNode` is a
    // parse error — so only the element type survives. The elements are the
    // children.
    //
    // The first position is props OR the first child, which is why it is a
    // union and not `props: OilProps?`. `cru.oil.row(a, b, c)` — no props at
    // all — is the common call, and declaring the first parameter as props
    // made `luau-lsp analyze` reject it. Luau spells "either shape" as an
    // overload, which `Ns::func` cannot carry, so the union says it instead.
    //
    // A table there is read as props only when it carries one of
    // `is_props_table`'s keys, which are
    // `gap padding border justify align fg bg bold margin`. `dim`, `italic`
    // and `underline` are read as style once a table qualifies, but a table
    // carrying ONLY those is taken for a child and REFUSED — which is why the
    // union's table arm is `OilProps` and not `table`.
    let container =
        format!("(props_or_first_child: (OilProps | OilNode | string)?, ...{NODE_ARG}) -> OilNode");
    let container = container.as_str();

    oil.func("col", container, |lua, args: MultiValue| {
        let (opts, children) = parse_container_args(lua, args)?;
        Ok(LuaNode(create_box_node(Direction::Column, opts, children)))
    })?;

    oil.func("row", container, |lua, args: MultiValue| {
        let (opts, children) = parse_container_args(lua, args)?;
        Ok(LuaNode(create_box_node(Direction::Row, opts, children)))
    })?;

    oil.func("spacer", "() -> OilNode", |_, ()| Ok(LuaNode(spacer())))?;

    // The one node that carries behaviour rather than appearance. See
    // `crucible_oil::Node::Action` for why activation is declared on the tree
    // instead of registered beside it, as TUI focus is.
    //
    // `params` is string-to-string. A view dispatches an identifier back to
    // its own plugin, and widening it to arbitrary JSON would put a second
    // serialization contract in the node vocabulary for no case yet seen.
    oil.func(
        "action",
        &format!("(action: string, params: {{ [string]: string }}?, child: {NODE_ARG}) -> OilNode"),
        |_, (name, params, child): (String, Option<Table>, Value)| {
            let child = parse::extract_node(child).ok_or_else(|| {
                mlua::Error::RuntimeError(
                    "oil.action: third argument must be a node or a string".to_string(),
                )
            })?;
            let params: BTreeMap<String, String> = match params {
                Some(t) => t.pairs::<String, String>().filter_map(Result::ok).collect(),
                None => BTreeMap::new(),
            };
            Ok(LuaNode(action(name, params, child)))
        },
    )?;

    oil.func(
        "spinner",
        "(label: string?) -> OilNode",
        |_, label: Option<String>| Ok(LuaNode(spinner(label, 0))),
    )?;

    // The declaration is STRICTER than the closure, deliberately. A child
    // that is neither a node nor a string is DROPPED here rather than
    // refused — unlike `col` and `row`, which name the offending position —
    // so `luau-lsp` reporting it is the only warning an author gets before
    // the element silently fails to render.
    oil.func(
        "fragment",
        &format!("(...{NODE_ARG}) -> OilNode"),
        |_, children: MultiValue| {
            Ok(LuaNode(fragment(parse::collect_child_nodes(
                children.into_iter(),
            ))))
        },
    )?;

    oil.func(
        "when",
        &format!("(condition: boolean, node: {NODE_ARG}) -> OilNode"),
        |_, (condition, node): (bool, LuaNode)| Ok(LuaNode(when(condition, node.0))),
    )?;

    oil.func(
        "either",
        &format!("(condition: boolean, if_true: {NODE_ARG}, if_false: {NODE_ARG}) -> OilNode"),
        |_, (condition, if_true, if_false): (bool, LuaNode, LuaNode)| {
            Ok(LuaNode(if_else(condition, if_true.0, if_false.0)))
        },
    )?;

    // The array half of `items` only: the closure iterates `pairs::<i64, _>`,
    // so a string key is never visited. `render` is called with the item and
    // its 1-based index, in that order, and must answer with a node.
    oil.func(
        "each",
        &format!(
            "(items: {{ any }}, render: (item: any, index: number) -> {NODE_RESULT}) -> OilNode"
        ),
        |_, (items, render): (Table, Function)| {
            let mut children = Vec::new();
            for pair in items.pairs::<i64, Value>() {
                let (idx, item) = pair?;
                let result: LuaNode = render.call((item, idx))?;
                children.push(result.0);
            }
            Ok(LuaNode(fragment(children)))
        },
    )?;

    // Table-driven state dispatch. A handler is a node, a string, or a
    // function of no arguments answering with one; `_` is the default. A
    // missing key with no `_` gives an empty node rather than raising, so a
    // state nobody drew yet renders as nothing.
    //
    // The handler VALUE is one of three things, and the union states it
    // rather than a comment: `luau-lsp analyze` accepts a union of a node, a
    // string and a thunk inside an index signature — checked against the real
    // analyzer, not assumed.
    oil.func(
        "match_state",
        &format!("(state: any, handlers: {{ [any]: OilNode | string | (() -> {NODE_RESULT}) }}) -> OilNode"),
        |ctx, (state, handlers): (Value, Table)| {
            let handler: Value = handlers.get(state.clone()).unwrap_or(Value::Nil);
            match handler {
                Value::Nil => {
                    let default: Value = handlers.get("_").unwrap_or(Value::Nil);
                    match default {
                        Value::Function(f) => f.call::<LuaNode>(()),
                        Value::Nil => Ok(LuaNode(Node::Empty)),
                        v => LuaNode::from_lua(v, ctx),
                    }
                }
                Value::Function(f) => f.call::<LuaNode>(()),
                v => LuaNode::from_lua(v, ctx),
            }
        },
    )?;

    // `focused` defaults to TRUE, which `text_input` sets and this only
    // overrides. An options table that omits it gives a focused input.
    oil.func(
        "input",
        "(options: { value: string?, cursor: number?, placeholder: string?, \
         focused: boolean? }?) -> OilNode",
        |_, options: Option<Table>| {
            let mut value = String::new();
            let mut cursor = 0;
            let mut placeholder = None;
            let mut focused = true;

            if let Some(t) = options {
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
        },
    )?;

    // An item is a plain string, or a table. `label` is read with
    // `unwrap_or_default`, so a table without one becomes an empty label
    // rather than an error — hence `label: string?`. An item of any other
    // type is dropped.
    oil.func(
        "popup",
        "(items: { string | { label: string?, desc: string?, kind: string? } }, \
         selected: number?, max_visible: number?) -> OilNode",
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

    // Only the FIRST character of `character` is used; the default is `─`,
    // and the default width is 80 columns.
    oil.func(
        "divider",
        "(character: string?, width: number?) -> OilNode",
        |_, (character, width): (Option<String>, Option<u16>)| {
            let ch = character.and_then(|s| s.chars().next()).unwrap_or('─');
            Ok(LuaNode(divider(ch, width.unwrap_or(80))))
        },
    )?;

    // A FRACTION, not a percentage: `progress_bar` clamps it to 0.0..=1.0,
    // so `cru.oil.progress(50)` draws a full bar rather than half of one.
    oil.func(
        "progress",
        "(fraction: number, width: number?) -> OilNode",
        |_, (fraction, width): (f64, Option<u16>)| {
            Ok(LuaNode(progress_bar(fraction as f32, width.unwrap_or(20))))
        },
    )?;

    oil.func(
        "badge",
        "(label: string, style: OilStyle?) -> OilNode",
        |_, (label, style): (String, Option<Table>)| {
            let style = style
                .map(|t| parse::style_from_table(&t))
                .transpose()?
                .unwrap_or_default();
            Ok(LuaNode(badge(label, style)))
        },
    )?;

    // The array half only, and a non-string entry is skipped rather than
    // stringified (`parse::string_list`).
    oil.func(
        "bullet_list",
        "(items: { string }) -> OilNode",
        |_, items: Table| Ok(LuaNode(bullet_list(parse::string_list(&items)))),
    )?;

    oil.func(
        "numbered_list",
        "(items: { string }) -> OilNode",
        |_, items: Table| Ok(LuaNode(numbered_list(parse::string_list(&items)))),
    )?;

    oil.func(
        "kv",
        "(key: string, value: string) -> OilNode",
        |_, (key, value): (String, String)| Ok(LuaNode(key_value(key, value))),
    )?;

    // Raises on malformed markup rather than answering with an empty node.
    oil.func(
        "markup",
        "(markup: string) -> OilNode",
        |_, markup: String| {
            let node =
                html_to_node(&markup).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            Ok(LuaNode(node))
        },
    )?;

    // Answers with a function that merges `defaults` UNDER the caller's own
    // first-argument props table, then forwards the rest to `base`. A first
    // argument that is not a table is treated as a child, not as props.
    oil.func(
        "component",
        &format!(
            "(base: (...any) -> {NODE_RESULT}, defaults: {{ [string]: any }}) \
             -> ((...any) -> OilNode)"
        ),
        |lua, (base, defaults): (Function, Table)| {
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

                base.call::<LuaNode>(MultiValue::from_iter(merged_args))
            })?;
            Ok(wrapper)
        },
    )?;

    // scrollback was removed — kept as a fragment for backward compatibility.
    // `key` is the old scroll key and is DISCARDED; everything after it is a
    // child.
    oil.func(
        "scrollback",
        &format!("(key: any, ...{NODE_ARG}) -> OilNode"),
        |_, args: MultiValue| {
            let mut args_iter = args.into_iter();
            let _key = args_iter.next();
            Ok(LuaNode(fragment(parse::collect_child_nodes(args_iter))))
        },
    )?;

    oil.publish()?;

    Ok(())
}

fn parse_container_args(_lua: &Lua, args: MultiValue) -> LuaResult<(Option<Table>, Vec<Node>)> {
    let args_vec: Vec<Value> = args.into_iter().collect();
    let mut children = Vec::new();
    let mut opts = None;

    for (i, arg) in args_vec.into_iter().enumerate() {
        match arg {
            Value::Table(t) if i == 0 && parse::is_props_table(&t)? => {
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
            node.border = Some(parse::border(&border_str));
        } else if t.get::<bool>("border").unwrap_or(false) {
            node.border = Some(Border::Single);
        }
        if let Ok(justify) = t.get::<String>("justify") {
            node.justify = parse::justify(&justify);
        }
        if let Ok(align) = t.get::<String>("align") {
            node.align = parse::align(&align);
        }
        if let Ok(style) = parse::style_from_table(&t) {
            node.style = style;
        }
    }

    Node::Box(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;
    use crucible_oil::{Color, Size};

    /// Every declaration this module registers is Luau the parser accepts.
    ///
    /// These become one `declare cru: { … }` file that `luau-lsp analyze`
    /// loads for every plugin. A malformed type there is not one bad
    /// completion: the file fails to load, and every plugin loses every
    /// declaration at once. Nothing else in the tree runs a Luau parser over
    /// them — the stub tests only assert the generator produced text — so
    /// this does.
    ///
    /// `cru.oil.popup`'s `{ string | { … } }` and `cru.oil.component`'s
    /// function-returning-function are the two that were written on a guess
    /// about Luau's grammar. The `OilNode` aliases go in first, so the test
    /// covers those too and a declaration cannot name a type nobody exports.
    #[test]
    fn every_declaration_is_luau_the_parser_accepts() {
        let lua = TestLuaBuilder::new().with_oil().build();
        let signatures = crate::host_registry::HostSignatures::of(&lua);
        let paths: Vec<String> = signatures
            .paths()
            .into_iter()
            .filter(|path| path.starts_with("cru.oil."))
            .collect();
        assert_eq!(paths.len(), 22, "every cru.oil function must be declared");

        // `export` is for a definitions file; a plain chunk takes the aliases
        // bare, and the parser is the same either way.
        let aliases = crate::host_api::OIL_TYPES.replace("export type", "type");
        for path in paths {
            let declared = signatures.get(&path).expect("just listed").to_luau();
            let source = format!("{aliases}\ntype Probe = {declared}\nreturn 1");
            if let Err(e) = lua.load(&source).exec() {
                panic!("{path} is declared `{declared}`, which Luau refuses: {e}");
            }
        }
    }

    #[test]
    fn test_register_oil_module() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        assert!(oil.contains_key("markup").unwrap());
        assert!(oil.contains_key("component").unwrap());
        assert!(oil.contains_key("match_state").unwrap());
        // Removed: if_else (use either), hr (use divider), maybe.
        assert!(!oil.contains_key("if_else").unwrap());
        assert!(!oil.contains_key("hr").unwrap());
        assert!(!oil.contains_key("maybe").unwrap());
    }

    #[test]
    fn test_oil_text() {
        let lua = TestLuaBuilder::new().with_oil().build();

        let result: LuaNode = lua.load(r#"return cru.oil.text("hello")"#).eval().unwrap();

        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "hello");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_text_with_style() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

        let result: LuaNode = lua
            .load(r#"return cru.oil.when(true, cru.oil.text("visible"))"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Text(_)));
    }

    #[test]
    fn test_oil_when_false() {
        let lua = TestLuaBuilder::new().with_oil().build();

        let result: LuaNode = lua
            .load(r#"return cru.oil.when(false, cru.oil.text("hidden"))"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Empty));
    }

    #[test]
    fn test_oil_either() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
    fn test_oil_match_state_hit() {
        let lua = TestLuaBuilder::new().with_oil().build();
        let result: LuaNode = lua
            .load(
                r#"return cru.oil.match_state("loading", {loading = cru.oil.text("Loading...")})"#,
            )
            .eval()
            .unwrap();
        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "Loading...");
        } else {
            panic!("Expected Text node");
        }
    }

    #[test]
    fn test_oil_match_state_miss_with_default() {
        let lua = TestLuaBuilder::new().with_oil().build();
        let result: LuaNode = lua
            .load(r#"return cru.oil.match_state("unknown", {_ = cru.oil.text("default")})"#)
            .eval()
            .unwrap();
        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "default");
        } else {
            panic!("Expected default Text node");
        }
    }

    #[test]
    fn test_oil_match_state_miss_no_default() {
        let lua = TestLuaBuilder::new().with_oil().build();
        let result: LuaNode = lua
            .load(
                r#"return cru.oil.match_state("unknown", {loading = cru.oil.text("Loading...")})"#,
            )
            .eval()
            .unwrap();
        // Missing key + no _ handler → Node::Empty
        assert!(matches!(result.0, Node::Empty));
    }

    #[test]
    fn test_oil_match_state_function_handler() {
        let lua = TestLuaBuilder::new().with_oil().build();
        let result: LuaNode = lua
            .load(r#"return cru.oil.match_state("ready", {ready = function() return cru.oil.text("Ready!") end})"#)
            .eval()
            .unwrap();
        if let Node::Text(t) = result.0 {
            assert_eq!(t.content, "Ready!");
        } else {
            panic!("Expected Text node from function handler");
        }
    }

    #[test]
    fn test_oil_spacer() {
        let lua = TestLuaBuilder::new().with_oil().build();

        let result: LuaNode = lua.load(r#"return cru.oil.spacer()"#).eval().unwrap();

        if let Node::Box(b) = result.0 {
            assert_eq!(b.size, Size::Flex(1));
        } else {
            panic!("Expected Box node");
        }
    }

    #[test]
    fn test_oil_spinner() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

        let result: LuaNode = lua
            .load(r#"return cru.oil.progress(0.5, 10)"#)
            .eval()
            .unwrap();

        assert!(matches!(result.0, Node::Text(_)));
    }

    #[test]
    fn test_oil_divider() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
    fn test_oil_component_with_defaults() {
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
        let lua = TestLuaBuilder::new().with_oil().build();

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
}

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;
    use crucible_oil::render_to_string;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn lua_text_nodes_render_without_panic(
            content in "[a-zA-Z0-9 ]{0,50}",
            bold in any::<bool>(),
            width in 20usize..120,
        ) {
            let lua = TestLuaBuilder::new().with_oil().build();
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
            let lua = TestLuaBuilder::new().with_oil().build();
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
            let lua = TestLuaBuilder::new().with_oil().build();
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
            let lua = TestLuaBuilder::new().with_oil().build();

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
            let lua = TestLuaBuilder::new().with_oil().build();
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
