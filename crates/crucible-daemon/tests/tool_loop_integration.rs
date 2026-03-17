use crucible_core::protocol::SessionEventMessage;
use crucible_core::traits::completion_backend::{BackendCompletionRequest, CompletionBackend};
use crucible_core::traits::context_ops::ContextMessage;
use crucible_core::traits::llm::ToolCall;
use crucible_daemon::agent_manager::tool_tracking::ToolCallTracker;
use crucible_daemon::test_support::MockCompletionBackend;
use futures::StreamExt;
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use tempfile::NamedTempFile;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};

mod streaming_mock;

use streaming_mock::next_event;

const SESSION_ID: &str = "tool-loop-test-session";
const MESSAGE_ID: &str = "msg-tool-loop";
const MAX_TOOL_DEPTH: usize = 10;
const TOOL_DEPTH_LIMIT_FINAL_PROMPT: &str =
    "You have reached the tool call limit. Please provide your final answer based on the information gathered so far.";

fn make_tool_call(call_id: &str, path: &str) -> ToolCall {
    ToolCall::new(call_id, "read_file", json!({ "path": path }).to_string())
}

fn make_named_tool_call(call_id: &str, tool_name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall::new(call_id, tool_name, args.to_string())
}

async fn execute_mock_tool_loop(
    backend: &MockCompletionBackend,
    user_message: &str,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) {
    let mut messages = vec![ContextMessage::user(user_message)];
    let mut full_response = String::new();
    let mut tool_depth = 0usize;
    let mut tracker = ToolCallTracker::new();
    let mut blocked_tools: HashSet<String> = HashSet::new();
    let mut last_failure_key: Option<(String, String)> = None;
    let mut consecutive_failure_count = 0usize;

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
                let mut forced_messages = messages.clone();
                forced_messages.push(ContextMessage::user(TOOL_DEPTH_LIMIT_FINAL_PROMPT));
                let forced_request = BackendCompletionRequest::new("You are helpful.", forced_messages);
                let mut forced_stream = backend.complete_stream(forced_request);

                let before_forced_len = full_response.len();
                let mut forced_done = false;

                while let Some(chunk_result) = forced_stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            if let Some(delta) = chunk.delta {
                                full_response.push_str(&delta);
                            }
                            if chunk.done {
                                forced_done = true;
                                break;
                            }
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

                if forced_done && full_response.len() > before_forced_len {
                    let _ = event_tx.send(SessionEventMessage::message_complete(
                        SESSION_ID,
                        MESSAGE_ID,
                        full_response,
                        None,
                    ));
                } else {
                    let _ = event_tx.send(SessionEventMessage::ended(
                        SESSION_ID,
                        format!("max_tool_depth exceeded ({MAX_TOOL_DEPTH})"),
                    ));
                }
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

                let mut result_text = String::new();
                let mut error_text = if blocked_tools.contains(&tool_name) {
                    Some(format!(
                        "Tool '{tool_name}' is blocked for this stream after repeated failures."
                    ))
                } else {
                    match execute_workspace_tool(&tool_name, &args) {
                        Ok(value) => {
                            result_text = value;
                            None
                        }
                        Err(e) => Some(format!("tool error: {e}")),
                    }
                };

                let attempt = tracker.record_call(&tool_name, &args);
                let args_key = serde_json::to_string(&args).unwrap_or_else(|_| "null".to_string());

                if let Some(error) = error_text.as_mut() {
                    let failure_key = (tool_name.clone(), args_key.clone());
                    if last_failure_key.as_ref() == Some(&failure_key) {
                        consecutive_failure_count += 1;
                    } else {
                        consecutive_failure_count = 1;
                        last_failure_key = Some(failure_key);
                    }

                    if attempt >= 3 && tracker.is_repeat_failure(&tool_name, &args, 3) {
                        let annotation = format!(
                            "Attempt {attempt}. This tool has failed {attempt} times with identical arguments. Try a different approach."
                        );
                        if !error.contains(&annotation) {
                            if !error.is_empty() {
                                error.push(' ');
                            }
                            error.push_str(&annotation);
                        }
                    }

                    if consecutive_failure_count >= 3 {
                        blocked_tools.insert(tool_name.clone());
                    }
                } else {
                    last_failure_key = None;
                    consecutive_failure_count = 0;

                    if tool_depth == MAX_TOOL_DEPTH.saturating_sub(2) {
                        result_text.push_str(&format!(
                            " [Note: You have used {} of {} available tool turns.]",
                            tool_depth, MAX_TOOL_DEPTH
                        ));
                    }
                }

                let event_payload = if let Some(error) = &error_text {
                    json!({ "error": error })
                } else {
                    json!({ "result": result_text.clone() })
                };

                let _ = event_tx.send(SessionEventMessage::tool_result(
                    SESSION_ID,
                    call_id.clone(),
                    tool_name,
                    event_payload,
                ));

                messages.push(ContextMessage::tool_result(
                    call_id,
                    error_text.unwrap_or(result_text),
                ));
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
    match tool_name {
        "read_file" => {
            let path = args
                .get("path")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "missing path argument".to_string())?;

            std::fs::read_to_string(Path::new(path)).map_err(|e| e.to_string())
        }
        "glob" => {
            let pattern = args
                .get("pattern")
                .and_then(|value| value.as_str())
                .ok_or_else(|| "missing pattern argument".to_string())?;
            if Path::new(pattern).exists() {
                Ok(pattern.to_string())
            } else {
                Err(format!("glob pattern did not match: {pattern}"))
            }
        }
        _ => Err(format!("unsupported tool: {tool_name}")),
    }
}

#[tokio::test]
async fn tool_loop_single_call_executes_tool_and_continues() {
    let test_file = NamedTempFile::new().expect("create temp file");
    std::fs::write(test_file.path(), "tool loop integration file").expect("write temp test file");
    let test_path = test_file.path().to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    backend.push_response_chunks(vec![
        Ok(
            crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                make_tool_call("call_1", &test_path),
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
    let test_file = NamedTempFile::new().expect("create temp file");
    std::fs::write(test_file.path(), "max depth integration file").expect("write temp test file");
    let test_path = test_file.path().to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    for idx in 0..11 {
        backend.push_response_chunks(vec![
            Ok(
                crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                    make_tool_call(&format!("call_{idx}"), &test_path),
                ),
            ),
            Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
        ]);
    }
    backend.push_text_response("final answer after tool limit");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "loop forever", &event_tx).await;

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(
        complete.data["full_response"],
        "final answer after tool limit"
    );

    assert_eq!(backend.request_count(), 12);
}

#[tokio::test]
async fn graceful_depth_forces_message_complete_after_limit() {
    let test_file = NamedTempFile::new().expect("create temp file");
    std::fs::write(test_file.path(), "graceful depth integration file")
        .expect("write temp test file");
    let test_path = test_file.path().to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    for idx in 0..11 {
        backend.push_response_chunks(vec![
            Ok(
                crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                    make_tool_call(&format!("call_{idx}"), &test_path),
                ),
            ),
            Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
        ]);
    }
    backend.push_text_response("graceful depth final response");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "loop forever", &event_tx).await;

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(
        complete.data["full_response"],
        "graceful depth final response"
    );
}

#[tokio::test]
async fn graceful_depth_warns_two_before_limit_in_tool_result_content() {
    let test_file = NamedTempFile::new().expect("create temp file");
    std::fs::write(test_file.path(), "graceful depth warning integration file")
        .expect("write temp test file");
    let test_path = test_file.path().to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    for idx in 0..8 {
        backend.push_response_chunks(vec![
            Ok(
                crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                    make_tool_call(&format!("call_{idx}"), &test_path),
                ),
            ),
            Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
        ]);
    }
    backend.push_text_response("completed before hard limit");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "loop near limit", &event_tx).await;

    let mut eighth_result = String::new();
    for idx in 0..8 {
        let _ = next_event(&mut event_rx, "tool_call").await;
        let result_event = next_event(&mut event_rx, "tool_result").await;
        if idx == 7 {
            eighth_result = result_event.data["result"]["result"]
                .as_str()
                .expect("eighth tool result should contain result text")
                .to_string();
        }
    }

    assert!(
        eighth_result.contains("You have used 8 of 10 available tool turns."),
        "expected depth warning annotation in 8th tool result, got: {eighth_result}"
    );
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

#[tokio::test]
async fn tool_loop_blocks_tool_after_three_identical_failures() {
    let missing_file = NamedTempFile::new().expect("create temp file");
    let missing_path = missing_file.path().with_file_name("does-not-exist.txt");
    let missing_path = missing_path.to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    for idx in 0..4 {
        backend.push_response_chunks(vec![
            Ok(
                crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                    make_tool_call(&format!("call_{idx}"), &missing_path),
                ),
            ),
            Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
        ]);
    }
    backend.push_text_response("done");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "loop failing tool", &event_tx).await;

    let mut fourth_error: Option<String> = None;
    for idx in 0..4 {
        let _ = next_event(&mut event_rx, "tool_call").await;
        let result_event = next_event(&mut event_rx, "tool_result").await;
        let error = result_event.data["result"]["error"]
            .as_str()
            .expect("tool_result should contain error")
            .to_string();
        if idx == 3 {
            fourth_error = Some(error);
        }
    }

    let fourth_error = fourth_error.expect("expected fourth tool error");
    assert!(
        fourth_error.contains("Attempt 4"),
        "expected attempt annotation on fourth failure, got: {fourth_error}"
    );
    assert!(
        fourth_error.contains("blocked"),
        "expected blocked marker on fourth failure, got: {fourth_error}"
    );

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "done");
}

#[tokio::test]
async fn tool_loop_different_successful_tools_have_no_retry_annotation() {
    let alpha_file = NamedTempFile::new().expect("create alpha temp file");
    std::fs::write(alpha_file.path(), "alpha").expect("write alpha fixture");
    let alpha_path = alpha_file.path().to_string_lossy().into_owned();

    let beta_file = NamedTempFile::new().expect("create beta temp file");
    std::fs::write(beta_file.path(), "beta").expect("write beta fixture");
    let beta_path = beta_file.path().to_string_lossy().into_owned();

    let backend = MockCompletionBackend::new();
    backend.push_response_chunks(vec![
        Ok(
            crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                make_tool_call("call_alpha", &alpha_path),
            ),
        ),
        Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
    ]);
    backend.push_response_chunks(vec![
        Ok(
            crucible_core::traits::completion_backend::BackendCompletionChunk::tool_call(
                make_named_tool_call(
                    "call_beta",
                    "glob",
                    json!({ "pattern": beta_path }),
                ),
            ),
        ),
        Ok(crucible_core::traits::completion_backend::BackendCompletionChunk::finished(None)),
    ]);
    backend.push_text_response("all tools succeeded");

    let (event_tx, mut event_rx) = broadcast::channel(256);
    execute_mock_tool_loop(&backend, "run distinct tools", &event_tx).await;

    for _ in 0..2 {
        let _ = next_event(&mut event_rx, "tool_call").await;
        let result_event = next_event(&mut event_rx, "tool_result").await;

        let maybe_error = result_event.data["result"]["error"].as_str();
        assert!(
            maybe_error.is_none(),
            "did not expect tool error for successful distinct tools"
        );

        let serialized = result_event.data.to_string();
        assert!(
            !serialized.contains("Attempt "),
            "did not expect retry annotation for successful tools: {serialized}"
        );
    }

    let complete = next_event(&mut event_rx, "message_complete").await;
    assert_eq!(complete.data["full_response"], "all tools succeeded");
}
