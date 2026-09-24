//! The canonical call a permission request is decided on, across the wire.
//!
//! A request joins the tool call of its `toolCallId`: a field that the
//! request does not set comes from the frames of the same id. The join is
//! tested over a real connection, with the frames the agents send, because
//! the frames and the request are separate JSON-RPC messages and the order
//! between them is the point.

use std::sync::{Arc, Mutex};

use agent_client_protocol::schema::v1::{
    ContentBlock, PromptRequest, RequestPermissionOutcome, SessionId,
};
use agent_client_protocol::ByteStreams;
use crucible_core::types::CanonicalToolCall;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::RawAgent;
use crate::acp::client::{ClientConfig, CrucibleAcpClient, PermissionRequestHandler};

type Seen = Arc<Mutex<Vec<CanonicalToolCall>>>;

/// A client whose permission handler records the canonical call it was
/// asked about, then rejects. The recorded call is what the tool policy
/// keys on.
async fn recording_client() -> (CrucibleAcpClient, RawAgent, Seen) {
    let seen: Seen = Arc::default();
    let recorder = Arc::clone(&seen);
    let permission: PermissionRequestHandler = Arc::new(move |call, _options| {
        recorder
            .lock()
            .expect("the recorder is not poisoned")
            .push(call);
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

/// Run one turn in which the agent sends `frames`, waits for the answer to
/// its permission request, and ends the turn. Returns the canonical call
/// that the handler saw.
async fn asked_in_a_turn(frames: Vec<serde_json::Value>) -> CanonicalToolCall {
    let (client, mut agent, seen) = recording_client().await;
    let (out, _chunks) = tokio::sync::mpsc::unbounded_channel();
    let turn = client.prompt(
        PromptRequest::new(SessionId::from("sess-1"), vec![ContentBlock::from("hi")]),
        &out,
    );
    let agent_side = async {
        let prompt = agent.read().await;
        for frame in frames {
            agent.write(frame).await;
        }
        let reply = tokio::time::timeout(std::time::Duration::from_secs(5), agent.read())
            .await
            .expect("the client answers the permission request");
        assert!(reply.get("result").is_some(), "the client answers: {reply}");
        agent
            .write(serde_json::json!({
                "jsonrpc": "2.0",
                "id": prompt["id"],
                "result": { "stopReason": "end_turn" }
            }))
            .await;
    };
    let (result, ()) = tokio::join!(turn, agent_side);
    result.expect("the turn ends");
    let seen = seen.lock().expect("the recorder is not poisoned");
    seen.first().cloned().expect("the handler was asked")
}

/// An MCP approval that names no tool takes the tool from the `tool_call`
/// frame with the same id.
///
/// Without the join the request carries only `kind: "execute"`. Every MCP
/// call an agent asked about would then be decided as an unnamed call.
#[tokio::test]
async fn an_unnamed_approval_takes_the_tool_of_its_tool_call() {
    let call = asked_in_a_turn(vec![codex_tool_call(), codex_permission_request("call-1")]).await;
    assert_eq!(
        call.tool, "read_note",
        "the tool must come from the frame that announced the call"
    );
}

/// A permission request carries the agent profile that made the call, so
/// the prompt and a Lua hook can name the agent.
#[tokio::test]
async fn a_permission_request_names_its_agent() {
    let call = asked_in_a_turn(vec![codex_tool_call(), codex_permission_request("call-1")]).await;
    assert_eq!(
        call.agent.as_deref(),
        Some("raw"),
        "the call must name the agent profile of the connection"
    );
}

/// A different id joins nothing. The request keeps its kind as its name rather
/// than the tool of some other call.
#[tokio::test]
async fn an_approval_for_another_id_takes_no_name() {
    let call = asked_in_a_turn(vec![codex_tool_call(), codex_permission_request("call-2")]).await;
    assert_eq!(
        call.tool, "command",
        "a name must never be guessed from another call"
    );
}

/// An agent that names the tool in the request keeps that name. It is the
/// authority on what it is about to run.
#[tokio::test]
async fn a_named_request_keeps_its_own_name() {
    // A `tool_call` for the same id that says something else. The request
    // wins.
    let call = asked_in_a_turn(vec![
        codex_tool_call(),
        serde_json::json!({
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
        }),
    ])
    .await;
    assert_eq!(
        call.tool, "write_note",
        "the agent's own name must not be rewritten by an earlier frame"
    );
}

/// A request outside a turn has no frames to join. It is classified from
/// its own fields, and it is still answered.
#[tokio::test]
async fn a_request_outside_a_turn_is_classified_alone() {
    let (_client, mut agent, seen) = recording_client().await;
    agent.write(codex_permission_request("call-1")).await;
    let reply = tokio::time::timeout(std::time::Duration::from_secs(5), agent.read())
        .await
        .expect("the client answers the permission request");
    assert!(reply.get("result").is_some(), "the client answers: {reply}");
    let seen = seen.lock().expect("the recorder is not poisoned");
    assert_eq!(seen.first().map(|c| c.tool.as_str()), Some("command"));
}
