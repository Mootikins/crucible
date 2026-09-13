//! The spec store: what `cru.plugin.setup` wrote, ranked by its source.
//!
//! `docs/Meta/CONTEXT.md` defines the words. The spec is the operator's list
//! of plugins. A spec entry is one table in that list. A fragment is a partial
//! entry that a plugin ships, or that the shipped defaults provide.
//!
//! The store is VM app data. It holds one [`Spec`] plus, per plugin name, the
//! `config` and `init` functions an entry gave. The functions stay in the VM
//! that defined them, as [`RegistryKey`]s. [`Spec`] is data only, so the
//! bootstrap and `plugin.list` can read it without a VM.
//!
//! The rank of a write comes from the [`LuaSource`] in force when `setup`
//! ran, never from an argument. A plugin's own fragment runs under
//! `LuaSource::Plugin`, so it cannot outrank the operator. A socket eval runs
//! under `LuaSource::Eval`, and the store refuses it: the spec is written in
//! `init.lua`, and a socket call must not rewrite it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use crucible_core::config::{Spec, SpecEntry, SpecRank, SpecSource};
use mlua::{Function, Lua, RegistryKey, Table, Value};

use crate::error::LuaError;
use crate::plugin_context::{current_source, LuaSource};

/// How deep `import` may nest before the store refuses the entry. A directory
/// that imports itself would otherwise recurse until the stack ends.
const MAX_IMPORT_DEPTH: usize = 8;

/// One function an entry gave, and the rank that gave it.
struct Held {
    rank: SpecRank,
    key: RegistryKey,
}

#[derive(Default)]
struct Inner {
    spec: Spec,
    config: HashMap<String, Held>,
    init: HashMap<String, Held>,
}

/// The store, shared by handle so a lock is held for one read or one write.
#[derive(Clone, Default)]
struct SpecStore(Arc<Mutex<Inner>>);

impl SpecStore {
    fn of(lua: &Lua) -> SpecStore {
        match lua.app_data_ref::<SpecStore>() {
            Some(existing) => existing.clone(),
            None => {
                let fresh = SpecStore::default();
                lua.set_app_data(fresh.clone());
                fresh
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The directory `import = "<dir>"` resolves under. A newtype so the app-data
/// slot holds exactly one path.
struct ImportRoot(PathBuf);

/// Set the directory `import` resolves under. The loader calls this once it
/// knows the config root; the value is `<config root>/lua`, the same root
/// `require` resolves user modules from.
pub fn set_import_root(lua: &Lua, root: PathBuf) {
    lua.set_app_data(ImportRoot(root));
}

fn import_root(lua: &Lua) -> Option<PathBuf> {
    lua.app_data_ref::<ImportRoot>().map(|root| root.0.clone())
}

/// A copy of the merged spec.
pub fn spec_of(lua: &Lua) -> Spec {
    SpecStore::of(lua).lock().spec.clone()
}

/// The `config` function the highest-ranked entry for `name` gave, if any.
pub fn config_of(lua: &Lua, name: &str) -> Option<Function> {
    let store = SpecStore::of(lua);
    let inner = store.lock();
    let held = inner.config.get(name)?;
    lua.registry_value::<Function>(&held.key).ok()
}

/// The `init` function the highest-ranked entry for `name` gave, if any.
pub fn init_of(lua: &Lua, name: &str) -> Option<Function> {
    let store = SpecStore::of(lua);
    let inner = store.lock();
    let held = inner.init.get(name)?;
    lua.registry_value::<Function>(&held.key).ok()
}

/// One parsed element of the `setup` argument, before it reaches the store.
struct Parsed {
    entry: SpecEntry,
    config: Option<Function>,
    init: Option<Function>,
}

/// Register `cru.plugin.setup(entries)`.
///
/// ```lua
/// cru.plugin.setup({
///   "reflection",
///   { "user/greeter", branch = "main", opts = { n = 1 } },
///   { "oci", enabled = false },
///   { import = "plugins" },
/// })
/// ```
pub fn register_plugin_spec_api(lua: &Lua) -> Result<(), LuaError> {
    // `cru.plugin` already carries members other modules registered, so the
    // namespace opens OVER the existing table and never publishes a fresh one.
    let plugin = crate::lua_util::get_or_create_module(lua, "plugin")?;
    let mut ns = crate::host_registry::Ns::over(lua, "cru.plugin", plugin);
    ns.func(
        "setup",
        "(entries: { any }) -> ()",
        |lua, entries: Table| setup(lua, entries),
    )?;
    ns.doc(
        "setup",
        "Write spec entries. The rank comes from the source that runs the call: \
         the operator's init.lua outranks the shipped defaults, which outrank a \
         plugin's own fragment. A socket eval is refused.",
    );
    Ok(())
}

fn setup(lua: &Lua, entries: Table) -> mlua::Result<()> {
    let rank = match current_source(lua) {
        LuaSource::Plugin(_) => SpecRank::PluginFragment,
        LuaSource::Builtin => SpecRank::Builtin,
        LuaSource::UserLua => SpecRank::Operator,
        LuaSource::Eval => {
            return Err(mlua::Error::runtime(
                "cru.plugin.setup: the spec is written in init.lua, not over the socket",
            ))
        }
    };

    // Parse first, write second. An `import` evaluates Lua files, and a file
    // may call `cru.plugin.setup` itself, so no lock is held while parsing.
    let mut parsed = Vec::new();
    parse_entries(lua, entries, 0, &mut parsed)?;

    let store = SpecStore::of(lua);
    let mut inner = store.lock();
    for item in parsed {
        let name = item.entry.name.clone();
        inner.spec.merge(item.entry, rank);
        hold(lua, &mut inner.config, &name, rank, item.config)?;
        hold(lua, &mut inner.init, &name, rank, item.init)?;
    }
    Ok(())
}

/// Keep `function` under `name`. A later entry at the same or a higher rank
/// replaces the held key. A lower rank never replaces one that exists.
fn hold(
    lua: &Lua,
    held: &mut HashMap<String, Held>,
    name: &str,
    rank: SpecRank,
    function: Option<Function>,
) -> mlua::Result<()> {
    let Some(function) = function else {
        return Ok(());
    };
    if let Some(existing) = held.get(name) {
        if rank < existing.rank {
            return Ok(());
        }
    }
    let key = lua.create_registry_value(function)?;
    held.insert(name.to_string(), Held { rank, key });
    Ok(())
}

fn parse_entries(
    lua: &Lua,
    entries: Table,
    depth: usize,
    out: &mut Vec<Parsed>,
) -> mlua::Result<()> {
    for pair in entries.pairs::<Value, Value>() {
        let (index, element) = pair?;
        let Value::Integer(index) = index else {
            return Err(mlua::Error::runtime(format!(
                "cru.plugin.setup: entries is a list, but it has the key {}",
                describe(&index)
            )));
        };
        match element {
            Value::String(text) => {
                out.push(Parsed {
                    entry: positional(index, &text.to_str()?)?,
                    config: None,
                    init: None,
                });
            }
            Value::Table(table) => parse_table(lua, index, table, depth, out)?,
            other => {
                return Err(mlua::Error::runtime(format!(
                    "cru.plugin.setup: entry {index} is {}, expected a string or a table",
                    describe(&other)
                )))
            }
        }
    }
    Ok(())
}

fn parse_table(
    lua: &Lua,
    index: i64,
    table: Table,
    depth: usize,
    out: &mut Vec<Parsed>,
) -> mlua::Result<()> {
    if let Value::String(text) = table.get::<Value>(1)? {
        out.push(parse_entry(lua, index, &text.to_str()?, &table)?);
        return Ok(());
    }
    if let Value::String(dir) = table.get::<Value>("import")? {
        return import(lua, index, &dir.to_str()?, depth, out);
    }
    Err(mlua::Error::runtime(format!(
        "cru.plugin.setup: entry {index} is a table with neither a plugin name at [1] nor `import`"
    )))
}

fn positional(index: i64, text: &str) -> mlua::Result<SpecEntry> {
    SpecEntry::from_positional(text).map_err(|reason| {
        mlua::Error::runtime(format!("cru.plugin.setup: entry {index}: {reason}"))
    })
}

fn parse_entry(lua: &Lua, index: i64, text: &str, table: &Table) -> mlua::Result<Parsed> {
    let mut entry = positional(index, text)?;
    let branch: Option<String> = field(lua, index, table, "branch")?;
    let pin: Option<String> = field(lua, index, table, "pin")?;
    if branch.is_some() || pin.is_some() {
        match &mut entry.source {
            SpecSource::Git {
                branch: at,
                pin: to,
                ..
            } => {
                *at = branch;
                *to = pin;
            }
            SpecSource::Runtimepath => {
                return Err(mlua::Error::runtime(format!(
                    "cru.plugin.setup: entry {index} ('{text}') names a runtimepath plugin, \
                     so `branch` and `pin` have nothing to check out"
                )))
            }
        }
    }
    entry.enabled = field(lua, index, table, "enabled")?;
    if let Some(opts) = field::<Table>(lua, index, table, "opts")? {
        entry.opts = crate::json_query::lua_to_json(lua, Value::Table(opts))?;
    }
    let config: Option<Function> = field(lua, index, table, "config")?;
    let init: Option<Function> = field(lua, index, table, "init")?;
    entry.has_config = config.is_some();
    entry.has_init = init.is_some();
    Ok(Parsed {
        entry,
        config,
        init,
    })
}

/// Read one named field. `nil` is "unsaid". A value of the wrong type is an
/// error that names the entry and the field.
fn field<T: mlua::FromLua>(
    lua: &Lua,
    index: i64,
    table: &Table,
    name: &str,
) -> mlua::Result<Option<T>> {
    match table.get::<Value>(name)? {
        Value::Nil => Ok(None),
        value => {
            let found = describe(&value);
            T::from_lua(value, lua).map(Some).map_err(|_| {
                mlua::Error::runtime(format!(
                    "cru.plugin.setup: entry {index}: `{name}` is {found}, which is not the type it takes"
                ))
            })
        }
    }
}

/// Read every `*.lua` and `*.luau` file under `<root>/<dir>`, in file-name
/// order. Each file returns a list of entries, which is parsed the same way.
fn import(
    lua: &Lua,
    index: i64,
    dir: &str,
    depth: usize,
    out: &mut Vec<Parsed>,
) -> mlua::Result<()> {
    if depth >= MAX_IMPORT_DEPTH {
        return Err(mlua::Error::runtime(format!(
            "cru.plugin.setup: entry {index}: `import = \"{dir}\"` nests deeper than \
             {MAX_IMPORT_DEPTH} levels; a directory imports itself"
        )));
    }
    let Some(root) = import_root(lua) else {
        return Err(mlua::Error::runtime(format!(
            "cru.plugin.setup: entry {index}: `import = \"{dir}\"` has no root to resolve under; \
             the loader did not set one on this VM"
        )));
    };
    let target = root.join(dir);
    let mut files = list_lua_files(&target).map_err(|e| {
        mlua::Error::runtime(format!(
            "cru.plugin.setup: entry {index}: cannot read `import = \"{dir}\"` at {}: {e}",
            target.display()
        ))
    })?;
    files.sort();
    for path in files {
        let source = std::fs::read_to_string(&path).map_err(|e| {
            mlua::Error::runtime(format!(
                "cru.plugin.setup: entry {index}: cannot read {}: {e}",
                path.display()
            ))
        })?;
        let name = format!("@{}", path.display());
        let table: Table = lua.load(&source).set_name(&name).eval().map_err(|e| {
            mlua::Error::runtime(format!(
                "cru.plugin.setup: entry {index}: {} must return a list of entries: {e}",
                path.display()
            ))
        })?;
        parse_entries(lua, table, depth + 1, out)?;
    }
    Ok(())
}

fn list_lua_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let is_lua = matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("lua" | "luau")
        );
        if is_lua && path.is_file() {
            files.push(path);
        }
    }
    Ok(files)
}

/// The Lua type name of a value, for an error message.
fn describe(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Boolean(_) => "a boolean".to_string(),
        Value::Integer(_) | Value::Number(_) => "a number".to_string(),
        Value::String(_) => "a string".to_string(),
        Value::Table(_) => "a table".to_string(),
        Value::Function(_) => "a function".to_string(),
        other => format!("a {}", other.type_name()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_context::set_source;
    use serde_json::json;

    fn test_vm() -> Lua {
        let lua = Lua::new();
        register_plugin_spec_api(&lua).unwrap();
        lua
    }

    fn test_vm_with_import_root(root: &Path) -> Lua {
        let lua = test_vm();
        set_import_root(&lua, root.to_path_buf());
        lua
    }

    /// Run `body` with `source` in force, then put the previous source back.
    fn with_source<R>(lua: &Lua, source: LuaSource, body: impl FnOnce() -> R) -> R {
        let previous = set_source(lua, source);
        let result = body();
        set_source(lua, previous);
        result
    }

    #[test]
    fn an_operator_entry_outranks_a_builtin_entry() {
        let lua = test_vm();
        with_source(&lua, LuaSource::Builtin, || {
            lua.load(r#"cru.plugin.setup({ "reflection" })"#)
                .exec()
                .unwrap()
        });
        with_source(&lua, LuaSource::UserLua, || {
            lua.load(r#"cru.plugin.setup({ { "reflection", enabled = false } })"#)
                .exec()
                .unwrap()
        });
        assert_eq!(
            spec_of(&lua).get("reflection").unwrap().enabled,
            Some(false)
        );
        assert_eq!(
            spec_of(&lua).rank_of("reflection"),
            Some(SpecRank::Operator)
        );
    }

    #[test]
    fn an_entry_may_carry_a_config_function_and_the_store_keeps_it() {
        let lua = test_vm();
        with_source(&lua, LuaSource::UserLua, || {
            lua.load(
                r#"cru.plugin.setup({ { "x", config = function(_, opts) _G.saw = opts.a end } })"#,
            )
            .exec()
            .unwrap()
        });
        assert!(spec_of(&lua).get("x").unwrap().has_config);
        let f = config_of(&lua, "x").unwrap();
        f.call::<()>((Value::Nil, lua.create_table_from([("a", 1)]).unwrap()))
            .unwrap();
        assert_eq!(lua.globals().get::<i64>("saw").unwrap(), 1);
        assert!(init_of(&lua, "x").is_none());
    }

    #[test]
    fn an_eval_cannot_change_the_spec() {
        let lua = test_vm();
        with_source(&lua, LuaSource::Eval, || {
            let err = lua.load(r#"cru.plugin.setup({ "x" })"#).exec().unwrap_err();
            assert!(err.to_string().contains("init.lua"), "{err}");
        });
        assert!(spec_of(&lua).get("x").is_none());
    }

    #[test]
    fn import_reads_every_file_of_a_directory_in_name_order() {
        let dir = tempfile::TempDir::new().unwrap();
        let plugins = dir.path().join("plugins");
        std::fs::create_dir(&plugins).unwrap();
        std::fs::write(plugins.join("b.lua"), r#"return { "beta" }"#).unwrap();
        std::fs::write(
            plugins.join("a.lua"),
            r#"return { { "alpha", opts = { n = 1 } } }"#,
        )
        .unwrap();
        std::fs::write(plugins.join("notes.md"), "not lua").unwrap();
        let lua = test_vm_with_import_root(dir.path());
        with_source(&lua, LuaSource::UserLua, || {
            lua.load(r#"cru.plugin.setup({ { import = "plugins" } })"#)
                .exec()
                .unwrap()
        });
        let spec = spec_of(&lua);
        let names: Vec<_> = spec.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, ["alpha", "beta"]);
        assert_eq!(spec.get("alpha").unwrap().opts, json!({ "n": 1 }));
    }

    #[test]
    fn import_without_a_root_says_so() {
        let lua = test_vm();
        let err = with_source(&lua, LuaSource::UserLua, || {
            lua.load(r#"cru.plugin.setup({ { import = "plugins" } })"#)
                .exec()
                .unwrap_err()
        });
        assert!(err.to_string().contains("no root"), "{err}");
    }

    #[test]
    fn a_lower_rank_never_replaces_a_held_config_function() {
        let lua = test_vm();
        with_source(&lua, LuaSource::UserLua, || {
            lua.load(
                r#"cru.plugin.setup({ { "x", config = function() _G.who = "operator" end } })"#,
            )
            .exec()
            .unwrap()
        });
        with_source(&lua, LuaSource::Plugin("x".into()), || {
            lua.load(r#"cru.plugin.setup({ { "x", config = function() _G.who = "plugin" end } })"#)
                .exec()
                .unwrap()
        });
        config_of(&lua, "x").unwrap().call::<()>(()).unwrap();
        assert_eq!(lua.globals().get::<String>("who").unwrap(), "operator");
    }

    #[test]
    fn a_git_entry_takes_branch_and_pin_and_a_bare_name_refuses_them() {
        let lua = test_vm();
        with_source(&lua, LuaSource::UserLua, || {
            lua.load(r#"cru.plugin.setup({ { "user/greeter", branch = "main", pin = "v1" } })"#)
                .exec()
                .unwrap();
            let err = lua
                .load(r#"cru.plugin.setup({ { "local", branch = "main" } })"#)
                .exec()
                .unwrap_err();
            assert!(err.to_string().contains("branch"), "{err}");
        });
        assert_eq!(
            spec_of(&lua).get("greeter").unwrap().source,
            SpecSource::Git {
                url: "user/greeter".into(),
                branch: Some("main".into()),
                pin: Some("v1".into()),
            }
        );
    }

    #[test]
    fn a_wrong_element_is_refused_by_index_and_type() {
        let lua = test_vm();
        let err = with_source(&lua, LuaSource::UserLua, || {
            lua.load(r#"cru.plugin.setup({ "ok", 42 })"#)
                .exec()
                .unwrap_err()
        });
        let text = err.to_string();
        assert!(text.contains("entry 2"), "{text}");
        assert!(text.contains("a number"), "{text}");
    }
}
