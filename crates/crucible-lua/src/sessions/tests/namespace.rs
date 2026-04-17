use crate::test_support::TestLuaBuilder;
use mlua::{Table, Value};

#[test]
fn sessions_module_registers_in_namespace() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let cru: Table = lua.globals().get("cru").expect("cru should exist");
    let sessions: Table = cru.get("sessions").expect("cru.sessions should exist");

    assert!(sessions.contains_key("create").unwrap());
    assert!(sessions.contains_key("get").unwrap());
    assert!(sessions.contains_key("list").unwrap());
    assert!(sessions.contains_key("configure_agent").unwrap());
    assert!(sessions.contains_key("send_message").unwrap());
    assert!(sessions.contains_key("cancel").unwrap());
    assert!(sessions.contains_key("subscribe").unwrap());
    assert!(sessions.contains_key("unsubscribe").unwrap());
    assert!(sessions.contains_key("interaction_respond").unwrap());
    assert!(sessions.contains_key("pause").unwrap());
    assert!(sessions.contains_key("resume").unwrap());
    assert!(sessions.contains_key("end_session").unwrap());
    assert!(sessions.contains_key("send_and_collect").unwrap());
    assert!(sessions.contains_key("collect_subagents").unwrap());
    assert!(sessions.contains_key("messages").unwrap());
    assert!(sessions.contains_key("inject").unwrap());
    assert!(sessions.contains_key("fork").unwrap());

    // Also registered under crucible.*
    let crucible: Table = lua
        .globals()
        .get("crucible")
        .expect("crucible should exist");
    let sessions2: Table = crucible
        .get("sessions")
        .expect("crucible.sessions should exist");
    assert!(sessions2.contains_key("create").unwrap());
}

#[tokio::test]
async fn sessions_stub_create_returns_nil() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.create({ type = "chat", kiln = "/tmp/kiln" })"#)
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
        .load(r#"return cru.sessions.list()"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
}

#[tokio::test]
async fn sessions_stub_get_returns_nil() {
    let lua = TestLuaBuilder::new().with_sessions().build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.get("some-id")"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
}
