//! Tests for `once` — a registration that retires itself after it runs.
//!
//! Neovim gives a handler three ways to retire: `once = true`, a truthy
//! callback return, and `nvim_del_autocmd(id)`. `once` is the one added here,
//! because it is the only one that asks the author for no state.
//!
//! # One test per FIRE PATH, and that is the point
//!
//! Four paths run a registration's body, and three do not go through
//! `execute_handler_with_payload`: the synchronous permission gate, the two
//! session-lifecycle paths, and the provider-auth gate. `timeout_ms` is
//! already honoured on one of the four and silently lost on the other three,
//! which is the fault these tests exist to stop `once` repeating.
//!
//! `Registration::take_body` is the one choke point — `body()` is gone, and
//! the field is private, so no path can run a body without passing the
//! retirement. Each test below fails if that call is taken out.
//!
//! Each asserts the row is GONE FROM THE STORE, not merely that the body ran
//! once. A handler that is skipped but still registered keeps costing a glob
//! match and a dispatch for the life of the daemon, which is the state `once`
//! exists to end.

use crate::auth_plugin::{fire_provider_auth_hooks, get_provider_auth_hooks, PROVIDER_AUTH_HOOK};
use crate::handlers::{
    execute_permission_hooks, register_cru_on_api, register_permission_hook_api, Firing,
    LuaScriptHandlerRegistry, PermissionHookResult, PermissionRequest, RegistrationSpec, StageId,
};
use crate::plugin_context::enter_plugin;
use mlua::Lua;

/// A VM with `cru.on` wired to a fresh store.
fn vm() -> (Lua, LuaScriptHandlerRegistry) {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).expect("register cru.on");
    enter_plugin(&lua, "ralph");
    (lua, registry)
}

fn event(name: &str) -> crucible_core::events::SessionEvent {
    crucible_core::events::SessionEvent::Custom {
        name: name.to_string(),
        payload: serde_json::json!({ "tool": "bash" }),
    }
}

fn bash_request() -> PermissionRequest {
    PermissionRequest {
        tool_name: "bash".to_string(),
        args: serde_json::json!({ "command": "ls" }),
        file_path: None,
        mode: None,
        is_safe: false,
    }
}

// ---------------------------------------------------------------- Path A
// `execute_handler_with_payload`, which every daemon `cru.on` dispatch
// reaches.

/// The whole contract, on the path that already had a per-registration
/// budget: the body runs once, and the row is gone afterwards.
#[tokio::test]
async fn a_once_handler_runs_once_and_leaves_the_store() {
    let (lua, registry) = vm();
    lua.load(
        r#"cru.on("turn:complete", { once = true }, function() count = (count or 0) + 1 end)"#,
    )
    .exec()
    .expect("registers");
    assert_eq!(registry.all().len(), 1);

    let selected =
        registry.runtime_handlers_for(StageId::TurnComplete.as_str(), None, Firing::Sessionless);
    assert_eq!(selected.len(), 1, "it is selected the first time");
    registry
        .execute_runtime_handler(&lua, selected[0].id, &event("turn:complete"), None)
        .await
        .expect("runs");

    assert_eq!(
        lua.globals().get::<i64>("count").expect("count"),
        1,
        "the body ran"
    );
    // Gone from the STORE, not merely skipped at selection.
    assert!(
        registry.all().is_empty(),
        "the row must leave the store, not linger as a row nothing selects"
    );
    assert!(
        registry
            .runtime_handlers_for(StageId::TurnComplete.as_str(), None, Firing::Sessionless)
            .is_empty(),
        "and a second dispatch selects nothing"
    );
}

/// A body that RAISES still retires the row. `once` counts the calls the host
/// makes, not the calls that succeed — Neovim reads the flag before it invokes
/// the body, for the same reason.
#[tokio::test]
async fn a_once_handler_that_raises_still_leaves_the_store() {
    let (lua, registry) = vm();
    lua.load(r#"cru.on("turn:complete", { once = true }, function() error("boom") end)"#)
        .exec()
        .expect("registers");

    let selected =
        registry.runtime_handlers_for(StageId::TurnComplete.as_str(), None, Firing::Sessionless);
    let _ = registry
        .execute_runtime_handler(&lua, selected[0].id, &event("turn:complete"), None)
        .await;

    assert!(
        registry.all().is_empty(),
        "a failed one-shot must not stay registered and fail again forever"
    );
}

/// An ordinary registration is untouched. Without this the tests above would
/// pass against a `take_body` that retired everything it handed out.
#[tokio::test]
async fn a_handler_without_once_stays_registered() {
    let (lua, registry) = vm();
    lua.load(r#"cru.on("turn:complete", function() count = (count or 0) + 1 end)"#)
        .exec()
        .expect("registers");

    for _ in 0..3 {
        let selected = registry.runtime_handlers_for(
            StageId::TurnComplete.as_str(),
            None,
            Firing::Sessionless,
        );
        assert_eq!(selected.len(), 1);
        registry
            .execute_runtime_handler(&lua, selected[0].id, &event("turn:complete"), None)
            .await
            .expect("runs");
    }
    assert_eq!(lua.globals().get::<i64>("count").expect("count"), 3);
    assert_eq!(registry.all().len(), 1, "it is durable");
}

// ---------------------------------------------------------------- Path B
// `execute_permission_hooks`, synchronous, and it returns on the first
// allow/deny.

/// The permission gate loads each body itself, so it needs the choke point as
/// much as the async path does.
#[test]
fn a_once_permission_hook_runs_once_and_leaves_the_store() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).expect("register the API");
    enter_plugin(&lua, "ralph");

    lua.load(
        r#"
        cru.permissions.on_request(function(request)
            count = (count or 0) + 1
            return { allow = true }
        end, { once = true })
        "#,
    )
    .exec()
    .expect("registers");

    assert_eq!(
        execute_permission_hooks(&lua, &registry, &bash_request(), Firing::Sessionless)
            .expect("the gate answers"),
        PermissionHookResult::Allow
    );
    assert_eq!(lua.globals().get::<i64>("count").expect("count"), 1);
    assert!(registry.all().is_empty(), "the row must leave the store");

    // With the hook gone the gate falls through to a prompt, and the body
    // does not run again.
    assert_eq!(
        execute_permission_hooks(&lua, &registry, &bash_request(), Firing::Sessionless)
            .expect("the gate answers"),
        PermissionHookResult::Prompt
    );
    assert_eq!(lua.globals().get::<i64>("count").expect("count"), 1);
}

/// A row retires when its body RUNS, never when it is merely selected.
///
/// The gate returns on the first hook that decides, so a `once` hook further
/// down the list is selected and never reached. Retiring per selection — over
/// the whole `Vec` the gate took — would silently discard a handler that
/// never ran.
#[test]
fn a_once_permission_hook_the_gate_never_reaches_keeps_its_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_permission_hook_api(&lua, registry.clone()).expect("register the API");
    enter_plugin(&lua, "ralph");

    // The first registration is asked first, and it answers, so the `once`
    // hook behind it never runs.
    lua.load(
        r#"
        cru.permissions.on_request(function() return { allow = true } end)
        cru.permissions.on_request(function()
            reached = true
            return nil
        end, { once = true, key = "watcher" })
        "#,
    )
    .exec()
    .expect("registers");
    assert_eq!(registry.all().len(), 2);

    assert_eq!(
        execute_permission_hooks(&lua, &registry, &bash_request(), Firing::Sessionless)
            .expect("the gate answers"),
        PermissionHookResult::Allow
    );
    assert!(
        lua.globals().get::<Option<bool>>("reached").expect("read") != Some(true),
        "the second hook must not have run"
    );
    assert_eq!(
        registry.all().len(),
        2,
        "a one-shot that never ran keeps its registration"
    );
}

// ---------------------------------------------------------------- Path C
// The two session-lifecycle paths in `executor.rs`.

/// `cru.on_session_start(handler, { once = true })` — the lifecycle path loads
/// its own bodies too, and it is the path an author is most likely to use a
/// one-shot on, because `on_session_start` fires again on every resume.
#[tokio::test]
async fn a_once_session_start_hook_runs_once_and_leaves_the_store() {
    use crate::session_api::Session;

    let executor = crate::executor::LuaExecutor::new().expect("executor");
    enter_plugin(executor.lua(), "ralph");
    executor
        .lua()
        .load(r#"cru.on_session_start(function(s) count = (count or 0) + 1 end, { once = true })"#)
        .exec()
        .expect("registers");

    let registry = crate::handlers::registry_of(executor.lua()).expect("the store");
    assert_eq!(registry.all().len(), 1);

    let session = Session::new("s1".to_string());
    session.bind(Box::new(crate::session_api::tests::MockRpc::new()));

    executor
        .fire_session_start_hooks(&session)
        .await
        .expect("fires");
    assert_eq!(
        executor.lua().globals().get::<i64>("count").expect("count"),
        1
    );
    assert!(registry.all().is_empty(), "the row must leave the store");

    // A resume fires the same path again, and finds nothing.
    executor
        .fire_session_start_hooks(&session)
        .await
        .expect("fires with no hooks");
    assert_eq!(
        executor.lua().globals().get::<i64>("count").expect("count"),
        1,
        "a resume must not run the one-shot again"
    );
}

/// The end path is a separate `lua.registry_value` call site, so it is a
/// separate gate.
#[tokio::test]
async fn a_once_session_end_hook_runs_once_and_leaves_the_store() {
    use crate::session_api::Session;

    let executor = crate::executor::LuaExecutor::new().expect("executor");
    enter_plugin(executor.lua(), "ralph");
    executor
        .lua()
        .load(r#"cru.on_session_end(function(s) count = (count or 0) + 1 end, { once = true })"#)
        .exec()
        .expect("registers");

    let registry = crate::handlers::registry_of(executor.lua()).expect("the store");
    let session = Session::new("s1".to_string());
    session.bind(Box::new(crate::session_api::tests::MockRpc::new()));

    executor
        .fire_session_end_hooks(&session)
        .await
        .expect("fires");
    assert_eq!(
        executor.lua().globals().get::<i64>("count").expect("count"),
        1
    );
    assert!(registry.all().is_empty(), "the row must leave the store");
}

// ---------------------------------------------------------------- Path D
// `fire_provider_auth_hooks`, synchronous, and it too answers on the first
// hook that decides.

/// `cru.on_provider_auth` takes no options table, so `once` is registered
/// through the spec here. The path is what is under test: it loads each body
/// itself, and it is the fourth place a retirement could have been forgotten.
#[test]
fn a_once_provider_auth_hook_runs_once_and_leaves_the_store() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    crate::handlers::install_registry(&lua, registry.clone());
    enter_plugin(&lua, "ralph");

    let handler = lua
        .load(
            r#"
        function(context)
            count = (count or 0) + 1
            return { Authorization = "Bearer t" }
        end
        "#,
        )
        .eval::<mlua::Function>()
        .expect("a handler");
    let mut spec = RegistrationSpec::new(PROVIDER_AUTH_HOOK);
    spec.once = true;
    registry.register(&lua, spec, handler).expect("registers");

    let hooks = get_provider_auth_hooks(&lua).expect("hooks");
    assert_eq!(hooks.len(), 1);
    assert!(
        fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o")
            .expect("fires")
            .is_some(),
        "the hook answered with headers"
    );
    assert_eq!(lua.globals().get::<i64>("count").expect("count"), 1);
    assert!(registry.all().is_empty(), "the row must leave the store");

    // The selection is empty now, so a second provider call asks nobody.
    assert!(
        get_provider_auth_hooks(&lua).expect("hooks").is_empty(),
        "and nothing is selected again"
    );
}

/// The auth gate also answers on the first hook, so the same
/// retire-on-run-not-on-selection rule applies as in the permission gate.
#[test]
fn a_once_provider_auth_hook_the_gate_never_reaches_keeps_its_registration() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    crate::handlers::install_registry(&lua, registry.clone());
    enter_plugin(&lua, "ralph");

    let answers = lua
        .load(r#"function(context) return { Authorization = "Bearer t" } end"#)
        .eval::<mlua::Function>()
        .expect("a handler");
    // Registered first, so it is asked first.
    let first = RegistrationSpec::new(PROVIDER_AUTH_HOOK);
    registry.register(&lua, first, answers).expect("registers");

    let never = lua
        .load(r#"function(context) reached = true return nil end"#)
        .eval::<mlua::Function>()
        .expect("a handler");
    let mut second = RegistrationSpec::new(PROVIDER_AUTH_HOOK);
    second.once = true;
    registry.register(&lua, second, never).expect("registers");

    let hooks = get_provider_auth_hooks(&lua).expect("hooks");
    assert_eq!(hooks.len(), 2);
    assert!(fire_provider_auth_hooks(&lua, &hooks, "openai", "gpt-4o")
        .expect("fires")
        .is_some());

    assert!(
        lua.globals().get::<Option<bool>>("reached").expect("read") != Some(true),
        "the second hook must not have run"
    );
    assert_eq!(
        registry.all().len(),
        2,
        "a one-shot that never ran keeps its registration"
    );
}

// ------------------------------------------------------- selection is not a run

/// Selecting handlers must retire nothing. `retrieval_stage::has_handlers`
/// asks only whether a handler exists, and a probe that retired a one-shot
/// would consume it before anything ran.
#[test]
fn selecting_a_once_handler_without_running_it_retires_nothing() {
    let (lua, registry) = vm();
    lua.load(r#"cru.on("turn:complete", { once = true }, function() end)"#)
        .exec()
        .expect("registers");

    for _ in 0..3 {
        assert_eq!(
            registry
                .runtime_handlers_for(StageId::TurnComplete.as_str(), None, Firing::Sessionless)
                .len(),
            1,
            "a probe must keep finding it"
        );
    }
    assert_eq!(registry.all().len(), 1, "and the row is still there");
}
