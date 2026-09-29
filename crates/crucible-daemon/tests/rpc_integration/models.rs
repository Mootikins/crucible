//! Model switching tests.

use crucible_core::config::BackendType;
use crucible_core::protocol::requests::SessionCreateRequest;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn plugin_approval_round_trips_over_socket_and_on_attach() {
    use crucible_core::session::PluginApproval;

    let server = TestServer::start().await.unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    let created = client
        .session_create(SessionCreateRequest {
            session_type: "chat".into(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    let id = created.id.as_str();

    client
        .session_set_plugin_approval(id, "alpha", PluginApproval::Ask)
        .await
        .unwrap();
    client
        .session_set_plugin_approval(id, "beta", PluginApproval::Stop)
        .await
        .unwrap();
    assert_eq!(
        client
            .session_get_plugin_approval(id, "alpha")
            .await
            .unwrap(),
        PluginApproval::Ask
    );
    assert_eq!(
        client
            .session_list_plugin_approvals(id)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        client
            .session_get(id)
            .await
            .unwrap()
            .plugin_approvals
            .get("beta")
            .copied(),
        Some(PluginApproval::Stop)
    );

    // A second client that attaches later reads the value and changes it.
    let attached = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    assert_eq!(
        attached
            .session_get_plugin_approval(id, "alpha")
            .await
            .unwrap(),
        PluginApproval::Ask
    );
    attached
        .session_set_plugin_approval(id, "alpha", PluginApproval::Inherit)
        .await
        .unwrap();
    assert_eq!(
        client
            .session_get_plugin_approval(id, "alpha")
            .await
            .unwrap(),
        PluginApproval::Inherit
    );
    assert_eq!(
        client
            .session_get_plugin_approval(id, "beta")
            .await
            .unwrap(),
        PluginApproval::Stop
    );
    assert_eq!(
        client
            .session_list_plugin_approvals(id)
            .await
            .unwrap()
            .len(),
        1
    );
    server.shutdown().await;
}

async fn chat_session(client: &DaemonClient) -> String {
    let created = client
        .session_create(SessionCreateRequest {
            session_type: "chat".into(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            ..Default::default()
        })
        .await
        .unwrap();
    created.id.as_str().to_owned()
}

/// The attach read (`session.status`) over the socket, decoded as the TUI
/// decodes it (`session_status_items`), holds the engine's plugin-turn item
/// for a plugin that the knob set to `stop`, and loses it at `inherit`.
#[tokio::test]
async fn the_status_read_over_the_socket_decodes_into_the_items() {
    use crucible_core::session::PluginApproval;
    use crucible_core::status_color::StatusColorGroup;
    use crucible_core::types::{StatusItemKind, PLUGIN_APPROVAL_ACTION};

    let server = TestServer::start().await.unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    let id = chat_session(&client).await;

    client
        .session_set_plugin_approval(&id, "beta", PluginApproval::Stop)
        .await
        .unwrap();
    let items = client.session_status_items(&id).await.unwrap();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].id, "plugin_turns:beta");
    assert_eq!(items[0].text, "beta · stop");
    assert_eq!(items[0].color_group, StatusColorGroup::Danger);
    assert!(items[0].pinned);
    assert_eq!(items[0].action.as_deref(), Some(PLUGIN_APPROVAL_ACTION));
    assert_eq!(items[0].kind, StatusItemKind::PluginTurns);
    assert_eq!(items[0].progress, None);

    client
        .session_set_plugin_approval(&id, "beta", PluginApproval::Inherit)
        .await
        .unwrap();
    assert!(client.session_status_items(&id).await.unwrap().is_empty());
    server.shutdown().await;
}

/// A subscribed client gets `status_items_changed` when another client
/// sets the knob, and the event decodes into the same items as the read.
#[tokio::test]
async fn a_knob_change_reaches_a_subscriber_as_decoded_status_items() {
    use crucible_core::session::PluginApproval;
    use crucible_core::types::StatusDisplayItem;

    let server = TestServer::start().await.unwrap();
    let (client, mut events) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .unwrap();
    let id = chat_session(&client).await;
    client.session_subscribe(&[id.as_str()]).await.unwrap();

    let other = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    other
        .session_set_plugin_approval(&id, "alpha", PluginApproval::Ask)
        .await
        .unwrap();

    let changed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("the event stream is open");
            if event.event == "status_items_changed" && event.session_id == id {
                return event;
            }
        }
    })
    .await
    .expect("a knob change sends status_items_changed");
    let items: Vec<StatusDisplayItem> =
        serde_json::from_value(changed.data["status"].clone()).expect("the event decodes");
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].text, "alpha · ask");
    assert_eq!(items, client.session_status_items(&id).await.unwrap());
    server.shutdown().await;
}

#[tokio::test]
async fn test_session_switch_model() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            ..Default::default()
        })
        .await
        .expect("session_create failed");

    let session_id = result.id.to_string();

    let agent = SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        max_context_tokens: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    let result = client
        .session_knob_set(
            &session_id,
            crucible_core::types::KnobValue::Model("gpt-4".to_string()),
        )
        .await;
    assert!(
        result.is_ok(),
        "session_switch_model should succeed: {:?}",
        result.err()
    );

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session
        .agent
        .as_ref()
        .expect("agent must be configured")
        .model
        .as_str();
    assert_eq!(model, "gpt-4", "Model should be updated in session");

    server.shutdown().await;
}

/// Full-flow: session.set_mode over a real socket persists the mode
/// (session.get reflects it) and rejects unknown modes. The TUI and the web
/// POST /api/session/{id}/mode both ride this RPC.
#[tokio::test]
async fn test_session_set_mode_round_trip() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            ..Default::default()
        })
        .await
        .expect("session_create failed");
    let session_id = result.id.to_string();

    let agent = SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        max_context_tokens: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    };
    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    // No mode set yet: session.get carries no mode field.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(
        session
            .agent
            .as_ref()
            .and_then(|a| a.mode.as_deref())
            .is_none(),
        "fresh session has no persisted mode"
    );

    client
        .session_knob_set(
            &session_id,
            crucible_core::types::KnobValue::Mode(Some("plan".to_string())),
        )
        .await
        .expect("session.knob.set(mode) should succeed");

    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session.agent.as_ref().and_then(|a| a.mode.as_deref()),
        Some("plan"),
        "mode persists and round-trips through session.get"
    );
    let mode = client
        .session_knob_get(&session_id, crucible_core::types::SessionKnob::Mode)
        .await
        .unwrap();
    assert_eq!(
        mode,
        crucible_core::types::KnobValue::Mode(Some("plan".to_string())),
        "session.knob.get(mode) returns what session.knob.set(mode) stored"
    );

    // Switching again overwrites.
    client
        .session_knob_set(
            &session_id,
            crucible_core::types::KnobValue::Mode(Some("ask".to_string())),
        )
        .await
        .unwrap();
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session.agent.as_ref().and_then(|a| a.mode.as_deref()),
        Some("ask")
    );

    // Unknown modes are rejected loudly, not persisted.
    let err = client
        .session_knob_set(
            &session_id,
            crucible_core::types::KnobValue::Mode(Some("yolo".to_string())),
        )
        .await;
    assert!(err.is_err(), "unknown mode must be rejected");
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session.agent.as_ref().and_then(|a| a.mode.as_deref()),
        Some("ask")
    );

    server.shutdown().await;
}
