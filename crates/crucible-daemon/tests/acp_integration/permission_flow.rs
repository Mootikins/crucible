//! Permission flow integration tests — verifies that ACP permission requests
//! from agents are correctly routed through the `PermissionRequestHandler`,
//! and that approved/denied outcomes are correctly communicated back to the agent.
//!
//! In the ACP model, the *agent* decides when to ask for permission (before running
//! unsafe tools like `bash` or `write_file`). It sends a `session/request_permission`
//! JSON-RPC request to the client. The client's `PermissionRequestHandler` evaluates
//! the request and responds with either `Selected` (approved) or `Cancelled` (denied).
//!
//! The scripted agent captures each reply the client writes and hands it back
//! to the test body, which asserts on it. The reply frame is the contract: the
//! agent's own follow-up text proves nothing about what the client answered.
//! An assertion inside the agent task would also report as "agent closed
//! connection" rather than as the mismatch.

use crate::scripted_agent::client_with_permission;
use crate::scripted_agent::prompt_with;
use crate::scripted_agent::{
    client_with_custom_transport, final_response, make_prompt_request, read_frame, read_request_id,
    text_chunk, tool_call_notification, tool_call_update_completed, write_json_line, AgentReader,
    AgentWriter,
};
use agent_client_protocol::schema::v1::{
    PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    SelectedPermissionOutcome, ToolKind,
};
use crucible_daemon::acp::client::PermissionRequestHandler;
use crucible_daemon::acp::StreamingChunk;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

fn permission_request_msg(
    session_id: &str,
    request_id: u64,
    tool_call_id: &str,
    tool_title: &str,
) -> Value {
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

/// Play one turn from the agent's side and return every reply the client
/// wrote to a permission request.
///
/// Each frame in `script` goes to the client in order. A frame that carries
/// both an `id` and a `method` is a request, so the agent waits for the
/// client's reply before it sends the next frame.
async fn permission_turn(
    mut reader: AgentReader,
    mut writer: AgentWriter,
    script: Vec<Value>,
) -> Vec<Value> {
    let prompt_request_id = read_request_id(&mut reader).await;
    let mut replies = Vec::new();
    for frame in script {
        let is_request = frame.get("id").is_some() && frame.get("method").is_some();
        write_json_line(&mut writer, frame).await;
        if is_request {
            replies.push(read_frame(&mut reader).await);
        }
    }
    write_json_line(&mut writer, final_response(prompt_request_id)).await;
    replies
}

/// Assert that `reply` answers request `id` with `outcome`, and with
/// `option_id` when the outcome is `selected`.
fn assert_permission_reply(reply: &Value, id: u64, outcome: &str, option_id: Option<&str>) {
    assert_eq!(reply["jsonrpc"], "2.0", "reply: {reply}");
    assert_eq!(
        reply["id"], id,
        "the reply must answer request {id}: {reply}"
    );
    assert!(
        reply.get("error").is_none(),
        "a permission answer is a result: {reply}"
    );
    assert_eq!(
        reply["result"]["outcome"]["outcome"], outcome,
        "reply: {reply}"
    );
    match option_id {
        Some(option_id) => assert_eq!(
            reply["result"]["outcome"]["optionId"], option_id,
            "reply: {reply}"
        ),
        None => assert!(
            reply["result"]["outcome"].get("optionId").is_none(),
            "a cancelled outcome selects no option: {reply}"
        ),
    }
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
                        PermissionOptionKind::AllowOnce | PermissionOptionKind::AllowAlways
                    )
                })
                .map(|o| o.option_id.clone())
                .unwrap_or_else(|| request.options[0].option_id.clone());
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
        })
    })
}

/// Build a permission handler that records all requests for later inspection,
/// then approves by selecting the first option.
fn recording_handler() -> (
    PermissionRequestHandler,
    Arc<Mutex<Vec<RequestPermissionRequest>>>,
) {
    let recorded: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_clone = Arc::clone(&recorded);

    let handler: PermissionRequestHandler = Arc::new(move |request| {
        let recorded = recorded_clone.clone();
        Box::pin(async move {
            recorded.lock().unwrap().push(request.clone());
            let option_id = request.options[0].option_id.clone();
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
        })
    });

    (handler, recorded)
}

/// Verifies that the permission handler is invoked with the correct request
/// details, and that its choice goes back to the agent under the request's id.
#[tokio::test]
async fn acp_permission_handler_receives_correct_request_details() {
    let (handler, recorded) = recording_handler();
    let (client, reader, writer) = client_with_permission(Some(500), Some(handler)).await;

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            permission_request_msg("ses-details", 400, "tool-exec-1", "execute_command"),
            text_chunk("ses-details", "Done."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-details", "exec something"),
        Box::new(|_| true),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    turn.expect("streaming should complete");

    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 1, "handler should have been called once");

    let req = &requests[0];
    assert_eq!(req.session_id.to_string(), "ses-details");
    assert_eq!(req.tool_call.tool_call_id.0.as_ref(), "tool-exec-1");
    assert_eq!(req.options.len(), 2);
    assert_eq!(req.options[0].option_id.0.as_ref(), "allow_once");
    assert_eq!(req.options[0].kind, PermissionOptionKind::AllowOnce);
    assert_eq!(req.options[1].option_id.0.as_ref(), "reject_once");
    assert_eq!(req.options[1].kind, PermissionOptionKind::RejectOnce);

    assert_eq!(replies.len(), 1, "one request, one reply: {replies:?}");
    assert_permission_reply(&replies[0], 400, "selected", Some("allow_once"));
}

/// Safe tools (like read_file, semantic_search) don't trigger permission requests
/// in the ACP model — the agent simply executes them. This test verifies that a
/// tool call without a preceding permission request works normally and the
/// permission handler is never invoked.
#[tokio::test]
async fn acp_safe_tool_no_permission_request_needed() {
    let (handler, recorded) = recording_handler();
    let (client, reader, writer) = client_with_permission(Some(500), Some(handler)).await;

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            tool_call_notification(
                "ses-safe",
                "tool-read-1",
                "read_file",
                Some(json!({"path": "src/main.rs"})),
            ),
            tool_call_update_completed(
                "ses-safe",
                "tool-read-1",
                Some(json!("fn main() { println!(\"hello\"); }")),
            ),
            text_chunk("ses-safe", "Here is the file content."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-safe", "read main.rs"),
        Box::new(move |chunk| {
            chunks_cb.lock().unwrap().push(chunk);
            true
        }),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    let (summary, _response) = turn.expect("streaming should complete without permission request");

    assert!(
        recorded.lock().unwrap().is_empty(),
        "safe tools should not trigger permission requests"
    );
    assert!(replies.is_empty(), "nothing asked, nothing answered");

    assert!(summary.announced_any);
    let captured = chunks.lock().unwrap();
    assert_eq!(
        crate::support::parity::tool_names_of(&captured),
        vec!["Read File"]
    );
    let chunk_kinds: Vec<&str> = captured
        .iter()
        .map(crate::support::parity::chunk_kind)
        .collect();
    assert_eq!(
        chunk_kinds,
        vec!["tool_start", "tool_end", "text"],
        "should see tool execution then text, no permission involved"
    );
}

#[tokio::test]
async fn acp_permission_approved_sends_selected_response_to_agent() {
    let (client, reader, writer) =
        client_with_permission(Some(2000), Some(always_approve_handler())).await;

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            permission_request_msg("ses-perm-approve", 100, "tool-bash-1", "bash"),
            tool_call_notification(
                "ses-perm-approve",
                "tool-bash-1",
                "bash",
                Some(json!({"command": "echo hello"})),
            ),
            tool_call_update_completed("ses-perm-approve", "tool-bash-1", Some(json!("hello\n"))),
            text_chunk("ses-perm-approve", "Command executed successfully."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-perm-approve", "run echo hello"),
        Box::new(move |chunk| {
            chunks_cb.lock().unwrap().push(chunk);
            true
        }),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    let (summary, _response) = turn.expect("streaming should complete after permission approval");

    assert_eq!(replies.len(), 1, "one request, one reply: {replies:?}");
    assert_permission_reply(&replies[0], 100, "selected", Some("allow_once"));

    assert!(summary.announced_any);
    assert_eq!(
        crate::support::parity::tool_names_of(&chunks.lock().unwrap()),
        vec!["Bash"]
    );
}

/// A handler that denies is invoked, and its denial reaches the agent as a
/// `cancelled` outcome under the request's id.
#[tokio::test]
async fn acp_permission_denied_sends_cancelled_response_to_agent() {
    let denied = Arc::new(Mutex::new(false));
    let denied_clone = Arc::clone(&denied);
    let handler: PermissionRequestHandler = Arc::new(move |_request| {
        let denied = denied_clone.clone();
        Box::pin(async move {
            *denied.lock().unwrap() = true;
            RequestPermissionOutcome::Cancelled
        })
    });

    let (client, reader, writer) = client_with_permission(Some(2000), Some(handler)).await;

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            permission_request_msg("ses-perm-deny", 200, "tool-write-1", "write_file"),
            text_chunk("ses-perm-deny", "Permission was denied by user."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-perm-deny", "write a file"),
        Box::new(|_| true),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    turn.expect("streaming should complete after permission denial");

    assert!(
        *denied.lock().unwrap(),
        "deny handler should have been invoked"
    );
    assert_eq!(replies.len(), 1, "one request, one reply: {replies:?}");
    assert_permission_reply(&replies[0], 200, "cancelled", None);
}

/// With no handler installed, the client still answers — `cancelled`, so the
/// agent neither hangs nor runs the tool.
#[tokio::test]
async fn acp_permission_handler_not_set_defaults_to_cancelled() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(2000)).await;

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            permission_request_msg("ses-no-handler", 300, "tool-rm-1", "bash"),
            text_chunk("ses-no-handler", "Operation cancelled."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-no-handler", "delete everything"),
        Box::new(|_| true),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    turn.expect("streaming should complete with auto-cancelled permission");

    assert_eq!(replies.len(), 1, "one request, one reply: {replies:?}");
    assert_permission_reply(&replies[0], 300, "cancelled", None);
}

#[tokio::test]
async fn acp_multiple_permission_requests_in_single_turn() {
    let (handler, recorded) = recording_handler();
    let (client, reader, writer) = client_with_permission(Some(2000), Some(handler)).await;

    let chunks: Arc<Mutex<Vec<StreamingChunk>>> = Arc::new(Mutex::new(Vec::new()));
    let chunks_cb = Arc::clone(&chunks);

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            permission_request_msg("ses-multi", 500, "tool-bash-m1", "bash"),
            tool_call_notification(
                "ses-multi",
                "tool-bash-m1",
                "bash",
                Some(json!({"command": "ls"})),
            ),
            tool_call_update_completed(
                "ses-multi",
                "tool-bash-m1",
                Some(json!("file1.rs\nfile2.rs")),
            ),
            permission_request_msg("ses-multi", 501, "tool-write-m1", "write_file"),
            tool_call_notification(
                "ses-multi",
                "tool-write-m1",
                "write_file",
                Some(json!({"path": "output.txt", "content": "data"})),
            ),
            tool_call_update_completed(
                "ses-multi",
                "tool-write-m1",
                Some(json!("Written 4 bytes")),
            ),
            text_chunk("ses-multi", "Both operations completed."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-multi", "list files then write output"),
        Box::new(move |chunk| {
            chunks_cb.lock().unwrap().push(chunk);
            true
        }),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    let (summary, _response) =
        turn.expect("streaming should complete with multiple permission requests");

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

    assert_eq!(replies.len(), 2, "two requests, two replies: {replies:?}");
    assert_permission_reply(&replies[0], 500, "selected", Some("allow_once"));
    assert_permission_reply(&replies[1], 501, "selected", Some("allow_once"));

    assert!(summary.announced_any);
    assert_eq!(
        crate::support::parity::tool_names_of(&chunks.lock().unwrap()),
        vec!["Bash", "Write File"]
    );
}

/// Hermes attaches a fresh id `perm-check-N` to a dangerous-command
/// permission request. The id matches no announced tool call. The daemon
/// names the tool from `kind`, not from the id, so the request still gets
/// an answer. This test pins that contract: a later "look the call up by
/// id" change must not break Hermes.
#[tokio::test]
async fn permission_request_with_an_unknown_tool_call_id_is_answered_from_kind() {
    // The handler reads only `kind`, the way the daemon's gate does. An
    // answer therefore proves the kind survived, and that the unknown id
    // did not stop the request.
    let handler: PermissionRequestHandler = Arc::new(|request| {
        Box::pin(async move {
            if request.tool_call.fields.kind != Some(ToolKind::Execute) {
                return RequestPermissionOutcome::Cancelled;
            }
            let option_id = request
                .options
                .iter()
                .find(|o| o.kind == PermissionOptionKind::AllowOnce)
                .map(|o| o.option_id.clone())
                .expect("hermes always offers allow_once");
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id))
        })
    });
    let (client, reader, writer) = client_with_permission(Some(2000), Some(handler)).await;

    let agent = tokio::spawn(permission_turn(
        reader,
        writer,
        vec![
            // The running call is announced under its own `tc-` id.
            tool_call_notification(
                "ses-hermes-perm",
                "tc-1a2b3c4d5e6f",
                "terminal: rm -rf build",
                Some(json!({"command": "rm -rf build"})),
            ),
            // The permission request carries a fresh id, in the hermes
            // shape: four options, and a toolCall that names no announced id.
            json!({
                "jsonrpc": "2.0",
                "id": 700,
                "method": "session/request_permission",
                "params": {
                    "sessionId": "ses-hermes-perm",
                    "toolCall": {
                        "toolCallId": "perm-check-1",
                        "title": "terminal: rm -rf build",
                        "kind": "execute",
                        "status": "pending"
                    },
                    "options": [
                        {"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"},
                        {"optionId": "allow_session", "name": "Allow for this session", "kind": "allow_always"},
                        {"optionId": "deny", "name": "Deny", "kind": "reject_once"},
                        {"optionId": "deny_always", "name": "Always deny", "kind": "reject_always"}
                    ]
                }
            }),
            // Permission granted. The running call completes under its own id.
            tool_call_update_completed("ses-hermes-perm", "tc-1a2b3c4d5e6f", None),
            text_chunk("ses-hermes-perm", "Removed."),
        ],
    ));

    let turn = prompt_with(
        &client,
        make_prompt_request("ses-hermes-perm", "remove the build directory"),
        Box::new(|_| true),
    )
    .await;
    let replies = agent.await.expect("the scripted agent finished its turn");
    let (summary, _response) = turn.expect("the turn must complete after the permission answer");

    assert_eq!(replies.len(), 1, "one request, one reply: {replies:?}");
    assert_permission_reply(&replies[0], 700, "selected", Some("allow_once"));
    assert!(summary.announced_any, "the tc- call was announced");
}

/// A turn that the daemon drops while a permission question waits for the
/// user ends that question with `cancelled`. The agent waits for the answer
/// before it ends the turn, so a question that stays open holds the turn
/// until the deadline.
#[tokio::test]
async fn a_dropped_turn_answers_a_pending_permission_request_with_cancelled() {
    // The user never answers.
    let handler: PermissionRequestHandler = Arc::new(|_| Box::pin(std::future::pending()));
    let (client, mut reader, mut writer) = client_with_permission(Some(2000), Some(handler)).await;

    let agent = tokio::spawn(async move {
        let prompt_id = read_request_id(&mut reader).await;
        write_json_line(
            &mut writer,
            permission_request_msg("ses-drop", 900, "tool-1", "execute_command"),
        )
        .await;
        write_json_line(&mut writer, text_chunk("ses-drop", "waiting")).await;
        // The client sends `session/cancel` and the answer, in either order.
        let frames = [read_frame(&mut reader).await, read_frame(&mut reader).await];
        write_json_line(
            &mut writer,
            json!({"jsonrpc": "2.0", "id": prompt_id, "result": {"stopReason": "cancelled"}}),
        )
        .await;
        frames
    });

    // The first chunk drops the turn, the way the daemon cancels one.
    let (_summary, response) =
        prompt_with(&client, make_prompt_request("ses-drop", "go"), |_| false)
            .await
            .expect("the cancelled turn ends");
    let frames = agent.await.expect("the scripted agent finished its turn");

    assert_eq!(
        response.stop_reason,
        agent_client_protocol::schema::v1::StopReason::Cancelled
    );
    let cancel = frames
        .iter()
        .find(|f| f["method"] == "session/cancel")
        .unwrap_or_else(|| panic!("the client sent no session/cancel: {frames:?}"));
    assert_eq!(cancel["params"]["sessionId"], "ses-drop");
    let reply = frames
        .iter()
        .find(|f| f.get("method").is_none())
        .unwrap_or_else(|| panic!("the client did not answer the question: {frames:?}"));
    assert_permission_reply(reply, 900, "cancelled", None);
}
