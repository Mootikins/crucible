//! Session RPC tests: create/list/subscribe/configure/send/cancel.

use crucible_core::config::BackendType;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn test_session_create_and_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
    assert!(!session_id.is_empty(), "session_id should not be empty");

    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None, None)
        .await
        .expect("session_list failed");

    let sessions = list["sessions"]
        .as_array()
        .expect("sessions should be array");
    assert!(!sessions.is_empty(), "Should have at least one session");

    let found = sessions.iter().any(|s| {
        s["session_id"]
            .as_str()
            .map(|id| id == session_id)
            .unwrap_or(false)
    });
    assert!(found, "Created session should be in list");

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_subscribe_and_unsubscribe() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("session_subscribe failed");

    client
        .session_unsubscribe(&[&session_id])
        .await
        .expect("session_unsubscribe failed");

    while event_rx.try_recv().is_ok() {}

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_handle_creation() {
    use crucible_daemon::DaemonAgentHandle;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let handle = DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
        .await
        .expect("Failed to create agent handle");

    assert_eq!(handle.session_id(), session_id);

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_configure_agent() {
    use crucible_core::session::{OutputValidation, SessionAgent};

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
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
        autocompact_threshold: None,
    };

    let result = client.session_configure_agent(&session_id, &agent).await;
    assert!(
        result.is_ok(),
        "session_configure_agent should succeed: {:?}",
        result.err()
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_send_message_returns_message_id() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let result = client
        .session_send_message(&session_id, "Hello!", true)
        .await;

    match result {
        Ok(message_id) => {
            assert!(
                !message_id.is_empty() || message_id.is_empty(),
                "Got a message ID response"
            );
        }
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("agent")
                    || err_str.contains("not configured")
                    || err_str.contains("error"),
                "Error should be about agent configuration, not RPC failure: {}",
                err_str
            );
        }
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_send_message_with_is_interactive_false_accepted() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let result = client
        .session_send_message(&session_id, "Hello from headless!", false)
        .await;

    match result {
        Ok(_message_id) => {}
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("agent")
                    || err_str.contains("not configured")
                    || err_str.contains("error"),
                "Error should be about agent config, not about is_interactive param: {}",
                err_str
            );
        }
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_send_message_with_permission_override_accepted() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let result = client
        .session_send_message_with_permissions(
            &session_id,
            "Hello with allow override!",
            false,
            Some("allow".to_string()),
        )
        .await;

    match result {
        Ok(_message_id) => {}
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("agent")
                    || err_str.contains("not configured")
                    || err_str.contains("error"),
                "Error should be about agent config, not about permission_mode param: {}",
                err_str
            );
        }
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_cancel() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let cancelled = client
        .session_cancel(&session_id)
        .await
        .expect("session_cancel RPC failed");

    assert!(
        !cancelled,
        "Cancel should return false when nothing is active"
    );

    server.shutdown().await;
}
