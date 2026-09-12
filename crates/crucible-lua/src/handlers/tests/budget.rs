//! The handler time budget, at the dispatch seam.
//!
//! Two mechanisms guard two different failures, and each test here names which
//! one it exercises. A handler that AWAITS is ended by the tokio timeout; a
//! handler that never yields is ended by the VM deadline. Before this, neither
//! existed: `handler.call_async` ran bare.

use std::time::{Duration, Instant};

use crate::handlers::{register_cru_on_api, LuaScriptHandlerRegistry};
use crucible_core::events::SessionEvent;
use mlua::Lua;

fn event() -> SessionEvent {
    SessionEvent::Custom {
        name: "test".to_string(),
        payload: serde_json::json!({}),
    }
}

/// A VM with `cru.on` and the deadline hook, as every real VM has.
fn vm(registry: &LuaScriptHandlerRegistry) -> Lua {
    let lua = Lua::new();
    crate::handler_budget::install_deadline_hook(&lua).expect("install the hook");
    crate::timer::register_timer_module(&lua).expect("cru.timer");
    register_cru_on_api(&lua, registry.clone()).expect("cru.on");
    lua
}

/// The awaiting case: a handler that sleeps past its budget is cancelled.
///
/// `{ timeout_ms = … }` is what makes this testable in a second rather than in
/// the 30 s a stage gets by default, and it is the registration option a
/// long-running handler declares for itself.
#[tokio::test]
async fn a_sleeping_handler_is_cancelled_at_its_budget() {
    let registry = LuaScriptHandlerRegistry::new();
    let lua = vm(&registry);
    lua.load(
        r#"
        cru.on("turn:complete", { timeout_ms = 100 }, function(ctx, event)
            cru.timer.sleep(30)
            return { cancel = true }
        end)
        "#,
    )
    .exec()
    .expect("register the handler");

    let started = Instant::now();
    let outcome = registry
        .execute_runtime_handler(&lua, 0, &event(), None)
        .await;

    let error = outcome.expect_err("a handler over its budget must not return a result");
    assert!(
        error.to_string().contains("time budget"),
        "the error must say what happened: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the budget must end the call, not wait for it: {:?}",
        started.elapsed()
    );
}

/// The case no timeout can reach: Lua that never yields.
///
/// A `tokio::time::timeout` around this call never fires — the future is never
/// polled again, because the worker thread is inside the Lua VM. Only the
/// instruction hook gets control back.
#[tokio::test]
async fn a_spinning_handler_is_stopped_at_its_budget() {
    let registry = LuaScriptHandlerRegistry::new();
    let lua = vm(&registry);
    lua.load(
        r#"
        cru.on("turn:complete", { timeout_ms = 200 }, function(ctx, event)
            while true do end
        end)
        "#,
    )
    .exec()
    .expect("register the handler");

    let started = Instant::now();
    let outcome = registry
        .execute_runtime_handler(&lua, 0, &event(), None)
        .await;

    let error = outcome.expect_err("a spinning handler must not return a result");
    assert!(
        error.to_string().contains("time budget"),
        "the error must say what happened: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the deadline must interrupt the VM: {:?}",
        started.elapsed()
    );
}

/// The error names the plugin, so an operator knows what to uninstall.
///
/// The attribution comes from the same plugin context the dispatcher installs
/// around the call, so it is right for a handler registered by a plugin and
/// absent for the user's own configuration.
#[tokio::test]
async fn the_overrun_error_names_the_plugin() {
    let registry = LuaScriptHandlerRegistry::new();
    let lua = vm(&registry);
    let previous = crate::plugin_context::enter_plugin(&lua, "grabby", false);
    lua.load(
        r#"
        cru.on("turn:complete", { timeout_ms = 200 }, function(ctx, event)
            while true do end
        end)
        "#,
    )
    .exec()
    .expect("register the handler");
    crate::plugin_context::set_owner(&lua, previous);

    let error = registry
        .execute_runtime_handler(&lua, 0, &event(), None)
        .await
        .expect_err("a spinning handler must not return a result");
    assert!(
        error.to_string().contains("[grabby]"),
        "the error must name the plugin: {error}"
    );
}

/// A handler that finishes inside its budget is untouched, and the budget does
/// not leak into the next call on the same VM.
#[tokio::test]
async fn a_handler_inside_its_budget_returns_normally() {
    let registry = LuaScriptHandlerRegistry::new();
    let lua = vm(&registry);
    lua.load(
        r#"
        cru.on("turn:complete", { timeout_ms = 5000 }, function(ctx, event)
            cru.timer.sleep(0.01)
            return { cancel = true }
        end)
        "#,
    )
    .exec()
    .expect("register the handler");

    for _ in 0..2 {
        let outcome = registry
            .execute_runtime_handler(&lua, 0, &event(), None)
            .await
            .expect("a handler inside its budget must return its result");
        assert!(
            matches!(outcome, crate::ScriptHandlerResult::Cancel { .. }),
            "the handler's own result must survive: {outcome:?}"
        );
    }
}

/// The declared budget wins over the name's default, in both directions.
#[test]
fn a_registration_can_name_its_own_budget() {
    use crate::handlers::{RegistrationSpec, StageId};

    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    let func = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut spec = RegistrationSpec::new(StageId::PreToolCall.into());
    let default_id = registry.register(&lua, spec.clone(), func.clone()).unwrap();
    spec.timeout_ms = Some(250);
    let named_id = registry.register(&lua, spec, func).unwrap();

    assert_eq!(
        registry.by_id(default_id).unwrap().budget(),
        crate::handler_budget::TURN_STAGE_BUDGET
    );
    assert_eq!(
        registry.by_id(named_id).unwrap().budget(),
        Duration::from_millis(250)
    );
}

/// The four merged names carry the budget their own fire path arms, not the
/// turn-stage default. A wrong answer here would cut `oci`'s container pull
/// off at 30 s, or let a permission hook hold the user's prompt for 30.
#[test]
fn a_merged_name_carries_its_own_budget() {
    use crate::handlers::{HookName, StageId};

    assert_eq!(
        HookName::from(StageId::PermissionRequest).budget(),
        crate::handler_budget::PERMISSION_BUDGET
    );
    assert_eq!(
        HookName::from(StageId::SessionStart).budget(),
        crate::handler_budget::LIFECYCLE_BUDGET
    );
    assert_eq!(
        HookName::from(StageId::SessionEnd).budget(),
        crate::handler_budget::LIFECYCLE_BUDGET
    );
    assert_eq!(
        HookName::from(StageId::ProviderAuth).budget(),
        crate::handler_budget::TURN_STAGE_BUDGET
    );
}
