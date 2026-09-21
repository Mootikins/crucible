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

use crate::scripted_agent::{
    client_with_custom_transport, final_response, make_prompt_request, read_frame_within,
    read_request_id, write_json_line, AgentReader, AgentWriter, FRAME_WAIT,
};
use serde_json::json;

/// Drive one turn from the agent's side: consume the prompt, emit `frame`
/// mid-turn, capture whatever the client writes back (if anything), then end
/// the turn so the client's future resolves either way.
async fn turn_emitting(
    mut reader: AgentReader,
    mut writer: AgentWriter,
    frame: serde_json::Value,
) -> Option<serde_json::Value> {
    let prompt_request_id = read_request_id(&mut reader).await;

    write_json_line(&mut writer, frame).await;
    let reply = read_frame_within(&mut reader, FRAME_WAIT).await;

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
        .send_prompt_with_callback(
            make_prompt_request("ses-inbound", "read a file"),
            Box::new(|_| true),
        )
        .await
        .expect("turn should complete");

    assert_method_not_found(agent.await.expect("agent task"), json!(901));
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
        .send_prompt_with_callback(
            make_prompt_request("ses-inbound", "read a file"),
            Box::new(|_| true),
        )
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
        .send_prompt_with_callback(
            make_prompt_request("ses-inbound", "read a file"),
            Box::new(|_| true),
        )
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
        .send_prompt_with_callback(
            make_prompt_request("ses-inbound", "read a file"),
            Box::new(|_| true),
        )
        .await
        .expect("turn should complete");

    let reply = agent.await.expect("agent task");
    assert!(
        reply.is_none(),
        "a notification must not be answered, but the client wrote {reply:?}"
    );
}
