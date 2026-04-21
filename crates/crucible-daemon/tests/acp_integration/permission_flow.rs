//! Permission flow integration tests — verifies that ACP permission requests
//! from agents are correctly routed through the `PermissionRequestHandler`,
//! and that approved/denied outcomes are correctly communicated back to the agent.
//!
//! In the ACP model, the *agent* decides when to ask for permission (before running
//! unsafe tools like `bash` or `write_file`). It sends a `session/request_permission`
//! JSON-RPC request to the client. The client's `PermissionRequestHandler` evaluates
//! the request and responds with either `Selected` (approved) or `Cancelled` (denied).
//!
//! Note: These tests caught a bug in `write_permission_response()` where the
//! transport guard only checked `agent_stdin` but not `boxed_writer`, silently
//! dropping permission responses on custom/in-process transports. Fixed by
//! changing the guard to check both.

use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient, PermissionRequestHandler};
use crucible_daemon::acp::StreamingChunk;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-permission-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms,
        max_retries: Some(1),
    }
}

fn client_with_custom_transport(
    timeout_ms: Option<u64>,
) -> (
    CrucibleAcpClient,
    BufReader<tokio::io::ReadHalf<DuplexStream>>,
    tokio::io::WriteHalf<DuplexStream>,
) {
    let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(65_536);
    let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(65_536);

    let (_client_read_unused, client_write) = tokio::io::split(client_to_agent_client);
    let (agent_read, _agent_write_unused) = tokio::io::split(client_to_agent_agent);

    let (_agent_read_unused, agent_write) = tokio::io::split(agent_to_client_agent);
    let (client_read, _client_write_unused) = tokio::io::split(agent_to_client_client);

    let client = CrucibleAcpClient::with_transport(
        test_config(timeout_ms),
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );

    (client, BufReader::new(agent_read), agent_write)
}

fn make_prompt_request(session_id: &str, text: &str) -> agent_client_protocol::PromptRequest {
    serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": text}],
        "_meta": null
    }))
    .expect("valid prompt request")
}

async fn write_json_line(
    writer: &mut tokio::io::WriteHalf<DuplexStream>,
    value: serde_json::Value,
) -> std::io::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&value).unwrap()).as_bytes())
        .await?;
    writer.flush().await
}

fn permission_request_msg(
    session_id: &str,
    request_id: u64,
    tool_call_id: &str,
    tool_title: &str,
) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "session/request_permission",
        "params": {
            "sessionId": session_id,
            "toolCall": {
                "toolCallId": tool_call_id,
                "title": tool_title,
                "status": "in_progress"
            },
            "options": [
                {
                    "optionId": "allow_once",
                    "name": "Allow once",
                    "kind": "allow_once"
                },
                {
                    "optionId": "reject_once",
                    "name": "Reject once",
                    "kind": "reject_once"
                }
            ]
        }
    })
}

fn tool_call_notification(
    session_id: &str,
    tool_call_id: &str,
    title: &str,
    raw_input: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut update = json!({
        "sessionUpdate": "tool_call",
        "toolCallId": tool_call_id,
        "title": title,
        "status": "in_progress"
    });
    if let Some(input) = raw_input {
        update["rawInput"] = input;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

fn tool_call_update_completed(
    session_id: &str,
    tool_call_id: &str,
    raw_output: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": tool_call_id,
        "status": "completed"
    });
    if let Some(output) = raw_output {
        update["rawOutput"] = output;
    }
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

fn text_chunk(session_id: &str, text: &str) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": text}
            }
        }
    })
}

fn final_response(request_id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {"stopReason": "end_turn", "_meta": null}
    })
}

/// Build a permission handler that always approves by selecting the first allow option.
fn always_approve_handler() -> PermissionRequestHandler {
    Arc::new(|request| {
        Box::pin(async move {
            let option_id = request
                .options
                .iter()
                .find(|o| {
                    matches!(
                        o.kind,
                        agent_client_protocol::PermissionOptionKind::AllowOnce
                            | agent_client_protocol::PermissionOptionKind::AllowAlways
                    )
                })
                .map(|o| o.option_id.clone())
                .unwrap_or_else(|| request.options[0].option_id.clone());
            agent_client_protocol::RequestPermissionOutcome::Selected(
                agent_client_protocol::SelectedPermissionOutcome::new(option_id),
            )
        })
    })
}

/// Build a permission handler that always denies (cancels) the request.
fn always_deny_handler() -> PermissionRequestHandler {
    Arc::new(|_request| {
        Box::pin(async move { agent_client_protocol::RequestPermissionOutcome::Cancelled })
    })
}

/// Build a permission handler that records all requests for later inspection,
/// then approves by selecting the first option.
fn recording_handler() -> (
    PermissionRequestHandler,
    Arc<Mutex<Vec<agent_client_protocol::RequestPermissionRequest>>>,
) {
    let recorded: Arc<Mutex<Vec<agent_client_protocol::RequestPermissionRequest>>> =
        Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded);

    let handler: PermissionRequestHandler = Arc::new(move |request| {
        let recorded = recorded_clone.clone();
        Box::pin(async move {
            recorded.lock().unwrap().push(request.clone());
            let option_id = request.options[0].option_id.clone();
            agent_client_protocol::RequestPermissionOutcome::Selected(
                agent_client_protocol::SelectedPermissionOutcome::new(option_id),
            )
        })
    });

    (handler, recorded)
}

// ---------------------------------------------------------------------------
// Tests that work without the write_permission_response fix
// ---------------------------------------------------------------------------

/// Verifies that the permission handler is invoked with the correct request
/// details when the agent sends a `session/request_permission` message.
///
/// The mock agent does NOT wait for the permission response (which would hang
/// due to the transport bug), instead it immediately proceeds with the turn.
#[tokio::test]
async fn acp_permission_handler_receives_correct_request_details() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let (handler, recorded) = recording_handler();
    client = client.with_permission_handler(handler);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Send permission request with specific tool details
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-details", 400, "tool-exec-1", "execute_command"),
        )
        .await
        .unwrap();

        // Read (and discard) the permission response
        let mut _response_line = String::new();
        agent_reader.read_line(&mut _response_line).await.unwrap();

        write_json_line(&mut agent_writer, text_chunk("ses-details", "Done."))
            .await
            .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-details", "exec something");
    client
        .send_prompt_with_streaming(request)
        .await
        .expect("streaming should complete");

    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 1, "handler should have been called once");

    let req = &requests[0];
    assert_eq!(req.session_id.to_string(), "ses-details");
    assert_eq!(req.tool_call.tool_call_id.0.as_ref(), "tool-exec-1");
    assert_eq!(req.options.len(), 2);
    assert_eq!(req.options[0].option_id.0.as_ref(), "allow_once");
    assert_eq!(
        req.options[0].kind,
        agent_client_protocol::PermissionOptionKind::AllowOnce
    );
    assert_eq!(req.options[1].option_id.0.as_ref(), "reject_once");
    assert_eq!(
        req.options[1].kind,
        agent_client_protocol::PermissionOptionKind::RejectOnce
    );
}

/// Verifies that when no permission handler is set, the handler is NOT called
/// (there's nothing to call) and the client silently drops the response.
/// The agent can still complete its turn — the permission request is a
/// fire-and-forget from the agent's perspective in this test.
#[tokio::test]
async fn acp_permission_no_handler_does_not_crash() {
    // No handler set — verifies graceful behavior
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Agent asks permission without a handler being set
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-no-handler", 300, "tool-rm-1", "bash"),
        )
        .await
        .unwrap();

        // Read (and discard) the permission response
        let mut _response_line = String::new();
        agent_reader.read_line(&mut _response_line).await.unwrap();

        // Proceed with turn
        write_json_line(
            &mut agent_writer,
            text_chunk("ses-no-handler", "Operation cancelled."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-no-handler", "delete everything");
    let (content, _tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("streaming should complete without crashing even with no handler");

    assert!(content.contains("cancelled"));
}

/// Safe tools (like read_file, semantic_search) don't trigger permission requests
/// in the ACP model — the agent simply executes them. This test verifies that a
/// tool call without a preceding permission request works normally and the
/// permission handler is never invoked.
#[tokio::test]
async fn acp_safe_tool_no_permission_request_needed() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let (handler, recorded) = recording_handler();
    client = client.with_permission_handler(handler);

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Agent calls read_file directly — no permission needed
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-safe",
                "tool-read-1",
                "read_file",
                Some(json!({"path": "src/main.rs"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                "ses-safe",
                "tool-read-1",
                Some(json!("fn main() { println!(\"hello\"); }")),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-safe", "Here is the file content."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-safe", "read main.rs");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete without permission request");

    // Permission handler should NOT have been called
    let requests = recorded.lock().unwrap();
    assert!(
        requests.is_empty(),
        "safe tools should not trigger permission requests"
    );

    assert!(content.contains("file content"));
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].title, "read_file");

    let captured = chunks.lock().unwrap();
    let chunk_kinds: Vec<&str> = captured
        .iter()
        .map(|c| match c {
            StreamingChunk::Text(_) => "text",
            StreamingChunk::Thinking(_) => "thinking",
            StreamingChunk::ToolStart { .. } => "tool_start",
            StreamingChunk::ToolEnd { .. } => "tool_end",
        })
        .collect();
    assert_eq!(
        chunk_kinds,
        vec!["tool_start", "tool_end", "text"],
        "should see tool execution then text, no permission involved"
    );
}

/// Verifies that the deny handler is invoked and returns `Cancelled`.
/// The mock agent does not wait for the response (transport bug), but we
/// verify the handler was called and returned the correct outcome.
#[tokio::test]
async fn acp_permission_deny_handler_invoked() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(500));

    let denied = Arc::new(Mutex::new(false));
    let denied_clone = Arc::clone(&denied);

    let handler: PermissionRequestHandler = Arc::new(move |_request| {
        let denied = denied_clone.clone();
        Box::pin(async move {
            *denied.lock().unwrap() = true;
            agent_client_protocol::RequestPermissionOutcome::Cancelled
        })
    });
    client = client.with_permission_handler(handler);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Agent asks permission for write_file
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-deny", 200, "tool-write-1", "write_file"),
        )
        .await
        .unwrap();

        // Read (and discard) the permission response
        let mut _response_line = String::new();
        agent_reader.read_line(&mut _response_line).await.unwrap();

        // Proceed with turn
        write_json_line(
            &mut agent_writer,
            text_chunk("ses-deny", "Permission was denied."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-deny", "write a file");
    let (content, _tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("streaming should complete");

    assert!(
        *denied.lock().unwrap(),
        "deny handler should have been invoked"
    );
    assert!(content.contains("denied"));
}

// ---------------------------------------------------------------------------
// Tests verifying permission response round-trip to the agent
// ---------------------------------------------------------------------------

#[tokio::test]
// Fixed: write_permission_response now checks boxed_writer too (client.rs guard fix)
async fn acp_permission_approved_sends_selected_response_to_agent() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(2000));

    let handler = always_approve_handler();
    client = client.with_permission_handler(handler);

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Agent asks permission for bash
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-perm-approve", 100, "tool-bash-1", "bash"),
        )
        .await
        .unwrap();

        // Read permission response (requires the transport bug fix)
        let mut response_line = String::new();
        agent_reader.read_line(&mut response_line).await.unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();

        assert_eq!(response["id"], 100, "response ID should match request ID");
        let result = &response["result"];
        assert_eq!(
            result["outcome"]["outcome"], "selected",
            "handler approved, so outcome should be 'selected'"
        );
        assert_eq!(
            result["outcome"]["optionId"], "allow_once",
            "should select the allow_once option"
        );

        // Permission granted — agent proceeds
        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-perm-approve",
                "tool-bash-1",
                "bash",
                Some(json!({"command": "echo hello"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed("ses-perm-approve", "tool-bash-1", Some(json!("hello\n"))),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-perm-approve", "Command executed successfully."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-perm-approve", "run echo hello");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete after permission approval");

    assert!(content.contains("Command executed successfully"));
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].title, "bash");
}

#[tokio::test]
// Fixed: write_permission_response now checks boxed_writer too (client.rs guard fix)
async fn acp_permission_denied_sends_cancelled_response_to_agent() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(2000));

    let handler = always_deny_handler();
    client = client.with_permission_handler(handler);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // Agent asks permission for write_file
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-perm-deny", 200, "tool-write-1", "write_file"),
        )
        .await
        .unwrap();

        // Read permission response (requires the transport bug fix)
        let mut response_line = String::new();
        agent_reader.read_line(&mut response_line).await.unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();

        assert_eq!(response["id"], 200);
        assert_eq!(
            response["result"]["outcome"]["outcome"], "cancelled",
            "handler denied, so outcome should be 'cancelled'"
        );

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-perm-deny", "Permission was denied by user."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-perm-deny", "write a file");
    let (content, _tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("streaming should complete after permission denial");

    assert!(content.contains("denied"));
}

#[tokio::test]
// Fixed: write_permission_response now checks boxed_writer too (client.rs guard fix)
async fn acp_permission_handler_not_set_defaults_to_cancelled() {
    // No handler set — the client should automatically cancel
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(2000));

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-no-handler", 300, "tool-rm-1", "bash"),
        )
        .await
        .unwrap();

        // Read response — should be cancelled (requires transport bug fix)
        let mut response_line = String::new();
        agent_reader.read_line(&mut response_line).await.unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_line).unwrap();

        assert_eq!(response["id"], 300);
        assert_eq!(
            response["result"]["outcome"]["outcome"], "cancelled",
            "no handler means automatic cancellation"
        );

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-no-handler", "Operation cancelled."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-no-handler", "delete everything");
    let (content, _tool_calls, _response) = client
        .send_prompt_with_streaming(request)
        .await
        .expect("streaming should complete with auto-cancelled permission");

    assert!(content.contains("cancelled"));
}

#[tokio::test]
// Fixed: write_permission_response now checks boxed_writer too (client.rs guard fix)
async fn acp_multiple_permission_requests_in_single_turn() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(2000));

    let (handler, recorded) = recording_handler();
    client = client.with_permission_handler(handler);

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    tokio::spawn(async move {
        let mut request_line = String::new();
        agent_reader.read_line(&mut request_line).await.unwrap();
        let request: serde_json::Value = serde_json::from_str(&request_line).unwrap();
        let prompt_request_id = request["id"].as_u64().unwrap();

        // First permission request: bash
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-multi", 500, "tool-bash-m1", "bash"),
        )
        .await
        .unwrap();

        let mut response_line = String::new();
        agent_reader.read_line(&mut response_line).await.unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-multi",
                "tool-bash-m1",
                "bash",
                Some(json!({"command": "ls"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                "ses-multi",
                "tool-bash-m1",
                Some(json!("file1.rs\nfile2.rs")),
            ),
        )
        .await
        .unwrap();

        // Second permission request: write_file
        write_json_line(
            &mut agent_writer,
            permission_request_msg("ses-multi", 501, "tool-write-m1", "write_file"),
        )
        .await
        .unwrap();

        let mut response_line2 = String::new();
        agent_reader.read_line(&mut response_line2).await.unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_notification(
                "ses-multi",
                "tool-write-m1",
                "write_file",
                Some(json!({"path": "output.txt", "content": "data"})),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            tool_call_update_completed(
                "ses-multi",
                "tool-write-m1",
                Some(json!("Written 4 bytes")),
            ),
        )
        .await
        .unwrap();

        write_json_line(
            &mut agent_writer,
            text_chunk("ses-multi", "Both operations completed."),
        )
        .await
        .unwrap();

        write_json_line(&mut agent_writer, final_response(prompt_request_id))
            .await
            .unwrap();
    });

    let request = make_prompt_request("ses-multi", "list files then write output");
    let (content, tool_calls, _response) = client
        .send_prompt_with_callback(
            request,
            Box::new(move |chunk| {
                chunks_cb.lock().unwrap().push(chunk);
                true
            }),
        )
        .await
        .expect("streaming should complete with multiple permission requests");

    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].tool_call.tool_call_id.0.as_ref(),
        "tool-bash-m1"
    );
    assert_eq!(
        requests[1].tool_call.tool_call_id.0.as_ref(),
        "tool-write-m1"
    );

    assert_eq!(tool_calls.len(), 2);
    assert!(content.contains("Both operations completed"));
}
