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

/// Pass a raw string spec — the Lua binding hands it to the daemon
/// verbatim (the daemon parses with `OutputValidation::from_str`).
#[tokio::test]
async fn sessions_set_output_validation_accepts_string_spec() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.set_output_validation("s1", "json")"#)
        .eval_async()
        .await
        .unwrap();
    assert!(matches!(result.0, Value::Boolean(true)));
    assert!(matches!(result.1, Value::Nil));

    let captured = mock.last_validation_spec().expect("api was invoked");
    assert_eq!(captured.0, "s1");
    assert_eq!(captured.1, "json");
}

/// Pass a `{ type = "lua", name = "..." }` table — the Lua binding
/// serialises it to the canonical `lua:<name>` form before calling the
/// trait. Same path used by all four typed shapes (none/json/regex/lua).
#[tokio::test]
async fn sessions_set_output_validation_serialises_lua_table_spec() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.set_output_validation("s1", { type = "lua", name = "x" })"#)
        .eval_async()
        .await
        .unwrap();
    assert!(matches!(result.0, Value::Boolean(true)));
    assert!(matches!(result.1, Value::Nil));

    let captured = mock.last_validation_spec().expect("api was invoked");
    assert_eq!(captured.0, "s1");
    assert_eq!(captured.1, "lua:x");
}

/// `{ type = "regex", pattern = "..." }` serialises to `regex:<pattern>`.
#[tokio::test]
async fn sessions_set_output_validation_serialises_regex_table_spec() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(
            r#"return cru.sessions.set_output_validation("s1", { type = "regex", pattern = "^OK$" })"#,
        )
        .eval_async()
        .await
        .unwrap();
    assert!(matches!(result.0, Value::Boolean(true)));

    let captured = mock.last_validation_spec().expect("api was invoked");
    assert_eq!(captured.1, "regex:^OK$");
}

/// `cru.sessions.undo(session_id, count?)` returns the number of turns
/// undone. Default count is 1.
#[tokio::test]
async fn sessions_undo_returns_count() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.undo("s1", 2)"#)
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
        .load(r#"return cru.sessions.undo("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Integer(1)));
    assert!(matches!(result.1, Value::Nil));
    let (_, count) = mock.last_undo_call().expect("undo invoked");
    assert_eq!(count, 1, "missing count must default to 1");
}

/// `cru.sessions.can_undo(session_id)` round-trips a boolean.
#[tokio::test]
async fn sessions_can_undo_returns_bool() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.can_undo("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Boolean(true)));
    assert!(matches!(result.1, Value::Nil));
}

/// `cru.sessions.undo_depth(session_id)` returns an integer count.
#[tokio::test]
async fn sessions_undo_depth_returns_int() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.undo_depth("s1")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Integer(2)));
    assert!(matches!(result.1, Value::Nil));
}

/// `cru.sessions.undo_history(session_id)` returns a list with one
/// table per undoable turn, each carrying `turn_index` and
/// `messages_removed`.
#[tokio::test]
async fn sessions_undo_history_returns_list() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local entries, err = cru.sessions.undo_history("s1")
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

/// Unknown `type` keys raise a runtime error from the Lua binding —
/// the daemon never sees the call.
#[tokio::test]
async fn sessions_set_output_validation_rejects_unknown_type() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let res: mlua::Result<(Value, Value)> = lua
        .load(r#"return cru.sessions.set_output_validation("s1", { type = "bogus" })"#)
        .eval_async()
        .await;
    let err = res.expect_err("expected error from unknown type");
    let msg = format!("{err}");
    assert!(
        msg.contains("unknown validation type"),
        "expected 'unknown validation type' in error, got: {msg}"
    );
    assert!(
        mock.last_validation_spec().is_none(),
        "api should not have been called"
    );
}
