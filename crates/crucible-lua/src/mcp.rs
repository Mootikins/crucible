//! MCP module stub for Lua
//!
//! Registers a global `mcp` table whose functions return empty/error results.
//! Plugins reach real MCP servers through the daemon's gateway tools, not
//! through these bindings — the stub exists so scripts that reference `mcp.*`
//! (and the stub generator, which introspects the VM) see a stable surface
//! instead of a nil global.

use crate::error::LuaError;
use mlua::{Lua, Table, Value};

/// Register MCP module without a client (stub for testing)
///
/// Creates a global `mcp` module with stub functions that return empty results.
/// Useful for testing Lua scripts that reference MCP but don't need real connections.
pub fn register_mcp_module_stub(lua: &Lua) -> Result<(), LuaError> {
    let mcp = lua.create_table()?;

    // Stub list_tools - returns empty array
    mcp.set(
        "list_tools",
        lua.create_function(|lua, _server: String| lua.create_table())?,
    )?;

    // Stub call - returns error
    mcp.set(
        "call",
        lua.create_function(|lua, (_server, _tool, _args): (String, String, Table)| {
            let result = lua.create_table()?;
            result.set("success", false)?;
            result.set("error", "MCP not configured (stub mode)")?;
            Ok(Value::Table(result))
        })?,
    )?;

    // Stub call_json - returns error
    mcp.set(
        "call_json",
        lua.create_function(|lua, (_server, _tool, _args): (String, String, String)| {
            let result = lua.create_table()?;
            result.set("success", false)?;
            result.set("error", "MCP not configured (stub mode)")?;
            Ok(Value::Table(result))
        })?,
    )?;

    // Stub servers - returns empty array
    mcp.set(
        "servers",
        lua.create_function(|lua, ()| lua.create_table())?,
    )?;

    // Stub has_tool - returns false
    mcp.set(
        "has_tool",
        lua.create_function(|_, (_server, _tool): (String, String)| Ok(false))?,
    )?;

    lua.globals().set("mcp", mcp.clone())?;
    crate::lua_util::register_in_namespaces(lua, "mcp", mcp)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn test_mcp_stub() {
        let lua = TestLuaBuilder::new().build();
        register_mcp_module_stub(&lua).unwrap();

        // All operations should work but return empty/error results
        let tools: Table = lua.load(r#"return mcp.list_tools("any")"#).eval().unwrap();
        assert_eq!(tools.raw_len(), 0);

        let servers: Table = lua.load(r#"return mcp.servers()"#).eval().unwrap();
        assert_eq!(servers.raw_len(), 0);

        let has: bool = lua
            .load(r#"return mcp.has_tool("any", "tool")"#)
            .eval()
            .unwrap();
        assert!(!has);

        let result: Table = lua
            .load(r#"return mcp.call("any", "tool", {})"#)
            .eval()
            .unwrap();
        assert!(!result.get::<bool>("success").unwrap());
        assert!(result.get::<String>("error").unwrap().contains("stub mode"));
    }
}
