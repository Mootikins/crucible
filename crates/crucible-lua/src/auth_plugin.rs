//! `cru.on_provider_auth` — headers a plugin supplies for a provider call.
//!
//! The hooks used to live in two Lua globals, `__crucible_hooks__` and
//! `__crucible_auth_hooks__`, as a hook-name list, a parallel source list, a
//! name→function map and a counter. The counter was a Lua global, so any
//! plugin could assign it and make the next registration collide with a live
//! hook's slot. They register into the shared store now, under `provider:auth`.

use crucible_core::traits::auth::AuthHeaders;
use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use tracing::{debug, warn};

use crate::handlers::{HookName, StageId};
use crate::handlers::{Registration, RegistrationSpec};

/// The name a provider auth hook registers under.
pub const PROVIDER_AUTH_HOOK: HookName = HookName::Stage(StageId::ProviderAuth);

pub fn register_auth_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    let mut ns = crate::host_registry::Ns::over(lua, "cru", crucible.clone());

    // The handler takes ONE argument, the context table `fire_provider_auth_hooks`
    // builds: `provider` and `model`, both strings, and nothing else.
    //
    // Its RETURN is read, unlike a session hook's. A table of header
    // name/value pairs is used, and so is `{ headers = { … } }` — the same
    // table either way, unwrapped one level when it has a `headers` key. Any
    // other value, `nil` included, means "this hook has no headers", and the
    // next hook is tried. So the return is `...any`: a hook that answers
    // nothing is correct, and the first hook that produces headers wins.
    ns.func(
        "on_provider_auth",
        "(handler: (context: { provider: string, model: string }) -> ...any) -> ()",
        |lua, func: Function| {
            crate::handlers::registry_of(lua)?.register(
                lua,
                RegistrationSpec::new(PROVIDER_AUTH_HOOK),
                func,
            )?;
            Ok(())
        },
    )
    .map_err(|e| mlua::Error::external(e.to_string()))?;
    Ok(())
}

/// Every `provider:auth` hook on this VM, priority first.
///
/// `Sessionless`: the agent factory builds a chat client from an agent
/// config, with no session in hand. `StageId::carries_session` says the same,
/// so a scoped registration here is refused rather than dropped here.
pub fn get_provider_auth_hooks(lua: &Lua) -> LuaResult<Vec<Registration>> {
    Ok(crate::handlers::registry_of(lua)?.for_hook(
        PROVIDER_AUTH_HOOK,
        None,
        crate::handlers::Firing::Sessionless,
    ))
}

/// Ask each hook in turn for headers; the first that answers wins.
///
/// Synchronous, like the permission gate: the provider factory holds no
/// runtime handle here. The VM deadline is armed all the same, so one hook
/// spinning cannot hold the factory forever.
pub fn fire_provider_auth_hooks(
    lua: &Lua,
    hooks: &[Registration],
    provider_name: &str,
    model: &str,
) -> LuaResult<Option<AuthHeaders>> {
    if hooks.is_empty() {
        return Ok(None);
    }

    let context = lua.create_table()?;
    context.set("provider", provider_name)?;
    context.set("model", model)?;

    let _budget =
        crate::handler_budget::enter(lua, PROVIDER_AUTH_HOOK.budget(), "the provider auth hook");

    for hook in hooks {
        let handler: Function = match hook.take_body(lua) {
            Ok(handler) => handler,
            Err(e) => {
                warn!("Failed to load provider auth hook {}: {e}", hook.id);
                continue;
            }
        };

        // The source the registration recorded, so a hook reaching
        // `cru.storage` for a stored token finds its own namespace.
        let previous = crate::plugin_context::set_source(lua, hook.source.clone());
        let result = handler.call::<Value>(context.clone());
        crate::plugin_context::set_source(lua, previous);

        let result = match result {
            Ok(result) => result,
            Err(e) => {
                warn!("Provider auth hook {} failed: {e}", hook.id);
                continue;
            }
        };

        let headers = match result {
            Value::Nil => None,
            Value::Table(table) => table_to_auth_headers(table)?,
            _ => {
                debug!(
                    "Provider auth hook {} returned non-table result; ignoring",
                    hook.id
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
    use crate::plugin_context::LuaSource;

    fn setup() -> Lua {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();
        register_auth_module(&lua, &cru).unwrap();
        lua.globals().set("cru", cru).unwrap();
        lua
    }

    fn register(lua: &Lua, source: LuaSource, header: &str) {
        crate::plugin_context::set_source(lua, source);
        lua.load(format!(
            r#"cru.on_provider_auth(function(ctx) return {{ headers = {{ ["X-Who"] = "{header}" }} }} end)"#
        ))
        .exec()
        .unwrap();
    }

    /// Auth hooks follow the same source contract as every other registration:
    /// a plugin's reload clears exactly its own, the user's own survive, and
    /// an id freed by clearing is never reissued — reuse would silently
    /// rebind a surviving hook's slot to the new function.
    #[test]
    fn clearing_an_owners_auth_hooks_keeps_others_and_never_reissues_an_id() {
        let lua = setup();
        let registry = crate::handlers::registry_of(&lua).unwrap();
        register(&lua, LuaSource::Plugin("alpha".into()), "alpha");
        register(&lua, LuaSource::Plugin("beta".into()), "beta");
        register(&lua, LuaSource::UserLua, "user");

        registry.clear_source(&LuaSource::Plugin("alpha".into()));

        let hooks = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 2, "beta's and the user's hook survive");
        // First surviving hook is beta's — fire proves the binding survived.
        let headers = fire_provider_auth_hooks(&lua, &hooks, "prov", "model")
            .unwrap()
            .expect("beta answers");
        assert_eq!(headers.get("X-Who"), Some(&"beta".to_string()));

        // A fresh registration must not reuse an id any live hook holds.
        let live: Vec<u64> = hooks.iter().map(|h| h.id).collect();
        register(&lua, LuaSource::Plugin("gamma".into()), "gamma");
        let after = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(after.len(), 3);
        let fresh = after.last().unwrap().id;
        assert!(
            !live.contains(&fresh),
            "id {fresh} was reissued while {live:?} still hold it"
        );
        // And beta still fires its own function, not gamma's.
        let headers = fire_provider_auth_hooks(&lua, &after, "prov", "model")
            .unwrap()
            .expect("first answer wins");
        assert_eq!(headers.get("X-Who"), Some(&"beta".to_string()));

        registry.clear_source(&LuaSource::Plugin("beta".into()));
        registry.clear_source(&LuaSource::Plugin("gamma".into()));
        let last = get_provider_auth_hooks(&lua).unwrap();
        assert_eq!(last.len(), 1, "only the user's hook survives every clear");
        let headers = fire_provider_auth_hooks(&lua, &last, "prov", "model")
            .unwrap()
            .expect("the user's hook still fires its own function");
        assert_eq!(headers.get("X-Who"), Some(&"user".to_string()));
    }
}
