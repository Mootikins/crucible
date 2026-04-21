//! TUI flow tests: sessions command, resume command, full daemon-agent flow.

use crucible_core::config::BackendType;
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn test_tui_sessions_command_flow() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let workspace_dir = tempfile::tempdir().expect("Failed to create workspace dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session1 = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: Some(workspace_dir.path().to_path_buf()),
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create 1 failed");
    let session1_id = session1["session_id"].as_str().unwrap();

    let session2 = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: Some(workspace_dir.path().to_path_buf()),
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create 2 failed");
    let session2_id = session2["session_id"].as_str().unwrap();

    let list_result = client
        .session_list(
            Some(kiln_dir.path()),
            Some(workspace_dir.path()),
            Some("chat"),
            None,
            None,
        )
        .await
        .expect("session_list failed");

    let sessions = list_result["sessions"]
        .as_array()
        .expect("result.sessions should be array");
    assert!(sessions.len() >= 2, "Should have at least 2 sessions");

    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(ids.contains(&session1_id), "Should contain session 1");
    assert!(ids.contains(&session2_id), "Should contain session 2");

    server.shutdown().await;
}

#[tokio::test]
async fn test_tui_resume_command_flow() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let create_result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");
    let session_id = create_result["session_id"]
        .as_str()
        .expect("should have session_id");

    client
        .session_pause(session_id)
        .await
        .expect("session_pause failed");

    let resume_result = client
        .session_resume(session_id)
        .await
        .expect("session_resume failed");

    let state = resume_result["state"].as_str().unwrap_or("");
    assert!(
        state.to_lowercase().contains("active"),
        "Resumed session should be active, got: {}",
        state
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_tui_daemon_agent_full_flow() {
    use crucible_core::session::{OutputValidation, SessionAgent};

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let create_result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");
    let session_id = create_result["session_id"]
        .as_str()
        .expect("should have session_id")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("subscribe failed");

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    let handle =
        crucible_daemon::DaemonAgentHandle::new(client.clone(), session_id.clone(), event_rx);

    assert_eq!(handle.session_id(), session_id);
    assert!(handle.is_connected());

    client
        .session_unsubscribe(&[&session_id])
        .await
        .expect("unsubscribe failed");

    server.shutdown().await;
}
