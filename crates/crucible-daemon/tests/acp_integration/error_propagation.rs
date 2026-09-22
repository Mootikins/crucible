use crate::scripted_agent::prompt_with;
use crate::scripted_agent::{client_with_custom_transport, make_prompt_request};
use crate::support::ThreadedMockAgent;
use crucible_daemon::acp::ClientError;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

#[tokio::test]
async fn test_error_connection_timeout_when_agent_never_responds() {
    tokio::time::pause();

    let (mut client, mut agent_reader, _agent_writer) = client_with_custom_transport(Some(1)).await;

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;
        std::future::pending::<()>().await;
    });

    let connect_task = tokio::spawn(async move { client.handshake(None, None).await });

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(301)).await;

    let result = connect_task.await.expect("join should succeed");

    assert!(
        matches!(result, Err(ClientError::Timeout(ref message)) if message.contains("timed out")),
        "expected Timeout error, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_error_streaming_timeout_when_agent_stalls_after_first_chunk() {
    tokio::time::pause();

    let (mut client, mut agent_reader, mut agent_writer) =
        client_with_custom_transport(Some(50)).await;

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;

        let update = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "ses-timeout",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": "partial"}
                }
            }
        });

        let _ = agent_writer
            .write_all(format!("{}\n", update).as_bytes())
            .await;
        let _ = agent_writer.flush().await;

        std::future::pending::<()>().await;
    });

    let stream_task = tokio::spawn(async move {
        prompt_with(
            &client,
            make_prompt_request("ses-timeout", "trigger streaming"),
            Box::new(|_| true),
        )
        .await
    });

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(1)).await;

    let result = stream_task.await.expect("join should succeed");

    assert!(
        matches!(result, Err(ClientError::Timeout(ref message)) if message.contains("Streaming operation timed out")),
        "expected streaming Timeout error, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_error_agent_crash_mid_stream_returns_connection_error() {
    let (mut client, mut agent_reader, mut agent_writer) =
        client_with_custom_transport(Some(100)).await;

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;

        let update = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "ses-crash",
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": "partial"}
                }
            }
        });

        let _ = agent_writer
            .write_all(format!("{}\n", update).as_bytes())
            .await;
        let _ = agent_writer.flush().await;
        drop(agent_writer);
    });

    let result = prompt_with(
        &client,
        make_prompt_request("ses-crash", "trigger streaming"),
        Box::new(|_| true),
    )
    .await;

    assert!(
        matches!(result, Err(ClientError::Connection(ref message)) if message.contains("closed the connection")),
        "expected Connection error after crash, got: {:?}",
        result
    );
}

/// An agent that dies after the handshake must surface as a lost connection,
/// and promptly: a turn that waits out its timeout instead reports the wrong
/// cause and holds the session for the whole timeout.
#[tokio::test]
async fn test_error_stream_abort_from_threaded_mock_agent_is_a_connection_error() {
    let config = crate::support::MockStdioAgentConfig::opencode();
    let (mut client, handle) = ThreadedMockAgent::spawn_with_client(config).await;

    let _ = client
        .handshake(None, None)
        .await
        .expect("handshake should succeed before abort");

    handle.abort();

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        prompt_with(
            &client,
            make_prompt_request("ses-abort", "trigger streaming"),
            Box::new(|_| true),
        ),
    )
    .await
    .expect("a dead agent must fail the turn, not stall it");

    assert!(
        matches!(result, Err(ClientError::Connection(_))),
        "an aborted agent must read as a lost connection, got: {result:?}"
    );
}
