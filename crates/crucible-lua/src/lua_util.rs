use mlua::{Lua, Result as LuaResult, Table};

pub fn get_or_create_namespace(lua: &Lua, name: &str) -> LuaResult<Table> {
    let globals = lua.globals();
    globals.get(name).or_else(|_: mlua::Error| {
        let t = lua.create_table()?;
        globals.set(name, t.clone())?;
        Ok(t)
    })
}

pub fn register_in_namespaces(lua: &Lua, module_name: &str, module: Table) -> LuaResult<()> {
    get_or_create_namespace(lua, "crucible")?.set(module_name, module.clone())?;
    get_or_create_namespace(lua, "cru")?.set(module_name, module)?;
    Ok(())
}
