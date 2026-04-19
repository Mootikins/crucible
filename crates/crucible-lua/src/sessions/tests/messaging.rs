use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use std::sync::Arc;

#[tokio::test]
async fn sessions_send_message_returns_response_id() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: String = lua
        .load(
            r#"
            local id, err = cru.sessions.send_message("session-1", "Hello agent")
            assert(err == nil)
            return id
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result, "msg-response-001");
}

#[tokio::test]
async fn sessions_cancel_returns_bool() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: bool = lua
        .load(
            r#"
            local ok, err = cru.sessions.cancel("session-1")
            assert(err == nil)
            return ok
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(result);
}

#[tokio::test]
async fn sessions_end_session_succeeds() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: bool = lua
        .load(
            r#"
            local ok, err = cru.sessions.end_session("session-1")
            assert(err == nil)
            return ok
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(result);
}
