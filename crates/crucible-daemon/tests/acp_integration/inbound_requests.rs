//! Inbound JSON-RPC request handling — verifies that a *request* the client has
//! no handler for still gets an answer.
//!
//! The ACP client dispatches inbound frames on `method` and handles exactly
//! `session/update` and `session/request_permission`. Everything else used to
//! fall through to a debug log regardless of whether the frame carried an `id`.
//! A frame with an `id` is a request: the agent blocks until it gets a response,
//! so dropping it hangs the turn until the read timeout fires rather than
//! failing fast. `fs/read_text_file` is the live example — Crucible advertises
//! no filesystem capability, but an agent that asks anyway must be told the
//! method is not there.
//!
//! Frames *without* an `id` are notifications and must stay silent.

use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

type AgentReader = BufReader<ReadHalf<DuplexStream>>;
type AgentWriter = WriteHalf<DuplexStream>;

/// How long the mock agent waits for the client to answer its request before
/// concluding no answer is coming. Well under the client's own overall timeout
/// so a missing reply reports as a failed assertion, not a timed-out turn.
const REPLY_WAIT: Duration = Duration::from_secs(2);

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-inbound-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms,
        max_retries: Some(1),
    }
}

fn client_with_custom_transport(
    timeout_ms: Option<u64>,
) -> (CrucibleAcpClient, AgentReader, AgentWriter) {
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

async fn write_json_line(writer: &mut AgentWriter, value: serde_json::Value) {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&value).unwrap()).as_bytes())
        .await
        .expect("write to client");
    writer.flush().await.expect("flush to client");
}

async fn read_json_line(reader: &mut AgentReader) -> serde_json::Value {
    let mut line = String::new();
    reader.read_line(&mut line).await.expect("read from client");
    serde_json::from_str(&line).expect("client wrote valid JSON")
}

/// Read the next frame the client writes, or `None` if it writes nothing within
/// [`REPLY_WAIT`].
async fn read_json_line_within(reader: &mut AgentReader) -> Option<serde_json::Value> {
    let mut line = String::new();
    match tokio::time::timeout(REPLY_WAIT, reader.read_line(&mut line)).await {
        Ok(Ok(0)) | Err(_) => None,
        Ok(Ok(_)) => Some(serde_json::from_str(&line).expect("client wrote valid JSON")),
        Ok(Err(e)) => panic!("read from client failed: {e}"),
    }
}

fn final_response(request_id: u64) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": {"stopReason": "end_turn", "_meta": null}
    })
}

/// Drive one turn from the agent's side: consume the prompt, emit `frame`
/// mid-turn, capture whatever the client writes back (if anything), then end
/// the turn so the client's future resolves either way.
async fn turn_emitting(
    mut reader: AgentReader,
    mut writer: AgentWriter,
    frame: serde_json::Value,
) -> Option<serde_json::Value> {
    let prompt_request = read_json_line(&mut reader).await;
    let prompt_request_id = prompt_request["id"]
        .as_u64()
        .expect("prompt request carries a numeric id");

    write_json_line(&mut writer, frame).await;
    let reply = read_json_line_within(&mut reader).await;

    write_json_line(&mut writer, final_response(prompt_request_id)).await;
    reply
}

fn unhandled_request(request_id: serde_json::Value) -> serde_json::Value {
    json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "fs/read_text_file",
        "params": {
            "sessionId": "ses-inbound",
            "path": "/etc/hosts"
        }
    })
}

fn assert_method_not_found(reply: Option<serde_json::Value>, expected_id: serde_json::Value) {
    let reply = reply.expect("client should answer an inbound request it cannot handle");

    assert_eq!(reply["jsonrpc"], "2.0");
    // JSON-RPC 2.0 requires the response id to equal the request id — same
    // value *and* same type. An agent keyed on `"req-7"` does not recognise a
    // reply addressed to `7`.
    assert_eq!(
        reply["id"], expected_id,
        "reply must carry the request's id unchanged, got {reply}"
    );
    assert_eq!(
        reply["error"]["code"].as_i64(),
        Some(-32601),
        "unhandled method must be answered with JSON-RPC method-not-found, got {reply}"
    );
    assert!(
        reply.get("result").is_none(),
        "an error reply must not also carry a result: {reply}"
    );
    assert!(
        reply["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("fs/read_text_file")),
        "the error message should name the method that was refused, got {reply}"
    );
}

#[tokio::test]
async fn an_unhandled_inbound_request_gets_a_method_not_found_reply() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(500));

    let agent = tokio::spawn(turn_emitting(reader, writer, unhandled_request(json!(901))));

    client
        .send_prompt_with_streaming(make_prompt_request("ses-inbound", "read a file"))
        .await
        .expect("turn should complete");

    assert_method_not_found(agent.await.expect("agent task"), json!(901));
}

#[tokio::test]
async fn an_unhandled_inbound_request_is_answered_on_the_callback_path_too() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(500));

    let agent = tokio::spawn(turn_emitting(reader, writer, unhandled_request(json!(902))));

    client
        .send_prompt_with_callback(
            make_prompt_request("ses-inbound", "read a file"),
            Box::new(|_| true),
        )
        .await
        .expect("turn should complete");

    assert_method_not_found(agent.await.expect("agent task"), json!(902));
}

/// JSON-RPC ids are strings, numbers or null — not just `u64`. A non-numeric
/// string id used to parse to `None`, which read as "this is a notification"
/// and dropped the request, reintroducing the very hang this module exists to
/// prevent.
#[tokio::test]
async fn an_unhandled_request_with_a_string_id_is_answered_with_that_string_id() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(500));

    let agent = tokio::spawn(turn_emitting(
        reader,
        writer,
        unhandled_request(json!("req-7")),
    ));

    client
        .send_prompt_with_streaming(make_prompt_request("ses-inbound", "read a file"))
        .await
        .expect("turn should complete");

    assert_method_not_found(agent.await.expect("agent task"), json!("req-7"));
}

/// Negative ids are legal JSON-RPC and `as_u64()` rejects them.
#[tokio::test]
async fn an_unhandled_request_with_a_negative_id_is_answered_with_that_id() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(500));

    let agent = tokio::spawn(turn_emitting(reader, writer, unhandled_request(json!(-3))));

    client
        .send_prompt_with_streaming(make_prompt_request("ses-inbound", "read a file"))
        .await
        .expect("turn should complete");

    assert_method_not_found(agent.await.expect("agent task"), json!(-3));
}

#[tokio::test]
async fn an_unhandled_inbound_notification_gets_no_reply() {
    let (mut client, reader, writer) = client_with_custom_transport(Some(500));

    // No `id` — a notification. The agent is not waiting for anything, and
    // answering would put an unsolicited frame on the wire.
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "fs/read_text_file",
        "params": {"sessionId": "ses-inbound", "path": "/etc/hosts"}
    });

    let agent = tokio::spawn(turn_emitting(reader, writer, notification));

    client
        .send_prompt_with_streaming(make_prompt_request("ses-inbound", "read a file"))
        .await
        .expect("turn should complete");

    let reply = agent.await.expect("agent task");
    assert!(
        reply.is_none(),
        "a notification must not be answered, but the client wrote {reply:?}"
    );
}
