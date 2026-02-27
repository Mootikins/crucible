use crucible_core::traits::auth::AuthHeaders;
use mlua::{Function, Lua, RegistryKey, Result as LuaResult, Table, Value};
use tracing::{debug, warn};

pub struct AuthHook {
    pub name: String,
}

pub fn register_auth_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let on_provider_auth = lua.create_function(|lua, func: Function| {
        let key = lua.create_registry_value(func)?;

        let globals = lua.globals();

        let hooks_table: Table = globals
            .get("__crucible_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let provider_auth_hooks: Table = hooks_table
            .get("on_provider_auth")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let auth_hook_functions: Table = globals
            .get("__crucible_auth_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let len = provider_auth_hooks.raw_len();
        let hook_name = format!("provider_auth_hook_{}", len + 1);
        provider_auth_hooks.raw_set(len + 1, hook_name.as_str())?;
        auth_hook_functions.set(hook_name.as_str(), key)?;

        hooks_table.set("on_provider_auth", provider_auth_hooks)?;
        globals.set("__crucible_hooks__", hooks_table)?;
        globals.set("__crucible_auth_hooks__", auth_hook_functions)?;

        Ok(())
    })?;

    crucible.set("on_provider_auth", on_provider_auth)?;
    Ok(())
}

pub fn get_provider_auth_hooks(lua: &Lua) -> LuaResult<Vec<AuthHook>> {
    let globals = lua.globals();
    let hooks_table: Table = match globals.get("__crucible_hooks__") {
        Ok(table) => table,
        Err(_) => return Ok(Vec::new()),
    };

    let provider_auth_hooks: Table = match hooks_table.get("on_provider_auth") {
        Ok(table) => table,
        Err(_) => return Ok(Vec::new()),
    };

    let mut hooks = Vec::new();
    for i in 1..=provider_auth_hooks.raw_len() {
        if let Ok(name) = provider_auth_hooks.raw_get::<String>(i) {
            hooks.push(AuthHook { name });
        }
    }

    Ok(hooks)
}

pub fn fire_provider_auth_hooks(
    lua: &Lua,
    hooks: &[AuthHook],
    provider_name: &str,
    model: &str,
) -> LuaResult<Option<AuthHeaders>> {
    if hooks.is_empty() {
        return Ok(None);
    }

    let globals = lua.globals();
    let auth_hook_functions: Table = match globals.get("__crucible_auth_hooks__") {
        Ok(table) => table,
        Err(_) => return Ok(None),
    };

    let context = lua.create_table()?;
    context.set("provider", provider_name)?;
    context.set("model", model)?;

    for hook in hooks {
        let key: RegistryKey = match auth_hook_functions.get(hook.name.as_str()) {
            Ok(key) => key,
            Err(_) => {
                warn!("Provider auth hook '{}' not found in registry", hook.name);
                continue;
            }
        };

        let handler: Function = match lua.registry_value(&key) {
            Ok(handler) => handler,
            Err(e) => {
                warn!(
                    "Failed to load provider auth hook '{}' from registry: {}",
                    hook.name, e
                );
                continue;
            }
        };

        let result: Value = match handler.call(context.clone()) {
            Ok(result) => result,
            Err(e) => {
                warn!("Provider auth hook '{}' failed: {}", hook.name, e);
                continue;
            }
        };

        let headers = match result {
            Value::Nil => None,
            Value::Table(table) => table_to_auth_headers(table)?,
            _ => {
                debug!(
                    "Provider auth hook '{}' returned non-table result; ignoring",
                    hook.name
                );
                None
            }
        };

        if headers.is_some() {
            return Ok(headers);
        }
    }

    Ok(None)
}

fn table_to_auth_headers(result_table: Table) -> LuaResult<Option<AuthHeaders>> {
    let header_table = match result_table.get::<Value>("headers") {
        Ok(Value::Table(headers)) => headers,
        Ok(Value::Nil) | Err(_) => result_table,
        Ok(_) => return Ok(None),
    };

    let mut headers = AuthHeaders::new();
    for pair in header_table.pairs::<String, String>() {
        match pair {
            Ok((name, value)) => {
                headers.insert(name, value);
            }
            Err(e) => {
                debug!("Skipping invalid auth header entry: {}", e);
            }
        }
    }

    if headers.is_empty() {
        Ok(None)
    } else {
        Ok(Some(headers))
    }
}
