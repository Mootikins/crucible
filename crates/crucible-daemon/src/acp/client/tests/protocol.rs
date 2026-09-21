use std::path::PathBuf;

use super::scripted_client;
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crate::acp::ClientError;

/// `initialize` sends the pinned frame, parses the reply, and stores the
/// capabilities that later decide the MCP transport and the shutdown path.
#[tokio::test]
async fn initialize_parses_the_reply_and_stores_the_agent_capabilities() {
    use agent_client_protocol::schema::v1::InitializeRequest;

    let (mut client, agent) = scripted_client(vec![serde_json::json!({
        "protocolVersion": 1,
        "agentCapabilities": {
            "mcpCapabilities": {"http": true, "sse": false},
            "sessionCapabilities": {"close": {}}
        }
    })]);
    assert!(!client.agent_supports_http_mcp());
    assert!(!client.agent_supports_session_close());

    let response = client
        .initialize(InitializeRequest::new(1u16.into()))
        .await
        .expect("the agent answered");
    let frames = agent.await.expect("agent task completes");

    assert_eq!(frames[0]["method"], "initialize");
    assert_eq!(frames[0]["params"]["protocolVersion"], 1);
    assert_eq!(
        response.protocol_version,
        agent_client_protocol::schema::ProtocolVersion::V1
    );
    assert!(client.agent_supports_http_mcp());
    assert!(client.agent_supports_session_close());
}

/// An `initialize` answer with no `result` is a session error, not a parse
/// of whatever the frame held.
#[tokio::test]
async fn initialize_without_a_result_is_a_session_error() {
    use agent_client_protocol::schema::v1::InitializeRequest;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (client_end, agent_end) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, mut agent_write) = tokio::io::split(agent_end);
    let mut client = CrucibleAcpClient::with_transport(
        ClientConfig::default(),
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );
    let agent = tokio::spawn(async move {
        let line = BufReader::new(agent_read)
            .lines()
            .next_line()
            .await
            .expect("line reads")
            .expect("a line arrives");
        let frame: serde_json::Value = serde_json::from_str(&line).expect("frame is JSON");
        let reply = serde_json::json!({
            "jsonrpc": "2.0",
            "id": frame["id"],
            "error": {"code": -32603, "message": "boom"}
        });
        agent_write
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .expect("reply writes");
    });

    let result = client.initialize(InitializeRequest::new(1u16.into())).await;
    agent.await.expect("agent task completes");

    match result {
        Err(ClientError::Session(msg)) => {
            assert_eq!(msg, "Missing result field in initialize response")
        }
        other => panic!("expected a session error, got {other:?}"),
    }
}

/// `create_new_session` sends `session/new` with the caller's cwd and
/// returns the id the agent chose.
#[tokio::test]
async fn create_new_session_sends_the_cwd_and_returns_the_agent_session_id() {
    use agent_client_protocol::schema::v1::NewSessionRequest;

    let (mut client, agent) = scripted_client(vec![serde_json::json!({"sessionId": "sess-42"})]);

    let response = client
        .create_new_session(NewSessionRequest::new(PathBuf::from("/test")))
        .await
        .expect("the agent answered");
    let frames = agent.await.expect("agent task completes");

    assert_eq!(frames[0]["method"], "session/new");
    assert_eq!(frames[0]["params"]["cwd"], "/test");
    assert_eq!(response.session_id.to_string(), "sess-42");
}

/// `connect_with_best_mcp` runs `initialize` then `session/new`, offers the
/// stdio MCP server when no URL is given, and only then marks the client
/// connected.
#[tokio::test]
async fn connect_with_best_mcp_performs_the_handshake_in_order() {
    let (mut client, agent) = scripted_client(vec![
        serde_json::json!({"protocolVersion": 1, "agentCapabilities": {}}),
        serde_json::json!({"sessionId": "sess-7"}),
    ]);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("the handshake completes");
    let frames = agent.await.expect("agent task completes");

    let methods: Vec<&str> = frames
        .iter()
        .map(|f| f["method"].as_str().expect("method"))
        .collect();
    assert_eq!(methods, ["initialize", "session/new"]);
    let mcp = &frames[1]["params"]["mcpServers"];
    assert_eq!(mcp.as_array().map(Vec::len), Some(1), "{mcp}");
    assert_eq!(mcp[0]["name"], "crucible");
    assert_eq!(
        mcp[0]["args"],
        serde_json::json!(["mcp", "--stdio", "--standalone"])
    );
    assert_eq!(session.id(), "sess-7");
    assert!(client.is_connected());
}

/// `set_config_option` writes one `session/set_config_option` frame and
/// reads the agent's full option list from the reply. The frame is pinned
/// byte for byte except `id`, which a process-wide counter assigns.
#[tokio::test]
async fn set_config_option_writes_the_pinned_frame_and_reads_the_reply() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (client_end, agent_end) = tokio::io::duplex(8192);
    let (client_read, client_write) = tokio::io::split(client_end);
    let (agent_read, mut agent_write) = tokio::io::split(agent_end);

    let mut client = CrucibleAcpClient::with_transport(
        ClientConfig::default(),
        Box::pin(client_write),
        Box::pin(BufReader::new(client_read)),
    );

    let agent = tokio::spawn(async move {
        let mut lines = BufReader::new(agent_read).lines();
        let line = lines
            .next_line()
            .await
            .expect("line reads")
            .expect("a line arrives");
        let mut frame: serde_json::Value = serde_json::from_str(&line).expect("frame is JSON");
        let id = frame
            .as_object_mut()
            .expect("frame is an object")
            .remove("id")
            .expect("frame carries an id");
        let reply = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "configOptions": [{
                    "id": "model",
                    "name": "Model",
                    "category": "model",
                    "type": "select",
                    "currentValue": "mock-opus",
                    "options": [
                        {"value": "mock-sonnet", "name": "Mock Sonnet"},
                        {"value": "mock-opus", "name": "Mock Opus"}
                    ]
                }]
            }
        });
        agent_write
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .expect("reply writes");
        frame
    });

    let response = client
        .set_config_option("sess-1", "model", "mock-opus")
        .await
        .expect("the agent answered");
    let frame = agent.await.expect("agent task completes");

    assert_eq!(
        frame,
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/set_config_option",
            "params": {
                "sessionId": "sess-1",
                "configId": "model",
                "value": "mock-opus"
            }
        })
    );
    let choice = crate::acp::session::ModelChoice::from_config_options(&response.config_options)
        .expect("the reply lists the model selector");
    assert_eq!(choice.current, "mock-opus");
}
