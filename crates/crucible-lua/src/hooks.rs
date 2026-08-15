//! Hook registration system for Crucible Lua API
//!
//! Provides `crucible.on_session_start(fn)` and `crucible.on_session_end(fn)`
//! for registering lifecycle hooks. (`crucible.on_tools_registered` existed
//! here for months without a single fire site anywhere in the daemon —
//! registered, synced, stored, never called. Deleted rather than documented.)
//!
//! Tool execution hooks use the RuntimeHandler system via `crucible.on("tool:before_execute", fn)`.
//! See `handlers.rs` for details.
//!
//! Hooks are owner-tagged: registration reads `__crucible_loading_plugin__`
//! (set by the daemon plugin loader around each plugin's execution) so that
//! [`clear_plugin_hooks`] can remove exactly one plugin's hooks on its reload,
//! leaving unowned registrations — the user's `init.lua` — untouched.
//! Per-session VMs (`agent_manager/session_vm.rs`) never set the loading
//! marker, so all their hooks are unowned by construction; `clear_plugin_hooks`
//! is never called against a session VM and no change is needed there.

use mlua::{Function, Lua, Result as LuaResult, Table};

/// Register the hooks module on the crucible table
///
/// This function is called during executor initialization to set up hook registration.
/// Hooks are stored in a Lua table that the executor can access via `get_session_start_hooks()`
/// and `get_session_end_hooks()`.
///
/// # Example
///
/// ```lua
/// crucible.on_session_start(function(session)
///     session.temperature = 0.5
/// end)
/// ```
pub fn register_hooks_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    // `crucible.on_session_start(fn)` — a failure is logged and the session
    // continues. `crucible.on_session_start(fn, { required = true })` — a
    // failure refuses the session.
    //
    // Opt-in deliberately. A hook that owns an isolation boundary (`oci` and
    // its container) must be able to stop a session that would otherwise run
    // unsandboxed. But making every hook fatal means one typo in any plugin
    // bricks session creation daemon-wide, so the default has to be the safe
    // one for ordinary plugins.
    let on_session_start =
        lua.create_function(|lua, (func, opts): (Function, Option<Table>)| {
            let required = opts
                .and_then(|o| o.get::<Option<bool>>("required").ok().flatten())
                .unwrap_or(false);
            let owner = plugin_owner(lua);
            let key = lua.create_registry_value(func)?;

            let globals = lua.globals();
            let hooks_table: Table = globals
                .get("__crucible_hooks__")
                .unwrap_or_else(|_| lua.create_table().unwrap());

            let session_start_hooks: Table = hooks_table
                .get("on_session_start")
                .unwrap_or_else(|_| lua.create_table().unwrap());
            // Parallel to the hook list by index; kept separate because the hook
            // slot holds a registry key, not a table we can hang a flag off.
            let required_flags: Table = hooks_table
                .get("on_session_start_required")
                .unwrap_or_else(|_| lua.create_table().unwrap());
            let owners: Table = hooks_table
                .get("on_session_start_owners")
                .unwrap_or_else(|_| lua.create_table().unwrap());

            let len = session_start_hooks.raw_len();
            session_start_hooks.raw_set(len + 1, key)?;
            required_flags.raw_set(len + 1, required)?;
            owners.raw_set(len + 1, owner)?;

            hooks_table.set("on_session_start", session_start_hooks)?;
            hooks_table.set("on_session_start_required", required_flags)?;
            hooks_table.set("on_session_start_owners", owners)?;
            globals.set("__crucible_hooks__", hooks_table)?;

            Ok(())
        })?;

    crucible.set("on_session_start", on_session_start)?;

    let on_session_end = lua.create_function(|lua, func: Function| {
        let owner = plugin_owner(lua);
        let key = lua.create_registry_value(func)?;

        let globals = lua.globals();
        let hooks_table: Table = globals
            .get("__crucible_hooks__")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let session_end_hooks: Table = hooks_table
            .get("on_session_end")
            .unwrap_or_else(|_| lua.create_table().unwrap());
        let owners: Table = hooks_table
            .get("on_session_end_owners")
            .unwrap_or_else(|_| lua.create_table().unwrap());

        let len = session_end_hooks.raw_len();
        session_end_hooks.raw_set(len + 1, key)?;
        owners.raw_set(len + 1, owner)?;

        hooks_table.set("on_session_end", session_end_hooks)?;
        hooks_table.set("on_session_end_owners", owners)?;
        globals.set("__crucible_hooks__", hooks_table)?;

        Ok(())
    })?;

    crucible.set("on_session_end", on_session_end)?;

    // Tool execution hooks use the RuntimeHandler system via crucible.on("tool:before_execute", fn).
    // See handlers.rs for execute_tool_before_execute_hooks().

    Ok(())
}

/// The plugin currently being loaded, or `false` when none is (user init.lua,
/// session VMs). `false` rather than nil because the owner slots live in Lua
/// array tables, and a nil mid-sequence truncates `raw_len`.
fn plugin_owner(lua: &Lua) -> mlua::Value {
    match lua
        .globals()
        .get::<Option<mlua::LuaString>>("__crucible_loading_plugin__")
    {
        Ok(Some(name)) => mlua::Value::String(name),
        _ => mlua::Value::Boolean(false),
    }
}

/// Remove every session hook registered by `plugin`; other plugins' hooks and
/// unowned (user-registered) hooks survive.
///
/// Each hook list is rebuilt together with ALL of its parallel tables in one
/// pass so the indices stay aligned. The tables hold the hook functions
/// themselves (`raw_set` converts the registration's `RegistryKey` through
/// `IntoLua`; the getters mint fresh keys per read), so dropping an entry here
/// drops the only reference — no registry-expiry choreography is involved.
pub fn clear_plugin_hooks(lua: &Lua, plugin: &str) -> LuaResult<()> {
    let globals = lua.globals();
    let Ok(hooks_table) = globals.get::<Table>("__crucible_hooks__") else {
        return Ok(());
    };

    // Start hooks carry a parallel `required` table; end hooks take no opts,
    // so their pair is hooks + owners only.
    retain_other_owners(
        lua,
        &hooks_table,
        plugin,
        "on_session_start",
        "on_session_start_owners",
        &["on_session_start_required"],
    )?;
    retain_other_owners(
        lua,
        &hooks_table,
        plugin,
        "on_session_end",
        "on_session_end_owners",
        &[],
    )?;
    // Auth hooks live in the same `__crucible_hooks__` container but carry a
    // name→function side table, so their clearing is owned by their module.
    crate::auth_plugin::clear_plugin_auth_hooks(lua, plugin)?;
    Ok(())
}

/// Rebuild `list_name` (plus its owner table and any extra parallel tables)
/// keeping only entries whose owner is not `plugin`.
fn retain_other_owners(
    lua: &Lua,
    hooks_table: &Table,
    plugin: &str,
    list_name: &str,
    owners_name: &str,
    extra_names: &[&str],
) -> LuaResult<()> {
    let Ok(hooks) = hooks_table.get::<Table>(list_name) else {
        return Ok(());
    };
    let Ok(owners) = hooks_table.get::<Table>(owners_name) else {
        // No owner table means nothing was ever attributed — all unowned.
        return Ok(());
    };
    let extras: Vec<Table> = extra_names
        .iter()
        .filter_map(|name| hooks_table.get::<Table>(*name).ok())
        .collect();

    let new_hooks = lua.create_table()?;
    let new_owners = lua.create_table()?;
    let new_extras: Vec<Table> = extras
        .iter()
        .map(|_| lua.create_table())
        .collect::<LuaResult<_>>()?;

    let mut kept = 0;
    for i in 1..=hooks.raw_len() {
        let owner: mlua::Value = owners.raw_get(i)?;
        let owned_by_plugin = match &owner {
            mlua::Value::String(s) => s.to_str().is_ok_and(|s| &*s == plugin),
            _ => false,
        };
        if owned_by_plugin {
            continue;
        }
        kept += 1;
        new_hooks.raw_set(kept, hooks.raw_get::<mlua::Value>(i)?)?;
        new_owners.raw_set(kept, owner)?;
        for (old, new) in extras.iter().zip(&new_extras) {
            new.raw_set(kept, old.raw_get::<mlua::Value>(i)?)?;
        }
    }

    hooks_table.set(list_name, new_hooks)?;
    hooks_table.set(owners_name, new_owners)?;
    for (name, new) in extra_names.iter().zip(new_extras) {
        hooks_table.set(*name, new)?;
    }
    Ok(())
}

pub fn get_session_start_hooks(lua: &Lua) -> LuaResult<Vec<mlua::RegistryKey>> {
    get_hooks_by_name(lua, "on_session_start")
}

/// Which start hooks opted into refusing the session on failure, by index.
///
/// Parallel to [`get_session_start_hooks`]; a missing entry means `false`, so
/// a hook registered before this flag existed stays non-fatal.
pub fn get_session_start_required_flags(lua: &Lua) -> LuaResult<Vec<bool>> {
    let globals = lua.globals();
    let Ok(hooks_table) = globals.get::<Table>("__crucible_hooks__") else {
        return Ok(Vec::new());
    };
    let Ok(flags) = hooks_table.get::<Table>("on_session_start_required") else {
        return Ok(Vec::new());
    };
    let len = flags.raw_len();
    let mut out = Vec::with_capacity(len);
    for i in 1..=len {
        out.push(flags.raw_get::<Option<bool>>(i)?.unwrap_or(false));
    }
    Ok(out)
}

pub fn get_session_end_hooks(lua: &Lua) -> LuaResult<Vec<mlua::RegistryKey>> {
    get_hooks_by_name(lua, "on_session_end")
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
    fn test_hooks_independent_of_each_other() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_session_end(function(e) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(get_session_start_hooks(&lua).unwrap().len(), 1);
        assert_eq!(get_session_end_hooks(&lua).unwrap().len(), 1);
    }

    #[test]
    fn test_on_session_end_stores_function() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(r#"crucible.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();

        let hooks = get_session_end_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 1);
    }

    #[test]
    fn test_on_session_end_multiple_hooks() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_session_end(function(s) end)
            crucible.on_session_end(function(s) end)
            crucible.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        let hooks = get_session_end_hooks(&lua).unwrap();
        assert_eq!(hooks.len(), 3);
    }

    /// Reloading a plugin re-runs its init.lua, so without owner-tagged
    /// clearing its session hooks accumulate one copy per reload. oci's
    /// on_session_start owns a container isolation boundary and is
    /// `required = true` — running it twice is not cosmetic.
    #[test]
    fn clearing_a_plugins_hooks_removes_only_that_plugins_and_keeps_flags_aligned() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.globals()
            .set("__crucible_loading_plugin__", "alpha")
            .unwrap();
        lua.load(r#"crucible.on_session_start(function(s) end, { required = true })"#)
            .exec()
            .unwrap();
        lua.globals()
            .set("__crucible_loading_plugin__", "beta")
            .unwrap();
        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();
        lua.globals()
            .set("__crucible_loading_plugin__", mlua::Value::Nil)
            .unwrap();
        // Unowned hook — user init.lua shape. Must survive every clear.
        lua.load(r#"crucible.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();

        clear_plugin_hooks(&lua, "alpha").unwrap();
        assert_eq!(
            get_session_start_hooks(&lua).unwrap().len(),
            1,
            "beta's start hook survives"
        );
        // Flags rebuilt in lockstep — the survivor is beta's non-required hook:
        assert_eq!(get_session_start_required_flags(&lua).unwrap(), vec![false]);
        assert_eq!(get_session_end_hooks(&lua).unwrap().len(), 2);

        clear_plugin_hooks(&lua, "beta").unwrap();
        assert_eq!(get_session_start_hooks(&lua).unwrap().len(), 0);
        assert_eq!(
            get_session_end_hooks(&lua).unwrap().len(),
            1,
            "the unowned hook is never cleared"
        );
    }

    #[test]
    fn test_session_start_and_end_hooks_independent() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            crucible.on_session_start(function(s) end)
            crucible.on_session_end(function(s) end)
            crucible.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(get_session_start_hooks(&lua).unwrap().len(), 1);
        assert_eq!(get_session_end_hooks(&lua).unwrap().len(), 2);
    }
}
