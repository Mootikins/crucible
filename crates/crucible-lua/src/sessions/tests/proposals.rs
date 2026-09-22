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
