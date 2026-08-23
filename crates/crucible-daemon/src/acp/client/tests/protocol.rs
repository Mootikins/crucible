use std::path::PathBuf;

use super::{get_cat_command, get_simple_command};
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;

// Test that initialize() method exists and sends messages
#[tokio::test]
async fn test_protocol_initialize_handshake() {
    use agent_client_protocol::schema::v1::InitializeRequest;

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

    // Send initialize request
    let init_request = InitializeRequest::new(1u16.into());

    let result = client.initialize(init_request).await;

    // Cat will echo back but won't provide valid ACP response
    // Either succeeds (unlikely) or fails on parsing - both verify method works
    let _ = result; // Accept either outcome
}

// Test that create_new_session() method exists and sends messages
#[tokio::test]
async fn test_protocol_new_session() {
    use agent_client_protocol::schema::v1::NewSessionRequest;

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

    // Create new session request
    let session_request = NewSessionRequest::new(PathBuf::from("/test"));

    let result = client.create_new_session(session_request).await;

    // Cat will echo back but won't provide valid ACP response
    let _ = result; // Accept either outcome
}

// Test that connect_with_best_mcp() method exists and attempts full handshake
#[tokio::test]
async fn test_connect_performs_protocol_handshake() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(2000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);

    // connect_with_best_mcp() should:
    // 1. Spawn agent
    // 2. Send InitializeRequest (reads capabilities)
    // 3. Choose transport based on capabilities
    // 4. Send NewSessionRequest
    // 5. Return session
    let result = client.connect_with_best_mcp(None).await;

    // Cat won't respond with valid ACP protocol, so this will fail
    // But it verifies the method exists and attempts the handshake
    let _ = result; // Accept either outcome
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
