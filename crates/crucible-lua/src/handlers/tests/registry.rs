//! Tests for the `cru.on` half of `LuaScriptHandlerRegistry`.
//!
//! The other half — a `Vec<LuaScriptHandler>` filled by annotation discovery —
//! is gone, and so are the tests that drove `add`/`handlers_for`/`iter`/`len`
//! against it. Nothing wrote to that vec once the annotation loader was
//! removed, so those cases asserted on a structure production never populated.

use crate::handlers::{register_cru_on_api, LuaScriptHandlerRegistry, StageId};
use mlua::Lua;

#[test]
fn test_crucible_on_api_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    // Verify cru.on exists. The name has to be a real hook: `cru.on`
    // validates against the name enums, because a name nothing dispatches
    // registered happily and then never fired.
    lua.load(
        r#"
        cru.on("pre_tool_call", function(event)
            return event
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers =
        registry.runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless);
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].name, StageId::PreToolCall.into());
}

/// The bug: `cru.on("pre_toolcall", …)` registered, logged at `debug!`, and
/// never fired. Nothing about a misspelt name was recoverable at dispatch time.
#[test]
fn crucible_on_rejects_a_hook_name_nothing_dispatches() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    let err = lua
        .load(r#"cru.on("pre_toolcall", function(event) return event end)"#)
        .exec()
        .expect_err("a misspelt hook name must not register");
    let msg = err.to_string();
    assert!(msg.contains("did you mean `pre_tool_call`"), "{msg}");
    assert_eq!(
        registry
            .runtime_handlers_for("pre_tool_call", None, crate::handlers::Firing::Sessionless)
            .len(),
        0,
        "nothing may be stored"
    );
}

/// `cru.on` and `cru.on_session_start` write the SAME store, so a plugin's
/// registrations are cleared together and counted together.
#[test]
fn every_registration_api_writes_one_store() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();
    let cru: mlua::Table = lua.globals().get("cru").unwrap();
    crate::hooks::register_hooks_module(&lua, &cru).unwrap();
    crate::handlers::register_permission_hook_api(&lua, registry.clone()).unwrap();

    crate::plugin_context::enter_plugin(&lua, "alpha", false);
    lua.load(
        r#"
        cru.on("turn:complete", function(ctx, event) end)
        cru.on_session_start(function(s) end)
        cru.permissions.on_request(function(req) end)
    "#,
    )
    .exec()
    .unwrap();

    assert_eq!(registry.plugin_handler_count("alpha"), 3);
    registry.clear_owner(&crate::plugin_context::LuaOwner::Plugin("alpha".into()));
    assert_eq!(registry.plugin_handler_count("alpha"), 0);
    assert_eq!(
        crate::hooks::session_start_hooks(&lua, crate::handlers::Firing::Sessionless)
            .unwrap()
            .len(),
        0
    );
}

// ============================================================================
// Return Convention Tests
// ============================================================================

#[test]
fn crucible_on_with_opts_table_sets_pattern_and_priority() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.on("pre_tool_call", { pattern = "bash", priority = 10 }, function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("bash"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 10);
    assert_eq!(handlers[0].pattern, Some("bash".to_string()));

    // Doesn't match other tools
    let handlers = registry.runtime_handlers_for(
        "pre_tool_call",
        Some("grep"),
        crate::handlers::Firing::Sessionless,
    );
    assert_eq!(handlers.len(), 0);
}

#[test]
fn crucible_on_backward_compat_no_opts() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.on("turn:complete", function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers =
        registry.runtime_handlers_for("turn:complete", None, crate::handlers::Firing::Sessionless);
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 100); // default
    assert_eq!(handlers[0].pattern, None);
}
