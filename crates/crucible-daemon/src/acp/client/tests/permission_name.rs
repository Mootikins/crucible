//! The tool name a permission request is decided on, across the wire.
//!
//! An agent that asks about a tool it does not name is answered from the
//! `tool_call` frame it sent for the same id. The join is tested over a real
//! connection, with the frames the agents send, because the two frames are
//! two separate JSON-RPC messages and the order between them is the point.

use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{RequestPermissionOutcome, RequestPermissionRequest};
use agent_client_protocol::ByteStreams;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::RawAgent;
use crate::acp::client::{ClientConfig, CrucibleAcpClient, PermissionRequestHandler};

/// A client whose permission handler records what it was asked, then
/// rejects. The recorded request is what the tool policy would key on.
async fn recording_client() -> (
    CrucibleAcpClient,
    RawAgent,
    Arc<Mutex<Vec<RequestPermissionRequest>>>,
) {
    let seen: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::default();
    let recorder = Arc::clone(&seen);
    let permission: PermissionRequestHandler = Arc::new(move |request| {
        recorder
            .lock()
            .expect("the recorder is not poisoned")
            .push(request);
        Box::pin(async { RequestPermissionOutcome::Cancelled })
    });

    let (client_end, agent_end) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, agent_write) = tokio::io::split(agent_end);
    let client = CrucibleAcpClient::connect(
        ClientConfig::default(),
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
        "raw",
        Some(permission),
    )
    .await
    .expect("the client connects");
    let agent = RawAgent {
        lines: BufReader::new(agent_read).lines(),
        write: agent_write,
    };
    (client, agent, seen)
}

/// The `tool_call` frame codex-acp sends for an MCP call. It names no tool:
/// the identity is `rawInput.server` and `rawInput.tool`, and the title is
/// the same pair as prose.
fn codex_tool_call() -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "sess-1",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "call-1",
                "kind": "execute",
                "title": "mcp.crucible.read_note",
                "status": "pending",
                "rawInput": {
                    "server": "crucible",
                    "tool": "read_note",
                    "arguments": { "title": "Some Note" }
                },
                "_meta": { "is_mcp_tool_call": true }
            }
        }
    })
}

/// The MCP approval codex-acp then sends. `kind: "execute"` and nothing
/// else: no name, no title, no raw input.
fn codex_permission_request(tool_call_id: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session/request_permission",
        "params": {
            "sessionId": "sess-1",
            "toolCall": {
                "toolCallId": tool_call_id,
                "kind": "execute",
                "status": "pending"
            },
            "_meta": { "is_mcp_tool_approval": true },
            "options": [
                { "optionId": "allow", "name": "Allow", "kind": "allow_once" },
                { "optionId": "reject", "name": "Reject", "kind": "reject_once" }
            ]
        }
    })
}

/// Wait for the agent's own request to be answered, then read what the
/// handler saw. The answer proves the client finished with the request.
async fn asked_about(
    agent: &mut RawAgent,
    seen: &Arc<Mutex<Vec<RequestPermissionRequest>>>,
) -> RequestPermissionRequest {
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), agent.read())
        .await
        .expect("the client answers the permission request");
    assert!(reply.get("result").is_some(), "the client answers: {reply}");
    seen.lock()
        .expect("the recorder is not poisoned")
        .first()
        .cloned()
        .expect("the handler was asked")
}

/// An MCP approval that names no tool takes the name from the `tool_call`
/// frame with the same id.
///
/// Without the join the request carries only `kind: "execute"`, which the
/// coarse mapping calls `bash`. Every MCP call an agent asked about would
/// then be decided by the operator's rules for the shell.
#[tokio::test]
async fn an_unnamed_approval_takes_the_name_of_its_tool_call() {
    let (_client, mut agent, seen) = recording_client().await;

    agent.write(codex_tool_call()).await;
    agent.write(codex_permission_request("call-1")).await;

    let request = asked_about(&mut agent, &seen).await;
    assert_eq!(
        request.tool_call.fields.name.as_deref(),
        Some("mcp.crucible.read_note"),
        "the name must come from the frame that announced the call"
    );
}

/// A different id joins nothing. The request stays unnamed rather than
/// taking the name of some other tool call.
#[tokio::test]
async fn an_approval_for_another_id_takes_no_name() {
    let (_client, mut agent, seen) = recording_client().await;

    agent.write(codex_tool_call()).await;
    agent.write(codex_permission_request("call-2")).await;

    let request = asked_about(&mut agent, &seen).await;
    assert_eq!(
        request.tool_call.fields.name, None,
        "a name must never be guessed from another call"
    );
}

/// An agent that names the tool keeps that name. It is the authority on
/// what it is about to run.
#[tokio::test]
async fn a_named_request_keeps_its_own_name() {
    let (_client, mut agent, seen) = recording_client().await;

    // A `tool_call` for the same id that says something else. The request
    // wins.
    agent.write(codex_tool_call()).await;
    agent
        .write(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "session/request_permission",
            "params": {
                "sessionId": "sess-1",
                "toolCall": {
                    "toolCallId": "call-1",
                    "name": "mcp__crucible__write_note",
                    "title": "Write the note",
                    "status": "pending"
                },
                "options": [
                    { "optionId": "reject", "name": "Reject", "kind": "reject_once" }
                ]
            }
        }))
        .await;

    let request = asked_about(&mut agent, &seen).await;
    assert_eq!(
        request.tool_call.fields.name.as_deref(),
        Some("mcp__crucible__write_note"),
        "the agent's own name must not be rewritten by an earlier frame"
    );
}
