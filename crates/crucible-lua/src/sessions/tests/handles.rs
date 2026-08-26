use super::MockDaemonApi;
use crate::sessions::DaemonSessionApi;
use crate::test_support::TestLuaBuilder;
use mlua::Value;
use std::sync::Arc;

/// A handle's method runs the shared operation body with the handle's own
/// id — the whole point of a handle — and answers in the same
/// `(value, nil) | (nil, err)` shape the free function uses.
#[tokio::test]
async fn a_handle_method_sends_with_the_handles_own_id() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let (response_id, err): (String, Value) = lua
        .load(
            r#"
            local s, cerr = cru.session.create({ type = "chat" })
            assert(cerr == nil, "unexpected error: " .. tostring(cerr))
            local rid, err = s:send_message("hello")
            return rid, err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(err, Value::Nil), "unexpected error: {err:?}");
    assert_eq!(response_id, "msg-response-001");
    // The id the mock saw is the id the handle was built from, not one the
    // caller had to repeat.
    let sends = mock.send_calls();
    assert_eq!(sends.len(), 1);
    assert!(sends[0].starts_with("chat-"));
}

/// One lifecycle verb and one review verb through the handle, proving the
/// macro-registered methods exist across the surface, not just the first.
#[tokio::test]
async fn handle_methods_cover_lifecycle_and_review_verbs() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let ok: bool = lua
        .load(
            r#"
            local s = cru.session.get("exists-123")
            local ok1 = s:end_session()
            local hunks = s:review_list_hunks()
            return ok1 and #hunks == 0
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(ok);
    assert_eq!(mock.end_calls(), vec!["exists-123".to_string()]);
    assert_eq!(mock.review_list_calls(), vec!["exists-123".to_string()]);
}

/// A handle with no API behind it — the bare-executor case — reports that it
/// is not connected rather than silently doing nothing.
#[tokio::test]
async fn an_unconnected_handle_method_reports_it() {
    let lua = TestLuaBuilder::new().build();

    let bare = crate::session_api::Session::new("s1".to_string());
    lua.globals()
        .set("bare_session", lua.create_userdata(bare).unwrap())
        .unwrap();

    let err: String = lua
        .load(
            r#"
            local _, err = bare_session:send_message("hello")
            return err
            "#,
        )
        .eval()
        .unwrap();

    assert!(
        err.contains("not connected"),
        "expected a not-connected error, got: {err}"
    );
}
