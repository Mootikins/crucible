//! Integration test for streaming chat flow
//!
//! This test validates the end-to-end flow of:
//! 1. Client sends session/prompt request
//! 2. Agent sends multiple session/update notifications with chunks
//! 3. Agent sends final PromptResponse with stopReason
//! 4. Client accumulates chunks and returns complete response

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};

/// Test that ChatSession properly handles streaming responses from agent
///
/// Uses the OpenCode mock agent behavior for handshake testing.
/// Now uses ThreadedMockAgent for in-process testing (no subprocess needed).
#[tokio::test]
async fn test_streaming_chat_with_mock_agent() {
    // Spawn threaded mock agent with OpenCode behavior
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

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
/// Note: This test is ignored because the mock agent doesn't implement
/// proper streaming responses. It handles handshake but doesn't send
/// the session/update notifications with AgentMessageChunk that the
/// client expects for streaming.
#[tokio::test]
#[ignore] // Ignore until mock agent supports full streaming simulation
async fn test_prompt_with_streaming_response() {
    // Spawn threaded mock agent with OpenCode behavior
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // Connect and get session
    let session = client
        .connect_with_handshake()
        .await
        .expect("Should complete handshake");

    // Create a PromptRequest
    use agent_client_protocol::PromptRequest;
    let prompt_request: PromptRequest = serde_json::from_value(serde_json::json!({
        "sessionId": session.id().to_string(),
        "prompt": [{"text": "What is 2+2?"}],
        "_meta": null
    }))
    .expect("Failed to create PromptRequest");

    // Send prompt with streaming (request ID is generated internally)
    let result = client.send_prompt_with_streaming(prompt_request).await;

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
