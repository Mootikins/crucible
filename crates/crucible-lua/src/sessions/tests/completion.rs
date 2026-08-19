//! `cru.sessions.complete` — the one-shot completion primitive.
//!
//! What is asserted here is the boundary, not the model: which options reach
//! the daemon, and what a plugin sees come back. The daemon half (resolving
//! the session's client, bounding the wait) is tested in `crucible-daemon`.

use super::MockDaemonApi;
use crate::sessions::{register_sessions_module_with_api, DaemonSessionApi};
use crate::test_support::TestLuaBuilder;
use mlua::Lua;
use std::sync::Arc;

fn lua_with(api: Arc<MockDaemonApi>) -> Lua {
    let lua = TestLuaBuilder::new().build();
    register_sessions_module_with_api(&lua, api as Arc<dyn DaemonSessionApi>)
        .expect("register cru.sessions");
    lua
}

#[tokio::test]
async fn an_options_table_crosses_whole() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    let answer: String = lua
        .load(
            r#"
            local text, err = cru.sessions.complete("chat-1", {
                system = "You name conversations.",
                prompt = "User: hello",
                timeout = 7,
            })
            assert(err == nil, tostring(err))
            return text
            "#,
        )
        .eval_async()
        .await
        .expect("complete");

    assert_eq!(answer, "answered: User: hello");
    let calls = api.completions();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "chat-1");
    assert_eq!(calls[0].1["system"], "You name conversations.");
    assert_eq!(calls[0].1["prompt"], "User: hello");
    assert_eq!(calls[0].1["timeout"], 7);
}

/// A caller with one self-contained prompt should not have to name the key.
#[tokio::test]
async fn a_bare_string_is_the_prompt() {
    let api = Arc::new(MockDaemonApi::new());
    let lua = lua_with(Arc::clone(&api));
    lua.load(r#"cru.sessions.complete("chat-1", "name this")"#)
        .exec_async()
        .await
        .expect("complete");

    assert_eq!(api.completions()[0].1["prompt"], "name this");
}

/// The `(nil, err)` convention every other `cru.sessions` function follows —
/// the auto-title plugin branches on exactly this.
#[tokio::test]
async fn a_call_with_no_daemon_answers_nil_and_a_reason() {
    let lua = TestLuaBuilder::new().build();
    crate::sessions::register_sessions_module(&lua).expect("stub module");
    let reason: String = lua
        .load(
            r#"
            local text, err = cru.sessions.complete("chat-1", "name this")
            assert(text == nil, "a stub must not invent an answer")
            return err
            "#,
        )
        .eval_async()
        .await
        .expect("stubbed complete");
    assert_eq!(reason, "no daemon connected");
}
