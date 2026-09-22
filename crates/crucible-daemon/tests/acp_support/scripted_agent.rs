//! A hand-scripted ACP agent on the far end of an in-memory transport.
//!
//! The mocks in this directory answer a fixed protocol. The tests that use
//! this module instead play the agent frame by frame, because the behavior
//! under test is a frame ordering or a reply shape that no mock profile
//! produces. Each test gets a `CrucibleAcpClient` wired to a duplex pipe, and
//! the reader and writer for the agent's side of that pipe.
//!
//! Every read on the agent's side carries a deadline. A client that never
//! writes then fails as an assertion that names the missing frame, not as a
//! hang that the harness kills.

use agent_client_protocol::schema::v1::PromptRequest;
use agent_client_protocol::ByteStreams;
use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient, PermissionRequestHandler};
use crucible_daemon::acp::{ClientError, StreamingChunk, TurnSummary};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// The agent's side of the pipe that carries the client's frames.
pub type AgentReader = BufReader<ReadHalf<DuplexStream>>;
/// The agent's side of the pipe that carries frames to the client.
pub type AgentWriter = WriteHalf<DuplexStream>;

/// How long the scripted agent waits for a client frame. It is shorter than
/// the harness timeout, so a missing frame reports as a failed read.
pub const FRAME_WAIT: Duration = Duration::from_secs(2);

/// The pipe capacity. It is large enough that no scripted frame blocks the
/// writer before the client reads it.
const PIPE_CAPACITY: usize = 65_536;

/// A client config for a custom transport. The agent path is a label only:
/// `connect` spawns no process.
pub fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-scripted-agent"),
        agent_args: None,
        timeout_ms,
        ..Default::default()
    }
}

/// A client wired to an in-memory pipe, and the agent's two ends of it.
pub async fn client_with_custom_transport(
    timeout_ms: Option<u64>,
) -> (CrucibleAcpClient, AgentReader, AgentWriter) {
    client_with_permission(timeout_ms, None).await
}

/// [`client_with_custom_transport`] with a permission handler.
pub async fn client_with_permission(
    timeout_ms: Option<u64>,
    permission: Option<PermissionRequestHandler>,
) -> (CrucibleAcpClient, AgentReader, AgentWriter) {
    let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(PIPE_CAPACITY);
    let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(PIPE_CAPACITY);

    let (_client_read_unused, client_write) = tokio::io::split(client_to_agent_client);
    let (agent_read, _agent_write_unused) = tokio::io::split(client_to_agent_agent);
    let (_agent_read_unused, agent_write) = tokio::io::split(agent_to_client_agent);
    let (client_read, _client_write_unused) = tokio::io::split(agent_to_client_client);

    let client = CrucibleAcpClient::connect(
        test_config(timeout_ms),
        ByteStreams::new(client_write.compat_write(), client_read.compat()),
        "mock-scripted-agent",
        permission,
    )
    .await
    .expect("the client connects");

    (client, BufReader::new(agent_read), agent_write)
}

/// Read the next frame the client writes, or `None` when the client writes
/// nothing within `wait` or closes the pipe.
pub async fn read_frame_within(reader: &mut AgentReader, wait: Duration) -> Option<Value> {
    let mut line = String::new();
    match tokio::time::timeout(wait, reader.read_line(&mut line)).await {
        Ok(Ok(0)) | Err(_) => None,
        Ok(Ok(_)) => {
            Some(serde_json::from_str(&line).unwrap_or_else(|e| {
                panic!("the client wrote a frame that is not JSON ({e}): {line}")
            }))
        }
        Ok(Err(e)) => panic!("read from the client failed: {e}"),
    }
}

/// Read the next frame the client writes. Panics when no frame arrives
/// within [`FRAME_WAIT`].
pub async fn read_frame(reader: &mut AgentReader) -> Value {
    read_frame_within(reader, FRAME_WAIT)
        .await
        .expect("the client wrote no frame before the deadline")
}

/// Read the client's next request and return its id.
pub async fn read_request_id(reader: &mut AgentReader) -> Value {
    let request = read_frame(reader).await;
    request
        .get("id")
        .cloned()
        .unwrap_or_else(|| panic!("the client's request carries no id: {request}"))
}

/// Write one frame to the client as a single JSON line.
pub async fn write_json_line(writer: &mut AgentWriter, value: Value) {
    writer
        .write_all(format!("{value}\n").as_bytes())
        .await
        .expect("write a frame to the client");
    writer.flush().await.expect("flush the frame to the client");
}

/// A one-text-block prompt for `session_id`.
pub fn make_prompt_request(session_id: &str, text: &str) -> PromptRequest {
    serde_json::from_value(json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": text}],
        "_meta": null
    }))
    .expect("valid prompt request")
}

/// The `session/prompt` reply that ends a turn.
pub fn final_response(request_id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {"stopReason": "end_turn", "_meta": null}
    })
}

/// Wrap one `sessionUpdate` payload in a `session/update` notification.
pub fn session_update(session_id: &str, update: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": session_id,
            "update": update
        }
    })
}

/// An `agent_message_chunk` that carries `text`.
pub fn text_chunk(session_id: &str, text: &str) -> Value {
    session_update(
        session_id,
        json!({
            "sessionUpdate": "agent_message_chunk",
            "content": {"type": "text", "text": text}
        }),
    )
}

/// A `tool_call` announcement in progress, with optional raw input.
pub fn tool_call_notification(
    session_id: &str,
    tool_call_id: &str,
    title: &str,
    raw_input: Option<Value>,
) -> Value {
    let mut update = json!({
        "sessionUpdate": "tool_call",
        "toolCallId": tool_call_id,
        "title": title,
        "status": "in_progress"
    });
    if let Some(input) = raw_input {
        update["rawInput"] = input;
    }
    session_update(session_id, update)
}

/// A `tool_call_update` that ends a call with `status`, with optional raw
/// output.
pub fn tool_call_update(
    session_id: &str,
    tool_call_id: &str,
    status: &str,
    raw_output: Option<Value>,
) -> Value {
    let mut update = json!({
        "sessionUpdate": "tool_call_update",
        "toolCallId": tool_call_id,
        "status": status
    });
    if let Some(output) = raw_output {
        update["rawOutput"] = output;
    }
    session_update(session_id, update)
}

/// A `tool_call_update` that completes a call.
pub fn tool_call_update_completed(
    session_id: &str,
    tool_call_id: &str,
    raw_output: Option<Value>,
) -> Value {
    tool_call_update(session_id, tool_call_id, "completed", raw_output)
}

/// Open an MCP session on a Streamable HTTP endpoint the way an agent does:
/// `initialize`, then `notifications/initialized`. Returns the session id the
/// server assigned.
pub async fn mcp_http_open_session(http: &reqwest::Client, url: &str) -> String {
    let init = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "scripted-agent", "version": "0.1.0"}
                }
            })
            .to_string(),
        )
        .send()
        .await
        .expect("MCP initialize reaches the host");
    assert!(
        init.status().is_success(),
        "MCP initialize failed: {}",
        init.status()
    );
    let session_id = init
        .headers()
        .get("mcp-session-id")
        .expect("the MCP host assigns a session id")
        .to_str()
        .expect("the session id is ASCII")
        .to_string();

    let initialized = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string())
        .send()
        .await
        .expect("MCP initialized notification reaches the host");
    assert!(
        initialized.status().is_success(),
        "MCP initialized notification failed: {}",
        initialized.status()
    );

    session_id
}

/// Send one JSON-RPC request on an open MCP session and return the reply.
/// The host may answer as plain JSON or as one SSE `data:` event.
pub async fn mcp_http_request(
    http: &reqwest::Client,
    url: &str,
    session_id: &str,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let response = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .body(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string())
        .send()
        .await
        .unwrap_or_else(|e| panic!("MCP {method} reaches the host: {e}"));
    assert!(
        response.status().is_success(),
        "MCP {method} failed: {}",
        response.status()
    );
    let body = response.text().await.expect("read the MCP reply body");
    let json_text = body
        .lines()
        .find_map(|line| line.strip_prefix("data: ").filter(|d| d.starts_with('{')))
        .unwrap_or(&body);
    serde_json::from_str(json_text)
        .unwrap_or_else(|e| panic!("MCP {method} reply is not JSON ({e}): {body}"))
}

/// Run one turn on `client`, and call `on_chunk` for each chunk. A `false`
/// from `on_chunk` drops the turn, the way the daemon cancels one.
pub async fn prompt_with(
    client: &CrucibleAcpClient,
    request: PromptRequest,
    mut on_chunk: impl FnMut(StreamingChunk) -> bool,
) -> Result<
    (
        TurnSummary,
        agent_client_protocol::schema::v1::PromptResponse,
    ),
    ClientError,
> {
    let (out, mut chunks) = tokio::sync::mpsc::unbounded_channel();
    let turn = client.prompt(request, &out);
    tokio::pin!(turn);
    let mut open = true;
    loop {
        tokio::select! {
            result = &mut turn => {
                while let Ok(chunk) = chunks.try_recv() {
                    if open {
                        on_chunk(chunk);
                    }
                }
                return result;
            }
            Some(chunk) = chunks.recv(), if open => {
                if !on_chunk(chunk) {
                    open = false;
                    chunks.close();
                }
            }
        }
    }
}
