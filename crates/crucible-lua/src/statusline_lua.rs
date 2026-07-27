//! `crucible.statusline` — the Lua surface for item trees.
//!
//! Items are userdata so three things read naturally in one vocabulary:
//!
//! ```lua
//! sl.mode                    -- a bare item, no call needed
//! sl.model{ max = 25 }       -- the same item, configured
//! sl.mode:hl("StatusMode")   -- and styled
//! ```
//!
//! Making `sl.mode` a value rather than a function is what keeps a bar readable
//! as a list. `__call` supplies the configured form so `model` does not need a
//! different spelling from `mode`.

use crate::error::LuaError;
use crate::statusline_items::{Element, Layout, Region, StatusCond, StatusItem};
use mlua::{AnyUserData, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};

/// The shipped default layout, authored in Lua.
const DEFAULT_STATUSLINE_LUA: &str = include_str!("../../../runtime/statusline/default.lua");

/// A statusline item as seen from Lua.
#[derive(Clone)]
struct LuaItem(StatusItem);

/// `sl.input` — a placeholder for the editor itself.
///
/// Distinct from [`LuaItem`] because the input is not a status item: it is a
/// whole component with focus, a cursor and a multi-line border, so it can only
/// be a position in a region, never something a row composes.
#[derive(Clone, Copy)]
struct LuaInput;

impl UserData for LuaInput {}

impl UserData for LuaItem {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // item:hl("Group")
        methods.add_method("hl", |_, this, group: String| {
            Ok(LuaItem(StatusItem::Hl {
                group,
                item: Box::new(this.0.clone()),
            }))
        });

        // sl.model{ max = 25 } — configure an item that was already a value.
        //
        // The table is optional so `sl.mode()` also works. The pre-item API
        // spelled every component as a call (`statusline.mode()`), and configs
        // written that way must keep loading.
        methods.add_meta_method(MetaMethod::Call, |_, this, opts: Option<Table>| {
            let Some(opts) = opts else {
                return Ok(LuaItem(this.0.clone()));
            };
            Ok(LuaItem(match &this.0 {
                StatusItem::Model { .. } => StatusItem::Model {
                    max: opts.get("max").ok(),
                    fallback: opts.get("fallback").ok(),
                },
                other => other.clone(),
            }))
        });
    }
}

/// Coerce a Lua value in an `items` list into an item.
///
/// Bare strings become literal text, so `{ sl.mode, " ", sl.model }` reads the
/// way it looks. Anything else is dropped with a warning rather than failing the
/// whole bar.
fn value_to_item(v: &Value) -> Option<StatusItem> {
    match v {
        Value::String(s) => Some(StatusItem::Text(s.to_str().ok()?.to_string())),
        Value::UserData(ud) => ud.borrow::<LuaItem>().ok().map(|i| i.0.clone()),
        other => {
            tracing::warn!(
                "statusline item of type '{}' is not renderable; dropping it",
                other.type_name()
            );
            None
        }
    }
}

fn items_from_table(table: &Table) -> Vec<StatusItem> {
    table
        .clone()
        .sequence_values::<Value>()
        .filter_map(|v| v.ok().as_ref().and_then(value_to_item))
        .collect()
}

/// Register `crucible.statusline`'s item vocabulary and `setup`.
pub fn register_statusline_items(lua: &Lua, statusline: &Table) -> Result<(), LuaError> {
    // Bare item values.
    for (name, item) in [
        ("mode", StatusItem::Mode),
        (
            "model",
            StatusItem::Model {
                max: None,
                fallback: None,
            },
        ),
        ("context", StatusItem::Context),
        ("cache", StatusItem::Cache),
        ("status", StatusItem::Status),
        ("notification", StatusItem::Notification),
        ("align", StatusItem::Align),
        // `spacer` is the old name for the same thing; kept so existing configs
        // and muscle memory keep working.
        ("spacer", StatusItem::Align),
    ] {
        statusline.set(name, lua.create_userdata(LuaItem(item))?)?;
    }

    // sl.text("literal")
    let text_fn = lua.create_function(|_, s: String| Ok(LuaItem(StatusItem::Text(s))))?;
    statusline.set("text", text_fn)?;

    // sl.any(a, b, ...) — first non-empty wins.
    let any_fn = lua.create_function(|_, args: mlua::Variadic<Value>| {
        Ok(LuaItem(StatusItem::Any(
            args.iter().filter_map(value_to_item).collect(),
        )))
    })?;
    statusline.set("any", any_fn)?;

    // sl.when("streaming", item)
    let when_fn = lua.create_function(|_, (cond, item): (String, Value)| {
        let Some(cond) = StatusCond::from_name(&cond) else {
            return Err(mlua::Error::RuntimeError(format!(
                "unknown statusline condition '{cond}'"
            )));
        };
        let Some(item) = value_to_item(&item) else {
            return Err(mlua::Error::RuntimeError(
                "sl.when needs a renderable item".to_string(),
            ));
        };
        Ok(LuaItem(StatusItem::When {
            cond,
            item: Box::new(item),
        }))
    })?;
    statusline.set("when", when_fn)?;

    // sl.expr("key") — placed here, populated by a daemon-side provider.
    let expr_fn = lua.create_function(|_, key: String| Ok(LuaItem(StatusItem::Expr { key })))?;
    statusline.set("expr", expr_fn)?;

    // sl.input — the editor's position within the prompt region.
    statusline.set("input", LuaInput)?;

    let setup_fn = lua.create_function(|_, config: Table| {
        crate::config::set_layout(layout_from_setup_table(&config));
        Ok(())
    })?;
    statusline.set("setup", setup_fn)?;

    Ok(())
}

/// Read one region's list.
///
/// An entry is the input marker, a table of items (one row), or a bare item —
/// the last being sugar so a single-item row does not need braces.
fn region_from_lua(value: &Value) -> Vec<Element> {
    let Value::Table(list) = value else {
        return Vec::new();
    };

    list.clone()
        .sequence_values::<Value>()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            match &entry {
                Value::UserData(ud) if ud.borrow::<LuaInput>().is_ok() => Some(Element::Input),
                Value::Table(t) => Some(Element::Row(items_from_table(t))),
                other => value_to_item(other).map(|i| Element::Row(vec![i])),
            }
        })
        .collect()
}

/// Parse a `setup{...}` table into a layout.
///
/// A region the config does not mention keeps the built-in, so `setup{}` with
/// only `prompt` does not silently blank the rest of the screen.
pub fn layout_from_setup_table(config: &Table) -> Layout {
    let mut layout = crate::statusline_items::builtin_default();

    for region in [Region::Top, Region::Prompt, Region::Bottom] {
        if let Ok(value) = config.get::<Value>(region.name()) {
            if !matches!(value, Value::Nil) {
                *layout.region_mut(region) = region_from_lua(&value);
            }
        }
    }

    // A key that is not a region places nothing. Saying so matters more than it
    // looks: bars used to be named (`main = {...}`), so that spelling reads as
    // valid and would otherwise leave the author staring at the default
    // statusline with no clue why their config did nothing.
    for pair in config.clone().pairs::<Value, Value>().flatten() {
        if let Value::String(key) = pair.0 {
            let key = key.to_str().map(|s| s.to_string()).unwrap_or_default();
            if Region::from_name(&key).is_none() {
                tracing::warn!(
                    "statusline setup: '{key}' is not a region, so nothing was placed \
                     — expected one of top, prompt, bottom"
                );
            }
        }
    }

    layout
}

/// The default layout, as authored in Lua.
///
/// The layout Crucible ships with lives in `runtime/statusline/default.lua`
/// rather than in Rust, so it reads as configuration a user can copy. Parsing
/// needs a VM, so [`crate::statusline_items::builtin_default`] stays as the
/// compiled-in twin the TUI falls back to with no daemon; a test asserts the
/// two agree.
pub fn default_layout_from_lua() -> Result<Layout, LuaError> {
    let lua = Lua::new();
    let crucible = lua.create_table()?;
    let statusline = lua.create_table()?;
    register_statusline_items(&lua, &statusline)?;
    crucible.set("statusline", statusline)?;
    lua.globals().set("crucible", crucible)?;

    let table: Table = lua.load(DEFAULT_STATUSLINE_LUA).eval()?;
    Ok(layout_from_setup_table(&table))
}

/// Helper for tests: is this userdata a statusline item?
pub fn is_status_item(ud: &AnyUserData) -> bool {
    ud.borrow::<LuaItem>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lua_with_statusline() -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        let sl = lua.create_table().unwrap();
        register_statusline_items(&lua, &sl).unwrap();
        crucible.set("statusline", sl).unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    fn bars(src: &str) -> Layout {
        let lua = lua_with_statusline();
        let table: Table = lua.load(src).eval().unwrap();
        layout_from_setup_table(&table)
    }

    /// The first row of the prompt region, for tests that only care about items.
    fn first_row(layout: &Layout) -> &Vec<StatusItem> {
        layout
            .prompt
            .iter()
            .find_map(|e| match e {
                Element::Row(items) => Some(items),
                Element::Input => None,
            })
            .expect("a row was defined")
    }

    /// Position in the list is the arrangement — no anchor, no order field.
    #[test]
    fn the_prompt_region_keeps_the_order_it_was_written_in() {
        let layout = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.context }, sl.input, { sl.mode } } }"#,
        );

        assert_eq!(
            layout.prompt,
            vec![
                Element::Row(vec![StatusItem::Context]),
                Element::Input,
                Element::Row(vec![StatusItem::Mode]),
            ]
        );
    }

    /// Several rows below the input is the multi-row case that used to need an
    /// `order` integer to disambiguate.
    #[test]
    fn a_region_holds_several_rows_without_an_order_field() {
        let layout = bars(
            r#"local sl = crucible.statusline
               return { prompt = { sl.input, { sl.mode }, { sl.context }, { sl.cache } } }"#,
        );

        assert_eq!(layout.rows_below_input(), 3);
        assert_eq!(layout.prompt[0], Element::Input);
    }

    #[test]
    fn a_region_the_config_omits_keeps_the_builtin() {
        let layout = bars(
            r#"local sl = crucible.statusline
               return { top = { { sl.mode } } }"#,
        );

        assert_eq!(layout.top, vec![Element::Row(vec![StatusItem::Mode])]);
        assert_eq!(
            layout.prompt,
            crate::statusline_items::builtin_default().prompt,
            "omitting `prompt` must not blank the input"
        );
    }

    /// Sugar: a single item needs no braces around it.
    #[test]
    fn a_bare_item_is_a_row_of_one() {
        let layout = bars(
            r#"local sl = crucible.statusline
               return { top = { sl.mode } }"#,
        );
        assert_eq!(layout.top, vec![Element::Row(vec![StatusItem::Mode])]);
    }

    #[test]
    fn a_bare_item_needs_no_call() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.mode, sl.context } } }"#,
        );
        assert_eq!(first_row(&b), &vec![StatusItem::Mode, StatusItem::Context]);
    }

    #[test]
    fn strings_in_a_row_are_literal_text() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.mode, " | ", sl.context } } }"#,
        );
        assert_eq!(
            first_row(&b),
            &vec![
                StatusItem::Mode,
                StatusItem::Text(" | ".into()),
                StatusItem::Context
            ]
        );
    }

    #[test]
    fn calling_an_item_configures_it() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.model{ max = 12, fallback = "none" } } } }"#,
        );
        assert_eq!(
            first_row(&b),
            &vec![StatusItem::Model {
                max: Some(12),
                fallback: Some("none".into())
            }]
        );
    }

    #[test]
    fn hl_wraps_an_item_without_changing_it() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.mode:hl("StatusMode") } } }"#,
        );
        assert_eq!(
            first_row(&b),
            &vec![StatusItem::Hl {
                group: "StatusMode".into(),
                item: Box::new(StatusItem::Mode)
            }]
        );
    }

    #[test]
    fn any_expresses_the_notification_else_context_fallback() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.any(sl.notification, sl.context) } } }"#,
        );
        assert_eq!(
            first_row(&b),
            &vec![StatusItem::Any(vec![
                StatusItem::Notification,
                StatusItem::Context
            ])]
        );
    }

    #[test]
    fn when_guards_an_item_on_a_tui_local_condition() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.when("streaming", sl.cache) } } }"#,
        );
        assert_eq!(
            first_row(&b),
            &vec![StatusItem::When {
                cond: StatusCond::Streaming,
                item: Box::new(StatusItem::Cache)
            }]
        );
    }

    #[test]
    fn expr_items_carry_only_their_key() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.expr("git") } } }"#,
        );
        assert_eq!(first_row(&b), &vec![StatusItem::Expr { key: "git".into() }]);
    }

    #[test]
    fn spacer_is_an_alias_for_align() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.mode, sl.align, sl.context } } }"#,
        );
        assert_eq!(first_row(&b)[1], StatusItem::Align);
    }

    #[test]
    fn a_non_renderable_value_is_dropped_rather_than_failing_the_row() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { { sl.mode, 42, sl.context } } }"#,
        );
        assert_eq!(first_row(&b), &vec![StatusItem::Mode, StatusItem::Context]);
    }

    /// The shipped default exists twice: authored in
    /// `runtime/statusline/default.lua`, and compiled into the TUI so it renders
    /// with no daemon. Two copies drift, so this is the guard — edit one without
    /// the other and this fails rather than the two silently disagreeing about
    /// what a fresh install looks like.
    #[test]
    fn the_lua_default_and_the_compiled_in_default_agree() {
        let from_lua = default_layout_from_lua().expect("the shipped default parses");
        assert_eq!(from_lua, crate::statusline_items::builtin_default());
    }

    #[test]
    fn the_input_marker_parses_as_the_input_element() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { prompt = { sl.input } }"#,
        );
        assert_eq!(b.prompt, vec![Element::Input]);
    }
}
