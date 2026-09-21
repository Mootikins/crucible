use std::path::PathBuf;

use super::{get_cat_command, get_simple_command};
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crate::acp::ClientError;

/// A client with no agent has nothing to write to. The error names the
/// missing writer rather than failing later on a read.
#[tokio::test]
async fn send_message_without_a_transport_is_a_connection_error() {
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: PathBuf::from("/nonexistent/agent"),
        ..Default::default()
    });

    let result = client
        .send_message(serde_json::json!({"method": "ping", "params": {}}))
        .await;

    match result {
        Err(ClientError::Connection(msg)) => assert!(
            msg.contains("No writer available"),
            "unexpected connection error: {msg}"
        ),
        other => panic!("expected a connection error, got {other:?}"),
    }
}

/// `send_request` over a real process pipe returns the reply that carries
/// the request's id. The agent is a shell that reads one frame and answers
/// it, so the outcome does not depend on how an echo is parsed.
#[cfg(unix)]
#[tokio::test]
async fn send_request_over_a_process_pipe_returns_the_correlated_reply() {
    use agent_client_protocol::schema::v1::{ClientRequest, InitializeRequest};

    let answer_one_frame = r#"read line
id=$(printf '%s' "$line" | grep -o '"id":[0-9]*' | head -n1 | cut -d: -f2)
printf '{"jsonrpc":"2.0","id":%s,"result":{"answered":"initialize"}}\n' "$id""#;
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: PathBuf::from("sh"),
        agent_args: Some(vec!["-c".to_string(), answer_one_frame.to_string()]),
        timeout_ms: Some(5000),
        ..Default::default()
    });
    client.spawn_agent().await.expect("sh spawns");

    let reply = client
        .send_request(ClientRequest::InitializeRequest(InitializeRequest::new(
            1u16.into(),
        )))
        .await
        .expect("the agent answers");

    assert_eq!(
        reply["result"],
        serde_json::json!({"answered": "initialize"})
    );
    assert!(reply["id"].is_u64(), "the reply keeps its id: {reply}");
}

/// A line the agent wrote comes back without its newline; the next read
/// after the agent exits reports the closed pipe, not an empty line.
#[tokio::test]
async fn read_response_line_returns_a_line_then_reports_the_closed_pipe() {
    let (cmd, args) = get_simple_command();
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(500),
        ..Default::default()
    });
    client.spawn_agent().await.expect("echo spawns");

    assert_eq!(
        client
            .read_response_line()
            .await
            .expect("echo writes a line"),
        "ok"
    );
    match client.read_response_line().await {
        Err(ClientError::Connection(msg)) => assert_eq!(msg, "Agent closed connection"),
        other => panic!("expected the closed pipe, got {other:?}"),
    }
}

#[tokio::test]
async fn test_write_agent_request() {
    let (cmd, args) = get_cat_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);

    // Spawn agent
    client.spawn_agent().await.unwrap();

    // Try to write a JSON-RPC message
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "test",
        "params": {}
    });

    let result = client.write_request(&request).await;

    // Should succeed - cat accepts stdin
    assert!(result.is_ok(), "Should successfully write to cat's stdin");
}

/// `send_message` writes one line and returns the next line parsed as JSON.
/// `cat` echoes the line, so the reply is the message itself.
#[tokio::test]
async fn send_message_returns_the_next_line_as_json() {
    let (cmd, args) = get_cat_command();
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(1000),
        ..Default::default()
    });
    client.spawn_agent().await.expect("cat spawns");

    let message = serde_json::json!({"test": "message", "value": 42});
    let reply = client
        .send_message(message.clone())
        .await
        .expect("cat echoes the line");

    assert_eq!(reply, message);
}

#[tokio::test(start_paused = true)]
async fn a_silent_transport_times_out_without_becoming_eof() {
    use std::time::Duration;
    use tokio::io::BufReader;

    for configured in [None, Some(100), Some(600_000)] {
        let (reader, _peer) = tokio::io::duplex(64);
        let mut client = CrucibleAcpClient::with_transport(
            ClientConfig {
                timeout_ms: configured,
                ..Default::default()
            },
            Box::pin(tokio::io::sink()),
            Box::pin(BufReader::new(reader)),
        );
        let start = tokio::time::Instant::now();
        assert!(matches!(
            client.read_response_line().await,
            Err(ClientError::Timeout(_))
        ));
        assert_eq!(
            start.elapsed(),
            Duration::from_millis(configured.unwrap_or(300_000).max(300_000))
        );
    }
}
