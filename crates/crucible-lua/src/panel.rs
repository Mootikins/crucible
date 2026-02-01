//! Interactive panel module for Lua scripts
//!
//! Provides the core `ui.panel()` primitive and convenience functions for
//! building interactive UI flows in Lua/Fennel.
//!
//! ## Lua Usage
//!
//! ```lua
//! local ui = require("ui")
//!
//! -- Basic panel with items
//! local result = ui.panel("Select database", {
//!     ui.item("PostgreSQL", "Full-featured RDBMS"),
//!     ui.item("SQLite", "Embedded, single-file"),
//!     ui.item("MongoDB", "Document store"),
//! })
//!
//! -- Panel with hints (filtering, multi-select)
//! local result = ui.panel("Select features", {
//!     ui.item("Auth"),
//!     ui.item("Logging"),
//!     ui.item("Caching"),
//! }, ui.hints():filterable():multi_select())
//!
//! -- Convenience functions
//! local yes = ui.confirm("Delete this file?")
//! local choice = ui.select("Pick one", {"A", "B", "C"})
//! local choices = ui.multi_select("Pick many", {"X", "Y", "Z"})
//! ```
//!
//! ## Fennel Usage
//!
//! ```fennel
//! (local ui (require :ui))
//!
//! ;; Basic panel
//! (local result (ui.panel "Select database"
//!                  [(ui.item "PostgreSQL" "Full-featured RDBMS")
//!                   (ui.item "SQLite" "Embedded")]))
//!
//! ;; With hints
//! (local hints (-> (ui.hints) (: :filterable) (: :multi_select)))
//! (local result (ui.panel "Select features" items hints))
//!
//! ;; Convenience
//! (when (ui.confirm "Proceed?")
//!   (do-something))
//! ```

use crate::error::LuaError;
use crucible_core::interaction::{InteractivePanel, PanelHints, PanelItem, PanelResult};
use mlua::{
    FromLua, Lua, MetaMethod, Result as LuaResult, Table, UserData, UserDataMethods, Value,
};

/// Lua wrapper for PanelHints with chainable methods
#[derive(Debug, Clone, Default)]
struct LuaPanelHints {
    inner: PanelHints,
}

impl FromLua for LuaPanelHints {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<LuaPanelHints>().map(|h| h.clone()),
            Value::Nil => Ok(LuaPanelHints::default()),
            _ => Err(mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "LuaPanelHints".to_string(),
                message: Some("expected PanelHints userdata or nil".to_string()),
            }),
        }
    }
}

impl UserData for LuaPanelHints {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("filterable", |_, this, ()| {
            let mut new = this.clone();
            new.inner.filterable = true;
            Ok(new)
        });

        methods.add_method("multi_select", |_, this, ()| {
            let mut new = this.clone();
            new.inner.multi_select = true;
            Ok(new)
        });

        methods.add_method("allow_other", |_, this, ()| {
            let mut new = this.clone();
            new.inner.allow_other = true;
            Ok(new)
        });

        methods.add_method("initial_selection", |_, this, indices: Vec<usize>| {
            let mut new = this.clone();
            new.inner.initial_selection = indices;
            Ok(new)
        });

        methods.add_method("initial_filter", |_, this, filter: String| {
            let mut new = this.clone();
            new.inner.initial_filter = Some(filter);
            Ok(new)
        });

        // Allow converting to table for inspection
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!(
                "PanelHints {{ filterable: {}, multi_select: {}, allow_other: {} }}",
                this.inner.filterable, this.inner.multi_select, this.inner.allow_other
            ))
        });
    }
}

/// Register the ui module with a Lua state
pub fn register_ui_module(lua: &Lua) -> Result<(), LuaError> {
    let ui = lua.create_table()?;

    // ui.item(label, description?) -> table
    let item_fn = lua.create_function(|lua, (label, description): (String, Option<String>)| {
        let item = lua.create_table()?;
        item.set("label", label)?;
        if let Some(desc) = description {
            item.set("description", desc)?;
        }
        Ok(item)
    })?;
    ui.set("item", item_fn)?;

    // ui.item_with_data(label, description, data) -> table
    let item_with_data_fn = lua.create_function(
        |lua, (label, description, data): (String, Option<String>, Value)| {
            let item = lua.create_table()?;
            item.set("label", label)?;
            if let Some(desc) = description {
                item.set("description", desc)?;
            }
            item.set("data", data)?;
            Ok(item)
        },
    )?;
    ui.set("item_with_data", item_with_data_fn)?;

    // ui.hints() -> LuaPanelHints (chainable)
    let hints_fn = lua.create_function(|_, ()| Ok(LuaPanelHints::default()))?;
    ui.set("hints", hints_fn)?;

    // ui.panel(header, items, hints?) -> table (InteractivePanel request)
    let panel_fn = lua.create_function(
        |lua, (header, items, hints): (String, Table, Option<LuaPanelHints>)| {
            let panel = lua.create_table()?;
            panel.set("header", header)?;
            panel.set("items", items)?;

            // Convert hints to table
            let hints_table = lua.create_table()?;
            let h = hints.map(|h| h.inner).unwrap_or_default();
            hints_table.set("filterable", h.filterable)?;
            hints_table.set("multi_select", h.multi_select)?;
            hints_table.set("allow_other", h.allow_other)?;
            if !h.initial_selection.is_empty() {
                let sel = lua.create_table()?;
                for (i, idx) in h.initial_selection.iter().enumerate() {
                    sel.set(i + 1, *idx)?;
                }
                hints_table.set("initial_selection", sel)?;
            }
            if let Some(filter) = h.initial_filter {
                hints_table.set("initial_filter", filter)?;
            }
            panel.set("hints", hints_table)?;

            Ok(panel)
        },
    )?;
    ui.set("panel", panel_fn)?;

    // ─────────────────────────────────────────────────────────────────────────
    // Convenience functions (built on top of panel)
    // ─────────────────────────────────────────────────────────────────────────

    // ui.confirm(message) -> table (panel request for yes/no)
    let confirm_fn = lua.create_function(|lua, message: String| {
        let panel = lua.create_table()?;
        panel.set("header", message)?;

        let items = lua.create_table()?;
        let yes = lua.create_table()?;
        yes.set("label", "Yes")?;
        items.set(1, yes)?;
        let no = lua.create_table()?;
        no.set("label", "No")?;
        items.set(2, no)?;
        panel.set("items", items)?;

        let hints = lua.create_table()?;
        hints.set("filterable", false)?;
        hints.set("multi_select", false)?;
        hints.set("allow_other", false)?;
        panel.set("hints", hints)?;

        Ok(panel)
    })?;
    ui.set("confirm", confirm_fn)?;

    // ui.select(header, choices) -> table (panel request for single selection)
    // choices can be strings or items
    let select_fn = lua.create_function(|lua, (header, choices): (String, Table)| {
        let panel = lua.create_table()?;
        panel.set("header", header)?;

        // Convert string choices to items if needed
        let items = lua.create_table()?;
        for (i, pair) in choices.pairs::<i64, Value>().enumerate() {
            let (_, value) = pair?;
            let item = match value {
                Value::String(s) => {
                    let t = lua.create_table()?;
                    t.set("label", s.to_str()?.to_string())?;
                    t
                }
                Value::Table(t) => t,
                _ => continue,
            };
            items.set(i + 1, item)?;
        }
        panel.set("items", items)?;

        let hints = lua.create_table()?;
        hints.set("filterable", false)?;
        hints.set("multi_select", false)?;
        hints.set("allow_other", false)?;
        panel.set("hints", hints)?;

        Ok(panel)
    })?;
    ui.set("select", select_fn)?;

    // ui.multi_select(header, choices) -> table (panel request for multi selection)
    let multi_select_fn = lua.create_function(|lua, (header, choices): (String, Table)| {
        let panel = lua.create_table()?;
        panel.set("header", header)?;

        // Convert string choices to items if needed
        let items = lua.create_table()?;
        for (i, pair) in choices.pairs::<i64, Value>().enumerate() {
            let (_, value) = pair?;
            let item = match value {
                Value::String(s) => {
                    let t = lua.create_table()?;
                    t.set("label", s.to_str()?.to_string())?;
                    t
                }
                Value::Table(t) => t,
                _ => continue,
            };
            items.set(i + 1, item)?;
        }
        panel.set("items", items)?;

        let hints = lua.create_table()?;
        hints.set("filterable", false)?;
        hints.set("multi_select", true)?;
        hints.set("allow_other", false)?;
        panel.set("hints", hints)?;

        Ok(panel)
    })?;
    ui.set("multi_select", multi_select_fn)?;

    // ui.search(header, items) -> table (panel request with filtering)
    let search_fn = lua.create_function(|lua, (header, items): (String, Table)| {
        let panel = lua.create_table()?;
        panel.set("header", header)?;
        panel.set("items", items)?;

        let hints = lua.create_table()?;
        hints.set("filterable", true)?;
        hints.set("multi_select", false)?;
        hints.set("allow_other", true)?;
        panel.set("hints", hints)?;

        Ok(panel)
    })?;
    ui.set("search", search_fn)?;

    // ─────────────────────────────────────────────────────────────────────────
    // Result constructors (for testing and scripted responses)
    // ─────────────────────────────────────────────────────────────────────────

    // ui.result_selected(indices) -> table
    let result_selected_fn = lua.create_function(|lua, indices: Vec<usize>| {
        let result = lua.create_table()?;
        result.set("cancelled", false)?;
        let sel = lua.create_table()?;
        for (i, idx) in indices.iter().enumerate() {
            sel.set(i + 1, *idx)?;
        }
        result.set("selected", sel)?;
        Ok(result)
    })?;
    ui.set("result_selected", result_selected_fn)?;

    // ui.result_other(text) -> table
    let result_other_fn = lua.create_function(|lua, text: String| {
        let result = lua.create_table()?;
        result.set("cancelled", false)?;
        result.set("other", text)?;
        result.set("selected", lua.create_table()?)?;
        Ok(result)
    })?;
    ui.set("result_other", result_other_fn)?;

    // ui.result_cancelled() -> table
    let result_cancelled_fn = lua.create_function(|lua, ()| {
        let result = lua.create_table()?;
        result.set("cancelled", true)?;
        result.set("selected", lua.create_table()?)?;
        Ok(result)
    })?;
    ui.set("result_cancelled", result_cancelled_fn)?;

    // Register as global module
    lua.globals().set("ui", ui)?;

    Ok(())
}

/// Convert a Lua panel item table to a crucible_core PanelItem
pub fn lua_item_to_core(item: &Table) -> LuaResult<PanelItem> {
    let label: String = item.get("label")?;
    let description: Option<String> = item.get("description").ok();

    let mut panel_item = PanelItem::new(label);
    if let Some(desc) = description {
        panel_item = panel_item.with_description(desc);
    }

    // Handle data field if present
    if let Ok(data) = item.get::<Value>("data") {
        if !matches!(data, Value::Nil) {
            if let Ok(json_data) = serde_json::to_value(&data) {
                panel_item = panel_item.with_data(json_data);
            }
        }
    }

    Ok(panel_item)
}

/// Convert a Lua panel request table to a crucible_core InteractivePanel
pub fn lua_panel_to_core(panel: &Table) -> LuaResult<InteractivePanel> {
    let header: String = panel.get("header")?;
    let items_table: Table = panel.get("items")?;

    let mut items = Vec::new();
    for pair in items_table.pairs::<i64, Table>() {
        let (_, item_table) = pair?;
        items.push(lua_item_to_core(&item_table)?);
    }

    let mut hints = PanelHints::default();
    if let Ok(hints_table) = panel.get::<Table>("hints") {
        hints.filterable = hints_table.get("filterable").unwrap_or(false);
        hints.multi_select = hints_table.get("multi_select").unwrap_or(false);
        hints.allow_other = hints_table.get("allow_other").unwrap_or(false);

        if let Ok(sel_table) = hints_table.get::<Table>("initial_selection") {
            let mut selection = Vec::new();
            for pair in sel_table.pairs::<i64, usize>() {
                let (_, idx) = pair?;
                selection.push(idx);
            }
            hints.initial_selection = selection;
        }

        if let Ok(filter) = hints_table.get::<String>("initial_filter") {
            hints.initial_filter = Some(filter);
        }
    }

    Ok(InteractivePanel::new(header).items(items).hints(hints))
}

/// Convert a Lua panel result table to a crucible_core PanelResult
pub fn lua_result_to_core(result: &Table) -> LuaResult<PanelResult> {
    let cancelled: bool = result.get("cancelled").unwrap_or(false);

    if cancelled {
        return Ok(PanelResult::cancelled());
    }

    let other: Option<String> = result.get("other").ok();
    if let Some(text) = other {
        return Ok(PanelResult::other(text));
    }

    let mut selected = Vec::new();
    if let Ok(sel_table) = result.get::<Table>("selected") {
        for pair in sel_table.pairs::<i64, usize>() {
            let (_, idx) = pair?;
            selected.push(idx);
        }
    }

    Ok(PanelResult::selected(selected))
}

/// Convert a crucible_core PanelResult to a Lua table
pub fn core_result_to_lua(lua: &Lua, result: &PanelResult) -> LuaResult<Table> {
    let table = lua.create_table()?;
    table.set("cancelled", result.cancelled)?;

    let selected = lua.create_table()?;
    for (i, idx) in result.selected.iter().enumerate() {
        selected.set(i + 1, *idx)?;
    }
    table.set("selected", selected)?;

    if let Some(ref other) = result.other {
        table.set("other", other.clone())?;
    }

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_ui_module() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        // Verify ui table exists
        let ui: Table = lua.globals().get("ui").expect("ui should exist");
        assert!(ui.contains_key("item").unwrap());
        assert!(ui.contains_key("panel").unwrap());
        assert!(ui.contains_key("hints").unwrap());
        assert!(ui.contains_key("confirm").unwrap());
        assert!(ui.contains_key("select").unwrap());
        assert!(ui.contains_key("multi_select").unwrap());
        assert!(ui.contains_key("search").unwrap());
    }

    #[test]
    fn test_ui_item_from_lua() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.item("PostgreSQL", "Full-featured RDBMS")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let item = lua_item_to_core(&result).expect("Should convert");

        assert_eq!(item.label, "PostgreSQL");
        assert_eq!(item.description, Some("Full-featured RDBMS".to_string()));
    }

    #[test]
    fn test_ui_item_without_description() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.item("SQLite")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let item = lua_item_to_core(&result).expect("Should convert");

        assert_eq!(item.label, "SQLite");
        assert!(item.description.is_none());
    }

    #[test]
    fn test_ui_panel_from_lua() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.panel("Select database", {
                ui.item("PostgreSQL", "Full-featured"),
                ui.item("SQLite", "Embedded"),
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert_eq!(panel.header, "Select database");
        assert_eq!(panel.items.len(), 2);
        assert_eq!(panel.items[0].label, "PostgreSQL");
        assert_eq!(panel.items[1].label, "SQLite");
        assert!(!panel.hints.filterable);
        assert!(!panel.hints.multi_select);
    }

    #[test]
    fn test_ui_panel_with_hints() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.panel("Select features", {
                ui.item("Auth"),
                ui.item("Logging"),
            }, ui.hints():filterable():multi_select():allow_other())
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert!(panel.hints.filterable);
        assert!(panel.hints.multi_select);
        assert!(panel.hints.allow_other);
    }

    #[test]
    fn test_ui_confirm() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.confirm("Delete this file?")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert_eq!(panel.header, "Delete this file?");
        assert_eq!(panel.items.len(), 2);
        assert_eq!(panel.items[0].label, "Yes");
        assert_eq!(panel.items[1].label, "No");
    }

    #[test]
    fn test_ui_select_with_strings() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.select("Pick one", {"A", "B", "C"})
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert_eq!(panel.header, "Pick one");
        assert_eq!(panel.items.len(), 3);
        assert_eq!(panel.items[0].label, "A");
        assert_eq!(panel.items[1].label, "B");
        assert_eq!(panel.items[2].label, "C");
        assert!(!panel.hints.multi_select);
    }

    #[test]
    fn test_ui_multi_select() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.multi_select("Pick many", {"X", "Y", "Z"})
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert!(panel.hints.multi_select);
    }

    #[test]
    fn test_ui_search() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.search("Find note", {
                ui.item("Daily Note"),
                ui.item("Todo List"),
            })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert!(panel.hints.filterable);
        assert!(panel.hints.allow_other);
    }

    #[test]
    fn test_ui_result_selected() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.result_selected({0, 2})
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel_result = lua_result_to_core(&result).expect("Should convert");

        assert!(!panel_result.cancelled);
        assert_eq!(panel_result.selected, vec![0, 2]);
    }

    #[test]
    fn test_ui_result_other() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.result_other("custom input")
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel_result = lua_result_to_core(&result).expect("Should convert");

        assert!(!panel_result.cancelled);
        assert_eq!(panel_result.other, Some("custom input".to_string()));
    }

    #[test]
    fn test_ui_result_cancelled() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.result_cancelled()
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel_result = lua_result_to_core(&result).expect("Should convert");

        assert!(panel_result.cancelled);
    }

    #[test]
    fn test_ui_item_with_data() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            return ui.item_with_data("Item", "Desc", { id = 123, tags = {"a", "b"} })
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let item = lua_item_to_core(&result).expect("Should convert");

        assert_eq!(item.label, "Item");
        assert!(item.data.is_some());
    }

    #[test]
    fn test_hints_chaining() {
        let lua = Lua::new();
        register_ui_module(&lua).expect("Should register ui module");

        let script = r#"
            local h = ui.hints()
            h = h:filterable()
            h = h:multi_select()
            h = h:initial_selection({1, 3})
            return ui.panel("Test", {ui.item("A")}, h)
        "#;

        let result: Table = lua.load(script).eval().expect("Should execute");
        let panel = lua_panel_to_core(&result).expect("Should convert");

        assert!(panel.hints.filterable);
        assert!(panel.hints.multi_select);
        assert_eq!(panel.hints.initial_selection, vec![1, 3]);
    }

    #[test]
    fn test_core_result_to_lua() {
        let lua = Lua::new();

        let result = PanelResult::selected([0, 1]);
        let table = core_result_to_lua(&lua, &result).expect("Should convert");

        assert!(!table.get::<bool>("cancelled").unwrap());
        let selected: Table = table.get("selected").unwrap();
        assert_eq!(selected.get::<usize>(1).unwrap(), 0);
        assert_eq!(selected.get::<usize>(2).unwrap(), 1);
    }
}
