use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use mlua::{Table, Value};
use std::sync::Arc;

#[tokio::test]
async fn sessions_with_mock_api_create_returns_id() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create({ type = "chat", kiln = "/tmp/kiln" })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let id: String = result.get("id").unwrap();
    assert!(
        id.starts_with("chat-"),
        "id should start with 'chat-': {}",
        id
    );
    assert_eq!(result.get::<String>("state").unwrap(), "active");
    assert_eq!(result.get::<String>("kiln").unwrap(), "/tmp/kiln");
}

#[tokio::test]
async fn sessions_with_mock_api_create_no_kiln_uses_default() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create({ type = "chat" })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let id: String = result.get("id").unwrap();
    assert!(id.starts_with("chat-"));
    // kiln should be the mock default
    assert_eq!(result.get::<String>("kiln").unwrap(), "/default/crucible");
}

#[tokio::test]
async fn sessions_with_mock_api_create_with_kilns() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create({
                type = "chat",
                kilns = { "/tmp/notes", "/tmp/docs" },
            })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let id: String = result.get("id").unwrap();
    assert!(id.starts_with("chat-"));
    // No explicit kiln → uses mock default
    assert_eq!(result.get::<String>("kiln").unwrap(), "/default/crucible");
}

#[tokio::test]
async fn sessions_create_with_invalid_arg_returns_error() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.create(42)"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
    match result.1 {
        Value::String(s) => assert!(s.to_str().unwrap().contains("expects a table")),
        _ => panic!("Expected error string"),
    }
}

#[tokio::test]
async fn sessions_with_mock_api_list_returns_array() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local sessions, err = cru.sessions.list()
            assert(err == nil, "unexpected error: " .. tostring(err))
            return sessions
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.len().unwrap(), 2);

    let first: Table = result.get(1).unwrap();
    assert_eq!(first.get::<String>("id").unwrap(), "chat-001");

    let second: Table = result.get(2).unwrap();
    assert_eq!(second.get::<String>("id").unwrap(), "agent-002");
    assert_eq!(second.get::<String>("state").unwrap(), "paused");
}

#[tokio::test]
async fn sessions_with_mock_api_get_existing() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.get("exists-123")
            assert(err == nil)
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.get::<String>("id").unwrap(), "exists-123");
}

#[tokio::test]
async fn sessions_with_mock_api_get_missing_returns_nil() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.get("nonexistent")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
    // No error — just not found
    assert!(matches!(result.1, Value::Nil));
}
