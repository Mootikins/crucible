//! Plugin settings declared once, rendered by every frontend.
//!
//! A plugin returns a nested options table; the daemon walks it into JSON that
//! a TUI or a browser renders in its own idiom, and calls back into Lua when a
//! value is read, written, or a button is pressed.
//!
//! The shape is [Ace3's AceConfig-3.0], which has done exactly this since 2007:
//! one table feeds a GUI, a slash-command parser, and a dropdown, none of which
//! know about each other. The alternative — an imperative builder like
//! Obsidian's `new Setting(el).addToggle(...)` — works only when there is
//! exactly one renderer, because the plugin draws the widget itself. Crucible
//! has two, so the plugin describes and the frontend draws.
//!
//! [Ace3's AceConfig-3.0]: https://www.wowace.com/projects/ace3/pages/ace-config-3-0-options-tables
//!
//! ```lua
//! cru.plugin.options{
//!   type = "group",
//!   args = {
//!     image = {
//!       type = "input", name = "Image", order = 1,
//!       desc = "Image to run workspace tools in",
//!       get = function() return config.image end,
//!       set = function(_, v) config.image = v end,
//!     },
//!     runtime = {
//!       type = "select", name = "Runtime", order = 2,
//!       -- Evaluated at render: only runtimes actually installed are offered.
//!       values = function() return installed_runtimes() end,
//!     },
//!     rebuild = { type = "execute", name = "Rebuild image", func = rebuild },
//!   },
//! }
//! ```
//!
//! Two Ace3 properties are load-bearing and deliberately kept:
//!
//! * **Any field may be a function**, evaluated when the tree is read. That is
//!   what lets `values` list the runtimes present on *this* box, and what makes
//!   a separate "dynamic UI" API unnecessary.
//! * **`get`/`set`/`disabled`/`hidden` inherit toward the root**, so a plugin
//!   writes one accessor at the top and every leaf works. `false` breaks
//!   inheritance for a node that means it.

pub mod admit;
pub mod app_config;
pub mod control;
pub mod validate;

pub use control::Control;

use mlua::{Function, Lua, LuaSerdeExt, Result as LuaResult, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A plugin's options tree, kept as the live Lua table.
///
/// Stored rather than converted once because the table is *live*: a `values`
/// or `disabled` function must run when the settings are read, not when the
/// plugin loaded, or the answer describes a box state that has since changed.
#[derive(Clone)]
struct OptionsTree {
    /// Kept alongside the table because callbacks need somewhere to build the
    /// `info` argument, and mlua 0.12 offers no way back from a `Table` to its
    /// state.
    lua: Lua,
    table: Table,
}

/// Options trees by plugin name.
#[derive(Clone, Default)]
pub struct OptionsRegistry {
    trees: Arc<Mutex<HashMap<String, OptionsTree>>>,
}

impl std::fmt::Debug for OptionsRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.trees.lock().map(|g| g.len()).unwrap_or(0);
        write!(f, "OptionsRegistry({n} plugins)")
    }
}

impl OptionsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Plugin names that declared options, sorted.
    pub fn plugins(&self) -> Vec<String> {
        let Ok(g) = self.trees.lock() else {
            return Vec::new();
        };
        let mut names: Vec<_> = g.keys().cloned().collect();
        names.sort();
        names
    }

    /// Drop a plugin's tree. Called on reload: the previous version's
    /// accessors close over the previous version's state.
    pub fn release_plugin(&self, plugin: &str) {
        if let Ok(mut g) = self.trees.lock() {
            g.remove(plugin);
        }
    }

    fn tree(&self, plugin: &str) -> Option<(Lua, Table)> {
        self.trees
            .lock()
            .ok()?
            .get(plugin)
            .map(|t| (t.lua.clone(), t.table.clone()))
    }

    fn set_tree(&self, plugin: &str, lua: Lua, table: Table) {
        if let Ok(mut g) = self.trees.lock() {
            g.insert(plugin.to_string(), OptionsTree { lua, table });
        }
    }

    /// Render a plugin's tree to JSON for a client, evaluating every
    /// function-valued field against the current state.
    ///
    /// `ui` is the frontend asking ("tui" or "web"). It reaches callbacks as
    /// `info.uiType` and drives the per-frontend hide flags, so an option that
    /// makes no sense in one renderer can say so rather than being duplicated.
    pub fn describe(&self, plugin: &str, ui: &str) -> Option<serde_json::Value> {
        let (lua, table) = self.tree(plugin)?;
        describe_node(&lua, &table, plugin, &[], ui, &table).ok()
    }

    /// Read one option's current value.
    pub fn get(
        &self,
        plugin: &str,
        path: &[String],
        ui: &str,
    ) -> Result<serde_json::Value, String> {
        let (lua, root) = self.tree(plugin).ok_or("no such plugin")?;
        resolve(&root, path)?;
        let getter = inherited(&root, path, "get")?.ok_or("option has no `get`")?;
        let info = info_table(&lua, plugin, path, ui)?;
        let value: Value = getter
            .call((info,))
            .map_err(|e| format!("get failed: {e}"))?;
        lua_to_json(&value)
    }

    /// Write one option's value.
    pub fn set(
        &self,
        plugin: &str,
        path: &[String],
        value: serde_json::Value,
        ui: &str,
    ) -> Result<(), String> {
        let (lua, root) = self.tree(plugin).ok_or("no such plugin")?;
        let node = resolve(&root, path)?;
        let info = info_table(&lua, plugin, path, ui)?;
        // Before the setter, and before `inherited` — a disabled or non-leaf
        // node must refuse identically whether or not it happens to have one.
        admit::admit_value(&node, &info, &value)?;
        let setter = inherited(&root, path, "set")?.ok_or("option is read-only (no `set`)")?;
        let lua_value = json_to_lua(&lua, &value)?;
        setter
            .call::<()>((info, lua_value))
            .map_err(|e| format!("set failed: {e}"))
    }

    /// Press a button (`type = "execute"`).
    pub fn execute(&self, plugin: &str, path: &[String], ui: &str) -> Result<(), String> {
        let (lua, root) = self.tree(plugin).ok_or("no such plugin")?;
        resolve(&root, path)?;
        let func = inherited(&root, path, "func")?.ok_or("option has no `func`")?;
        let info = info_table(&lua, plugin, path, ui)?;
        func.call::<()>((info,))
            .map_err(|e| format!("execute failed: {e}"))
    }
}

/// Resolve a dotted path to its node table.
fn resolve(root: &Table, path: &[String]) -> Result<Table, String> {
    let mut node = root.clone();
    for segment in path {
        let args: Table = node
            .get("args")
            .map_err(|_| format!("'{segment}' has no parent group"))?;
        node = args
            .get(segment.as_str())
            .map_err(|_| format!("no option named '{segment}'"))?;
    }
    Ok(node)
}

/// Find `field` on the node at `path`, walking toward the root.
///
/// Ace3's inheritance: one `set` at the top serves every leaf. An explicit
/// `false` stops the walk, so a node that genuinely has no setter is not
/// handed its parent's.
fn inherited(root: &Table, path: &[String], field: &str) -> Result<Option<Function>, String> {
    for depth in (0..=path.len()).rev() {
        let node = resolve(root, &path[..depth])?;
        match node.get::<Value>(field) {
            Ok(Value::Function(f)) => return Ok(Some(f)),
            Ok(Value::Boolean(false)) => return Ok(None),
            _ => continue,
        }
    }
    Ok(None)
}

/// The `info` table every callback receives.
///
/// Carries the path so one shared accessor can tell which option it is serving
/// — Ace3's `info[#info]` idiom — plus `uiType`, for the rare case where a
/// handler genuinely must know which frontend asked.
fn info_table(lua: &Lua, plugin: &str, path: &[String], ui: &str) -> Result<Table, String> {
    let info = lua.create_table().map_err(|e| e.to_string())?;
    for (i, segment) in path.iter().enumerate() {
        info.set(i + 1, segment.as_str())
            .map_err(|e| e.to_string())?;
    }
    info.set("plugin", plugin).map_err(|e| e.to_string())?;
    info.set("uiType", ui).map_err(|e| e.to_string())?;
    if let Some(last) = path.last() {
        info.set("option", last.as_str())
            .map_err(|e| e.to_string())?;
    }
    Ok(info)
}

/// Evaluate a field that may be a plain value or a function returning one.
fn evaluate(node: &Table, field: &str, info: &Table) -> Option<Value> {
    match node.get::<Value>(field) {
        Ok(Value::Function(f)) => f.call::<Value>((info.clone(),)).ok(),
        Ok(Value::Nil) => None,
        Ok(other) => Some(other),
        Err(_) => None,
    }
}

fn describe_node(
    lua: &Lua,
    node: &Table,
    plugin: &str,
    path: &[String],
    ui: &str,
    root: &Table,
) -> LuaResult<serde_json::Value> {
    let info = info_table(lua, plugin, path, ui)
        .map_err(|e| mlua::Error::runtime(format!("info table: {e}")))?;

    let mut out = serde_json::Map::new();
    let node_type: String = node.get("type").unwrap_or_else(|_| "group".to_string());
    out.insert("type".into(), serde_json::json!(node_type));

    for field in ["name", "desc", "usage"] {
        if let Some(v) = evaluate(node, field, &info) {
            if let Ok(json) = lua_to_json(&v) {
                out.insert(field.into(), json);
            }
        }
    }
    // Ace3's default is 100, with 0 first and -1 last. Kept so a plugin ported
    // from that world orders the same way here.
    let order = evaluate(node, "order", &info)
        .and_then(|v| lua_to_json(&v).ok())
        .unwrap_or(serde_json::json!(100));
    out.insert("order".into(), order);

    for (field, key) in [("min", "min"), ("max", "max"), ("step", "step")] {
        if let Some(v) = evaluate(node, field, &info) {
            if let Ok(json) = lua_to_json(&v) {
                out.insert(key.into(), json);
            }
        }
    }

    // `values` is the reason fields may be functions at all: the choices are a
    // property of this box, not of the plugin's source.
    if let Some(Value::Table(values)) = evaluate(node, "values", &info) {
        // Array form (`{"podman", "docker"}`) means value == label, and its
        // order is the plugin's own — oci lists runtimes in the order it will
        // actually try them, which sorting alphabetically would misreport as a
        // preference it does not have. Only the hash form needs sorting, and
        // only because Lua's `pairs` order is unspecified.
        let mut choices: Vec<serde_json::Value> = Vec::new();
        let mut sequence: Vec<serde_json::Value> = Vec::new();
        for value in values.sequence_values::<Value>().flatten() {
            let label = lua_to_json(&value).unwrap_or(serde_json::Value::Null);
            sequence.push(serde_json::json!({ "value": label, "label": label }));
        }
        if sequence.is_empty() {
            for (k, v) in values.pairs::<Value, Value>().flatten() {
                let key = lua_to_json(&k).unwrap_or(serde_json::Value::Null);
                let label = lua_to_json(&v).unwrap_or(serde_json::Value::Null);
                choices.push(serde_json::json!({ "value": key, "label": label }));
            }
            choices.sort_by(|a, b| a["label"].to_string().cmp(&b["label"].to_string()));
        } else {
            choices = sequence;
        }
        out.insert("values".into(), serde_json::json!(choices));
    }

    for flag in ["disabled", "hidden"] {
        let truthy = match evaluate(node, flag, &info) {
            Some(Value::Boolean(b)) => b,
            Some(Value::Function(_)) | None => false,
            Some(_) => true,
        };
        if truthy {
            out.insert(flag.into(), serde_json::json!(true));
        }
    }

    // Per-frontend hiding. An option meaningless in one renderer says so, and
    // no client has to learn which options are "for" it.
    let hide_key = if ui == "tui" {
        "tuiHidden"
    } else {
        "webHidden"
    };
    if matches!(node.get::<Value>(hide_key), Ok(Value::Boolean(true))) {
        out.insert("hidden".into(), serde_json::json!(true));
    }

    // Whether the leaf is actually writable, so a client can render it
    // read-only instead of offering an edit that will be refused.
    if node_type != "group" && node_type != "execute" {
        let writable = inherited(root, path, "set").ok().flatten().is_some();
        out.insert("writable".into(), serde_json::json!(writable));
    }

    if let Ok(args) = node.get::<Table>("args") {
        let mut children = Vec::new();
        for pair in args.pairs::<String, Table>().flatten() {
            let (key, child) = pair;
            let mut child_path = path.to_vec();
            child_path.push(key.clone());
            let mut described = describe_node(lua, &child, plugin, &child_path, ui, root)?;
            if let Some(obj) = described.as_object_mut() {
                obj.insert("key".into(), serde_json::json!(key));
            }
            children.push(described);
        }
        children.sort_by(|a, b| {
            let ao = a["order"].as_f64().unwrap_or(100.0);
            let bo = b["order"].as_f64().unwrap_or(100.0);
            // Ace3: 0 first, -1 last, everything else by value.
            let rank = |o: f64| if o < 0.0 { f64::MAX } else { o };
            rank(ao)
                .partial_cmp(&rank(bo))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.insert("args".into(), serde_json::json!(children));
    }

    Ok(serde_json::Value::Object(out))
}

fn lua_to_json(value: &Value) -> Result<serde_json::Value, String> {
    Ok(match value {
        Value::Nil => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::json!(b),
        Value::Integer(i) => serde_json::json!(i),
        Value::Number(n) => serde_json::json!(n),
        Value::String(s) => serde_json::json!(s.to_str().map_err(|e| e.to_string())?.to_owned()),
        Value::Table(_) => {
            return Err("table values must be read through `values`, not returned raw".into())
        }
        other => {
            return Err(format!(
                "value of type {} is not renderable",
                other.type_name()
            ))
        }
    })
}

fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> Result<Value, String> {
    lua.to_value(value)
        .map_err(|e| format!("value is not representable in Lua: {e}"))
}

/// Register `cru.plugin.options`.
///
/// `plugin` comes from the loader, not the caller, for the same reason
/// `cru.plugin.publish` takes it that way: a plugin declaring settings under
/// another's name would make the whole tree untrustworthy.
pub fn register_options_module(
    lua: &Lua,
    registry: OptionsRegistry,
    plugin: String,
) -> LuaResult<()> {
    let options = lua.create_function(move |lua, tree: Table| {
        // Refused here, which means refused at `setup()` — the call propagates
        // out through `activate` and the plugin ends inert with its
        // tree released, rather than registering a tree the frontends cannot
        // draw. (The `args` check this replaced used `matches!` rather than
        // `is_err()` for a reason worth keeping in mind: mlua answers a MISSING
        // key with `Ok(Value::Nil)`, so an `is_err()` guard could never fire.)
        validate::validate_tree(&tree)
            .map_err(|e| mlua::Error::runtime(format!("cru.plugin.options: {e}")))?;
        registry.set_tree(&plugin, lua.clone(), tree);
        Ok(())
    })?;
    crate::lua_util::get_or_create_module(lua, "plugin")?.set("options", options)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(src: &str) -> (Lua, OptionsRegistry) {
        let lua = Lua::new();
        let reg = OptionsRegistry::new();
        register_options_module(&lua, reg.clone(), "oci".to_string()).unwrap();
        lua.load(src).exec().unwrap();
        (lua, reg)
    }

    const SIMPLE: &str = r#"
        state = { image = "alpine", verbose = false }
        cru.plugin.options{
          type = "group", name = "OCI",
          get = function(info) return state[info.option] end,
          set = function(info, v) state[info.option] = v end,
          args = {
            image   = { type = "input",  name = "Image",   order = 1 },
            verbose = { type = "toggle", name = "Verbose", order = 2 },
          },
        }
    "#;

    #[test]
    fn a_declared_tree_describes_itself_for_a_client() {
        let (_lua, reg) = registry_with(SIMPLE);
        let described = reg.describe("oci", "web").expect("described");
        assert_eq!(described["type"], "group");
        let args = described["args"].as_array().unwrap();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0]["key"], "image");
        assert_eq!(args[0]["name"], "Image");
        assert_eq!(args[0]["type"], "input");
    }

    /// One `get`/`set` at the root serves every leaf — the Ace3 idiom that
    /// makes a settings tree worth declaring instead of hand-wiring.
    #[test]
    fn accessors_inherit_from_the_root_and_know_which_option_they_serve() {
        let (_lua, reg) = registry_with(SIMPLE);
        let image = vec!["image".to_string()];
        assert_eq!(reg.get("oci", &image, "web").unwrap(), "alpine");

        reg.set("oci", &image, serde_json::json!("debian"), "web")
            .expect("set");
        assert_eq!(reg.get("oci", &image, "web").unwrap(), "debian");

        // ...and the sibling is untouched, so `info.option` really did route.
        let verbose = vec!["verbose".to_string()];
        assert_eq!(reg.get("oci", &verbose, "web").unwrap(), false);
    }

    /// The reason any field may be a function: the choices describe this box,
    /// not the plugin's source.
    #[test]
    fn values_are_evaluated_when_read_not_when_declared() {
        let (lua, reg) = registry_with(
            r#"
            installed = { "podman" }
            cru.plugin.options{
              type = "group",
              args = {
                runtime = {
                  type = "select", name = "Runtime",
                  values = function() return installed end,
                },
              },
            }
            "#,
        );
        let first = reg.describe("oci", "web").unwrap();
        let choices = first["args"][0]["values"].as_array().unwrap();
        assert_eq!(choices.len(), 1);

        lua.load(r#"installed = { "podman", "docker" }"#)
            .exec()
            .unwrap();
        let second = reg.describe("oci", "web").unwrap();
        assert_eq!(second["args"][0]["values"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_button_runs_its_func() {
        let (lua, reg) = registry_with(
            r#"
            pressed = 0
            cru.plugin.options{
              type = "group",
              args = { rebuild = { type = "execute", name = "Rebuild",
                                   func = function() pressed = pressed + 1 end } },
            }
            "#,
        );
        reg.execute("oci", &["rebuild".to_string()], "web").unwrap();
        let pressed: i64 = lua.globals().get("pressed").unwrap();
        assert_eq!(pressed, 1, "a button is a settings node, not a new API");
    }

    /// `order` decides render order, with Ace3's 0-first/-1-last convention, so
    /// a settings pane does not reshuffle with hash order.
    #[test]
    fn children_render_in_declared_order_with_negatives_last() {
        let (_lua, reg) = registry_with(
            r#"
            cru.plugin.options{
              type = "group",
              args = {
                z_last  = { type = "input", name = "Last",  order = -1 },
                b_mid   = { type = "input", name = "Mid",   order = 5 },
                a_first = { type = "input", name = "First", order = 0 },
              },
            }
            "#,
        );
        let described = reg.describe("oci", "web").unwrap();
        let keys: Vec<_> = described["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["key"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(keys, vec!["a_first", "b_mid", "z_last"]);
    }

    /// An option meaningless in one renderer says so once, rather than each
    /// client learning which options are "for" it.
    #[test]
    fn a_node_can_hide_itself_from_one_frontend_only() {
        let (_lua, reg) = registry_with(
            r#"
            cru.plugin.options{
              type = "group",
              args = { colours = { type = "toggle", name = "Colours", tuiHidden = true } },
            }
            "#,
        );
        let web = reg.describe("oci", "web").unwrap();
        assert!(web["args"][0].get("hidden").is_none(), "visible on web");
        let tui = reg.describe("oci", "tui").unwrap();
        assert_eq!(tui["args"][0]["hidden"], true, "hidden in the TUI");
    }

    /// A node that genuinely has no setter must not be handed its parent's —
    /// otherwise a read-only option silently becomes writable.
    #[test]
    fn set_false_breaks_inheritance_rather_than_falling_through() {
        let (_lua, reg) = registry_with(
            r#"
            state = { locked = "x" }
            cru.plugin.options{
              type = "group",
              get = function(info) return state[info.option] end,
              set = function(info, v) state[info.option] = v end,
              args = { locked = { type = "input", name = "Locked", set = false } },
            }
            "#,
        );
        let described = reg.describe("oci", "web").unwrap();
        assert_eq!(described["args"][0]["writable"], false);

        let err = reg
            .set(
                "oci",
                &["locked".to_string()],
                serde_json::json!("y"),
                "web",
            )
            .expect_err("an option declaring `set = false` is read-only");
        assert!(err.contains("read-only"), "{err}");
    }

    /// Array-form choices keep the plugin's order; hash-form ones are sorted,
    /// because Lua's `pairs` order is unspecified and a settings pane must not
    /// reshuffle between reads.
    #[test]
    fn array_choices_keep_their_declared_order_and_hash_choices_are_sorted() {
        let (_lua, reg) = registry_with(
            r#"
            cru.plugin.options{
              type = "group",
              args = {
                runtime = { type = "select", name = "Runtime", order = 1,
                            values = function() return { "podman", "docker", "nerdctl" } end },
                level   = { type = "select", name = "Level", order = 2,
                            values = { warn = "Warn", debug = "Debug" } },
              },
            }
            "#,
        );
        let described = reg.describe("oci", "web").unwrap();
        let labels = |node: &serde_json::Value| -> Vec<String> {
            node["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["label"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(
            labels(&described["args"][0]),
            vec!["podman", "docker", "nerdctl"],
            "detection order is the plugin's statement, not something to alphabetise"
        );
        assert_eq!(labels(&described["args"][1]), vec!["Debug", "Warn"]);
    }

    /// The root check used `is_err()`, but mlua answers a missing key with
    /// `Ok(Nil)` — so the error it promised could never fire.
    #[test]
    fn a_root_without_an_args_table_is_refused() {
        let lua = Lua::new();
        let reg = OptionsRegistry::new();
        register_options_module(&lua, reg.clone(), "oci".to_string()).unwrap();

        let err = lua
            .load(r#"cru.plugin.options{ type = "group", name = "X" }"#)
            .exec()
            .expect_err("a root with no args table must be refused");
        assert!(err.to_string().contains("args"), "{err}");
        assert!(reg.plugins().is_empty(), "and nothing may be registered");
    }

    #[test]
    fn an_unknown_option_path_is_an_error_not_a_panic() {
        let (_lua, reg) = registry_with(SIMPLE);
        let err = reg
            .get("oci", &["nope".to_string()], "web")
            .expect_err("unknown option");
        assert!(err.contains("nope"), "{err}");
    }

    #[test]
    fn a_reloaded_plugins_previous_tree_is_dropped() {
        let (_lua, reg) = registry_with(SIMPLE);
        assert_eq!(reg.plugins(), vec!["oci".to_string()]);
        reg.release_plugin("oci");
        assert!(reg.plugins().is_empty());
        assert!(reg.describe("oci", "web").is_none());
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;

    /// Registers a tree, returning the refusal message when the load is
    /// refused. `exec` propagates the error out of `cru.plugin.options`, which
    /// is exactly what happens to a real plugin inside `setup()`.
    fn try_register(src: &str) -> Result<(Lua, OptionsRegistry), String> {
        let lua = Lua::new();
        let reg = OptionsRegistry::new();
        register_options_module(&lua, reg.clone(), "p".to_string()).unwrap();
        match lua.load(src).exec() {
            Ok(()) => Ok((lua, reg)),
            Err(e) => Err(e.to_string()),
        }
    }

    fn register(src: &str) -> (Lua, OptionsRegistry) {
        try_register(src).expect("tree should register")
    }

    const GOOD: &str = r#"
        state = { image = "alpine", verbose = false, jobs = 4, runtime = "podman" }
        cru.plugin.options{
          type = "group", name = "P",
          get = function(i) return state[i.option] end,
          set = function(i, v) state[i.option] = v end,
          args = {
            image   = { type = "input",  name = "Image" },
            verbose = { type = "toggle", name = "Verbose" },
            jobs    = { type = "range",  name = "Jobs", min = 1, max = 8, step = 1 },
            runtime = { type = "select", name = "Runtime", values = {"podman", "docker"} },
          },
        }
    "#;

    // ── Declaration ─────────────────────────────────────────────────────

    #[test]
    fn a_tree_declaring_an_unknown_control_is_refused_at_load() {
        // The defect this whole table exists for. Before the closed set this
        // registered, stored, and rendered as a text box.
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 shade = { type = "colour-wheel", name = "Shade" } } }"#,
        )
        .expect_err("an unknown type must refuse the load");
        assert!(
            err.contains("colour-wheel"),
            "message must name the type: {err}"
        );
        assert!(err.contains("shade"), "message must name the path: {err}");
        assert!(
            err.contains("toggle"),
            "message must list the valid types: {err}"
        );
    }

    #[test]
    fn a_typo_is_refused_rather_than_downgraded_to_text() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 verbose = { type = "toggel", name = "Verbose" } } }"#,
        )
        .expect_err("a typo must not become a text field");
        assert!(err.contains("toggel"), "{err}");
    }

    #[test]
    fn a_select_without_choices_is_refused() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 runtime = { type = "select", name = "Runtime" } } }"#,
        )
        .expect_err("a select with no values cannot be rendered");
        assert!(err.contains("values"), "{err}");
    }

    #[test]
    fn a_values_function_satisfies_a_select() {
        // The choices are a property of the box; a function is the whole reason
        // fields may be functions, so it must not be refused as "missing".
        register(
            r#"cru.plugin.options{ type = "group", args = {
                 runtime = { type = "select", name = "R",
                             values = function() return {"podman"} end } } }"#,
        );
    }

    #[test]
    fn an_inverted_range_is_refused() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 jobs = { type = "range", min = 8, max = 1 } } }"#,
        )
        .expect_err("min above max can never admit a value");
        assert!(err.contains("min") && err.contains("max"), "{err}");
    }

    #[test]
    fn a_non_positive_step_is_refused() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 jobs = { type = "range", min = 1, max = 8, step = 0 } } }"#,
        )
        .expect_err("a zero step divides nothing");
        assert!(err.contains("step"), "{err}");
    }

    #[test]
    fn an_execute_without_a_func_is_refused() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 go = { type = "execute", name = "Go" } } }"#,
        )
        .expect_err("a button with nothing to press is not a button");
        assert!(err.contains("func"), "{err}");
    }

    #[test]
    fn only_a_group_may_carry_children() {
        let err = try_register(
            r#"cru.plugin.options{ type = "group", args = {
                 image = { type = "input", args = { nested = { type = "input" } } } } }"#,
        )
        .expect_err("a leaf with children renders as neither");
        assert!(err.contains("args"), "{err}");
    }

    #[test]
    fn a_tree_nested_past_the_limit_is_refused() {
        // Depth is the cost of every settings read: `describe_node` walks the
        // whole tree and evaluates every function-valued field.
        let mut src = String::from("cru.plugin.options{ type = \"group\", args = { ");
        let depth = 8;
        for i in 0..depth {
            src.push_str(&format!("g{i} = {{ type = \"group\", args = {{ "));
        }
        src.push_str("leaf = { type = \"input\" } ");
        for _ in 0..depth {
            src.push_str("} } ");
        }
        src.push_str("} }");
        let err = try_register(&src).expect_err("a deep tree must be refused");
        assert!(err.contains("deeper"), "{err}");
    }

    #[test]
    fn every_control_the_enum_knows_is_declarable() {
        // Derived from the enum itself, so a variant added without a way to
        // declare it fails here rather than at some plugin author's desk.
        for control in Control::ALL {
            let extra = match control {
                Control::Group => ", args = { x = { type = \"input\" } }",
                Control::Select | Control::MultiSelect => ", values = {\"a\"}",
                Control::Execute => ", func = function() end",
                _ => "",
            };
            let src = format!(
                r#"cru.plugin.options{{ type = "group", args = {{
                     probe = {{ type = "{}"{extra} }} }} }}"#,
                control.as_str()
            );
            try_register(&src).unwrap_or_else(|e| panic!("{:?} must be declarable: {e}", control));
        }
    }

    // ── Writes ──────────────────────────────────────────────────────────

    #[test]
    fn a_toggle_refuses_a_string() {
        let (_lua, reg) = register(GOOD);
        let err = reg
            .set("p", &["verbose".into()], serde_json::json!("yes"), "web")
            .expect_err("a toggle is not a text field");
        assert!(err.contains("boolean"), "{err}");
        // And the plugin's own state was never touched.
        assert_eq!(
            reg.get("p", &["verbose".into()], "web").unwrap(),
            serde_json::json!(false)
        );
    }

    #[test]
    fn a_range_refuses_a_value_outside_its_declared_bounds() {
        let (_lua, reg) = register(GOOD);
        // The exact shape that made an oci image pull hang: a declared floor of
        // 60 accepting 0.
        let err = reg
            .set("p", &["jobs".into()], serde_json::json!(0), "web")
            .expect_err("below min");
        assert!(err.contains("minimum"), "{err}");
        assert!(reg
            .set("p", &["jobs".into()], serde_json::json!(99), "web")
            .is_err());
        // A value inside the bounds still writes.
        reg.set("p", &["jobs".into()], serde_json::json!(6), "web")
            .expect("6 is admissible");
    }

    #[test]
    fn a_range_refuses_a_value_off_the_declared_step() {
        let (_lua, reg) = register(GOOD);
        assert!(reg
            .set("p", &["jobs".into()], serde_json::json!(2.5), "web")
            .is_err());
    }

    #[test]
    fn a_select_refuses_a_choice_its_values_never_offered() {
        let (_lua, reg) = register(GOOD);
        let err = reg
            .set(
                "p",
                &["runtime".into()],
                serde_json::json!("containerd"),
                "web",
            )
            .expect_err("not an offered choice");
        assert!(err.contains("containerd"), "{err}");
        reg.set("p", &["runtime".into()], serde_json::json!("docker"), "web")
            .expect("docker is offered");
    }

    #[test]
    fn membership_is_checked_against_the_choices_this_box_offers_now() {
        // The reason `values` may be a function at all. A runtime that has been
        // uninstalled must stop being SELECTABLE, not merely stop being listed.
        let (lua, reg) = register(
            r#"
            installed = { "podman", "docker" }
            state = { runtime = "podman" }
            cru.plugin.options{
              type = "group",
              get = function(i) return state[i.option] end,
              set = function(i, v) state[i.option] = v end,
              args = { runtime = { type = "select",
                                   values = function() return installed end } },
            }
        "#,
        );
        reg.set("p", &["runtime".into()], serde_json::json!("docker"), "web")
            .expect("docker is installed");
        lua.load("installed = { \"podman\" }").exec().unwrap();
        assert!(
            reg.set("p", &["runtime".into()], serde_json::json!("docker"), "web")
                .is_err(),
            "docker is gone and must no longer be selectable"
        );
    }

    #[test]
    fn a_disabled_option_refuses_a_write() {
        let (_lua, reg) = register(
            r#"
            state = { image = "alpine" }
            cru.plugin.options{
              type = "group",
              get = function(i) return state[i.option] end,
              set = function(i, v) state[i.option] = v end,
              args = { image = { type = "input", disabled = true } },
            }
        "#,
        );
        let err = reg
            .set("p", &["image".into()], serde_json::json!("busybox"), "web")
            .expect_err("a disabled option is not writable");
        assert!(err.contains("disabled"), "{err}");
    }

    #[test]
    fn a_group_refuses_a_write() {
        let (_lua, reg) = register(GOOD);
        assert!(reg
            .set("p", &[], serde_json::json!("anything"), "web")
            .is_err());
    }

    #[test]
    fn an_admissible_write_still_reaches_the_plugin() {
        // The gate must not be a wall. Every type that shipped before it still
        // writes.
        let (_lua, reg) = register(GOOD);
        reg.set("p", &["image".into()], serde_json::json!("busybox"), "web")
            .unwrap();
        reg.set("p", &["verbose".into()], serde_json::json!(true), "web")
            .unwrap();
        assert_eq!(
            reg.get("p", &["image".into()], "web").unwrap(),
            serde_json::json!("busybox")
        );
        assert_eq!(
            reg.get("p", &["verbose".into()], "web").unwrap(),
            serde_json::json!(true)
        );
    }
}
