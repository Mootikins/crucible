//! Hook registration system for Crucible Lua API
//!
//! Provides `crucible.on_session_start(fn)` and `crucible.on_tools_registered(fn)`
//! for registering lifecycle hooks.

use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use tracing::{debug, warn};

/// Register the hooks module on the crucible table
///
/// This function is called during executor initialization to set up hook registration.
/// Hooks are stored in a Lua table that the executor can access via `get_session_start_hooks()`
/// and `get_tools_registered_hooks()`.
///
/// # Example
///
/// ```lua
/// crucible.on_session_start(function(session)
///     session.temperature = 0.5
/// end)
///
/// crucible.on_tools_registered(function(event)
///     for _, tool in ipairs(event.tools) do
///         tool.display_name = tool.name:gsub("_", " ")
///     end
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

        let len = session_start_hooks.raw_len();
        session_start_hooks.raw_set(len + 1, key)?;

        hooks_table.set("on_session_start", session_start_hooks)?;
        globals.set("__crucible_hooks__", hooks_table)?;

        Ok(())
    })?;

    crucible.set("on_session_start", on_session_start)?;

    let on_tools_registered = lua.create_function(|lua, func: Function| {
        let key = lua.create_registry_value(func)?;

        let globals = lua.globals();
        let hooks_table: Table = globals
            .get("__crucible_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let tools_registered_hooks: Table = hooks_table
            .get("on_tools_registered")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let len = tools_registered_hooks.raw_len();
        tools_registered_hooks.raw_set(len + 1, key)?;

        hooks_table.set("on_tools_registered", tools_registered_hooks)?;
        globals.set("__crucible_hooks__", hooks_table)?;

        Ok(())
    })?;

    crucible.set("on_tools_registered", on_tools_registered)?;

    Ok(())
}

pub fn get_session_start_hooks(lua: &Lua) -> LuaResult<Vec<mlua::RegistryKey>> {
    get_hooks_by_name(lua, "on_session_start")
}

pub fn get_tools_registered_hooks(lua: &Lua) -> LuaResult<Vec<mlua::RegistryKey>> {
    get_hooks_by_name(lua, "on_tools_registered")
}

fn get_hooks_by_name(lua: &Lua, name: &str) -> LuaResult<Vec<mlua::RegistryKey>> {
    let globals = lua.globals();
    let hooks_table: Table = match globals.get("__crucible_hooks__") {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let hook_list: Table = match hooks_table.get(name) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()),
    };

    let mut keys = Vec::new();
    for i in 1..=hook_list.raw_len() {
        if let Ok(key) = hook_list.raw_get::<mlua::RegistryKey>(i) {
            keys.push(key);
        }
    }

    Ok(keys)
}

/// Tool information passed to and returned from `on_tools_registered` hooks.
#[derive(Debug, Clone)]
pub struct ToolRegistrationInfo {
    pub name: String,
    pub description: String,
    pub display_name: Option<String>,
}

/// Fire all registered `on_tools_registered` hooks.
///
/// Each hook receives `{ server_name, tools }` where tools is an array of
/// `{ name, description, display_name }`. Hooks can modify `display_name` in-place.
///
/// If no hooks are registered, applies `default_display_name_fn` to each tool as fallback.
pub fn fire_tools_registered_hooks(
    lua: &Lua,
    hooks: &[mlua::RegistryKey],
    server_name: &str,
    tools: &mut [ToolRegistrationInfo],
    default_display_name_fn: fn(&str) -> String,
) -> LuaResult<()> {
    if hooks.is_empty() {
        for tool in tools.iter_mut() {
            if tool.display_name.is_none() {
                tool.display_name = Some(default_display_name_fn(&tool.name));
            }
        }
        return Ok(());
    }

    let event_table = lua.create_table()?;
    event_table.set("server_name", server_name)?;

    let tools_table = lua.create_table()?;
    for (i, tool) in tools.iter().enumerate() {
        let t = lua.create_table()?;
        t.set("name", tool.name.as_str())?;
        t.set("description", tool.description.as_str())?;
        if let Some(ref dn) = tool.display_name {
            t.set("display_name", dn.as_str())?;
        } else {
            t.set("display_name", default_display_name_fn(&tool.name))?;
        }
        tools_table.raw_set(i + 1, t)?;
    }
    event_table.set("tools", tools_table)?;

    for key in hooks {
        match lua.registry_value::<Function>(key) {
            Ok(func) => {
                let start = std::time::Instant::now();
                if let Err(e) = func.call::<()>(event_table.clone()) {
                    warn!("on_tools_registered hook failed: {}", e);
                }
                let elapsed = start.elapsed();
                if elapsed.as_millis() > 100 {
                    warn!(
                        "on_tools_registered hook took {}ms (>100ms threshold)",
                        elapsed.as_millis()
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to retrieve on_tools_registered hook from registry: {}",
                    e
                );
            }
        }
    }

    let updated_tools: Table = event_table.get("tools")?;
    for (i, tool) in tools.iter_mut().enumerate() {
        if let Ok(t) = updated_tools.raw_get::<Table>(i + 1) {
            if let Ok(dn) = t.get::<Value>("display_name") {
                match dn {
                    Value::String(s) => {
                        tool.display_name = Some(s.to_str()?.to_string());
                    }
                    _ => {
                        debug!(
                            "Tool '{}' display_name was set to non-string, using default",
                            tool.name
                        );
                        if tool.display_name.is_none() {
                            tool.display_name = Some(default_display_name_fn(&tool.name));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn test_register_hooks_module() {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();

        register_hooks_module(&lua, &crucible).unwrap();

        let _func: Function = crucible.get("on_session_start").unwrap();
        let _func2: Function = crucible.get("on_tools_registered").unwrap();
    }

    #[test]
    fn test_on_session_start_stores_function() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(r#"crucible.on_session_start(function(s) end)"#)
            .exec()
            .unwrap();

        let hooks_table: Table = lua.globals().get("__crucible_hooks__").unwrap();
        let session_start_hooks: Table = hooks_table.get("on_session_start").unwrap();
        assert_eq!(session_start_hooks.raw_len(), 1);
    }

    #[test]
    fn test_multiple_hooks_append() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_session_start(function(s) end)
            crucible.on_session_start(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks_table: Table = lua.globals().get("__crucible_hooks__").unwrap();
        let session_start_hooks: Table = hooks_table.get("on_session_start").unwrap();
        assert_eq!(session_start_hooks.raw_len(), 3);
    }

    #[test]
    fn test_on_tools_registered_stores_function() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(r#"crucible.on_tools_registered(function(event) end)"#)
            .exec()
            .unwrap();

        let hooks = get_tools_registered_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 1);
    }

    #[test]
    fn test_on_tools_registered_multiple_hooks() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_tools_registered(function(event) end)
            crucible.on_tools_registered(function(event) end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks = get_tools_registered_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 2);
    }

    #[test]
    fn test_fire_tools_registered_hooks_no_hooks_uses_default() {
        let lua = Lua::new();
        let mut tools = vec![
            ToolRegistrationInfo {
                name: "mcp__server__read_file".to_string(),
                description: "Read a file".to_string(),
                display_name: None,
            },
            ToolRegistrationInfo {
                name: "mcp__server__write_file".to_string(),
                description: "Write a file".to_string(),
                display_name: None,
            },
        ];

        fn uppercase_name(name: &str) -> String {
            name.to_uppercase()
        }

        fire_tools_registered_hooks(&lua, &[], "test-server", &mut tools, uppercase_name).unwrap();

        assert_eq!(
            tools[0].display_name.as_deref(),
            Some("MCP__SERVER__READ_FILE")
        );
        assert_eq!(
            tools[1].display_name.as_deref(),
            Some("MCP__SERVER__WRITE_FILE")
        );
    }

    #[test]
    fn test_fire_tools_registered_hooks_modifies_display_name() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_tools_registered(function(event)
                for _, tool in ipairs(event.tools) do
                    tool.display_name = "custom_" .. tool.name
                end
            end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks = get_tools_registered_hooks(&lua).unwrap();
        let mut tools = vec![
            ToolRegistrationInfo {
                name: "search".to_string(),
                description: "Search things".to_string(),
                display_name: None,
            },
            ToolRegistrationInfo {
                name: "read".to_string(),
                description: "Read things".to_string(),
                display_name: None,
            },
        ];

        fn identity(name: &str) -> String {
            name.to_string()
        }

        fire_tools_registered_hooks(&lua, &hooks, "my-server", &mut tools, identity).unwrap();

        assert_eq!(tools[0].display_name.as_deref(), Some("custom_search"));
        assert_eq!(tools[1].display_name.as_deref(), Some("custom_read"));
    }

    #[test]
    fn test_fire_tools_registered_hooks_receives_server_name() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_tools_registered(function(event)
                for _, tool in ipairs(event.tools) do
                    tool.display_name = event.server_name .. ":" .. tool.name
                end
            end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks = get_tools_registered_hooks(&lua).unwrap();
        let mut tools = vec![ToolRegistrationInfo {
            name: "search".to_string(),
            description: "Search".to_string(),
            display_name: None,
        }];

        fn identity(name: &str) -> String {
            name.to_string()
        }

        fire_tools_registered_hooks(&lua, &hooks, "filesystem", &mut tools, identity).unwrap();

        assert_eq!(tools[0].display_name.as_deref(), Some("filesystem:search"));
    }

    #[test]
    fn test_fire_tools_registered_hooks_error_isolation() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_tools_registered(function(event)
                error("intentional failure")
            end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks = get_tools_registered_hooks(&lua).unwrap();
        let mut tools = vec![ToolRegistrationInfo {
            name: "test".to_string(),
            description: "Test".to_string(),
            display_name: None,
        }];

        fn identity(name: &str) -> String {
            name.to_string()
        }

        let result = fire_tools_registered_hooks(&lua, &hooks, "server", &mut tools, identity);
        assert!(result.is_ok(), "Hook errors should not propagate");
    }

    #[test]
    fn test_hooks_independent_of_each_other() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_tools_registered(function(e) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(get_session_start_hooks(&lua).unwrap().len(), 1);
        assert_eq!(get_tools_registered_hooks(&lua).unwrap().len(), 1);
    }
}
