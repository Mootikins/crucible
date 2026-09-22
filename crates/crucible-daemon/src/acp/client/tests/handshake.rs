use std::path::PathBuf;

use serde_json::json;

use super::{raw_client, scripted_client};
use crate::acp::client::{ClientConfig, CrucibleAcpClient};
use crate::acp::ClientError;

/// The handshake runs `initialize` then `session/new`. It keeps the agent
/// capabilities, and it offers the stdio MCP server when no URL is given.
#[tokio::test]
async fn the_handshake_runs_in_order_and_keeps_the_capabilities() {
    let (mut client, agent) = scripted_client(vec![
        json!({"result": {
            "protocolVersion": 1,
            "agentCapabilities": {
                "mcpCapabilities": {"http": true, "sse": false},
                "sessionCapabilities": {"close": {}}
            }
        }}),
        json!({"result": {"sessionId": "sess-7"}}),
    ])
    .await;
    assert!(!client.agent_supports_http_mcp());
    assert!(!client.agent_supports_session_close());

    let session = client
        .handshake(None, None)
        .await
        .expect("the handshake completes");
    let frames = agent.await.expect("agent task completes");

    let methods: Vec<&str> = frames
        .iter()
        .map(|f| f["method"].as_str().expect("method"))
        .collect();
    assert_eq!(methods, ["initialize", "session/new"]);
    assert_eq!(frames[0]["params"]["protocolVersion"], 1);
    let mcp = &frames[1]["params"]["mcpServers"];
    assert_eq!(mcp.as_array().map(Vec::len), Some(1), "{mcp}");
    assert_eq!(mcp[0]["name"], "crucible");
    assert_eq!(mcp[0]["args"], json!(["mcp", "--stdio", "--standalone"]));
    assert_eq!(session.id(), "sess-7");
    assert!(client.agent_supports_http_mcp());
    assert!(client.agent_supports_session_close());
}

/// An `initialize` error is a session error that keeps the agent's own
/// text. The user reads that text to fix the cause, for example a missing
/// login.
#[tokio::test]
async fn an_initialize_error_keeps_the_agent_message() {
    let (mut client, agent) = scripted_client(vec![json!({"error": {
        "code": -32603,
        "message": "Internal error",
        "data": {"message": "not logged in"}
    }})])
    .await;

    let result = client.handshake(None, None).await;
    agent.await.expect("agent task completes");

    match result {
        Err(ClientError::Session(msg)) => {
            assert_eq!(msg, "initialize failed: Internal error: not logged in")
        }
        other => panic!("expected a session error, got {other:?}"),
    }
}

/// A `session/new` error keeps the agent's own text.
#[tokio::test]
async fn a_session_new_error_keeps_the_agent_message() {
    let (mut client, agent) = scripted_client(vec![
        json!({"result": {"protocolVersion": 1, "agentCapabilities": {}}}),
        json!({"error": {"code": -32000, "message": "Authentication required"}}),
    ])
    .await;

    let result = client.handshake(None, None).await;
    agent.await.expect("agent task completes");

    match result {
        Err(ClientError::Session(msg)) => {
            assert_eq!(msg, "session/new failed: Authentication required")
        }
        other => panic!("expected a session error, got {other:?}"),
    }
}

/// `request` sends one typed request and reads the typed reply.
#[tokio::test]
async fn a_request_sends_the_params_and_reads_the_reply() {
    use agent_client_protocol::schema::v1::SetSessionConfigOptionRequest;

    let (client, agent) = scripted_client(vec![json!({"result": {"configOptions": [{
        "id": "model",
        "name": "Model",
        "category": "model",
        "type": "select",
        "currentValue": "mock-opus",
        "options": [
            {"value": "mock-sonnet", "name": "Mock Sonnet"},
            {"value": "mock-opus", "name": "Mock Opus"}
        ]
    }]}})])
    .await;

    let response = client
        .request(SetSessionConfigOptionRequest::new(
            "sess-1".to_string(),
            "model".to_string(),
            "mock-opus",
        ))
        .await
        .expect("the agent answered");
    let frames = agent.await.expect("agent task completes");

    assert_eq!(frames[0]["method"], "session/set_config_option");
    assert_eq!(
        frames[0]["params"],
        json!({"sessionId": "sess-1", "configId": "model", "value": "mock-opus"})
    );
    let choice = crate::acp::session::ModelChoice::from_config_options(&response.config_options)
        .expect("the reply lists the model selector");
    assert_eq!(choice.current, "mock-opus");
}

/// A permission request that the client cannot read gets `-32602`. The
/// agent waits for a reply, so no reply at all would hang its turn.
#[tokio::test]
async fn a_permission_request_with_unreadable_params_gets_invalid_params() {
    let (_client, mut agent) = raw_client().await;

    agent
        .write(json!({
            "jsonrpc": "2.0",
            "id": "perm-1",
            "method": "session/request_permission",
            "params": {"sessionId": "s1"}
        }))
        .await;
    let reply = agent.read().await;

    assert_eq!(reply["id"], "perm-1");
    assert_eq!(reply["error"]["code"], -32602, "{reply}");
}

/// A request that the client does not know gets `-32601`.
#[tokio::test]
async fn an_unknown_request_gets_method_not_found() {
    let (_client, mut agent) = raw_client().await;

    agent
        .write(json!({"jsonrpc": "2.0", "id": 7, "method": "fs/read_text_file", "params": {}}))
        .await;
    let reply = agent.read().await;

    assert_eq!(reply["id"], 7);
    assert_eq!(reply["error"]["code"], -32601, "{reply}");
}

/// An agent path that does not exist is a connection error.
#[tokio::test]
async fn a_missing_agent_binary_is_a_connection_error() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/nonexistent/acp-agent"),
        ..Default::default()
    };

    let result = CrucibleAcpClient::spawn(config, "missing", None).await;

    assert!(
        matches!(result, Err(ClientError::Connection(_))),
        "{result:?}"
    );
}

/// A request completes while a turn runs. The daemon sends a knob change,
/// for example a model switch, without a wait for the turn to end.
#[tokio::test]
async fn a_request_completes_while_a_turn_is_held() {
    use agent_client_protocol::schema::v1::{
        ContentBlock, PromptRequest, SessionId, SetSessionConfigOptionRequest,
    };
    use std::sync::Arc;
    use std::time::Duration;

    let (client, mut agent) = raw_client().await;
    let client = Arc::new(client);

    let turn = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            let (out, _chunks) = tokio::sync::mpsc::unbounded_channel();
            client
                .prompt(
                    PromptRequest::new(SessionId::from("s1"), vec![ContentBlock::from("go")]),
                    &out,
                )
                .await
        }
    });
    let prompt = agent.read().await;
    assert_eq!(prompt["method"], "session/prompt");

    // The agent holds the turn. It answers the next request only.
    let request = tokio::spawn({
        let client = Arc::clone(&client);
        async move {
            client
                .request(SetSessionConfigOptionRequest::new(
                    "s1".to_string(),
                    "model".to_string(),
                    "other",
                ))
                .await
        }
    });
    let set = tokio::time::timeout(Duration::from_secs(5), agent.read())
        .await
        .expect("the client must send the request while the turn is held");
    assert_eq!(set["method"], "session/set_config_option");
    agent
        .write(json!({"jsonrpc": "2.0", "id": set["id"], "result": {"configOptions": []}}))
        .await;
    tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .expect("the request must complete while the turn is held")
        .expect("the request task")
        .expect("the agent answered");

    agent
        .write(json!({"jsonrpc": "2.0", "id": prompt["id"], "result": {"stopReason": "end_turn"}}))
        .await;
    turn.await
        .expect("the turn task")
        .expect("the turn ends normally");
}

/// A drop of the client kills the whole agent process group. An agent that a
/// launcher starts (`npx`, `uvx`, a sandbox prefix) runs as a child of the
/// launcher, and a kill of the launcher alone leaves that child alive.
#[cfg(unix)]
#[tokio::test]
async fn a_client_drop_kills_the_children_of_the_agent_process() {
    use std::time::{Duration, Instant};

    let dir = tempfile::TempDir::new().expect("temp dir");
    let pid_file = dir.path().join("grandchild.pid");
    let config = ClientConfig {
        agent_path: PathBuf::from("sh"),
        agent_args: Some(vec![
            "-c".to_string(),
            format!("sleep 300 & echo $! > {}; wait", pid_file.display()),
        ]),
        ..Default::default()
    };

    let client = CrucibleAcpClient::spawn(config, "launcher", None)
        .await
        .expect("the launcher starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid: libc::pid_t = loop {
        if let Some(pid) = std::fs::read_to_string(&pid_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "the launcher started no child");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // SAFETY: signal 0 sends nothing; it only checks that the pid exists.
    let alive = || unsafe { libc::kill(pid, 0) == 0 };
    assert!(alive(), "the child runs before the drop");

    drop(client);

    let deadline = Instant::now() + Duration::from_secs(10);
    while alive() {
        assert!(
            Instant::now() < deadline,
            "the child of the agent process outlived the client"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
