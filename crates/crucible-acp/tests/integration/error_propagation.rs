use crate::support::ThreadedMockAgent;
use agent_client_protocol::{InitializeRequest, PromptRequest};
use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_acp::protocol::ProtocolVersion;
use crucible_acp::{ClientError, MessageHandler};
use crucible_core::traits::acp::SessionManager;
use crucible_core::types::acp::{SessionConfig, SessionId};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

fn test_config(timeout_ms: Option<u64>) -> ClientConfig {
    ClientConfig {
        agent_path: PathBuf::from("mock-threaded-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms,
        max_retries: Some(1),
    }
}

fn make_prompt_request(session_id: &str) -> PromptRequest {
    serde_json::from_value(serde_json::json!({
        "sessionId": session_id,
        "prompt": [{"type": "text", "text": "trigger streaming"}],
        "_meta": null
    }))
    .expect("valid prompt request")
}

fn client_with_custom_transport(
    timeout_ms: Option<u64>,
) -> (
    CrucibleAcpClient,
    BufReader<tokio::io::ReadHalf<DuplexStream>>,
    tokio::io::WriteHalf<DuplexStream>,
) {
    let (client_to_agent_client, client_to_agent_agent) = tokio::io::duplex(8192);
    let (agent_to_client_agent, agent_to_client_client) = tokio::io::duplex(8192);

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

fn protocol_guard(agent_version: ProtocolVersion, local_version: ProtocolVersion) -> Result<(), ClientError> {
    if agent_version.is_compatible_with(&local_version) {
        Ok(())
    } else {
        Err(ClientError::Protocol(agent_client_protocol::Error::new(
            -32600,
            format!(
                "protocol mismatch: agent={}, local={}",
                agent_version, local_version
            ),
        )))
    }
}

#[test]
fn test_error_client_error_variants_are_constructible() {
    let variants = vec![
        ClientError::Protocol(agent_client_protocol::Error::internal_error()),
        ClientError::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "io")),
        ClientError::Serialization(serde_json::Error::io(std::io::Error::other("serde"))),
        ClientError::Session("session".to_string()),
        ClientError::Connection("connection".to_string()),
        ClientError::Timeout("timeout".to_string()),
        ClientError::InvalidConfig("invalid config".to_string()),
        ClientError::FileSystem("filesystem".to_string()),
        ClientError::PermissionDenied("denied".to_string()),
        ClientError::NotFound("missing".to_string()),
        ClientError::Validation("validation".to_string()),
        ClientError::Other(anyhow::anyhow!("other")),
    ];

    assert_eq!(variants.len(), 12);

    for variant in variants {
        match variant {
            ClientError::Protocol(_)
            | ClientError::Io(_)
            | ClientError::Serialization(_)
            | ClientError::Session(_)
            | ClientError::Connection(_)
            | ClientError::Timeout(_)
            | ClientError::InvalidConfig(_)
            | ClientError::FileSystem(_)
            | ClientError::PermissionDenied(_)
            | ClientError::NotFound(_)
            | ClientError::Validation(_)
            | ClientError::Other(_) => {}
        }
    }
}

#[tokio::test]
async fn test_error_malformed_json_response_returns_serialization_error() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(100));

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;
        let _ = agent_writer.write_all(b"{ this is not valid json }\n").await;
        let _ = agent_writer.flush().await;
    });

    let result = client.initialize(InitializeRequest::new(1u16.into())).await;

    assert!(
        matches!(result, Err(ClientError::Serialization(_))),
        "expected Serialization error, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_error_invalid_json_rpc_shape_returns_session_error() {
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(100));

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;
        let _ = agent_writer
            .write_all(br#"{"jsonrpc":"2.0","id":1}"#)
            .await;
        let _ = agent_writer.write_all(b"\n").await;
        let _ = agent_writer.flush().await;
    });

    let result = client.initialize(InitializeRequest::new(1u16.into())).await;

    assert!(
        matches!(result, Err(ClientError::Session(ref message)) if message.contains("Missing result field")),
        "expected Session error for invalid JSON-RPC shape, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_error_connection_timeout_when_agent_never_responds() {
    tokio::time::pause();

    let (mut client, mut agent_reader, _agent_writer) = client_with_custom_transport(Some(1));

    tokio::spawn(async move {
        let mut request_line = String::new();
        let _ = agent_reader.read_line(&mut request_line).await;
        std::future::pending::<()>().await;
    });

    let connect_task = tokio::spawn(async move { client.connect_with_handshake().await });

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

    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(50));

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
        client
            .send_prompt_with_streaming(make_prompt_request("ses-timeout"))
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
    let (mut client, mut agent_reader, mut agent_writer) = client_with_custom_transport(Some(100));

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

    let result = client
        .send_prompt_with_streaming(make_prompt_request("ses-crash"))
        .await;

    assert!(
        matches!(result, Err(ClientError::Connection(ref message)) if message.contains("closed connection")),
        "expected Connection error after crash, got: {:?}",
        result
    );
}

#[tokio::test]
async fn test_error_stream_abort_from_threaded_mock_agent_is_recoverable() {
    let config = crate::support::MockStdioAgentConfig::opencode();
    let (mut client, handle) = ThreadedMockAgent::spawn_with_client(config);

    let _ = client
        .connect_with_handshake()
        .await
        .expect("handshake should succeed before abort");

    handle.abort();

    let result = client
        .send_prompt_with_streaming(make_prompt_request("ses-abort"))
        .await;

    assert!(result.is_err(), "aborted agent should return an error, not panic");
}

#[tokio::test]
async fn test_error_session_double_close_is_graceful() {
    let mut client = CrucibleAcpClient::new(test_config(None));
    let session_id = client
        .create_session(SessionConfig::new(std::env::temp_dir()))
        .await
        .expect("session creation should succeed");

    let first = client.end_session(session_id.clone()).await;
    let second = client.end_session(session_id).await;

    assert!(first.is_ok(), "first close should succeed");
    assert!(second.is_ok(), "second close should be idempotent");
}

#[tokio::test]
async fn test_error_session_already_ended_is_graceful() {
    let mut client = CrucibleAcpClient::new(test_config(None));
    let ended_session = SessionId::new();
    let result = client.end_session(ended_session).await;

    assert!(result.is_ok(), "ending non-active session should be safe");
}

#[test]
fn test_error_protocol_version_mismatch_is_reported() {
    let local = MessageHandler::default().version().clone();
    let incompatible = ProtocolVersion::new(local.major + 1, 0, 0);

    let result = protocol_guard(incompatible, local);

    assert!(
        matches!(result, Err(ClientError::Protocol(_))),
        "expected Protocol error for version mismatch"
    );
}
