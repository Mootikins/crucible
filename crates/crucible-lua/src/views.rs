//! Plugin views: an Oil tree declared in Lua, drawn by every frontend.
//!
//! The sibling module [`crate::options`] proved this contract over one narrow
//! domain — the plugin describes, the frontend draws, and neither knows the
//! other's idiom. A *view* is the same contract over the whole Oil node
//! vocabulary rather than over eight widget kinds, which is what lets a plugin
//! ship a board, a table or a report instead of only a form.
//!
//! ```lua
//! cru.plugin.views({
//!   board = {
//!     render = function(params) return cru.oil.col(...) end,
//!     on_action = function(action, params) ... end,
//!   },
//! })
//! ```
//!
//! Two properties are carried over from options deliberately:
//!
//! * **`render` runs per request.** A view describes what is true on this box
//!   now, not what was true when the plugin loaded. It is the same reason an
//!   options `values` field may be a function.
//! * **The tree is kept live**, not converted once, because `render` closes
//!   over plugin state that a reload replaces.
//!
//! What options does NOT have, and this does, is a way back: a node wrapped in
//! `cru.oil.action` carries an action name and string params, and `dispatch`
//! delivers them to `on_action`. See [`crucible_oil::Node::Action`] for why
//! that affordance lives on the tree rather than beside it.

use crate::error::LuaError;
use crate::oil::LuaNode;
use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// One plugin's view table, kept as the live Lua value.
#[derive(Clone)]
struct PluginViews {
    lua: Lua,
    table: Table,
}

/// View tables by plugin name.
#[derive(Clone, Default)]
pub struct ViewRegistry {
    views: Arc<Mutex<HashMap<String, PluginViews>>>,
}

impl std::fmt::Debug for ViewRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.views.lock().map(|g| g.len()).unwrap_or(0);
        write!(f, "ViewRegistry({n} plugins)")
    }
}

impl ViewRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Plugin names that declared views, sorted.
    pub fn plugins(&self) -> Vec<String> {
        let Ok(g) = self.views.lock() else {
            return Vec::new();
        };
        let mut names: Vec<_> = g.keys().cloned().collect();
        names.sort();
        names
    }

    /// View names a plugin declared, sorted.
    pub fn view_names(&self, plugin: &str) -> Vec<String> {
        let Some((_, table)) = self.entry(plugin) else {
            return Vec::new();
        };
        let mut names: Vec<String> = table
            .pairs::<String, Value>()
            .filter_map(|r| r.ok().map(|(k, _)| k))
            .collect();
        names.sort();
        names
    }

    /// Drop a plugin's views. Called on reload, for the same reason options
    /// are dropped: the previous version's closures hold the previous
    /// version's state.
    pub fn release_plugin(&self, plugin: &str) {
        if let Ok(mut g) = self.views.lock() {
            g.remove(plugin);
        }
    }

    fn entry(&self, plugin: &str) -> Option<(Lua, Table)> {
        self.views
            .lock()
            .ok()?
            .get(plugin)
            .map(|v| (v.lua.clone(), v.table.clone()))
    }

    fn set_views(&self, plugin: &str, lua: Lua, table: Table) {
        if let Ok(mut g) = self.views.lock() {
            g.insert(plugin.to_string(), PluginViews { lua, table });
        }
    }

    fn view_table(&self, plugin: &str, view: &str) -> Result<(Lua, Table), String> {
        let (lua, table) = self.entry(plugin).ok_or("no such plugin")?;
        let entry: Value = table
            .get(view)
            .map_err(|e| format!("reading view '{view}': {e}"))?;
        match entry {
            Value::Table(t) => Ok((lua, t)),
            Value::Nil => Err(format!("no such view '{view}'")),
            other => Err(format!(
                "view '{view}' must be a table, got {}",
                other.type_name()
            )),
        }
    }

    /// Render one view to a serialized Oil node tree.
    ///
    /// `params` is handed through untouched. A view that takes none still gets
    /// an empty table rather than `nil`, so `params.foo` is always an index
    /// rather than an error on a missing argument.
    pub fn render(
        &self,
        plugin: &str,
        view: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let (lua, entry) = self.view_table(plugin, view)?;
        let render: mlua::Function = entry
            .get("render")
            .map_err(|_| format!("view '{view}' has no `render`"))?;
        let arg = params_table(&lua, params)?;
        let node: LuaNode = render
            .call(arg)
            .map_err(|e| format!("render failed: {e}"))?;
        serde_json::to_value(&node.0).map_err(|e| format!("serializing the tree: {e}"))
    }

    /// Deliver an action from a rendered tree back to the plugin.
    ///
    /// Returns nothing on purpose. A client re-renders after a successful
    /// dispatch rather than reading a return value, so a plugin has exactly one
    /// way to describe its state and no second, divergent one.
    pub fn dispatch(
        &self,
        plugin: &str,
        view: &str,
        action: &str,
        params: &serde_json::Value,
    ) -> Result<(), String> {
        let (lua, entry) = self.view_table(plugin, view)?;
        let handler: mlua::Function = entry
            .get("on_action")
            .map_err(|_| format!("view '{view}' has no `on_action`"))?;
        let arg = params_table(&lua, params)?;
        handler
            .call((action.to_string(), arg))
            .map_err(|e| format!("on_action failed: {e}"))
    }
}

/// A JSON object as a Lua table; anything else becomes an empty table.
fn params_table(lua: &Lua, params: &serde_json::Value) -> Result<Table, String> {
    if params.is_null() {
        return lua.create_table().map_err(|e| e.to_string());
    }
    match lua.to_value(params) {
        Ok(Value::Table(t)) => Ok(t),
        Ok(_) | Err(_) => lua.create_table().map_err(|e| e.to_string()),
    }
}

/// Register `cru.plugin.views` for one plugin's VM.
pub fn register_views_module(lua: &Lua, registry: ViewRegistry, plugin: String) -> LuaResult<()> {
    let views = lua.create_function(move |lua, table: Table| {
        validate(&table).map_err(|e| mlua::Error::runtime(format!("cru.plugin.views: {e}")))?;
        registry.set_views(&plugin, lua.clone(), table);
        Ok(())
    })?;
    crate::lua_util::get_or_create_module(lua, "plugin")?.set("views", views)?;
    Ok(())
}

/// Refuse a view table the frontends could not draw.
///
/// Refused here means refused at `setup()`, which leaves the plugin inert with
/// no views rather than registered with a view that errors on first render —
/// the same trade `cru.plugin.options` makes.
fn validate(table: &Table) -> Result<(), LuaError> {
    for pair in table.pairs::<String, Value>() {
        let (name, value) = pair.map_err(|e| LuaError::Runtime(e.to_string()))?;
        let Value::Table(entry) = value else {
            return Err(LuaError::Runtime(format!("view '{name}' must be a table")));
        };
        match entry.get::<Value>("render") {
            Ok(Value::Function(_)) => {}
            _ => {
                return Err(LuaError::Runtime(format!(
                    "view '{name}' needs a `render` function"
                )))
            }
        }
        // `on_action` is optional — a read-only view is a legitimate view —
        // but a non-function one is a typo, not a choice.
        match entry.get::<Value>("on_action") {
            Ok(Value::Function(_)) | Ok(Value::Nil) => {}
            _ => {
                return Err(LuaError::Runtime(format!(
                    "view '{name}': `on_action` must be a function"
                )))
            }
        }
    }
    Ok(())
}
