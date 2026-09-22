use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use std::sync::Arc;

#[tokio::test]
async fn rejected_proposals_answers_the_daemon_rows() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = mock.clone();
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (count, title, reason, path): (usize, String, String, String) = lua
        .load(
            r#"
            local rows, err = cru.proposals.rejected()
            assert(err == nil, "unexpected error: " .. tostring(err))
            return #rows, rows[1].title, rows[1].reason, rows[1].paths[1]
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(count, 2);
    assert_eq!(title, "Change b.md");
    assert_eq!(reason, "a duplicate");
    assert_eq!(path, "b.md");
    assert_eq!(
        mock.rejected_calls(),
        vec![proposals::DEFAULT_REJECTED_LIMIT]
    );
}

#[tokio::test]
async fn rejected_proposals_passes_the_limit() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = mock.clone();
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let count: usize = lua
        .load("return #cru.proposals.rejected(1)")
        .eval_async()
        .await
        .unwrap();

    assert_eq!(count, 1);
    assert_eq!(mock.rejected_calls(), vec![1]);
}

#[tokio::test]
async fn the_rejected_proposals_stub_answers_no_daemon() {
    let lua = mlua::Lua::new();
    register_sessions_module(&lua).expect("stub module");

    let (rows, err): (mlua::Value, Option<String>) = lua
        .load("return cru.proposals.rejected(5)")
        .eval_async()
        .await
        .unwrap();

    assert!(rows.is_nil(), "the stub answers no rows: {rows:?}");
    assert_eq!(err.as_deref(), Some("no daemon connected"));
}

#[tokio::test]
async fn list_passes_the_session_and_all() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = mock.clone();
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (id, session): (String, String) = lua
        .load(
            r#"
            local rows, err = cru.proposals.list({ session = "child-1", all = true })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return rows[1].id, rows[1].session
            "#,
        )
        .eval_async()
        .await
        .unwrap();
    assert_eq!((id.as_str(), session.as_str()), ("p1", "child-1"));

    let _: mlua::Value = lua
        .load("return cru.proposals.list()")
        .eval_async()
        .await
        .unwrap();
    assert_eq!(
        mock.list_calls(),
        vec![(Some("child-1".to_string()), true), (None, false)]
    );
}

#[tokio::test]
async fn accept_and_reject_pass_their_params_to_the_decision() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = mock.clone();
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (accepted, rejected): (String, String) = lua
        .load(
            r#"
            local a = assert(cru.proposals.accept({ id = "p1", paths = { "a.md" } }))
            local r = assert(cru.proposals.reject({ id = "p2", reason = "a duplicate" }))
            return a.state.kind, r.state.kind
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(
        (accepted.as_str(), rejected.as_str()),
        ("accepted", "rejected")
    );
    assert_eq!(
        mock.decisions(),
        vec![
            (
                ProposalDecision::Accept,
                serde_json::json!({ "id": "p1", "paths": ["a.md"] })
            ),
            (
                ProposalDecision::Reject,
                serde_json::json!({ "id": "p2", "reason": "a duplicate" })
            ),
        ]
    );
}

#[tokio::test]
async fn the_decision_stubs_answer_no_daemon() {
    let lua = mlua::Lua::new();
    register_sessions_module(&lua).expect("stub module");

    for call in [
        r#"return cru.proposals.list()"#,
        r#"return cru.proposals.accept({ id = "p1" })"#,
        r#"return cru.proposals.reject({ id = "p1" })"#,
    ] {
        let (rows, err): (mlua::Value, Option<String>) = lua.load(call).eval_async().await.unwrap();
        assert!(rows.is_nil(), "{call}: the stub answers nothing: {rows:?}");
        assert_eq!(err.as_deref(), Some("no daemon connected"), "{call}");
    }
}
