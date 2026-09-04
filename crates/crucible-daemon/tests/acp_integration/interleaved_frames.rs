//! Frames that arrive while a request/response call is in flight.
//!
//! `send_request` writes one frame and reads one line back. That holds only
//! if the agent answers with nothing in between, which the mocks in this
//! suite all do. Real agents do not: codex-acp emits a `session/update`
//! reporting MCP server startup *before* it answers `session/new`, so the
//! very first line the client reads is a notification and the session never
//! opens. The symptom is "Missing result field in new session response",
//! which names the frame it got rather than the frame it skipped.
//!
//! Every request/response call shares this path — `initialize`,
//! `session/new`, `session/resume`, `session/close`, `session/set_mode`,
//! `session/set_config_option` — so the rule belongs to the reader, not to
//! any one caller:
//!
//! * a response carrying our `id` is the answer,
//! * a notification (no `id`) is skipped,
//! * an inbound *request* is answered `-32601`, then skipped — dropping it
//!   hangs the agent, which is what `inbound_requests.rs` establishes for
//!   the streaming path,
//! * a response carrying a foreign `id` is a straggler and is skipped.
//!
//! These tests script the agent side by hand rather than using a mock
//! profile, because the point is the frame ordering no mock produces.

use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

type AgentReader = BufReader<ReadHalf<DuplexStream>>;
type AgentWriter = WriteHalf<DuplexStream>;

/// How long the scripted agent waits for a client frame before giving up.
/// Short, so a client that never writes fails as an assertion rather than as
/// the suite's own timeout.
const FRAME_WAIT: Duration = Duration::from_secs(2);

fn client_with_custom_transport() -> (CrucibleAcpClient, AgentReader, AgentWriter) {
    let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(65_536);
    let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(65_536);

    let (_unused_a, client_write) = tokio::io::split(client_to_agent_client);
    let (agent_read, _unused_b) = tokio::io::split(client_to_agent_agent);
    let (_unused_c, agent_write) = tokio::io::split(agent_to_client_agent);
    let (client_read, _unused_d) = tokio::io::split(agent_to_client_client);

    let config = ClientConfig {
        agent_path: PathBuf::from("mock-interleaving-agent"),
        agent_args: None,
        timeout_ms: Some(5_000),
        ..Default::default()
    };

    let client = CrucibleAcpClient::with_transport(
        config,
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );

    (client, BufReader::new(agent_read), agent_write)
}

async fn read_frame(reader: &mut AgentReader) -> serde_json::Value {
    let mut line = String::new();
    tokio::time::timeout(FRAME_WAIT, reader.read_line(&mut line))
        .await
        .expect("client wrote no frame before the deadline")
        .expect("read the client's frame");
    serde_json::from_str(&line).expect("the client's frame is JSON")
}

async fn write_frame(writer: &mut AgentWriter, frame: serde_json::Value) {
    let line = format!("{frame}\n");
    writer
        .write_all(line.as_bytes())
        .await
        .expect("write a frame to the client");
    writer.flush().await.expect("flush the frame");
}

/// The notification codex-acp sends while starting an MCP server.
fn mcp_startup_notification() -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "sess-interleaved",
            "update": {
                "sessionUpdate": "tool_call",
                "toolCallId": "mcp_startup.crucible",
                "kind": "other",
                "title": "mcp__crucible__startup",
                "status": "failed",
                "content": [{
                    "type": "content",
                    "content": {"type": "text", "text": "MCP server `crucible` failed to start"}
                }]
            }
        }
    })
}

fn new_session_request(cwd: &str) -> agent_client_protocol::schema::v1::NewSessionRequest {
    serde_json::from_value(json!({
        "cwd": cwd,
        "mcpServers": [],
        "_meta": null
    }))
    .expect("valid new session request")
}

fn initialize_result() -> serde_json::Value {
    json!({
        "protocolVersion": 1,
        "agentCapabilities": {"loadSession": true},
        "authMethods": []
    })
}

#[tokio::test]
async fn session_new_skips_a_notification_that_precedes_its_response() {
    let (mut client, mut agent_read, mut agent_write) = client_with_custom_transport();

    let agent = tokio::spawn(async move {
        let request = read_frame(&mut agent_read).await;
        assert_eq!(request["method"], "session/new");
        let id = request["id"].clone();

        // The frame that breaks a one-line read: a notification first.
        write_frame(&mut agent_write, mcp_startup_notification()).await;
        write_frame(
            &mut agent_write,
            json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "sess-interleaved"}}),
        )
        .await;

        agent_write
    });

    let session = client
        .create_new_session(new_session_request("/tmp"))
        .await
        .expect("session/new must survive a notification arriving before its response");

    assert_eq!(
        session.session_id.0.as_ref(),
        "sess-interleaved",
        "the session id must come from the response, not from the notification"
    );

    let _ = agent.await.expect("the scripted agent finished");
}

#[tokio::test]
async fn initialize_skips_a_notification_that_precedes_its_response() {
    let (mut client, mut agent_read, mut agent_write) = client_with_custom_transport();

    let agent = tokio::spawn(async move {
        let request = read_frame(&mut agent_read).await;
        assert_eq!(request["method"], "initialize");
        let id = request["id"].clone();

        write_frame(&mut agent_write, mcp_startup_notification()).await;
        write_frame(
            &mut agent_write,
            json!({"jsonrpc": "2.0", "id": id, "result": initialize_result()}),
        )
        .await;

        agent_write
    });

    client
        .initialize(agent_client_protocol::schema::v1::InitializeRequest::new(
            1.into(),
        ))
        .await
        .expect("initialize must survive a notification arriving before its response");

    let _ = agent.await.expect("the scripted agent finished");
}

#[tokio::test]
async fn a_response_for_another_id_does_not_satisfy_the_pending_request() {
    let (mut client, mut agent_read, mut agent_write) = client_with_custom_transport();

    let agent = tokio::spawn(async move {
        let request = read_frame(&mut agent_read).await;
        let id = request["id"].as_u64().expect("the client's id is a number");

        // A straggler from an earlier exchange. Answering the pending call
        // with it would hand the caller another request's payload.
        write_frame(
            &mut agent_write,
            json!({"jsonrpc": "2.0", "id": id + 4_096, "result": {"sessionId": "wrong-session"}}),
        )
        .await;
        write_frame(
            &mut agent_write,
            json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "right-session"}}),
        )
        .await;

        agent_write
    });

    let session = client
        .create_new_session(new_session_request("/tmp"))
        .await
        .expect("session/new must skip a response carrying a foreign id");

    assert_eq!(
        session.session_id.0.as_ref(),
        "right-session",
        "the client answered the call with another exchange's response"
    );

    let _ = agent.await.expect("the scripted agent finished");
}

#[tokio::test]
async fn an_inbound_request_during_a_call_is_answered_and_the_response_still_lands() {
    let (mut client, mut agent_read, mut agent_write) = client_with_custom_transport();

    let agent = tokio::spawn(async move {
        let request = read_frame(&mut agent_read).await;
        assert_eq!(request["method"], "session/new");
        let id = request["id"].clone();

        // Crucible advertises no filesystem capability, but an agent may ask
        // anyway. Skipping this frame silently leaves the agent waiting on a
        // reply that never comes.
        write_frame(
            &mut agent_write,
            json!({
                "jsonrpc": "2.0",
                "id": "fs-req-1",
                "method": "fs/read_text_file",
                "params": {"sessionId": "sess-interleaved", "path": "/etc/hostname"}
            }),
        )
        .await;
        write_frame(
            &mut agent_write,
            json!({"jsonrpc": "2.0", "id": id, "result": {"sessionId": "sess-interleaved"}}),
        )
        .await;

        // The client must answer the inbound request before it returns.
        let reply = read_frame(&mut agent_read).await;
        (reply, agent_write)
    });

    let session = client
        .create_new_session(new_session_request("/tmp"))
        .await
        .expect("session/new must survive an inbound request arriving before its response");

    assert_eq!(session.session_id.0.as_ref(), "sess-interleaved");

    let (reply, _writer) = agent.await.expect("the scripted agent finished");

    assert_eq!(
        reply["id"], "fs-req-1",
        "the reply must echo the agent's own request id, unchanged"
    );
    assert_eq!(
        reply["error"]["code"], -32601,
        "an unhandled inbound request is answered method-not-found"
    );
    assert!(
        reply.get("result").is_none(),
        "a JSON-RPC error reply carries no result"
    );
}
