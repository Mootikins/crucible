//! Integration test for streaming chat flow
//!
//! This test validates the end-to-end flow of:
//! 1. Client sends session/prompt request
//! 2. Agent sends multiple session/update notifications with chunks
//! 3. Agent sends final PromptResponse with stopReason
//! 4. Client accumulates chunks and returns complete response

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};
use crucible_daemon::acp::StreamingChunk;

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
    let result = client.connect_with_best_mcp(None).await;
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

/// Build a PromptRequest for the given session.
fn prompt_request(session_id: &str) -> agent_client_protocol::schema::v1::PromptRequest {
    serde_json::from_value(serde_json::json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": "What is 2+2?"}],
        "_meta": null
    }))
    .expect("Failed to create PromptRequest")
}

/// Streaming accumulation: multiple `session/update` `agent_message_chunk`
/// notifications from the agent must be concatenated by the client, and the
/// final PromptResponse must end the turn.
#[tokio::test]
async fn test_prompt_with_streaming_response() {
    let mut config = MockStdioAgentConfig::opencode();
    config.stream_chunks = vec!["The ".into(), "answer ".into(), "is 4".into()];
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("Should complete handshake");

    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, response) = client
        .send_prompt_with_callback(prompt_request(session.id()), callback)
        .await
        .expect("Should successfully receive streaming response");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert_eq!(
        content, "The answer is 4",
        "chunks must accumulate in order into the final content"
    );
    assert!(!summary.announced_any, "no tool calls were streamed");
    assert!(summary.produced_content, "the answer is visible content");
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::EndTurn
    );
}

/// A streamed `tool_call` + completed `tool_call_update` pair must surface
/// in the client's recorded tool calls alongside the text chunks.
#[tokio::test]
async fn test_prompt_with_streamed_tool_call() {
    let mut config = MockStdioAgentConfig::opencode();
    config.stream_chunks = vec!["Calculating…".into()];
    config.stream_tool_call = true;
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("Should complete handshake");

    let (chunks, callback) = crate::support::parity::capture_chunks();
    let (summary, response) = client
        .send_prompt_with_callback(prompt_request(session.id()), callback)
        .await
        .expect("Should successfully receive streaming response");
    let content = crate::support::parity::text_of(&chunks.lock().unwrap());

    assert_eq!(
        content, "Calculating…",
        "the answer text must hold the text chunks only: {content:?}"
    );
    assert!(
        chunks.lock().unwrap().iter().any(|chunk| matches!(
            chunk,
            StreamingChunk::ToolStart { name, .. } if name == "Mock Tool"
        )),
        "the tool call must reach the stream as a ToolStart chunk"
    );
    assert!(
        summary.announced_any,
        "the streamed tool_call must be recorded"
    );
    assert_eq!(
        crate::support::parity::tool_names_of(&chunks.lock().unwrap()),
        vec!["Mock Tool"]
    );
    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::EndTurn
    );
}

/// Cancellation propagation: when the streaming callback reports a dropped
/// receiver (returns `false`), the client must send `session/cancel` to the
/// agent, and the agent's `cancelled` final response must end the turn.
/// The mock holds the turn open until cancel arrives, so this test hangs
/// into the client timeout (and fails) if the cancel is never sent.
#[tokio::test]
async fn test_cancel_mid_stream_reaches_agent() {
    let mut config = MockStdioAgentConfig::opencode();
    config.stream_chunks = vec!["partial ".into(), "answer".into()];
    config.hold_turn_until_cancel = true;
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("Should complete handshake");

    // A callback that refuses the first chunk models the daemon's turn
    // stream being dropped — the user cancelled.
    let callback: crucible_daemon::acp::StreamingCallback = Box::new(|_chunk| false);

    let (_summary, response) = client
        .send_prompt_with_callback(prompt_request(session.id()), callback)
        .await
        .expect("cancelled turn should still complete cleanly");

    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::Cancelled,
        "the agent only sends `cancelled` after receiving session/cancel, \
         so this proves the client propagated the cancellation"
    );
}
