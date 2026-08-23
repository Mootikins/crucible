use agent_client_protocol::schema::v1::StopReason;

use super::test_path;
use crate::acp::client::types::{ClientConfig, StreamingState};
use crate::acp::client::CrucibleAcpClient;
use crate::acp::streaming::StreamingCallback;

/// A callback that accepts every chunk and never cancels.
fn keep_going() -> StreamingCallback {
    Box::new(|_| true)
}

#[tokio::test]
async fn process_streaming_message_prioritizes_methods() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let request_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 0,
        "method": "session/request_permission",
        "params": {}
    });

    let result = client
        .process_streaming_message_with_callback(&request_payload, 1, &mut state, &mut keep_going())
        .await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
    assert_eq!(state.notification_count, 1);
}

#[tokio::test]
async fn process_streaming_message_returns_prompt_response() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let response_payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 5,
        "result": {
            "stopReason": "end_turn"
        }
    });

    let result = client
        .process_streaming_message_with_callback(
            &response_payload,
            5,
            &mut state,
            &mut keep_going(),
        )
        .await
        .expect("Should parse prompt response");
    assert!(result.is_some());
    assert_eq!(result.unwrap().stop_reason, StopReason::EndTurn);
}

#[tokio::test]
async fn process_streaming_message_tracks_available_commands() {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);
    let mut state = StreamingState::default();

    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "session-123",
            "update": {
                "sessionUpdate": "available_commands_update",
                "availableCommands": [
                    {
                        "name": "models",
                        "description": "Choose a model",
                        "input": null,
                        "meta": {
                            "secondary": ["claude-3.5-sonnet", "claude-3-opus"]
                        }
                    }
                ]
            }
        }
    });

    let result = client
        .process_streaming_message_with_callback(&payload, 1, &mut state, &mut keep_going())
        .await
        .expect("Should parse notification");

    assert!(
        result.is_none(),
        "Notifications should not return prompt response"
    );
    assert_eq!(client.available_commands.len(), 1);
    assert_eq!(client.available_commands[0].name, "models");
}

/// Build a client and a fresh state for one frame.
fn frame_client() -> (CrucibleAcpClient, StreamingState) {
    let config = ClientConfig {
        agent_path: test_path("test-agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    (CrucibleAcpClient::new(config), StreamingState::default())
}

/// Feed one raw `session/update` frame and collect the emitted chunks.
async fn chunks_for_frame(
    payload: serde_json::Value,
) -> Vec<crate::acp::streaming::StreamingChunk> {
    let (mut client, mut state) = frame_client();
    let collected: std::sync::Arc<std::sync::Mutex<Vec<crate::acp::streaming::StreamingChunk>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = collected.clone();
    let mut callback: StreamingCallback = Box::new(move |chunk| {
        sink.lock().unwrap().push(chunk);
        true
    });
    client
        .process_streaming_message_with_callback(&payload, 1, &mut state, &mut callback)
        .await
        .expect("frame processed");
    let guard = collected.lock().unwrap();
    guard.clone()
}

/// Wire pin for the `usage_update` frame on `session/update`. The JSON is
/// the claude 2.1.114 shape, byte for byte the fixture shape in
/// `tests/fixtures/acp/recorded/claude/basic-chat.jsonl`. The frame must
/// become one `ContextWindow` chunk, before and after any parser change.
#[tokio::test]
async fn a_usage_update_frame_becomes_one_context_window_chunk() {
    let chunks = chunks_for_frame(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "c299d62f",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 22700,
                "size": 1_000_000,
                "cost": { "amount": 0.14204, "currency": "USD" }
            }
        }
    }))
    .await;

    assert_eq!(
        chunks,
        vec![crate::acp::streaming::StreamingChunk::ContextWindow {
            used: 22700,
            limit: 1_000_000
        }]
    );
}

/// A frame that reports only one half of the window describes neither an
/// occupancy nor a window. The frame is dropped; the turn continues.
#[tokio::test]
async fn a_half_reported_usage_update_frame_is_dropped() {
    let chunks = chunks_for_frame(serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "s",
            "update": { "sessionUpdate": "usage_update", "used": 22700 }
        }
    }))
    .await;

    assert!(chunks.is_empty(), "a half window must emit nothing");
}
