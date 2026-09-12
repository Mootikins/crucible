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

    crate::plugin_context::enter_plugin(&lua, "alpha");
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
    registry.clear_source(&crate::plugin_context::LuaSource::Plugin("alpha".into()));
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
fn crucible_on_with_opts_table_sets_pattern() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();

    register_cru_on_api(&lua, registry.clone()).unwrap();

    lua.load(
        r#"
        cru.on("pre_tool_call", { pattern = "bash" }, function(ctx, event)
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
    assert_eq!(handlers[0].pattern, None);
}

// ── A wrong-typed option RAISES ──────────────────────────────────────────
//
// Every option on a registration table is read through `string_option`,
// `integer_option` or `bool_option`, and none of them is `.ok()`. Before
// that, four of them swallowed: a wrong type read as ABSENT and the
// registration succeeded.
//
// `pattern` is the one with teeth. An absent pattern matches every dispatch
// identifier, so `{ pattern = tool }` where `tool` is a table put the handler
// against every tool call in every session — on the one hook that fails
// closed.

/// A table where a pattern belongs must not become an every-tool handler.
#[test]
fn a_pattern_that_is_not_a_string_is_refused() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    let err = lua
        .load(r#"cru.on("pre_tool_call", { pattern = { "bash" } }, function() end)"#)
        .exec()
        .expect_err("a non-string pattern must not register");
    let msg = err.to_string();
    assert!(msg.contains("`pattern` must be a string"), "{msg}");
    assert!(
        registry.all().is_empty(),
        "and it must not become a handler against every tool call"
    );
}

/// A budget that is not a number must not silently fall back to the hook's.
#[test]
fn a_timeout_ms_that_is_not_an_integer_is_refused() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    for value in [r#""5000""#, "-1", "1.5"] {
        let err = lua
            .load(format!(
                r#"cru.on("turn:complete", {{ timeout_ms = {value} }}, function() end)"#
            ))
            .exec()
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("`timeout_ms` must be a non-negative integer"),
            "timeout_ms = {value}: {msg}"
        );
    }
    assert!(registry.all().is_empty(), "nothing may be stored");

    // A Luau number literal is a double, so an integral one is the same
    // option and must still be accepted.
    lua.load(r#"cru.on("turn:complete", { timeout_ms = 5000 }, function() end)"#)
        .exec()
        .expect("an integral number is a valid budget");
    assert_eq!(registry.all()[0].timeout_ms, Some(5000));
}

/// `once` opts in to retirement, so a swallowed value reads as "never
/// retires" and the author sees a registration that succeeded.
#[test]
fn an_once_that_is_not_a_boolean_is_refused() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();

    let err = lua
        .load(r#"cru.on("turn:complete", { once = "yes" }, function() end)"#)
        .exec()
        .expect_err("a non-boolean once must not register");
    assert!(
        err.to_string().contains("`once` must be a boolean"),
        "{err}"
    );
    assert!(registry.all().is_empty(), "nothing may be stored");
}
