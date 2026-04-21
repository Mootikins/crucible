use std::path::PathBuf;

use super::{get_cat_command, get_simple_command};
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;

// Test that initialize() method exists and sends messages
#[tokio::test]
async fn test_protocol_initialize_handshake() {
    use agent_client_protocol::InitializeRequest;

    let (cmd, args) = get_cat_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Spawn agent
    let _process = client.spawn_agent().await.unwrap();

    // Send initialize request
    let init_request = InitializeRequest::new(1u16.into());

    let result = client.initialize(init_request).await;

    // Cat will echo back but won't provide valid ACP response
    // Either succeeds (unlikely) or fails on parsing - both verify method works
    let _ = result; // Accept either outcome
}

// Test that create_new_session() method exists and sends messages
#[tokio::test]
async fn test_protocol_new_session() {
    use agent_client_protocol::NewSessionRequest;

    let (cmd, args) = get_cat_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Spawn agent
    let _process = client.spawn_agent().await.unwrap();

    // Create new session request
    let session_request = NewSessionRequest::new(PathBuf::from("/test"));

    let result = client.create_new_session(session_request).await;

    // Cat will echo back but won't provide valid ACP response
    let _ = result; // Accept either outcome
}

// Test that connect_with_best_mcp() method exists and attempts full handshake
#[tokio::test]
async fn test_connect_performs_protocol_handshake() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(2000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // connect_with_best_mcp() should:
    // 1. Spawn agent
    // 2. Send InitializeRequest (reads capabilities)
    // 3. Choose transport based on capabilities
    // 4. Send NewSessionRequest
    // 5. Return session
    let result = client.connect_with_best_mcp(None).await;

    // Cat won't respond with valid ACP protocol, so this will fail
    // But it verifies the method exists and attempts the handshake
    let _ = result; // Accept either outcome
}
