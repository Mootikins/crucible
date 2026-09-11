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
        .load(r#"return cru.session.inject("test-session", "system", "injected context")"#)
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

    let (id, parent_id, copied): (String, String, i64) = lua
        .load(
            r#"
            local info, err = cru.session.fork("parent-session")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return info.id, info.parent_id, info.messages_copied
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(id, "fork-123");
    assert_eq!(parent_id, "parent-123");
    assert_eq!(copied, 3);
}

#[tokio::test]
async fn sessions_fork_with_up_to() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let id: String = lua
        .load(
            r#"
            local info, err = cru.session.fork("parent-session", { up_to = 5 })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return info.id
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(id, "fork-123");
}

#[tokio::test]
async fn sessions_collect_subagents_returns_results() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(
            r#"
            return cru.session.collect_subagents({"job-1", "job-2"}, 5)
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

/// `cru.session.cache_stats(session_id)` returns a table with the
/// cache aggregate fields.
#[tokio::test]
async fn sessions_cache_stats_returns_aggregate_table() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local stats, err = cru.session.cache_stats("test-session")
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

#[tokio::test]
async fn sessions_undo_returns_count() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.undo("s1", 2)"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Integer(2)));
    assert!(matches!(result.1, Value::Nil));
    let (sid, count) = mock.last_undo_call().expect("undo invoked");
    assert_eq!(sid, "s1");
    assert_eq!(count, 2);
}

/// Calling `undo` without a count argument defaults to 1.
#[tokio::test]
async fn sessions_undo_default_count_is_one() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.undo("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Integer(1)));
    assert!(matches!(result.1, Value::Nil));
    let (_, count) = mock.last_undo_call().expect("undo invoked");
    assert_eq!(count, 1, "missing count must default to 1");
}

/// `cru.session.can_undo(session_id)` round-trips a boolean.
#[tokio::test]
async fn sessions_can_undo_returns_bool() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.can_undo("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Boolean(true)));
    assert!(matches!(result.1, Value::Nil));
}

/// `cru.session.undo_depth(session_id)` returns an integer count.
#[tokio::test]
async fn sessions_undo_depth_returns_int() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.undo_depth("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Integer(2)));
    assert!(matches!(result.1, Value::Nil));
}

/// `cru.session.undo_history(session_id)` returns a list with one
/// table per undoable turn, each carrying `turn_index` and
/// `messages_removed`.
#[tokio::test]
async fn sessions_undo_history_returns_list() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local entries, err = cru.session.undo_history("s1")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return entries
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.len().unwrap(), 2);
    let first: Table = result.get(1).unwrap();
    assert_eq!(first.get::<i64>("turn_index").unwrap(), 0);
    assert_eq!(first.get::<i64>("messages_removed").unwrap(), 2);
    let second: Table = result.get(2).unwrap();
    assert_eq!(second.get::<i64>("turn_index").unwrap(), 1);
    assert_eq!(second.get::<i64>("messages_removed").unwrap(), 3);
}
