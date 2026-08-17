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
            local session, err = cru.sessions.create({ type = "chat", kilns = { "notes" } })
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
    let kilns: Table = result.get("kilns").unwrap();
    assert_eq!(kilns.get::<String>(1).unwrap(), "notes");
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
    // kilns should be the mock default
    let kilns: Table = result.get("kilns").unwrap();
    assert_eq!(kilns.get::<String>(1).unwrap(), "default");
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
                kilns = { "notes", "docs" },
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
    // The whole kiln set crossed the boundary, in order. It used to be
    // plucked into a positional argument the mock threw away, so this file
    // asserted nothing about it.
    let kilns: Table = result.get("kilns").unwrap();
    assert_eq!(kilns.len().unwrap(), 2);
    assert_eq!(kilns.get::<String>(1).unwrap(), "notes");
    assert_eq!(kilns.get::<String>(2).unwrap(), "docs");
}

/// `kilns` is both the plugin spelling and the wire name now, so the binding
/// sends the table's own key untranslated.
///
/// Asserted on the JSON because that is the whole hop this side owns: an
/// ordered list of strings has to survive mlua's table encoding intact (a
/// dropped element or an object instead of an array is what a hand-rolled
/// conversion got wrong).
#[tokio::test]
async fn create_sends_kilns_as_an_ordered_array() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(r#"return (cru.sessions.create({ kilns = { "a", "b", "c" } }))"#)
        .eval_async()
        .await
        .unwrap();

    let params = mock.last_create_params().expect("api was invoked");
    assert_eq!(params["kilns"], serde_json::json!(["a", "b", "c"]));
    // A read kiln is not an agent field, so it must not turn an agent-less
    // create into an agent-configuring one.
    assert_eq!(params.get("configure_agent"), None);
}

/// mlua encodes an empty Lua table as a JSON object, not an array — the shape
/// `Option<Vec<String>>` refuses. `kilns = {}` worked before whole-table serde
/// and has to keep working.
#[tokio::test]
async fn create_tolerates_an_empty_kilns_table() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create({ type = "chat", kilns = {} })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(result.get::<String>("id").unwrap().starts_with("chat-"));
    let params = mock.last_create_params().expect("api was invoked");
    assert_eq!(
        params.get("kilns"),
        Some(&serde_json::json!([])),
        "an empty kilns table must reach the daemon as an array"
    );
}

/// Every create-time field reaches the daemon, not just the four the binding
/// used to pluck. This is what lets a plugin ask for an agent card at all.
///
/// Whole-object equality rather than per-key assertions: the failure this
/// guards against is a field going *missing* on the way out, which a
/// `params["x"] == y` list can only catch for keys someone remembered to add.
/// Every key here is one the daemon's `SessionCreateRequest` reads.
#[tokio::test]
async fn create_forwards_the_whole_options_table() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create({
                type = "chat",
                workspace = "/tmp/ws",
                kilns = { "notes" },
                configure_agent = true,
                agent_card = "researcher",
                provider = "ollama",
                provider_key = "ollama",
                model = "llama3.2",
                endpoint = "http://localhost:11434",
                tool_policy = { bash = "deny" },
                recording_mode = "granular",
                recording_path = "/tmp/rec.jsonl",
                isolation = false,
            })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let params = mock.last_create_params().expect("api was invoked");
    assert_eq!(
        params,
        serde_json::json!({
            "type": "chat",
            "workspace": "/tmp/ws",
            "kilns": ["notes"],
            "configure_agent": true,
            "agent_card": "researcher",
            "provider": "ollama",
            "provider_key": "ollama",
            "model": "llama3.2",
            "endpoint": "http://localhost:11434",
            "tool_policy": { "bash": "deny" },
            "recording_mode": "granular",
            "recording_path": "/tmp/rec.jsonl",
            "isolation": false,
        })
    );
}

/// The other create shape: an ACP session names a *profile* in `agent_name`.
///
/// Separate from the card table above because the daemon refuses `agent_card`
/// and `agent_name` together, so no single call can carry both — and this is
/// the pair (`agent_type` + `agent_name`) the Discord plugin's ACP path sends.
#[tokio::test]
async fn create_forwards_the_acp_profile_shape() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(
            r#"return (cru.sessions.create({
                type = "agent",
                agent_type = "acp",
                agent_name = "claude",
            }))"#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(
        mock.last_create_params().expect("api was invoked"),
        serde_json::json!({
            "type": "agent",
            "agent_type": "acp",
            "agent_name": "claude",
            // Implied by `agent_name`: without the flag the daemon creates the
            // session agent-less and never launches the profile.
            "configure_agent": true,
        })
    );
}

/// The daemon reads the agent fields only when `configure_agent` is set, so a
/// plugin naming a card without it would be silently ignored.
#[tokio::test]
async fn create_implies_configure_agent_when_an_agent_field_is_present() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(r#"return (cru.sessions.create({ agent_card = "researcher" }))"#)
        .eval_async()
        .await
        .unwrap();
    assert_eq!(
        mock.last_create_params().unwrap()["configure_agent"],
        serde_json::json!(true)
    );

    // …and a table with no agent field is left alone: create stays agent-less
    // and the caller configures it afterwards, as Discord does.
    let _: Table = lua
        .load(r#"return (cru.sessions.create({ type = "chat" }))"#)
        .eval_async()
        .await
        .unwrap();
    assert_eq!(
        mock.last_create_params().unwrap().get("configure_agent"),
        None
    );
}

/// A tool policy is an agent field too. It is the one a plugin most needs at
/// create — setting it afterwards means a whole-agent `configure_agent`, which
/// would overwrite a card resolved here.
#[tokio::test]
async fn create_forwards_a_tool_policy_and_implies_configure_agent() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(r#"return (cru.sessions.create({ tool_policy = { bash = "deny" } }))"#)
        .eval_async()
        .await
        .unwrap();

    let params = mock.last_create_params().unwrap();
    assert_eq!(params["tool_policy"]["bash"], "deny");
    assert_eq!(params["configure_agent"], serde_json::json!(true));
}

/// Implied, not forced — an explicit `false` still means "create agent-less".
#[tokio::test]
async fn create_respects_an_explicit_configure_agent_false() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let _: Table = lua
        .load(r#"return (cru.sessions.create({ model = "llama3.2", configure_agent = false }))"#)
        .eval_async()
        .await
        .unwrap();

    assert_eq!(
        mock.last_create_params().unwrap()["configure_agent"],
        serde_json::json!(false)
    );
}

/// The documented legacy form: a bare string is the session type.
#[tokio::test]
async fn create_accepts_the_legacy_positional_string_form() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = Arc::clone(&mock) as _;
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: Table = lua
        .load(
            r#"
            local session, err = cru.sessions.create("agent")
            assert(err == nil, "unexpected error: " .. tostring(err))
            return session
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(result.get::<String>("id").unwrap().starts_with("agent-"));
    let params = mock.last_create_params().expect("api was invoked");
    assert_eq!(params, serde_json::json!({ "type": "agent" }));
}

/// A positional list is a mistake, not an options table — say so rather than
/// quietly creating a default chat session.
#[tokio::test]
async fn create_rejects_a_list_argument() {
    let api: Arc<dyn DaemonSessionApi> = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    let result: (Value, Value) = lua
        .load(r#"return cru.sessions.create({ "chat" })"#)
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(result.0, Value::Nil));
    match result.1 {
        Value::String(s) => assert!(s.to_str().unwrap().contains("named options")),
        other => panic!("expected error string, got {other:?}"),
    }
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
