//! Session lifecycle hook registration for the Crucible Lua API.
//!
//! Provides `cru.on_session_start(fn)` and `cru.on_session_end(fn)`.
//! (`cru.on_tools_registered` existed here for months without a single fire
//! site anywhere in the daemon — registered, synced, stored, never called.
//! Deleted rather than documented.)
//!
//! Both register into the one shared store, under `session:start` and
//! `session:end`. Six Lua tables under `__crucible_hooks__` held them before:
//! the hook list, a parallel `required` list and a parallel owner list, twice
//! over, each rebuilt in lockstep by a clear path that had to keep three
//! indices aligned. The store keys by owner instead, so `clear_source` removes
//! a plugin's hooks with no index arithmetic at all.
//!
//! The APIs stay separate from `cru.on` because their ARGUMENT differs: a
//! session hook is called with the session handle alone, and a `cru.on`
//! handler with `(ctx, event)`. A store is not a dispatcher.

use mlua::{Function, Lua, Result as LuaResult, Table};

use crate::handlers::{scope_from_opts, Firing, HookName, StageId};
use crate::handlers::{Registration, RegistrationSpec};

/// The name a session start hook registers under.
pub const SESSION_START_HOOK: HookName = HookName::Stage(StageId::SessionStart);
/// The name a session end hook registers under.
pub const SESSION_END_HOOK: HookName = HookName::Stage(StageId::SessionEnd);

/// What a session lifecycle hook is called with: the session HANDLE, and
/// nothing else.
///
/// One argument, from the two fire sites — `LuaExecutor::call_lifecycle_hook`
/// (`func.call_async::<()>(session.clone())`) and the synchronous
/// `fire_session_start_hooks` (`func.call::<()>(lua_session.clone())`). A
/// two-parameter handler type would reject every correct hook a plugin has.
///
/// The handle is userdata, which a declaration has no name for, so it is
/// `any`. The return is `...any` rather than `()` because both fire sites
/// DISCARD the result: a hook that ends in `return something` is correct, and
/// a declared `-> ()` would make it a type error.
const SESSION_HOOK: &str = "(session: any) -> ...any";

/// Register the lifecycle hooks on the given `cru` table
///
/// This function is called during executor initialization to set up hook
/// registration. Hooks land in the VM's shared registration store, which
/// [`session_start_hooks`] and [`session_end_hooks`] read back.
///
/// # Example
///
/// ```lua
/// cru.on_session_start(function(session)
/// end)
/// ```
pub fn register_hooks_module(lua: &Lua, crucible: &Table) -> LuaResult<()> {
    // `cru.on_session_start(fn)` — a failure is logged and the session
    // continues. `cru.on_session_start(fn, { required = true })` — a
    // failure refuses the session.
    //
    // Opt-in deliberately. A hook that owns an isolation boundary (`oci` and
    // its container) must be able to stop a session that would otherwise run
    // unsandboxed. But making every hook fatal means one typo in any plugin
    // bricks session creation daemon-wide, so the default has to be the safe
    // one for ordinary plugins.
    let mut ns = crate::host_registry::Ns::over(lua, "cru", crucible.clone());
    ns.func(
        "on_session_start",
        &format!(
            "(handler: {SESSION_HOOK}, options: {{ required: boolean?, \
             session: string?, key: string? }}?) -> ()"
        ),
        |lua, (func, opts): (Function, Option<Table>)| {
            let mut spec = RegistrationSpec::new(SESSION_START_HOOK);
            if let Some(opts) = &opts {
                spec.required = opts.get::<Option<bool>>("required").ok().flatten() == Some(true);
                let (scope, key) =
                    scope_from_opts(lua, "cru.on_session_start", SESSION_START_HOOK, opts)?;
                spec.scope = scope;
                spec.key = key;
            }
            crate::handlers::registry_of(lua)?.register(lua, spec, func)?;
            Ok(())
        },
    )
    .map_err(|e| mlua::Error::external(e.to_string()))?;

    // The options table carries no `required`: an end hook has nothing to
    // escalate — the session is already over, so a failure is logged and that
    // is all. It does carry the scope, so a plugin activated for one session
    // can tear down for that session alone. The sweep runs after these fire.
    ns.func(
        "on_session_end",
        &format!("(handler: {SESSION_HOOK}, options: {{ session: string?, key: string? }}?) -> ()"),
        |lua, (func, opts): (Function, Option<Table>)| {
            let mut spec = RegistrationSpec::new(SESSION_END_HOOK);
            if let Some(opts) = &opts {
                let (scope, key) =
                    scope_from_opts(lua, "cru.on_session_end", SESSION_END_HOOK, opts)?;
                spec.scope = scope;
                spec.key = key;
            }
            crate::handlers::registry_of(lua)?.register(lua, spec, func)?;
            Ok(())
        },
    )
    .map_err(|e| mlua::Error::external(e.to_string()))?;

    // Tool execution hooks use the shared store via cru.on("tool:before_execute", fn).
    // See handlers/mod.rs.

    Ok(())
}

/// Every `session:start` hook on this VM that serves `firing`, priority
/// first.
pub fn session_start_hooks(lua: &Lua, firing: Firing<'_>) -> LuaResult<Vec<Registration>> {
    Ok(crate::handlers::registry_of(lua)?.for_hook(SESSION_START_HOOK, None, firing))
}

/// Every `session:end` hook on this VM that serves `firing`, priority first.
pub fn session_end_hooks(lua: &Lua, firing: Firing<'_>) -> LuaResult<Vec<Registration>> {
    Ok(crate::handlers::registry_of(lua)?.for_hook(SESSION_END_HOOK, None, firing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_context::LuaSource;
    use crate::test_support::TestLuaBuilder;

    #[test]
    fn test_register_hooks_module() {
        let lua = Lua::new();
        let cru = lua.create_table().unwrap();

        register_hooks_module(&lua, &cru).unwrap();

        let _func: Function = cru.get("on_session_start").unwrap();
    }

    #[test]
    fn test_on_session_start_stores_function() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(r#"cru.on_session_start(function(s) end)"#)
            .exec()
            .unwrap();

        assert_eq!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_multiple_hooks_append() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            cru.on_session_start(function(s) end)
            cru.on_session_start(function(s) end)
            cru.on_session_start(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn test_hooks_independent_of_each_other() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            cru.on_session_start(function(s) end)
            cru.on_session_end(function(e) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_on_session_end_stores_function() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(r#"cru.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();

        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn test_on_session_end_multiple_hooks() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            cru.on_session_end(function(s) end)
            cru.on_session_end(function(s) end)
            cru.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            3
        );
    }

    /// Reloading a plugin re-runs its init.lua, so without owner-keyed
    /// clearing its session hooks accumulate one copy per reload. oci's
    /// on_session_start owns a container isolation boundary and is
    /// `required = true` — running it twice is not cosmetic.
    #[test]
    fn clearing_an_owner_removes_only_that_owners_hooks_and_keeps_the_flags() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();
        let registry = crate::handlers::registry_of(&lua).unwrap();

        crate::plugin_context::enter_plugin(&lua, "alpha");
        lua.load(r#"cru.on_session_start(function(s) end, { required = true })"#)
            .exec()
            .unwrap();
        crate::plugin_context::enter_plugin(&lua, "beta");
        lua.load(
            r#"
            cru.on_session_start(function(s) end)
            cru.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();
        crate::plugin_context::set_source(&lua, LuaSource::UserLua);
        // The user's own init.lua. A plugin's clear must never touch it.
        lua.load(r#"cru.on_session_end(function(s) end)"#)
            .exec()
            .unwrap();

        assert!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless).unwrap()[0].required,
            "alpha registered a required hook"
        );

        registry.clear_source(&LuaSource::Plugin("alpha".into()));
        let start = session_start_hooks(&lua, crate::handlers::Firing::Sessionless).unwrap();
        assert_eq!(start.len(), 1, "beta's start hook survives");
        assert!(
            !start[0].required,
            "the survivor is beta's non-required hook"
        );
        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            2
        );

        registry.clear_source(&LuaSource::Plugin("beta".into()));
        assert_eq!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1,
            "the user's own hook is never cleared by a plugin"
        );
    }

    #[test]
    fn test_session_start_and_end_hooks_independent() {
        let (lua, _) = TestLuaBuilder::new().build_with_hooks();

        lua.load(
            r#"
            cru.on_session_start(function(s) end)
            cru.on_session_end(function(s) end)
            cru.on_session_end(function(s) end)
        "#,
        )
        .exec()
        .unwrap();

        assert_eq!(
            session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            session_end_hooks(&lua, crate::handlers::Firing::Sessionless)
                .unwrap()
                .len(),
            2
        );
    }
}
