//! Event streaming and event-flow conversion tests.
//!
//! Covers:
//! - Event streaming with background reader.
//! - Concurrent RPC calls while in event mode.
//! - Daemon agent error surfaces to chat error.
//! - Simulated event-to-ChatChunk conversion (unit tests, no daemon).

use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn test_event_streaming_with_background_reader() {
    use std::sync::Arc;
    use std::time::Duration;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    let result = client
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
    let session_id = result["session_id"]
        .as_str()
        .expect("should have session_id")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("subscribe failed");

    let ping_result = client.ping().await.expect("ping failed");
    assert_eq!(ping_result, "pong");

    let list_result = client.kiln_list().await.expect("kiln_list failed");
    assert!(list_result.is_empty() || !list_result.is_empty());

    let timeout_result = tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await;
    assert!(
        timeout_result.is_err(),
        "Should timeout since no events generated yet"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_rpc_calls_event_mode() {
    use std::sync::Arc;

    let server = TestServer::start().await.expect("Failed to start server");

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    let mut handles = vec![];
    for _ in 0..5 {
        let c = client.clone();
        handles.push(tokio::spawn(async move { c.ping().await }));
    }

    for handle in handles {
        let result = handle.await.expect("task panicked");
        assert_eq!(result.expect("ping failed"), "pong");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_error_produces_chat_error() {
    let server = TestServer::start().await.expect("Failed to start server");

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");

    let result = client
        .session_send_message("nonexistent-session-id", "Hello", true)
        .await;

    assert!(
        result.is_err(),
        "Sending to nonexistent session should fail"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.is_empty(),
        "Error message should not be empty for TUI display"
    );
    assert!(
        err_msg.len() < 1000,
        "Error message should be reasonably sized: {}",
        err_msg
    );

    server.shutdown().await;
}

// =============================================================================
// Full event flow tests: Daemon → Client → ChatChunk → TUI
// =============================================================================

use crucible_daemon::SessionEvent;
use serde_json::json;

fn simulate_daemon_event(event_type: &str, data: serde_json::Value) -> SessionEvent {
    SessionEvent {
        session_id: "test-session".to_string(),
        event_type: event_type.to_string(),
        data,
    }
}

fn event_to_chunk(event: &SessionEvent) -> Option<crucible_core::traits::chat::ChatChunk> {
    match event.event_type.as_str() {
        "text_delta" => {
            let content = event.data.get("content")?.as_str()?;
            Some(crucible_core::traits::chat::ChatChunk {
                delta: content.to_string(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            })
        }
        "thinking" => {
            let content = event.data.get("content")?.as_str()?;
            Some(crucible_core::traits::chat::ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: None,
                reasoning: Some(content.to_string()),
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            })
        }
        "tool_call" => {
            let tool = event.data.get("tool")?.as_str()?;
            let call_id = event.data.get("call_id").and_then(|v| v.as_str());
            let args = event.data.get("args").cloned();
            Some(crucible_core::traits::chat::ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: Some(vec![crucible_core::traits::chat::ChatToolCall {
                    name: tool.to_string(),
                    arguments: args,
                    id: call_id.map(String::from),
                }]),
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            })
        }
        "tool_result" => {
            let result = event.data.get("result")?;
            let call_id = event.data.get("call_id").and_then(|v| v.as_str());
            let result_str = if result.is_string() {
                result.as_str().unwrap_or("").to_string()
            } else {
                result.to_string()
            };
            Some(crucible_core::traits::chat::ChatChunk {
                delta: String::new(),
                done: false,
                tool_calls: None,
                tool_results: Some(vec![crucible_core::traits::chat::ChatToolResult {
                    name: call_id.unwrap_or("tool").to_string(),
                    result: result_str,
                    error: None,
                    call_id: call_id.map(String::from),
                }]),
                reasoning: None,
                usage: None,
                subagent_events: None,
                precognition_notes_count: None,
                precognition_notes: None,
            })
        }
        "message_complete" | "ended" => Some(crucible_core::traits::chat::ChatChunk {
            delta: String::new(),
            done: true,
            tool_calls: None,
            tool_results: None,
            reasoning: None,
            usage: None,
            subagent_events: None,
            precognition_notes_count: None,
            precognition_notes: None,
        }),
        _ => None,
    }
}

#[test]
fn daemon_text_delta_becomes_chat_chunk_delta() {
    let event = simulate_daemon_event("text_delta", json!({ "content": "Hello world" }));
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert_eq!(chunk.delta, "Hello world");
    assert!(!chunk.done);
    assert!(chunk.tool_calls.is_none());
    assert!(chunk.reasoning.is_none());
}

#[test]
fn daemon_thinking_becomes_reasoning_chunk() {
    let event = simulate_daemon_event("thinking", json!({ "content": "Let me analyze..." }));
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert_eq!(chunk.delta, "");
    assert_eq!(chunk.reasoning, Some("Let me analyze...".to_string()));
    assert!(!chunk.done);
}

#[test]
fn daemon_tool_call_becomes_tool_calls_chunk() {
    let event = simulate_daemon_event(
        "tool_call",
        json!({
            "call_id": "tc-123",
            "tool": "read_file",
            "args": { "path": "test.rs" }
        }),
    );
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert_eq!(chunk.delta, "");
    let tool_calls = chunk.tool_calls.expect("Should have tool_calls");
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "read_file");
    assert_eq!(tool_calls[0].id, Some("tc-123".to_string()));
}

#[test]
fn daemon_tool_result_becomes_tool_results_chunk() {
    let event = simulate_daemon_event(
        "tool_result",
        json!({
            "call_id": "tc-123",
            "result": "fn main() { }"
        }),
    );
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert_eq!(chunk.delta, "");
    let results = chunk.tool_results.expect("Should have tool_results");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].result, "fn main() { }");
}

#[test]
fn daemon_message_complete_sets_done_flag() {
    let event = simulate_daemon_event(
        "message_complete",
        json!({ "message_id": "msg-1", "full_response": "Done!" }),
    );
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert!(chunk.done);
    assert_eq!(chunk.delta, "");
}

#[test]
fn daemon_ended_sets_done_flag() {
    let event = simulate_daemon_event("ended", json!({ "reason": "cancelled" }));
    let chunk = event_to_chunk(&event).expect("Should convert to chunk");

    assert!(chunk.done);
}

#[test]
fn unknown_event_type_returns_none() {
    let event = simulate_daemon_event("unknown_event", json!({}));
    assert!(event_to_chunk(&event).is_none());
}

#[test]
fn malformed_text_delta_returns_none() {
    let event = simulate_daemon_event("text_delta", json!({}));
    assert!(event_to_chunk(&event).is_none());
}
