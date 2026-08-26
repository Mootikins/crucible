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

/// Register a module table on the `cru` global — the one Lua namespace.
pub fn register_module(lua: &Lua, module_name: &str, module: Table) -> LuaResult<()> {
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

/// Install the deprecated `cru.sessions` alias over `cru.session`.
///
/// Every key read forwards to `cru.session[k]` resolved at call time, so the
/// upgrade that swaps stub functions for daemon-backed ones is picked up
/// without reinstalling the alias — and the alias hands back the *same*
/// function objects, never a copy. The first read in a VM warns once, through
/// `cru.log` when that module exists and `print` when it does not.
pub fn install_sessions_alias(lua: &Lua) -> LuaResult<()> {
    get_or_create_namespace(lua, "cru")?;
    lua.load(
        r#"
        do
            local warned = false
            cru.sessions = setmetatable({}, {
                __index = function(_, k)
                    if not warned then
                        warned = true
                        local ok = pcall(function()
                            cru.log("warn", "cru.sessions is deprecated; use cru.session")
                        end)
                        if not ok then
                            print("cru.sessions is deprecated; use cru.session")
                        end
                    end
                    return cru.session[k]
                end,
            })
        end
        "#,
    )
    .exec()
}
