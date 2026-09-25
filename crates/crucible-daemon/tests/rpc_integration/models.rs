//! Model switching tests.

use crucible_core::config::BackendType;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn plugin_approval_round_trips_over_socket_and_on_attach() {
    use crucible_core::session::PluginApproval;
    use crucible_core::traits::chat::SessionKnobs;
    use crucible_daemon::DaemonAgentHandle;

    let server = TestServer::start().await.unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    let created = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".into(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    let id = created["session_id"].as_str().unwrap();

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
        client.session_get(id).await.unwrap()["plugin_approvals"]["beta"],
        "stop"
    );

    let (attached_client, events) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .unwrap();
    let mut handle = DaemonAgentHandle::new_and_subscribe(
        std::sync::Arc::new(attached_client),
        id.to_owned(),
        events,
    )
    .await
    .unwrap();
    assert_eq!(handle.get_plugin_approval("alpha"), PluginApproval::Ask);
    handle
        .set_plugin_approval("alpha", PluginApproval::Inherit)
        .await
        .unwrap();
    assert_eq!(handle.get_plugin_approval("alpha"), PluginApproval::Inherit);
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
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".into(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    created["session_id"].as_str().unwrap().to_owned()
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
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

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

    let result = client.session_switch_model(&session_id, "gpt-4").await;
    assert!(
        result.is_ok(),
        "session_switch_model should succeed: {:?}",
        result.err()
    );

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session["agent"]["model"]
        .as_str()
        .expect("model should be string");
    assert_eq!(model, "gpt-4", "Model should be updated in session");

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_handle_switch_model() {
    use crucible_core::session::SessionAgent;
    use crucible_core::traits::chat::SessionKnobs;
    use crucible_daemon::DaemonAgentHandle;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

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

    let mut handle =
        DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
            .await
            .expect("Failed to create agent handle");

    let result = handle.switch_model("gpt-4-turbo").await;
    assert!(
        result.is_ok(),
        "DaemonAgentHandle::switch_model should succeed: {:?}",
        result.err()
    );

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session["agent"]["model"]
        .as_str()
        .expect("model should be string");
    assert_eq!(
        model, "gpt-4-turbo",
        "Model should be updated via AgentHandle"
    );

    server.shutdown().await;
}

/// Full-flow: session.set_mode over a real socket persists the mode
/// (session.get reflects it) and rejects unknown modes. The TUI's
/// DaemonAgentHandle::set_mode_str and the web POST /api/session/{id}/mode
/// both ride this RPC.
#[tokio::test]
async fn test_session_set_mode_round_trip() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");
    let session_id = result["session_id"].as_str().unwrap().to_string();

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
        session["agent"]["mode"].is_null(),
        "fresh session has no persisted mode"
    );

    client
        .session_set_mode(&session_id, "plan")
        .await
        .expect("session_set_mode should succeed");

    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["agent"]["mode"].as_str(),
        Some("plan"),
        "mode persists and round-trips through session.get"
    );
    assert_eq!(
        client
            .session_get_mode(&session_id)
            .await
            .unwrap()
            .as_deref(),
        Some("plan"),
        "session.get_mode returns what session.set_mode stored"
    );

    // Switching again overwrites.
    client.session_set_mode(&session_id, "ask").await.unwrap();
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(session["agent"]["mode"].as_str(), Some("ask"));

    // Unknown modes are rejected loudly, not persisted.
    let err = client.session_set_mode(&session_id, "yolo").await;
    assert!(err.is_err(), "unknown mode must be rejected");
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(session["agent"]["mode"].as_str(), Some("ask"));

    server.shutdown().await;
}

/// The exact call path the TUI takes on `/plan` / Shift+Tab: the mode must
/// reach the daemon (it was previously a client-local no-op, making plan
/// mode cosmetic for every daemon-backed session).
#[tokio::test]
async fn test_daemon_agent_handle_set_mode_reaches_daemon() {
    use crucible_core::session::SessionAgent;
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::DaemonAgentHandle;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");
    let session_id = result["session_id"].as_str().unwrap().to_string();

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

    let mut handle =
        DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
            .await
            .expect("Failed to create agent handle");

    handle
        .set_mode_str("plan")
        .await
        .expect("set_mode_str should reach the daemon");
    assert_eq!(handle.get_mode_id(), "plan", "local mirror updated");

    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["agent"]["mode"].as_str(),
        Some("plan"),
        "TUI mode switch must persist daemon-side, not just locally"
    );

    // A rejected mode surfaces an error and leaves both sides unchanged.
    let err = handle.set_mode_str("yolo").await;
    assert!(err.is_err(), "unknown mode must fail loudly");
    assert_eq!(handle.get_mode_id(), "plan");

    server.shutdown().await;
}

/// The handle's approval cache follows `plugin_approval_changed`, so a change
/// by the daemon (the loop limit) or by another client reads back at once.
#[tokio::test]
async fn plugin_approval_cache_follows_the_change_event() {
    use crucible_core::session::PluginApproval;
    use crucible_core::traits::chat::SessionKnobs;
    use crucible_daemon::DaemonAgentHandle;

    let server = TestServer::start().await.unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    let created = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".into(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .unwrap();
    let id = created["session_id"].as_str().unwrap();
    let (attached_client, events) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .unwrap();
    let handle = DaemonAgentHandle::new_and_subscribe_with_raw_forwarding(
        std::sync::Arc::new(attached_client),
        id.to_owned(),
        events,
    )
    .await
    .unwrap();
    assert_eq!(handle.get_plugin_approval("alpha"), PluginApproval::Inherit);

    client
        .session_set_plugin_approval(id, "alpha", PluginApproval::Ask)
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while handle.get_plugin_approval("alpha") != PluginApproval::Ask {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the cache must follow plugin_approval_changed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    server.shutdown().await;
}
