use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use mlua::Table;
use std::sync::Arc;

#[tokio::test]
async fn sessions_messages_returns_all_roles() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local msgs, err = cru.sessions.messages("test-session")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return msgs
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.len().unwrap(), 3); // system + user + assistant from mock
}

#[tokio::test]
async fn sessions_messages_filters_by_role() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local msgs, err = cru.sessions.messages("test-session", { role = "user" })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return msgs
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.len().unwrap(), 1);
}

#[tokio::test]
async fn sessions_messages_respects_limit() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local msgs, err = cru.sessions.messages("test-session", { limit = 1 })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return msgs
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.len().unwrap(), 1);
}
