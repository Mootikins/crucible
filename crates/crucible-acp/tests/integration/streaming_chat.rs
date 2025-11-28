//! Integration test for streaming chat flow
//!
//! This test validates the end-to-end flow of:
//! 1. Client sends session/prompt request
//! 2. Agent sends multiple session/update notifications with chunks
//! 3. Agent sends final PromptResponse with stopReason
//! 4. Client accumulates chunks and returns complete response

use crucible_acp::client::ClientConfig;
use crucible_acp::CrucibleAcpClient;
use std::path::PathBuf;

/// Test that ChatSession properly handles streaming responses from agent
///
/// Uses the OpenCode mock agent behavior for handshake testing.
/// Note: Full streaming validation is in the ignored test below.
#[tokio::test]
async fn test_streaming_chat_with_mock_agent() {
    // Get workspace root and build path to mock agent
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    println!("Using mock agent at: {}", mock_agent_path.display());

    // Configure client to use mock agent with OpenCode behavior (default)
    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None, // Use default OpenCode behavior
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(10000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // Connect and perform handshake
    let result = client.connect_with_handshake().await;
    assert!(
        result.is_ok(),
        "Should complete handshake: {:?}",
        result.err()
    );

    let session = result.unwrap();
    println!("Session ID: {}", session.id());

    // Verify handshake completed successfully
    assert!(!session.id().is_empty(), "Should have valid session ID");
    assert!(
        client.is_connected(),
        "Client should be connected after handshake"
    );
}

/// Test the actual streaming response accumulation
///
/// This test will send a PromptRequest and verify:
/// 1. Multiple session/update notifications are received
/// 2. Content from AgentMessageChunk is accumulated
/// 3. Final PromptResponse triggers completion
/// 4. Result contains all accumulated content
///
/// Note: This test is ignored because it requires the mock agent to implement
/// proper streaming responses. The current mock agent handles handshake but
/// doesn't fully simulate streaming message chunks.
#[tokio::test]
#[ignore] // Ignore until mock agent supports full streaming simulation
async fn test_prompt_with_streaming_response() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None, // Use default OpenCode behavior
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(10000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // Connect and get session
    let session = client
        .connect_with_handshake()
        .await
        .expect("Should complete handshake");

    // Create a PromptRequest
    use agent_client_protocol::{ContentBlock, PromptRequest, SessionId};
    let prompt_request = PromptRequest {
        session_id: SessionId::from(session.id().to_string()),
        prompt: vec![ContentBlock::from("What is 2+2?".to_string())],
        meta: None,
    };

    // Send prompt with streaming
    let result = client.send_prompt_with_streaming(prompt_request, 1).await;

    assert!(
        result.is_ok(),
        "Should successfully receive streaming response: {:?}",
        result.err()
    );

    let (content, _tool_calls, stop_reason) = result.unwrap();

    // Verify we got content
    assert!(!content.is_empty(), "Should receive non-empty response");
    println!("Received content: {}", content);

    // Verify stop reason
    println!("Stop reason: {:?}", stop_reason);

    // Expected content based on mock agent behavior
    // The mock should send multiple chunks that concat to "The answer is 4"
    assert!(
        content.contains("answer"),
        "Response should contain 'answer'"
    );
}
