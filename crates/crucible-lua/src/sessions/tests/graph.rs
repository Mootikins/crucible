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

/// `cru.sessions.cache_stats(session_id)` returns a table with the
/// cache aggregate fields.
#[tokio::test]
async fn sessions_cache_stats_returns_aggregate_table() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local stats, err = cru.sessions.cache_stats("test-session")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return stats
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.get::<u64>("hits").unwrap(), 0);
    assert_eq!(result.get::<u64>("misses").unwrap(), 0);
    // hit_rate is null on a fresh session — Lua-side surfaces as either
    // `nil` (LuaSerdeExt's default) or `mlua::LightUserData(NULL)` /
    // a JSON-null sentinel depending on the converter. Both indicate
    // "no data" and are valid; what we don't want is a numeric value.
    let hit_rate: Value = result.get("hit_rate").unwrap();
    assert!(
        !matches!(hit_rate, Value::Number(_) | Value::Integer(_)),
        "hit_rate must not be a number when no cache events have fired; got {:?}",
        hit_rate
    );
}
