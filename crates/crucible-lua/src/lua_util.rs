use crate::error::LuaError;
use mlua::{Lua, Result as LuaResult, Table, Value};

pub fn get_or_create_namespace(lua: &Lua, name: &str) -> LuaResult<Table> {
    let globals = lua.globals();
    globals.get(name).or_else(|_: mlua::Error| {
        let t = lua.create_table()?;
        globals.set(name, t.clone())?;
        Ok(t)
    })
}

/// Get or create a named sub-table of the `cru` global.
pub fn get_or_create_module(lua: &Lua, name: &str) -> LuaResult<Table> {
    let cru = get_or_create_namespace(lua, "cru")?;
    cru.get(name).or_else(|_: mlua::Error| {
        let t = lua.create_table()?;
        cru.set(name, t.clone())?;
        Ok(t)
    })
}

pub fn register_in_namespaces(lua: &Lua, module_name: &str, module: Table) -> LuaResult<()> {
    get_or_create_namespace(lua, "crucible")?.set(module_name, module.clone())?;
    get_or_create_namespace(lua, "cru")?.set(module_name, module)?;
    Ok(())
}

/// Compare the keys of a registered module table against its name list.
///
/// Returns the names that only one side has. A module that registers its
/// functions by hand calls this before it publishes the table, so a name
/// that lands in one registration path only fails at registration, not in a
/// plugin at run time.
pub fn key_set_difference(table: &Table, names: &[&str]) -> LuaResult<Vec<String>> {
    let mut diff = Vec::new();
    for name in names {
        if !table.contains_key(*name)? {
            diff.push(format!("missing {name}"));
        }
    }
    for pair in table.pairs::<String, Value>() {
        let (key, _) = pair?;
        if !names.contains(&key.as_str()) {
            diff.push(format!("unlisted {key}"));
        }
    }
    Ok(diff)
}

/// Refuse a module table whose keys differ from `names`.
pub fn gate_module_keys(module: &str, table: &Table, names: &[&str]) -> Result<(), LuaError> {
    let diff = key_set_difference(table, names)?;
    if diff.is_empty() {
        return Ok(());
    }
    Err(LuaError::Runtime(format!(
        "cru.{module} daemon functions disagree with its name list: {}",
        diff.join(", ")
    )))
}
