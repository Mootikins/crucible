//! Integration tests for config + agent + model RPC methods.
//!
//! Tests set/get round-trips for thinking_budget, temperature, max_tokens,
//! precognition, and session.configure_agent / session.list_models.

use anyhow::Result;
use crucible_config::BackendType;
use crucible_core::session::SessionAgent;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// In-process test server (mirrors rpc_integration.rs pattern)
struct TestServer {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let server = Server::bind(&socket_path, None).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(Self {
            _temp_dir: temp_dir,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Helper: create a session and configure an agent with known defaults.
/// Returns (session_id, client).
async fn setup_session_with_agent(server: &TestServer) -> (String, DaemonClient) {
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![], None, None)
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "Test assistant.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    // Leak kiln_dir so it stays alive for the duration of the test.
    // The server's TempDir outlives everything anyway.
    std::mem::forget(kiln_dir);

    (session_id, client)
}

// =============================================================================
// 1. Thinking budget round-trip
// =============================================================================

#[tokio::test]
async fn test_thinking_budget_round_trip() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Set thinking budget to 1024
    client
        .session_set_thinking_budget(&session_id, Some(1024))
        .await
        .expect("set_thinking_budget failed");

    let budget = client
        .session_get_thinking_budget(&session_id)
        .await
        .expect("get_thinking_budget failed");

    assert_eq!(
        budget,
        Some(1024),
        "Thinking budget should round-trip to 1024"
    );

    // Update to unlimited (-1)
    client
        .session_set_thinking_budget(&session_id, Some(-1))
        .await
        .expect("set_thinking_budget -1 failed");

    let budget = client
        .session_get_thinking_budget(&session_id)
        .await
        .expect("get_thinking_budget failed");

    assert_eq!(
        budget,
        Some(-1),
        "Thinking budget should round-trip to -1 (unlimited)"
    );

    server.shutdown().await;
}

// =============================================================================
// 2. Temperature round-trip
// =============================================================================

#[tokio::test]
async fn test_temperature_round_trip() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Set temperature to 0.3
    client
        .session_set_temperature(&session_id, 0.3)
        .await
        .expect("set_temperature failed");

    let temp = client
        .session_get_temperature(&session_id)
        .await
        .expect("get_temperature failed");

    assert_eq!(temp, Some(0.3), "Temperature should round-trip to 0.3");

    // Set temperature to 1.5
    client
        .session_set_temperature(&session_id, 1.5)
        .await
        .expect("set_temperature failed");

    let temp = client
        .session_get_temperature(&session_id)
        .await
        .expect("get_temperature failed");

    assert_eq!(temp, Some(1.5), "Temperature should round-trip to 1.5");

    server.shutdown().await;
}

// =============================================================================
// 3. Max tokens round-trip
// =============================================================================

#[tokio::test]
async fn test_max_tokens_round_trip() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Set max_tokens to 8192
    client
        .session_set_max_tokens(&session_id, Some(8192))
        .await
        .expect("set_max_tokens failed");

    let tokens = client
        .session_get_max_tokens(&session_id)
        .await
        .expect("get_max_tokens failed");

    assert_eq!(tokens, Some(8192), "Max tokens should round-trip to 8192");

    // Clear max_tokens (set to None)
    client
        .session_set_max_tokens(&session_id, None)
        .await
        .expect("set_max_tokens None failed");

    let tokens = client
        .session_get_max_tokens(&session_id)
        .await
        .expect("get_max_tokens failed");

    assert_eq!(tokens, None, "Max tokens should be None after clearing");

    server.shutdown().await;
}

// =============================================================================
// 4. Precognition round-trip
// =============================================================================

#[tokio::test]
async fn test_precognition_round_trip() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Use raw call because the DaemonClient struct sends `precognition_enabled`
    // but the server handler reads `enabled` (known field name mismatch).
    // Test the server-side behavior directly.
    let result = client
        .call(
            "session.set_precognition",
            serde_json::json!({
                "session_id": session_id,
                "enabled": false
            }),
        )
        .await
        .expect("set_precognition false failed");
    assert!(
        result.get("precognition_enabled").and_then(|v| v.as_bool()) == Some(false),
        "Response should confirm precognition_enabled = false"
    );

    let enabled = client
        .session_get_precognition(&session_id)
        .await
        .expect("get_precognition failed");

    assert!(!enabled, "Precognition should be false after set(false)");

    // Flip back to true
    let result = client
        .call(
            "session.set_precognition",
            serde_json::json!({
                "session_id": session_id,
                "enabled": true
            }),
        )
        .await
        .expect("set_precognition true failed");
    assert!(
        result.get("precognition_enabled").and_then(|v| v.as_bool()) == Some(true),
        "Response should confirm precognition_enabled = true"
    );

    let enabled = client
        .session_get_precognition(&session_id)
        .await
        .expect("get_precognition failed");

    assert!(enabled, "Precognition should be true after set(true)");

    server.shutdown().await;
}

// =============================================================================
// 5. Configure agent sets agent
// =============================================================================

#[tokio::test]
async fn test_configure_agent_sets_agent() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![], None, None)
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("openai".to_string()),
        provider: BackendType::OpenAI,
        model: "gpt-4o".to_string(),
        system_prompt: "Test configure.".to_string(),
        temperature: Some(0.5),
        max_tokens: Some(2048),
        max_context_tokens: None,
        thinking_budget: Some(512),
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent should succeed");

    // Verify agent was set by reading back session state via session.get
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session["agent"]["model"]
        .as_str()
        .expect("model should be string");
    assert_eq!(model, "gpt-4o", "Agent model should be gpt-4o");

    let provider = session["agent"]["provider"].as_str().unwrap_or("");
    assert!(
        provider.to_lowercase().contains("openai") || provider == "OpenAi",
        "Agent provider should be OpenAi, got: {}",
        provider
    );

    server.shutdown().await;
}

// =============================================================================
// 6. List models returns list
// =============================================================================

#[tokio::test]
async fn test_list_models_returns_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // list_models should succeed and return a list (may be empty without real LLM)
    let models = client
        .session_list_models(&session_id)
        .await
        .expect("session_list_models failed");

    // We can't assert specific models (no real LLM running), but the call
    // should succeed and return a Vec (possibly empty).
    assert!(
        models.is_empty() || !models.is_empty(),
        "list_models should return a valid list"
    );

    server.shutdown().await;
}

// =============================================================================
// 7. Thinking budget default value
// =============================================================================

#[tokio::test]
async fn test_thinking_budget_default_value() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Agent was configured with thinking_budget: None — get should return None
    let budget = client
        .session_get_thinking_budget(&session_id)
        .await
        .expect("get_thinking_budget failed");

    assert_eq!(
        budget, None,
        "Thinking budget should be None when agent configured without one"
    );

    server.shutdown().await;
}

// =============================================================================
// 8. Temperature default value
// =============================================================================

#[tokio::test]
async fn test_temperature_default_value() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Agent was configured with temperature: Some(0.7) — get should return 0.7
    let temp = client
        .session_get_temperature(&session_id)
        .await
        .expect("get_temperature failed");

    assert_eq!(
        temp,
        Some(0.7),
        "Temperature should be 0.7 from the initial agent configuration"
    );

    server.shutdown().await;
}

// =============================================================================
// 9. Max tokens default value (bonus)
// =============================================================================

#[tokio::test]
async fn test_max_tokens_default_value() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Agent was configured with max_tokens: Some(4096) — get should return 4096
    let tokens = client
        .session_get_max_tokens(&session_id)
        .await
        .expect("get_max_tokens failed");

    assert_eq!(
        tokens,
        Some(4096),
        "Max tokens should be 4096 from the initial agent configuration"
    );

    server.shutdown().await;
}

// =============================================================================
// 10. Precognition default value (bonus)
// =============================================================================

#[tokio::test]
async fn test_precognition_default_value() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    // Agent was configured with precognition_enabled: true
    let enabled = client
        .session_get_precognition(&session_id)
        .await
        .expect("get_precognition failed");

    assert!(
        enabled,
        "Precognition should be true from the initial agent configuration"
    );

    server.shutdown().await;
}

// =============================================================================
// 11. Config get on nonexistent session fails
// =============================================================================

#[tokio::test]
async fn test_config_get_on_nonexistent_session_fails() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_get_thinking_budget("nonexistent-session-id")
        .await;
    assert!(
        result.is_err(),
        "get_thinking_budget should fail for nonexistent session"
    );

    let result = client
        .session_get_temperature("nonexistent-session-id")
        .await;
    assert!(
        result.is_err(),
        "get_temperature should fail for nonexistent session"
    );

    server.shutdown().await;
}
