use crate::test_support::TestLuaBuilder;
use mlua::{Table, Value};

#[tokio::test]
async fn sessions_stub_create_returns_nil() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.create({ type = "chat", kiln = "/tmp/kiln" })"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
    // Second return value should be the error string
    match result.1 {
        Value::String(s) => assert_eq!(s.to_str().unwrap(), "no daemon connected"),
        _ => panic!("Expected error string, got {:?}", result.1),
    }
}

#[tokio::test]
async fn sessions_stub_list_returns_nil() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.list()"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
}

#[tokio::test]
async fn sessions_stub_get_returns_nil() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let result: (Value, Value) = lua
        .load(r#"return cru.session.get("some-id")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
}

fn sorted_keys(sessions: &Table) -> Vec<String> {
    let mut keys: Vec<String> = sessions
        .pairs::<String, Value>()
        .map(|pair| pair.expect("string key").0)
        .collect();
    keys.sort();
    keys
}

/// The stub table and the daemon-backed table expose the same function names,
/// and both match `SESSION_FN_NAMES`. A name that lands in one path only fails
/// here, not in a plugin at run time.
#[test]
fn stub_and_daemon_tables_expose_the_same_functions() {
    use crate::sessions::register::SESSION_FN_NAMES;
    use crate::sessions::{register_sessions_module_with_api, DaemonSessionApi};
    use std::sync::Arc;

    let stub = TestLuaBuilder::new().with_sessions().build();
    let stub_keys = sorted_keys(
        &stub
            .globals()
            .get::<Table>("cru")
            .unwrap()
            .get("session")
            .unwrap(),
    );

    let real = TestLuaBuilder::new().build();
    let api = Arc::new(super::MockDaemonApi::new()) as Arc<dyn DaemonSessionApi>;
    register_sessions_module_with_api(&real, api).expect("daemon-backed module");
    let real_keys = sorted_keys(
        &real
            .globals()
            .get::<Table>("cru")
            .unwrap()
            .get("session")
            .unwrap(),
    );

    let mut listed: Vec<String> = SESSION_FN_NAMES.iter().map(|s| s.to_string()).collect();
    listed.sort();

    assert_eq!(stub_keys, listed);
    assert_eq!(real_keys, listed);
}

/// `cru.sessions` is a deprecated alias: it must hand back the *same*
/// function objects as `cru.session`, so a plugin still on the old name runs
/// the new implementation — never a copy that could drift.
#[tokio::test]
async fn the_deprecated_sessions_alias_forwards_the_same_functions() {
    let api: std::sync::Arc<dyn crate::sessions::DaemonSessionApi> =
        std::sync::Arc::new(super::MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let same: (bool, bool) = lua
        .load(
            r#"
            return cru.sessions.send_message == cru.session.send_message,
                   cru.sessions.end_session == cru.session.end_session
            "#,
        )
        .eval()
        .unwrap();
    assert!(
        same.0,
        "send_message through the alias must be the same function"
    );
    assert!(
        same.1,
        "end_session through the alias must be the same function"
    );
}

/// The alias works end to end, and the forward target resolves at call time —
/// the stub-to-daemon upgrade after the alias was installed is picked up.
#[tokio::test]
async fn the_alias_answers_calls_with_the_underlying_module() {
    let api: std::sync::Arc<dyn crate::sessions::DaemonSessionApi> =
        std::sync::Arc::new(super::MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let id: String = lua
        .load(
            r#"
            local session, err = cru.sessions.create({ type = "chat" })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session.id
            "#,
        )
        .eval_async()
        .await
        .unwrap();
    assert!(id.starts_with("chat-"));
}
