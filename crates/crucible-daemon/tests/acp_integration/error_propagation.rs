use crate::support::mock_agent::make_prompt_request;
use crate::support::{connect, prompt_with, MockScript, Step};
use crucible_daemon::acp::client::ClientConfig;
use crucible_daemon::acp::{ClientError, CrucibleAcpClient};
use std::time::Duration;

#[tokio::test]
async fn test_error_connection_timeout_when_agent_never_responds() {
    tokio::time::pause();

    // Every mock agent answers `initialize`. An agent that never answers is a
    // pipe that nobody reads, so this test uses a bare pipe.
    let (client_end, _agent_end) = tokio::io::duplex(64 * 1024);
    let (client_read, client_write) = tokio::io::split(client_end);
    let mut client = {
        use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
        CrucibleAcpClient::connect(
            ClientConfig {
                timeout_ms: Some(1),
                ..Default::default()
            },
            agent_client_protocol::ByteStreams::new(
                client_write.compat_write(),
                client_read.compat(),
            ),
            "silent-agent",
            None,
        )
        .await
        .expect("the client connects")
    };

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

    // The agent sends one chunk. Then it holds the turn open, also after a
    // cancel.
    let script = MockScript {
        turn: vec![
            Step::Text("partial".into()),
            Step::Hold {
                tick_ms: None,
                ignore_cancel: true,
            },
        ],
        ..MockScript::default()
    };
    let (client, _agent) = connect(script, Some(50), None).await;

    let stream_task = tokio::spawn(async move {
        prompt_with(
            &client,
            make_prompt_request("ses-timeout", "trigger streaming"),
            |_| true,
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
    let script = MockScript {
        turn: vec![Step::Text("partial".into()), Step::Exit],
        ..MockScript::default()
    };
    let (client, _agent) = connect(script, Some(100), None).await;

    let result = prompt_with(
        &client,
        make_prompt_request("ses-crash", "trigger streaming"),
        |_| true,
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
async fn test_error_stream_abort_from_mock_agent_is_a_connection_error() {
    let (mut client, agent) = connect(MockScript::default(), None, None).await;

    let _ = client
        .handshake(None, None)
        .await
        .expect("handshake should succeed before abort");

    agent.abort();

    let result = tokio::time::timeout(
        Duration::from_secs(5),
        prompt_with(
            &client,
            make_prompt_request("ses-abort", "trigger streaming"),
            |_| true,
        ),
    )
    .await
    .expect("a dead agent must fail the turn, not stall it");

    assert!(
        matches!(result, Err(ClientError::Connection(_))),
        "an aborted agent must read as a lost connection, got: {result:?}"
    );
}
