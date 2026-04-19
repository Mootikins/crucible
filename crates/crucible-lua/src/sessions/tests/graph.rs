use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use mlua::{Table, Value};
use std::sync::Arc;

#[tokio::test]
async fn sessions_inject_succeeds() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.inject("test-session", "system", "injected context")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Boolean(true)));
    assert!(matches!(result.1, Value::Nil));
}

#[tokio::test]
async fn sessions_fork_returns_child_info() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local info, err = cru.sessions.fork("parent-session")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return info
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let id: String = result.get("id").unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn sessions_fork_with_up_to() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local info, err = cru.sessions.fork("parent-session", { up_to = 5 })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return info
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let id: String = result.get("id").unwrap();
    assert!(!id.is_empty());
}

#[tokio::test]
async fn sessions_collect_subagents_returns_results() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(
            r#"
            return cru.sessions.collect_subagents({"job-1", "job-2"}, 5)
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    // MockDaemonApi returns empty vec, so result should be an empty table
    match result.0 {
        Value::Table(t) => assert_eq!(t.len().unwrap(), 0),
        _ => panic!("Expected table, got {:?}", result.0),
    }
    assert!(matches!(result.1, Value::Nil));
}
