//! Tests for the `crucible.on` half of `LuaScriptHandlerRegistry`.
//!
//! The other half — a `Vec<LuaScriptHandler>` filled by annotation discovery —
//! is gone, and so are the tests that drove `add`/`handlers_for`/`iter`/`len`
//! against it. Nothing wrote to that vec once the annotation loader was
//! removed, so those cases asserted on a structure production never populated.

use crate::handlers::{register_crucible_on_api, LuaScriptHandlerRegistry};
use mlua::Lua;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[test]
fn test_crucible_on_api_registration() {
    let lua = Lua::new();
    let handlers = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));

    register_crucible_on_api(&lua, handlers.clone(), functions.clone()).unwrap();

    // Verify crucible.on exists. The name has to be a real hook: `crucible.on`
    // now validates against `HOOK_NAMES`, because a name nothing dispatches
    // registered happily and then never fired.
    lua.load(
        r#"
        crucible.on("pre_tool_call", function(event)
            return event
        end)
    "#,
    )
    .exec()
    .unwrap();

    // Check that handler was registered
    let guard = handlers.lock().unwrap();
    assert_eq!(guard.len(), 1);
    assert_eq!(guard[0].event_type, "pre_tool_call");
}

/// The bug: `crucible.on("pre_toolcall", …)` registered, logged at `debug!`, and
/// never fired. The registry compares `event_type` with `==`, so nothing about a
/// misspelt name was recoverable at dispatch time.
#[test]
fn crucible_on_rejects_a_hook_name_nothing_dispatches() {
    let lua = Lua::new();
    let handlers = Arc::new(Mutex::new(Vec::new()));
    let functions = Arc::new(Mutex::new(HashMap::new()));
    register_crucible_on_api(&lua, handlers.clone(), functions.clone()).unwrap();

    let err = lua
        .load(r#"crucible.on("pre_toolcall", function(event) return event end)"#)
        .exec()
        .expect_err("a misspelt hook name must not register");
    let msg = err.to_string();
    assert!(msg.contains("did you mean `pre_tool_call`"), "{msg}");
    assert!(handlers.lock().unwrap().is_empty(), "nothing may be stored");
}

// ============================================================================
// Return Convention Tests
// ============================================================================

#[test]
fn crucible_on_with_opts_table_sets_pattern_and_priority() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("pre_tool_call", { pattern = "bash", priority = 10 }, function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("bash"));
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 10);
    assert_eq!(handlers[0].pattern, Some("bash".to_string()));

    // Doesn't match other tools
    let handlers = registry.runtime_handlers_for("pre_tool_call", Some("grep"));
    assert_eq!(handlers.len(), 0);
}

#[test]
fn crucible_on_backward_compat_no_opts() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_crucible_on_api(
        &lua,
        registry.runtime_handlers(),
        registry.handler_functions(),
    )
    .unwrap();

    lua.load(
        r#"
        crucible.on("turn:complete", function(ctx, event)
            return nil
        end)
    "#,
    )
    .exec()
    .unwrap();

    let handlers = registry.runtime_handlers_for("turn:complete", None);
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers[0].priority, 100); // default
    assert_eq!(handlers[0].pattern, None);
}
