//! Tests for `SessionScope` — which sessions a handler fires for.
//!
//! A workflow plugin must fire for the sessions a user turned it on for and
//! for no others. Activation registers, so the set of sessions a handler
//! serves is the set of registrations that exist; these tests pin the
//! properties that make that legal — an idempotent registration, isolation
//! between sessions, the refusals that keep a scope from being silently
//! wrong, and a sweep at session end.

use crate::handlers::{
    register_cru_on_api, Firing, LuaScriptHandlerRegistry, SessionScope, StageId,
};
use crate::plugin_context::{enter_plugin, enter_session, LuaSource};
use mlua::Lua;

/// A VM with `cru.on` wired to a fresh store.
fn vm() -> (Lua, LuaScriptHandlerRegistry) {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).expect("register cru.on");
    (lua, registry)
}

/// Run `source` as if the host were inside `session`, the way the handler
/// dispatcher and the `session:start` fire path both bracket it.
fn load_in_session(lua: &Lua, session: &str, source: &str) -> mlua::Result<()> {
    let _guard = enter_session(lua, Some(session));
    lua.load(source).exec()
}

/// What a plugin registers on activation, spelled as a plugin would.
const ACTIVATE: &str = r#"
    cru.on("pre_tool_call", { session = SESSION, key = "ralph" }, function(ctx, event)
        fired = (fired or "") .. ctx.session_id
    end)
"#;

fn activation(session: &str) -> String {
    ACTIVATE.replace("SESSION", &format!("\"{session}\""))
}

/// `on_session_start` fires on create, on resume AND on `resume_from_storage`
/// — and a web history fetch calls `resume_from_storage` on every request.
/// The store has no unregister, so an appending registration would leave one
/// stale handler per fetch, firing for the life of the daemon. `oci` records
/// exactly this bug, which it escaped only by never registering per session.
#[test]
fn registering_for_the_same_session_again_leaves_one_handler() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");

    // Three activations for one session: a create and two history fetches.
    for _ in 0..3 {
        load_in_session(&lua, "s1", &activation("s1")).expect("registers");
    }

    let handlers = registry.runtime_handlers_for(
        StageId::PreToolCall.as_str(),
        Some("bash"),
        Firing::InSession("s1"),
    );
    assert_eq!(
        handlers.len(),
        1,
        "three activations of one session are one registration"
    );
    assert_eq!(registry.all().len(), 1, "and nothing else is left behind");
}

/// The same plugin may hold two scoped handlers for one session, told apart
/// by `key`. Without that, a plugin could not register two — which is a legal
/// thing to want.
#[test]
fn two_keys_are_two_registrations_for_one_session() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");

    load_in_session(
        &lua,
        "s1",
        r#"
        cru.on("pre_tool_call", { session = "s1", key = "watch" }, function() end)
        cru.on("pre_tool_call", { session = "s1", key = "guard" }, function() end)
        "#,
    )
    .expect("registers");

    assert_eq!(
        registry
            .runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s1"))
            .len(),
        2
    );
}

/// `priority` is NOT in the replacement key, so two scoped rows that differ
/// only by it collapse — and the LATER registration is the one that stands.
///
/// This is why `key` cannot go while `replaces` stays. An author separating a
/// guard from a logger by priority alone writes two `cru.on` calls, reads two
/// successes, and holds one handler. The narrower axes do not cover it:
/// `pattern` is the same for both, and neither is a one-shot.
///
/// Pinned, not endorsed. The fix is a host-derived identity for the
/// definition site (Neovim's `AutoCmd.script_ctx`); until then an author
/// separates the two rows with `key`.
#[tokio::test]
async fn two_scoped_registrations_differing_only_by_priority_collapse() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");

    load_in_session(
        &lua,
        "s1",
        r#"
        cru.on("pre_tool_call", { session = "s1", priority = 10 }, function() fired = "guard" end)
        cru.on("pre_tool_call", { session = "s1", priority = 90 }, function() fired = "logger" end)
        "#,
    )
    .expect("both register without complaint");

    let handlers =
        registry.runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s1"));
    assert_eq!(handlers.len(), 1, "priority does not separate two rows");

    // WHICH one survived, read by running it rather than assumed.
    let event = crucible_core::events::SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({}),
    };
    registry
        .execute_runtime_handler(&lua, handlers[0].id, &event, Some("s1"))
        .await
        .expect("the survivor runs");
    assert_eq!(
        lua.globals().get::<String>("fired").expect("fired"),
        "logger",
        "the registration made LAST is the one that stands"
    );
}

/// The harm this whole section exists to stop: a loop that re-prompts a model
/// must not take over a turn in a session nobody enabled it for.
#[tokio::test]
async fn a_handler_scoped_to_one_session_does_not_fire_for_another() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");
    load_in_session(&lua, "s1", &activation("s1")).expect("registers");

    let event = crucible_core::events::SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({ "tool": "bash" }),
    };

    // The session it was turned on for.
    let matched = registry.runtime_handlers_for(
        StageId::PreToolCall.as_str(),
        Some("bash"),
        Firing::InSession("s1"),
    );
    assert_eq!(
        matched.len(),
        1,
        "it fires for the session it was enabled for"
    );
    registry
        .execute_runtime_handler(&lua, matched[0].id, &event, Some("s1"))
        .await
        .expect("the handler runs");

    // Another session, and a dispatch with no session at all.
    assert!(
        registry
            .runtime_handlers_for(
                StageId::PreToolCall.as_str(),
                Some("bash"),
                Firing::InSession("s2")
            )
            .is_empty(),
        "another session must not reach it"
    );
    assert!(
        registry
            .runtime_handlers_for(
                StageId::PreToolCall.as_str(),
                Some("bash"),
                Firing::Sessionless
            )
            .is_empty(),
        "a dispatch with no session must not reach it either"
    );

    let fired: String = lua.globals().get("fired").expect("the handler ran once");
    assert_eq!(fired, "s1", "it ran for s1 and for nothing else");
}

/// An unscoped handler is every session's, as it always was.
#[test]
fn an_unscoped_handler_fires_for_every_session_and_for_none() {
    let (lua, registry) = vm();
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("registers");

    for firing in [
        Firing::InSession("s1"),
        Firing::InSession("s2"),
        Firing::Sessionless,
    ] {
        assert_eq!(
            registry
                .runtime_handlers_for(StageId::PreToolCall.as_str(), None, firing)
                .len(),
            1,
            "an unscoped handler serves {firing:?}"
        );
    }
}

/// A file event and a webhook carry no session, so a scope on one could never
/// fire. Refused at REGISTRATION, not dropped at fire time: a handler that
/// never runs and never complains is the failure `cru.on` already refuses for
/// a misspelt hook name.
#[test]
fn a_scope_on_a_sessionless_name_is_refused_at_registration() {
    for name in ["FileChanged", "note:created", "webhook:received"] {
        let (lua, registry) = vm();
        let err = load_in_session(
            &lua,
            "s1",
            &format!(r#"cru.on("{name}", {{ session = "s1" }}, function() end)"#),
        )
        .expect_err("a scope on a sessionless name must not register");
        let msg = err.to_string();
        assert!(msg.contains(name), "{msg}");
        assert!(msg.contains("without a session"), "{msg}");
        assert!(
            registry.all().is_empty(),
            "`{name}` must store nothing at all"
        );
    }
}

/// The host resolves the session id; a caller never writes one it chose.
/// `LuaSource::Eval` exists because a socket call is not the operator — if a
/// literal id were taken as written, one `lua.eval` could put a
/// `pre_tool_call` handler on a session it merely names.
#[test]
fn a_scope_naming_another_session_is_refused() {
    let (lua, registry) = vm();
    let err = load_in_session(
        &lua,
        "mine",
        r#"cru.on("pre_tool_call", { session = "yours" }, function() end)"#,
    )
    .expect_err("naming another session must not register");
    let msg = err.to_string();
    assert!(msg.contains("mine"), "{msg}");
    assert!(msg.contains("yours"), "{msg}");
    assert!(registry.all().is_empty(), "nothing may be stored");
}

/// The mistake an author will make is `{ session = session }` — the handle
/// instead of its id. A wrong type must raise: reading it as absent would
/// widen the handler from one session to every session, which is the harm.
#[test]
fn a_session_that_is_not_a_string_is_refused() {
    let (lua, registry) = vm();
    let err = load_in_session(
        &lua,
        "s1",
        r#"cru.on("pre_tool_call", { session = { id = "s1" } }, function() end)"#,
    )
    .expect_err("a non-string session must not register");
    assert!(err.to_string().contains("must be a string"), "{err}");
    assert!(
        registry.all().is_empty(),
        "and it must not become an every-session handler"
    );
}

/// And with no session in scope there is nothing to resolve against. A plugin
/// body runs at load, outside every session, so this is the message an author
/// sees when they scope a handler in the wrong place.
#[test]
fn a_scope_outside_every_session_is_refused() {
    let (lua, registry) = vm();
    let err = lua
        .load(r#"cru.on("pre_tool_call", { session = "s1" }, function() end)"#)
        .exec()
        .expect_err("a scope outside every session must not register");
    assert!(
        err.to_string().contains("cru.on_session_start"),
        "the message must name where to register instead: {err}"
    );
    assert!(registry.all().is_empty(), "nothing may be stored");
}

/// Without the owner in the replacement key, two plugins registering the same
/// hook, pattern and session would silently overwrite each other — the fault
/// A3 warns about for per-session variables, one level down.
#[test]
fn two_plugins_scoping_the_same_session_both_survive() {
    let (lua, registry) = vm();

    enter_plugin(&lua, "alpha");
    load_in_session(&lua, "s1", &activation("s1")).expect("alpha registers");
    enter_plugin(&lua, "beta");
    load_in_session(&lua, "s1", &activation("s1")).expect("beta registers");

    let handlers =
        registry.runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s1"));
    assert_eq!(handlers.len(), 2, "the owner is part of the key");
    let owners: Vec<&LuaSource> = handlers.iter().map(|h| &h.source).collect();
    assert!(owners.contains(&&LuaSource::Plugin("alpha".into())));
    assert!(owners.contains(&&LuaSource::Plugin("beta".into())));
}

/// The sweep is what makes activation-registers legal. Without it every
/// session that ever enabled a plugin leaves a row behind for the daemon's
/// life.
#[test]
fn session_end_drops_that_sessions_handlers_and_keeps_the_rest() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");

    load_in_session(&lua, "s1", &activation("s1")).expect("s1 activates");
    load_in_session(&lua, "s2", &activation("s2")).expect("s2 activates");
    lua.load(r#"cru.on("pre_tool_call", function() end)"#)
        .exec()
        .expect("an unscoped handler too");
    assert_eq!(registry.all().len(), 3);

    assert_eq!(registry.clear_session("s1"), 1, "one row swept");

    let left = registry.all();
    assert_eq!(left.len(), 2);
    assert!(
        left.iter()
            .any(|r| r.scope == SessionScope::Session("s2".into())),
        "another session's handler survives"
    );
    assert!(
        left.iter().any(|r| r.scope == SessionScope::Global),
        "an unscoped handler survives: it belongs to a load, not to a session"
    );
    // Idempotent, because two concurrent teardowns both reach it.
    assert_eq!(registry.clear_session("s1"), 0);
}

/// Lifecycle and scope stay separate, as Neovim keeps an augroup separate
/// from `buffer=`. Clearing the plugin drops it; ending the session drops it;
/// neither implies the other.
#[test]
fn clearing_the_owner_and_ending_the_session_are_independent() {
    let (lua, registry) = vm();

    enter_plugin(&lua, "alpha");
    load_in_session(&lua, "s1", &activation("s1")).expect("alpha registers");
    enter_plugin(&lua, "beta");
    load_in_session(&lua, "s1", &activation("s1")).expect("beta registers");

    assert_eq!(registry.clear_source(&LuaSource::Plugin("alpha".into())), 1);
    assert_eq!(registry.all().len(), 1, "beta's row is untouched");
    assert_eq!(registry.clear_session("s1"), 1, "and the sweep takes it");
}

/// A handler that registers another handler resolves the session from the
/// host, without ever naming an id itself — which is what makes a mid-turn
/// activation work.
#[tokio::test]
async fn a_handler_can_scope_a_registration_to_the_session_it_runs_in() {
    let (lua, registry) = vm();
    enter_plugin(&lua, "ralph");
    lua.load(
        r#"
        cru.on("turn:complete", function(ctx, event)
            cru.on("pre_tool_call", { session = ctx.session_id, key = "ralph" }, function() end)
        end)
        "#,
    )
    .exec()
    .expect("registers");

    let event = crucible_core::events::SessionEvent::Custom {
        name: "turn:complete".to_string(),
        payload: serde_json::json!({}),
    };
    let outer = registry.runtime_handlers_for(
        StageId::TurnComplete.as_str(),
        None,
        Firing::InSession("s1"),
    );
    assert_eq!(outer.len(), 1);
    registry
        .execute_runtime_handler(&lua, outer[0].id, &event, Some("s1"))
        .await
        .expect("the handler runs");

    let inner =
        registry.runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s1"));
    assert_eq!(inner.len(), 1, "it registered for the session it ran in");
    assert_eq!(inner[0].scope, SessionScope::Session("s1".into()));
    assert!(
        registry
            .runtime_handlers_for(StageId::PreToolCall.as_str(), None, Firing::InSession("s2"))
            .is_empty(),
        "and for no other"
    );
}
