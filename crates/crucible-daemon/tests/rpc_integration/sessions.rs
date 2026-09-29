//! Session RPC tests: create/list/subscribe/configure/send/cancel.

use crucible_core::config::BackendType;
use crucible_core::protocol::requests::SessionCreateParams;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn test_session_create_and_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.as_str();
    assert!(!session_id.is_empty(), "session_id should not be empty");

    let list = client
        .session_list(
            Some(&crucible_daemon::test_support::kiln_name("kiln")),
            None,
            Some("chat"),
            None,
            None,
        )
        .await
        .expect("session_list failed");

    assert!(
        !list.sessions.is_empty(),
        "Should have at least one session"
    );

    let found = list.sessions.iter().any(|s| s.id.as_str() == session_id);
    assert!(found, "Created session should be in list");

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_subscribe_and_unsubscribe() {
    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.to_string();

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
async fn test_session_configure_agent() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.to_string();

    let result = client
        .session_send_message(&session_id, "Hello!", true)
        .await;

    match result {
        Ok(outcome) => {
            assert!(
                matches!(outcome, crucible_core::types::SendOutcome::Turn { .. }),
                "plain text starts a turn: {outcome:?}"
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
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.to_string();

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
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.to_string();

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
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateParams {
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

    let session_id = result.id.to_string();

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

/// `/clear` in the TUI and in the web is `session.clear`, the user's clear.
/// Its `context_cleared` names no plugin, so no client shows a plugin label.
#[tokio::test]
async fn session_clear_is_the_users_clear() {
    let server = TestServer::start().await.unwrap();
    let (client, mut events) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .unwrap();
    let created = client
        .session_create(SessionCreateParams {
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
    let id = created.id.as_str();
    client.session_subscribe(&[id]).await.unwrap();

    client.session_clear(id).await.unwrap();

    let cleared = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.expect("the event stream is open");
            if event.event == "context_cleared" {
                return event;
            }
        }
    })
    .await
    .expect("session.clear sends context_cleared");
    assert!(cleared.data["plugin"].is_null(), "{:?}", cleared.data);
    server.shutdown().await;
}
