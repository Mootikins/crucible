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
use crate::statusline_items::{Anchor, StatusBarDef, StatusBars, StatusCond, StatusItem};
use mlua::{AnyUserData, Lua, MetaMethod, Table, UserData, UserDataMethods, Value};

/// A statusline item as seen from Lua.
#[derive(Clone)]
struct LuaItem(StatusItem);

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
    let text_fn =
        lua.create_function(|_, s: String| Ok(LuaItem(StatusItem::Text(s))))?;
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

    // sl.setup{...} — accepts either shape.
    let setup_fn = lua.create_function(|_, config: Table| {
        crate::config::set_status_bars(bars_from_setup_table(&config)?);
        Ok(())
    })?;
    statusline.set("setup", setup_fn)?;

    Ok(())
}

/// Parse a `setup{...}` table into named bars.
pub fn bars_from_setup_table(config: &Table) -> mlua::Result<StatusBars> {
    let mut bars = StatusBars::new();
    for pair in config.clone().pairs::<String, Table>() {
        let Ok((name, def)) = pair else { continue };

        let anchor_name: String = def
            .get("anchor")
            .unwrap_or_else(|_| "footer.below_input".to_string());
        let Some(anchor) = Anchor::from_name(&anchor_name) else {
            return Err(mlua::Error::RuntimeError(format!(
                "statusline bar '{name}': unknown anchor '{anchor_name}'"
            )));
        };

        let items = def
            .get::<Table>("items")
            .map(|t| items_from_table(&t))
            .unwrap_or_default();

        bars.insert(name, StatusBarDef { anchor, items });
    }
    Ok(bars)
}

/// Read a bar definition straight from a Lua table (used by tests and by any
/// caller that wants to parse without going through the global store).
pub fn bars_from_lua(config: &Table) -> StatusBars {
    let mut bars = StatusBars::new();
    for pair in config.clone().pairs::<String, Table>() {
        let Ok((name, def)) = pair else { continue };
        let anchor = def
            .get::<String>("anchor")
            .ok()
            .and_then(|a| Anchor::from_name(&a))
            .unwrap_or(Anchor::FooterBelowInput);
        let items = def
            .get::<Table>("items")
            .map(|t| items_from_table(&t))
            .unwrap_or_default();
        bars.insert(name, StatusBarDef { anchor, items });
    }
    bars
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

    fn bars(src: &str) -> StatusBars {
        let lua = lua_with_statusline();
        let table: Table = lua.load(src).eval().unwrap();
        bars_from_lua(&table)
    }

    #[test]
    fn a_bare_item_needs_no_call() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.mode, sl.context } } }"#,
        );
        assert_eq!(b["main"].items, vec![StatusItem::Mode, StatusItem::Context]);
    }

    #[test]
    fn strings_in_an_items_list_are_literal_text() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.mode, " | ", sl.context } } }"#,
        );
        assert_eq!(b["main"].items[1], StatusItem::Text(" | ".to_string()));
    }

    #[test]
    fn calling_an_item_configures_it() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.model{ max = 25 } } } }"#,
        );
        assert_eq!(
            b["main"].items[0],
            StatusItem::Model {
                max: Some(25),
                fallback: None
            }
        );
    }

    #[test]
    fn hl_wraps_an_item_without_changing_it() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.mode:hl("StatusMode") } } }"#,
        );
        assert_eq!(
            b["main"].items[0],
            StatusItem::Hl {
                group: "StatusMode".to_string(),
                item: Box::new(StatusItem::Mode),
            }
        );
    }

    /// The fallback that a format string could not express without a special
    /// case, and that plain Lua `or` cannot express because items are truthy.
    #[test]
    fn any_expresses_the_notification_else_context_fallback() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.any(sl.notification, sl.context) } } }"#,
        );
        assert_eq!(
            b["main"].items[0],
            StatusItem::Any(vec![StatusItem::Notification, StatusItem::Context])
        );
    }

    #[test]
    fn when_guards_an_item_on_a_tui_local_condition() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.when("streaming", sl.cache) } } }"#,
        );
        assert_eq!(
            b["main"].items[0],
            StatusItem::When {
                cond: StatusCond::Streaming,
                item: Box::new(StatusItem::Cache),
            }
        );
    }

    #[test]
    fn expr_items_carry_only_their_key() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.expr("git") } } }"#,
        );
        assert_eq!(
            b["main"].items[0],
            StatusItem::Expr {
                key: "git".to_string()
            }
        );
    }

    #[test]
    fn multiple_bars_can_be_defined_at_different_anchors() {
        let b = bars(
            r#"local sl = crucible.statusline
               return {
                 main = { anchor = "footer.below_input", items = { sl.mode } },
                 top  = { anchor = "top", items = { sl.model } },
               }"#,
        );
        assert_eq!(b["main"].anchor, Anchor::FooterBelowInput);
        assert_eq!(b["top"].anchor, Anchor::Top);
    }

    #[test]
    fn a_bar_without_an_anchor_defaults_to_the_usual_place() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.mode } } }"#,
        );
        assert_eq!(b["main"].anchor, Anchor::FooterBelowInput);
    }

    /// `spacer` was the old name; a config using it must keep working.
    #[test]
    fn spacer_is_an_alias_for_align() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.spacer, sl.align } } }"#,
        );
        assert_eq!(b["main"].items, vec![StatusItem::Align, StatusItem::Align]);
    }

    #[test]
    fn a_non_renderable_value_is_dropped_rather_than_failing_the_bar() {
        let b = bars(
            r#"local sl = crucible.statusline
               return { main = { items = { sl.mode, 42, sl.context } } }"#,
        );
        assert_eq!(
            b["main"].items,
            vec![StatusItem::Mode, StatusItem::Context]
        );
    }
}
