use std::path::PathBuf;

use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crucible_core::traits::acp::SessionManager;
use crucible_core::types::acp::SessionConfig;

#[tokio::test]
async fn test_client_implements_session_manager() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/agent"),
        agent_args: None,
        working_dir: Some(PathBuf::from("/test/workspace")),
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Should start with no active session
    assert!(client.active_session().is_none());

    // Should implement SessionManager trait
    let session_config = SessionConfig {
        cwd: PathBuf::from("/test/workspace"),
        mode_id: "plan".to_string(),
        context_size: 5,
        enable_enrichment: true,
        enrichment_count: 5,
        metadata: std::collections::HashMap::new(),
    };

    // This should now succeed and create a session
    let result = client.create_session(session_config).await;
    assert!(result.is_ok(), "Should successfully create session");

    // Should track active session
    let session_id = result.unwrap();
    assert!(client.active_session().is_some());
    assert_eq!(client.active_session(), Some(&session_id));
}

#[tokio::test]
async fn test_session_lifecycle() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/agent"),
        agent_args: None,
        working_dir: Some(PathBuf::from("/test/workspace")),
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    let session_config = SessionConfig {
        cwd: PathBuf::from("/test/workspace"),
        mode_id: "plan".to_string(),
        context_size: 5,
        enable_enrichment: true,
        enrichment_count: 5,
        metadata: std::collections::HashMap::new(),
    };

    // Create session should now succeed
    let create_result = client.create_session(session_config).await;
    assert!(create_result.is_ok());
    let session_id = create_result.unwrap();

    // Should be able to load session
    let load_result = client.load_session(session_id.clone()).await;
    assert!(load_result.is_ok());
    assert_eq!(client.active_session(), Some(&session_id));

    // Should be able to end session
    let end_result = client.end_session(session_id).await;
    assert!(end_result.is_ok());
    assert!(client.active_session().is_none());
}

#[tokio::test]
async fn test_session_creation_with_mock_agent() {
    use crate::acp::mock_agent::{MockAgent, MockAgentConfig};
    use std::collections::HashMap;

    // Create a mock agent that will respond successfully
    let mut responses = HashMap::new();
    responses.insert(
        "initialize".to_string(),
        serde_json::json!({
            "agent_capabilities": {},
            "agent_info": {
                "name": "mock-agent",
                "version": "0.1.0"
            }
        }),
    );
    responses.insert(
        "new_session".to_string(),
        serde_json::json!({
            "session_id": "test-session-123"
        }),
    );

    let mock_config = MockAgentConfig {
        responses,
        simulate_delay: false,
        delay_ms: 0,
        simulate_errors: false,
    };
    let _mock_agent = MockAgent::new(mock_config);

    // TODO: Once we implement the actual connection logic,
    // this test will verify that we can create a session with the mock agent
    // For now, this is a placeholder showing the expected API
}

#[tokio::test]
async fn test_session_initialization_flow() {
    // 1. Connect to agent (or mock)
    // 2. Send initialize request
    // 3. Create new session
    // 4. Return session ID

    // This will fail until we implement the connection logic
    // but defines the expected behavior
}
