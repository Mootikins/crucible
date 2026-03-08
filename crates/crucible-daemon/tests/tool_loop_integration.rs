use crucible_core::protocol::SessionEventMessage;
use crucible_core::traits::completion_backend::{BackendCompletionRequest, CompletionBackend};
use crucible_core::traits::context_ops::ContextMessage;
use crucible_core::traits::llm::ToolCall;
use crucible_daemon::test_support::MockCompletionBackend;
use futures::StreamExt;
use serde_json::json;
use std::path::Path;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};

mod streaming_mock;

use streaming_mock::next_event;

const SESSION_ID: &str = "tool-loop-test-session";
const MESSAGE_ID: &str = "msg-tool-loop";
const MAX_TOOL_DEPTH: usize = 10;

fn make_tool_call(call_id: &str, path: &str) -> ToolCall {
    ToolCall::new(call_id, "read_file", json!({ "path": path }).to_string())
}

async fn execute_mock_tool_loop(
    backend: &MockCompletionBackend,
    user_message: &str,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) {
    let mut messages = vec![ContextMessage::user(user_message)];
    let mut full_response = String::new();
    let mut tool_depth = 0usize;

    loop {
        let request = BackendCompletionRequest::new("You are helpful.", messages.clone());
        let mut stream = backend.complete_stream(request);

        let mut text_delta = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Some(delta) = chunk.delta {
                        text_delta.push_str(&delta);
                    }
                    tool_calls.extend(chunk.tool_calls);
                }
                Err(err) => {
                    let _ = event_tx.send(SessionEventMessage::ended(
                        SESSION_ID,
                        format!("error: {err}"),
                    ));
                    return;
                }
            }
        }

        if !tool_calls.is_empty() {
            tool_depth += 1;
            if tool_depth > MAX_TOOL_DEPTH {
                let _ = event_tx.send(SessionEventMessage::ended(
                    SESSION_ID,
                    format!("max_tool_depth exceeded ({MAX_TOOL_DEPTH})"),
                ));
                return;
            }

            messages.push(ContextMessage::assistant_with_tools("", tool_calls.clone()));

            for tool_call in tool_calls {
                let args = serde_json::from_str::<serde_json::Value>(&tool_call.function.arguments)
                    .unwrap_or(serde_json::Value::Null);
                let call_id = tool_call.id.clone();
                let tool_name = tool_call.function.name.clone();

                let _ = event_tx.send(SessionEventMessage::tool_call(
                    SESSION_ID,
                    call_id.clone(),
                    tool_name.clone(),
                    args.clone(),
                ));

                let result_text = execute_workspace_tool(&tool_name, &args)
                    .unwrap_or_else(|e| format!("tool error: {e}"));

                let _ = event_tx.send(SessionEventMessage::tool_result(
                    SESSION_ID,
                    call_id.clone(),
                    tool_name,
                    json!(result_text.clone()),
                ));

                messages.push(ContextMessage::tool_result(call_id, result_text));
            }

            continue;
        }

        if !text_delta.is_empty() {
            full_response.push_str(&text_delta);
        }

        let _ = event_tx.send(SessionEventMessage::message_complete(
            SESSION_ID,
            MESSAGE_ID,
            full_response,
            None,
        ));
        return;
    }
}

fn execute_workspace_tool(tool_name: &str, args: &serde_json::Value) -> Result<String, String> {
    if tool_name != "read_file" {
        return Err(format!("unsupported tool: {tool_name}"));
    }

    let path = args
        .get("path")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "missing path argument".to_string())?;

    std::fs::read_to_string(Path::new(path)).map_err(|e| e.to_string())
}

#[tokio::test]
async fn tool_loop_single_call_executes_tool_and_continues() {
    std::fs::write("/tmp/test.txt", "tool loop integration file").expect("write /tmp/test.txt");

    let backend = MockCompletionBackend::new();
    backend.push_response_chunks(vec![
        Ok(
            crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                make_tool_call("call_1", "/tmp/test.txt"),
            ),
        ),
        Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
    ]);
    backend.push_text_response("I read the file");

    let (event_tx, mut event_rx) = broadcast::channel(128);
    execute_mock_tool_loop(&backend, "read the file", &event_tx).await;

    let tool_call = next_event(&mut event_rx, "tool_call").await;
    assert_eq!(tool_call.data["tool"], "read_file");

    let tool_result = next_event(&mut event_rx, "tool_result").await;
    assert_eq!(tool_result.data["tool"], "read_file");

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "I read the file");

    assert_eq!(backend.request_count(), 2);
    let requests = backend.requests();
    assert!(
        requests[1]
            .messages
            .iter()
            .any(|message| message.role == crucible_core::traits::llm::MessageRole::Tool),
        "expected continuation request to include tool role message"
    );
}

#[tokio::test]
async fn tool_loop_stops_when_max_tool_depth_exceeded() {
    std::fs::write("/tmp/test.txt", "max depth integration file").expect("write /tmp/test.txt");

    let backend = MockCompletionBackend::new();
    for idx in 0..11 {
        backend.push_response_chunks(vec![
            Ok(
                crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                    make_tool_call(&format!("call_{idx}"), "/tmp/test.txt"),
                ),
            ),
            Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
        ]);
    }

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "loop forever", &event_tx).await;

    let ended = next_event(&mut event_rx, "ended").await;
    let reason = ended.data["reason"]
        .as_str()
        .expect("ended reason should be string");
    assert!(
        reason.contains("max_tool_depth exceeded"),
        "unexpected end reason: {reason}"
    );

    assert_eq!(backend.request_count(), 11);
}

#[tokio::test]
async fn tool_loop_without_tool_calls_streams_normal_response() {
    let backend = MockCompletionBackend::new();
    backend.push_text_response("Plain response");

    let (event_tx, mut event_rx) = broadcast::channel(128);
    execute_mock_tool_loop(&backend, "plain response please", &event_tx).await;

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "Plain response");

    let no_tool_call_seen = timeout(Duration::from_millis(300), async {
        loop {
            let event = event_rx.recv().await.expect("event channel closed");
            if event.event == "tool_call" {
                return false;
            }
        }
    })
    .await
    .is_err();

    assert!(
        no_tool_call_seen,
        "unexpected tool_call event in plain response flow"
    );
    assert_eq!(backend.request_count(), 1);
}
