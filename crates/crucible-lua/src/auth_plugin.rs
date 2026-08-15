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

        // Parallel to the hook list by index, same contract as the session
        // hooks in `hooks.rs`: `false` marks an unowned registration (user
        // init.lua), which no plugin's clear ever removes.
        let owners: Table = hooks_table
            .get("on_provider_auth_owners")
            .unwrap_or_else(|_| lua.create_table().unwrap());
        let owner = globals
            .get::<Option<mlua::LuaString>>("__crucible_loading_plugin__")
            .ok()
            .flatten();

        let auth_hook_functions: Table = globals
            .get("__crucible_auth_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        // Monotonic, never derived from the list length: clearing shrinks
        // the list, and a length-derived name would then collide with one a
        // surviving hook still holds in `__crucible_auth_hooks__` — dispatch
        // is by name, so the collision rebinds the survivor to the new
        // function (the same defect `crucible_on.rs` documents at length).
        let seq: u64 = globals
            .get::<Option<u64>>("__crucible_auth_hook_seq__")
            .ok()
            .flatten()
            .unwrap_or(0);
        globals.set("__crucible_auth_hook_seq__", seq + 1)?;
        let hook_name = format!("provider_auth_hook_{seq}");

        let len = provider_auth_hooks.raw_len();
        provider_auth_hooks.raw_set(len + 1, hook_name.as_str())?;
        match owner {
            Some(ref o) => owners.raw_set(len + 1, o)?,
            None => owners.raw_set(len + 1, false)?,
        }
        auth_hook_functions.set(hook_name.as_str(), key)?;

        hooks_table.set("on_provider_auth", provider_auth_hooks)?;
        hooks_table.set("on_provider_auth_owners", owners)?;
        globals.set("__crucible_hooks__", hooks_table)?;
        globals.set("__crucible_auth_hooks__", auth_hook_functions)?;

        Ok(())
    })?;

    crucible.set("on_provider_auth", on_provider_auth)?;
    Ok(())
}

/// Drop the auth hooks `plugin` registered, rebuilding the hook and owner
/// lists in lockstep and deleting the cleared names from
/// `__crucible_auth_hooks__` so the function references are released.
/// Called from [`crate::hooks::clear_plugin_hooks`], the single entry point
/// the daemon uses when a plugin re-executes or is made inert.
pub(crate) fn clear_plugin_auth_hooks(lua: &Lua, plugin: &str) -> LuaResult<()> {
    let globals = lua.globals();
    let Ok(hooks_table) = globals.get::<Table>("__crucible_hooks__") else {
        return Ok(());
    };
    let Ok(hooks) = hooks_table.get::<Table>("on_provider_auth") else {
        return Ok(());
    };
    let Ok(owners) = hooks_table.get::<Table>("on_provider_auth_owners") else {
        return Ok(());
    };
    let auth_hook_functions: Table = globals
        .get("__crucible_auth_hooks__")
        .unwrap_or_else(|_| lua.create_table().unwrap());

    let kept_hooks = lua.create_table()?;
    let kept_owners = lua.create_table()?;
    for i in 1..=hooks.raw_len() {
        // A missing owner entry means unowned — always kept.
        let owned_by_plugin = owners
            .raw_get::<Option<mlua::LuaString>>(i)
            .ok()
            .flatten()
            .is_some_and(|o| o.to_string_lossy() == plugin);
        let name: String = hooks.raw_get(i)?;
        if owned_by_plugin {
            auth_hook_functions.set(name, mlua::Value::Nil)?;
        } else {
            let idx = kept_hooks.raw_len() + 1;
            kept_hooks.raw_set(idx, name)?;
            kept_owners.raw_set(idx, owners.raw_get::<mlua::Value>(i)?)?;
        }
    }

    hooks_table.set("on_provider_auth", kept_hooks)?;
    hooks_table.set("on_provider_auth_owners", kept_owners)?;
    globals.set("__crucible_auth_hooks__", auth_hook_functions)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        register_auth_module(&lua, &crucible).unwrap();
        lua.globals().set("crucible", crucible).unwrap();
        lua
    }

    fn register(lua: &Lua, owner: Option<&str>, header: &str) {
        match owner {
            Some(o) => lua.globals().set("__crucible_loading_plugin__", o).unwrap(),
            None => lua
                .globals()
                .set("__crucible_loading_plugin__", mlua::Value::Nil)
                .unwrap(),
        }
        lua.load(format!(
            r#"crucible.on_provider_auth(function(ctx) return {{ headers = {{ ["X-Who"] = "{header}" }} }} end)"#
        ))
        .exec()
        .unwrap();
    }

    /// Auth hooks follow the same owner-tag contract as session hooks: a
    /// plugin's reload clears exactly its own registrations, unowned ones
    /// survive, and a name freed by clearing is never reissued — reuse would
    /// silently rebind a surviving hook's slot to the new function.
    #[test]
    fn clearing_a_plugins_auth_hooks_keeps_others_and_never_reissues_names() {
        let lua = setup();
        register(&lua, Some("alpha"), "alpha");
        register(&lua, Some("beta"), "beta");
        register(&lua, None, "user");

        crate::hooks::clear_plugin_hooks(&lua, "alpha").unwrap();

        let hooks = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 2, "beta's and the unowned hook survive");
        // First surviving hook is beta's — fire proves the binding survived.
        let headers = fire_provider_auth_hooks(&lua, &hooks, "prov", "model")
            .unwrap()
            .expect("beta answers");
        assert_eq!(headers.get("X-Who"), Some(&"beta".to_string()));

        // A fresh registration must not reuse a name any live hook holds.
        let live: Vec<String> = hooks.iter().map(|h| h.name.clone()).collect();
        register(&lua, Some("gamma"), "gamma");
        let after = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(after.len(), 3);
        let fresh = &after.last().unwrap().name;
        assert!(
            !live.contains(fresh),
            "name '{fresh}' was reissued while {live:?} still hold it"
        );
        // And beta still fires its own function, not gamma's.
        let headers = fire_provider_auth_hooks(&lua, &after, "prov", "model")
            .unwrap()
            .expect("first answer wins");
        assert_eq!(headers.get("X-Who"), Some(&"beta".to_string()));

        // A SECOND clear exercises the rebuilt tables — the one path where a
        // skew introduced by the first rebuild would surface.
        crate::hooks::clear_plugin_hooks(&lua, "beta").unwrap();
        crate::hooks::clear_plugin_hooks(&lua, "gamma").unwrap();
        let last = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(last.len(), 1, "only the unowned hook survives every clear");
        let headers = fire_provider_auth_hooks(&lua, &last, "prov", "model")
            .unwrap()
            .expect("the unowned hook still fires its own function");
        assert_eq!(headers.get("X-Who"), Some(&"user".to_string()));
    }
}
