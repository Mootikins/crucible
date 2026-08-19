//! `cru.ui.*` — the plugin-facing side of `InteractionRequest`.
//!
//! Lives beside the session tests rather than beside `ui.rs` because
//! `MockDaemonApi` lives here, and a second stub for a 31-method trait is the
//! duplication these tests exist to avoid.

use super::MockDaemonApi;
use crate::sessions::DaemonSessionApi;
use crate::test_support::TestLuaBuilder;
use crate::ui::INTERACTION_KINDS;
use mlua::Lua;
use serde_json::json;
use std::sync::Arc;

fn lua_with(api: Arc<MockDaemonApi>) -> Lua {
    TestLuaBuilder::new()
        .with_ui_api(api as Arc<dyn DaemonSessionApi>)
        .build()
}

/// The guard against a variant being added to `crucible-core` and silently not
/// being callable from Lua. `INTERACTION_KINDS` is the one list; this asserts
/// every entry became a function.
#[tokio::test]
async fn every_interaction_kind_is_a_callable_function() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    for kind in INTERACTION_KINDS {
        let is_fn: bool = lua
            .load(format!(r#"return type(cru.ui.{kind}) == "function""#))
            .eval_async()
            .await
            .unwrap();
        assert!(is_fn, "cru.ui.{kind} is not a function");
    }
}

#[tokio::test]
async fn each_call_stamps_its_own_kind_on_the_request() {
    for kind in INTERACTION_KINDS {
        let api = Arc::new(MockDaemonApi::new());
        let lua = lua_with(Arc::clone(&api));
        lua.load(format!(r#"cru.ui.{kind}("s1", {{ title = "t" }})"#))
            .exec_async()
            .await
            .unwrap_or_else(|e| panic!("cru.ui.{kind} failed: {e}"));

        let calls = api.interaction_calls();
        assert_eq!(calls.len(), 1, "{kind}: expected exactly one daemon call");
        assert_eq!(calls[0].0, "s1", "{kind}: wrong session id");
        assert_eq!(
            calls[0].1.get("kind").and_then(|v| v.as_str()),
            Some(*kind),
            "{kind}: request carried the wrong kind"
        );
    }
}

/// A caller naming a different `kind` has made a typo with one sensible
/// reading — the function name already chose the variant.
#[tokio::test]
async fn the_function_name_wins_over_a_kind_key_in_the_options() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(r#"cru.ui.ask("s1", { kind = "permission", question = "q" })"#)
        .exec_async()
        .await
        .unwrap();
    assert_eq!(api.interaction_calls()[0].1["kind"], "ask");
}

#[tokio::test]
async fn options_reach_the_daemon_unaltered() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(
        r#"cru.ui.ask("s1", { question = "Which?", choices = {"a","b"}, allow_other = true })"#,
    )
    .exec_async()
    .await
    .unwrap();

    let req = api.interaction_calls()[0].1.clone();
    assert_eq!(req["question"], "Which?");
    assert_eq!(req["choices"], json!(["a", "b"]));
    assert_eq!(req["allow_other"], json!(true));
}

#[tokio::test]
async fn the_default_timeout_matches_the_permission_prompt() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(r#"cru.ui.ask("s1", { question = "q" })"#)
        .exec_async()
        .await
        .unwrap();
    assert_eq!(api.interaction_calls()[0].2, 300);
}

#[tokio::test]
async fn a_timeout_option_overrides_the_default() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(r#"cru.ui.ask("s1", { question = "q", timeout = 5 })"#)
        .exec_async()
        .await
        .unwrap();
    assert_eq!(api.interaction_calls()[0].2, 5);
}

/// Zero would mean "give up before asking", which no caller means; it falls
/// back rather than producing a request nobody can answer.
#[tokio::test]
async fn a_zero_timeout_falls_back_to_the_default() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(r#"cru.ui.ask("s1", { question = "q", timeout = 0 })"#)
        .exec_async()
        .await
        .unwrap();
    assert_eq!(api.interaction_calls()[0].2, 300);
}

#[tokio::test]
async fn the_response_arrives_as_a_lua_table() {
    let api = Arc::new(MockDaemonApi::new());
    api.set_interaction_answer(json!({ "kind": "ask", "selected": [1] }));
    let lua = lua_with(api);
    let (kind, first): (String, i64) = lua
        .load(r#"local r = cru.ui.ask("s1", { question = "q" }) return r.kind, r.selected[1]"#)
        .eval_async()
        .await
        .unwrap();
    assert_eq!(kind, "ask");
    assert_eq!(first, 1);
}

/// Nobody answering is the common case on a headless daemon. It must be a
/// value the plugin inspects, not an error it has to `pcall` around.
#[tokio::test]
async fn cancelled_is_a_successful_call_not_an_error() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(api);
    let (kind, err): (String, Option<String>) = lua
        .load(r#"local r, e = cru.ui.ask("s1", { question = "q" }) return r.kind, e"#)
        .eval_async()
        .await
        .unwrap();
    assert_eq!(kind, "cancelled");
    assert!(err.is_none(), "cancelled must not surface as an error");
}

/// The stub VM exists so a plugin gets a readable error instead of "attempt to
/// index a nil value" in a VM with no daemon wired.
#[tokio::test]
async fn a_stub_vm_reports_no_daemon() {
    let lua = TestLuaBuilder::new().with_ui().build();
    let err: String = lua
        .load(r#"local r, e = cru.ui.ask("s1", {}) return e"#)
        .eval_async()
        .await
        .unwrap();
    assert_eq!(err, "no daemon connected");
}

/// `crucible.*` is the long-form alias of `cru.*`; a module registered on one
/// and not the other is the bug `register_in_namespaces` exists to prevent.
#[tokio::test]
async fn ui_is_registered_on_both_namespaces() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(api);
    let same: bool = lua
        .load(r#"return cru.ui.ask == crucible.ui.ask"#)
        .eval_async()
        .await
        .unwrap();
    assert!(same, "cru.ui and crucible.ui must be the same table");
}
