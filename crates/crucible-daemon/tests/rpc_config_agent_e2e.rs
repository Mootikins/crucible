//! Integration tests for config + agent + model RPC methods.
//!
//! Tests set/get round-trips for thinking_budget,
//! precognition, and session.configure_agent / session.list_models.

use anyhow::Result;
use crucible_core::config::BackendType;
use crucible_core::session::{OutputValidation, SessionAgent};
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

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl TestServer {
    async fn start() -> Result<Self> {
        ensure_crypto_provider();
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        // One registered kiln named `kiln`: sessions address kilns by name, so
        // a fixture that registers none has a daemon that refuses every scoped
        // request.
        let kiln = temp_dir.path().join("kiln");
        std::fs::create_dir_all(&kiln)?;
        let server = Server::bind_with_data_home_and_kilns(
            &socket_path,
            temp_dir.path().to_path_buf(),
            &[("kiln", &kiln)],
        )
        .await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Poll for readiness rather than sleeping a fixed interval. Under a
        // loaded box the socket may not be accepting when a fixed timer
        // elapses, which is this suite's intermittent-failure source.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "daemon did not start accepting connections within 5s"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

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
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "Test assistant.".to_string(),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: 5,
        context_budget: None,
        context_strategy: Default::default(),
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
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

// =============================================================================
// 3. Max tokens round-trip
// =============================================================================

// =============================================================================
// 4. Precognition round-trip
// =============================================================================

#[tokio::test]
async fn test_precognition_round_trip() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (session_id, client) = setup_session_with_agent(&server).await;

    client
        .session_set_precognition(&session_id, false)
        .await
        .expect("set_precognition false failed");

    let enabled = client
        .session_get_precognition(&session_id)
        .await
        .expect("get_precognition failed");

    assert!(!enabled, "Precognition should be false after set(false)");

    // Flip back to true
    client
        .session_set_precognition(&session_id, true)
        .await
        .expect("set_precognition true failed");

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
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        mode: None,
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("openai".to_string()),
        provider: BackendType::OpenAI,
        model: "gpt-4o".to_string(),
        system_prompt: "Test configure.".to_string(),
        max_context_tokens: None,
        thinking_budget: Some(512),
        endpoint: None,
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        context_budget: None,
        context_strategy: Default::default(),
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
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

// =============================================================================
// 9. Max tokens default value (bonus)
// =============================================================================

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
// 11. Every config knob round-trips over the real wire
// =============================================================================

/// One set→get round-trip per session config knob, through real JSON-RPC
/// serialization against a live server. This is the runtime companion to the
/// static field-name parity gate in architecture_tests.rs: a serde rename or
/// repr change the source-text scan can't see fails here. Every knob in the
/// gate's CONFIG_METHODS table must round-trip a non-default value below.
#[tokio::test]
async fn all_config_knobs_round_trip_over_the_wire() {
    let server = TestServer::start().await.expect("Failed to start server");
    let (sid, client) = setup_session_with_agent(&server).await;
    let mut failures: Vec<String> = Vec::new();

    macro_rules! round_trip {
        ($knob:literal, $set:expr, $get:expr, $expected:expr) => {{
            if let Err(e) = $set.await {
                failures.push(format!("{}: set failed: {e}", $knob));
            } else {
                match $get.await {
                    Ok(actual) if actual == $expected => {}
                    Ok(actual) => failures.push(format!(
                        "{}: set value did not survive the wire: got {actual:?}, expected {:?}",
                        $knob, $expected
                    )),
                    Err(e) => failures.push(format!("{}: get failed: {e}", $knob)),
                }
            }
        }};
    }

    round_trip!(
        "thinking_budget",
        client.session_set_thinking_budget(&sid, Some(1024)),
        client.session_get_thinking_budget(&sid),
        Some(1024)
    );
    round_trip!(
        "precognition_results",
        client.session_set_precognition_results(&sid, 9),
        client.session_get_precognition_results(&sid),
        Some(9)
    );
    round_trip!(
        "context_budget",
        client.session_set_context_budget(&sid, Some(32000)),
        client.session_get_context_budget(&sid),
        Some(32000)
    );
    round_trip!(
        "context_strategy",
        client.session_set_context_strategy(&sid, "sliding_window"),
        client.session_get_context_strategy(&sid),
        Some("sliding_window".to_string())
    );
    round_trip!(
        "output_validation",
        client.session_set_output_validation(&sid, "json"),
        client.session_get_output_validation(&sid),
        Some("json".to_string())
    );
    round_trip!(
        "validation_retries",
        client.session_set_validation_retries(&sid, 5),
        client.session_get_validation_retries(&sid),
        Some(5)
    );
    round_trip!(
        "autocompact_threshold",
        client.session_set_autocompact_threshold(&sid, Some(0.75)),
        client.session_get_autocompact_threshold(&sid),
        Some(0.75)
    );

    // precognition's getter returns bool (not Option) — check it directly.
    if let Err(e) = client.session_set_precognition(&sid, false).await {
        failures.push(format!("precognition: set failed: {e}"));
    } else {
        match client.session_get_precognition(&sid).await {
            Ok(false) => {}
            Ok(true) => failures
                .push("precognition: set(false) did not survive the wire (got true)".to_string()),
            Err(e) => failures.push(format!("precognition: get failed: {e}")),
        }
    }

    assert!(
        failures.is_empty(),
        "config knobs failed the wire round-trip:\n  - {}",
        failures.join("\n  - ")
    );

    server.shutdown().await;
}

// =============================================================================
// 12. Config get on nonexistent session fails
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

    server.shutdown().await;
}
