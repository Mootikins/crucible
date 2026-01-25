//! Hook registration system for Crucible Lua API
//!
//! Provides `crucible.on_session_start(fn)` for registering session lifecycle hooks.

use mlua::{Function, Lua, Result as LuaResult, Table};

/// Register the hooks module on the crucible table
///
/// This function is called during executor initialization to set up hook registration.
/// Hooks are stored in a Lua table that the executor can access via `get_session_start_hooks()`.
///
/// # Example
///
/// ```lua
/// crucible.on_session_start(function(session)
///     session.temperature = 0.5
/// end)
/// ```
pub fn register_hooks_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let on_session_start = lua.create_function(|lua, func: Function| {
        let key = lua.create_registry_value(func)?;

        let globals = lua.globals();
        let hooks_table: Table = globals
            .get("__crucible_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let session_start_hooks: Table = hooks_table
            .get("on_session_start")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let len = session_start_hooks.raw_len() as usize;
        session_start_hooks.raw_set(len + 1, key)?;

        hooks_table.set("on_session_start", session_start_hooks)?;
        globals.set("__crucible_hooks__", hooks_table)?;

        Ok(())
    })?;

    crucible.set("on_session_start", on_session_start)?;

    Ok(())
}

/// Get all registered session start hooks from the Lua environment
pub fn get_session_start_hooks(lua: &Lua) -> LuaResult<Vec<mlua::RegistryKey>> {
    let globals = lua.globals();
    let hooks_table: Table = match globals.get("__crucible_hooks__") {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let session_start_hooks: Table = match hooks_table.get("on_session_start") {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let mut keys = Vec::new();
    for i in 1..=session_start_hooks.raw_len() {
        if let Ok(key) = session_start_hooks.raw_get::<mlua::RegistryKey>(i) {
            keys.push(key);
        }
    }

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_hooks_module() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();

        register_hooks_module(&lua, &crucible).unwrap();

        let _func: Function = crucible.get("on_session_start").unwrap();
    }

    #[test]
    fn test_on_session_start_stores_function() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible.clone()).unwrap();

        register_hooks_module(&lua, &crucible).unwrap();

        // Register a hook
        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        // Verify it was stored
        let hooks_table: Table = lua.globals().get("__crucible_hooks__").unwrap();
        let session_start_hooks: Table = hooks_table.get("on_session_start").unwrap();
        assert_eq!(session_start_hooks.raw_len(), 1);
    }

    #[test]
    fn test_multiple_hooks_append() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible.clone()).unwrap();

        register_hooks_module(&lua, &crucible).unwrap();

        // Register multiple hooks
        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_session_start(function(s) end)
            crucible.on_session_start(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        // Verify all were stored
        let hooks_table: Table = lua.globals().get("__crucible_hooks__").unwrap();
        let session_start_hooks: Table = hooks_table.get("on_session_start").unwrap();
        assert_eq!(session_start_hooks.raw_len(), 3);
    }
}
