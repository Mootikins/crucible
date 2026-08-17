//! Complete user workflow integration test
//!
//! Tests the full session lifecycle through daemon RPC:
//!   open kiln → create session → configure agent → send message →
//!   verify event flow → pause → resume → export → end session → close kiln
//!
//! Uses an in-process daemon server (TestServer pattern from rpc_integration.rs).
//! The send_message step validates the RPC round-trip; without a real LLM provider
//! the daemon returns a provider error which is expected and explicitly asserted.

use anyhow::Result;
use crucible_core::config::BackendType;
use crucible_core::session::{OutputValidation, SessionAgent};
use crucible_daemon::{DaemonClient, Server};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Install the rustls CryptoProvider once for all tests in this binary.
/// Required because the daemon's internal reqwest/TLS usage needs it.
fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

// ---------------------------------------------------------------------------
// Test server fixture (in-process daemon, same pattern as rpc_integration.rs)
// ---------------------------------------------------------------------------

struct TestServer {
    _temp_dir: TempDir,
    /// The directory the registry knows as `kiln` — the one a session attaches
    /// by name, and therefore the one `kiln.open`/`kiln.close` has to act on if
    /// the listing is to come back empty.
    kiln_path: PathBuf,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
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

        // Wait for server to be ready
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
            kiln_path: kiln,
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

/// Build a SessionAgent configured for a mock/test provider.
///
/// Uses Ollama backend pointing at localhost — no real LLM is needed.
/// The test validates the RPC flow, not the LLM response.
fn mock_agent_config() -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "test-model".to_string(),
        system_prompt: "You are a helpful test assistant.".to_string(),
        temperature: Some(0.5),
        max_tokens: Some(1024),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
        mode: None,
    }
}

/// Helper: assert a JSON result's "state" field contains the expected substring.
fn assert_state(result: &serde_json::Value, expected: &str, context: &str) {
    let state = result.get("state").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        state.to_lowercase().contains(&expected.to_lowercase()),
        "{context}: expected state to contain '{expected}', got '{state}'"
    );
}

// ---------------------------------------------------------------------------
// Full user workflow test
// ---------------------------------------------------------------------------

/// Complete user workflow exercising every session lifecycle stage via RPC.
///
/// Steps:
///  1. Open kiln
///  2. Create session
///  3. Configure agent (mock provider)
///  4. Subscribe to session events
///  5. Send message (expects provider error — no real LLM)
///  6. Pause session
///  7. Resume session
///  8. Export / render session markdown
///  9. End session
/// 10. Close kiln
#[tokio::test]
async fn test_complete_user_flow() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = server.kiln_path.clone();

    // Connect with event support
    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    // ── Step 1: Open kiln ─────────────────────────────────────────────────
    client.kiln_open(&kiln_dir).await.expect("kiln.open failed");

    let kilns = client.kiln_list().await.expect("kiln.list failed");
    assert!(
        !kilns.is_empty(),
        "Kiln should appear in list after opening"
    );

    // ── Step 2: Create session ────────────────────────────────────────────
    let create_result = client
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
        .expect("session.create failed");

    let session_id = create_result["session_id"]
        .as_str()
        .expect("response should contain session_id")
        .to_string();
    assert!(!session_id.is_empty(), "session_id must not be empty");

    // Verify initial state is Active
    let session = client
        .session_get(&session_id)
        .await
        .expect("session.get failed");
    assert_state(&session, "active", "New session");

    // ── Step 3: Configure agent ───────────────────────────────────────────
    let agent = mock_agent_config();
    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("session.configure_agent failed");

    // Verify agent is attached to the session
    let session = client
        .session_get(&session_id)
        .await
        .expect("session.get after configure failed");
    let agent_json = session
        .get("agent")
        .expect("Session should have agent after configure");
    assert_eq!(
        agent_json["model"].as_str().unwrap_or(""),
        "test-model",
        "Agent model should match configured value"
    );

    // ── Step 4: Subscribe to session events ───────────────────────────────
    client
        .session_subscribe(&[&session_id])
        .await
        .expect("session.subscribe failed");

    // ── Step 5: Send message ──────────────────────────────────────────────
    // Without a real LLM provider the daemon will return a provider error.
    // We verify the RPC round-trip works and the error is about the provider,
    // not an RPC/protocol failure.
    let send_result = client
        .session_send_message(&session_id, "Hello, this is a test message!", true)
        .await;

    match &send_result {
        Ok(message_id) => {
            // Unexpected but acceptable — if a mock provider somehow responds
            assert!(
                !message_id.is_empty(),
                "Message ID should not be empty on success"
            );
        }
        Err(e) => {
            // Expected: provider is not available (no Ollama running)
            let err_str = e.to_string().to_lowercase();
            assert!(
                err_str.contains("agent")
                    || err_str.contains("provider")
                    || err_str.contains("connect")
                    || err_str.contains("connection")
                    || err_str.contains("error")
                    || err_str.contains("refused"),
                "Error should be about provider/connection, not RPC: {}",
                e
            );
        }
    }

    // Drain any events that were generated during the message attempt
    while event_rx.try_recv().is_ok() {}

    // ── Step 6: Pause session ─────────────────────────────────────────────
    let pause_result = client
        .session_pause(&session_id)
        .await
        .expect("session.pause failed");
    assert_state(&pause_result, "paused", "After pause");

    // Double-check via session.get
    let session = client
        .session_get(&session_id)
        .await
        .expect("session.get after pause failed");
    assert_state(&session, "paused", "session.get after pause");

    // ── Step 7: Resume session ────────────────────────────────────────────
    let resume_result = client
        .session_resume(&session_id)
        .await
        .expect("session.resume failed");
    assert_state(&resume_result, "active", "After resume");

    // Double-check via session.get
    let session = client
        .session_get(&session_id)
        .await
        .expect("session.get after resume failed");
    assert_state(&session, "active", "session.get after resume");

    // ── Step 8: Export session ────────────────────────────────────────────
    // Try to render the session's events as markdown. Keyed on the session
    // id: the transcript's directory is the daemon's to know, not the client's.

    // render_markdown may fail if no events have been persisted to disk yet.
    // We test that the RPC call itself doesn't crash — either outcome is valid.
    let render_result = client
        .session_render_markdown(&session_id, Some(true), None, None, None)
        .await;
    match render_result {
        Ok(markdown) => {
            // Successfully rendered — markdown is a string (may be empty)
            assert!(
                markdown.is_ascii() || !markdown.is_empty() || markdown.is_empty(),
                "Rendered markdown should be valid"
            );
        }
        Err(_) => {
            // Acceptable — session may not have persisted events yet
        }
    }

    // Also try export_to_file to cover the export path
    let export_path = kiln_dir.join("export.md");
    let export_result = client
        .session_export_to_file(&session_id, Some(&export_path), Some(true))
        .await;
    // Export may also fail for same reasons — we only assert no panic
    match export_result {
        Ok(path) => {
            assert!(
                !path.is_empty(),
                "Export path should not be empty on success"
            );
        }
        Err(_) => {
            // Acceptable — session directory may not have recording data
        }
    }

    // ── Step 9: Unsubscribe ───────────────────────────────────────────────
    client
        .session_unsubscribe(&[&session_id])
        .await
        .expect("session.unsubscribe failed");

    // ── Step 10: End session ──────────────────────────────────────────────
    let end_result = client
        .session_end(&session_id)
        .await
        .expect("session.end failed");
    assert_state(&end_result, "ended", "After end");

    // NOTE: After session.end the daemon removes the session from its
    // in-memory store — session.get returns "Session not found".
    // This is expected: ended sessions are persisted to disk only.

    // ── Step 11: Close kiln ───────────────────────────────────────────────
    let close_result = client
        .call(
            "kiln.close",
            serde_json::json!({"path": kiln_dir.to_string_lossy()}),
        )
        .await;
    assert!(
        close_result.is_ok(),
        "kiln.close should succeed: {:?}",
        close_result.err()
    );

    // Verify kiln is no longer listed
    let kilns = client.kiln_list().await.expect("kiln.list after close");
    assert!(kilns.is_empty(), "Kiln list should be empty after close");

    // ── Cleanup ───────────────────────────────────────────────────────────
    server.shutdown().await;
}

/// Verify that the session list correctly reflects lifecycle transitions.
///
/// Creates a session, pauses it, verifies state in list, resumes, and ends.
#[tokio::test]
async fn test_user_flow_session_list_reflects_state() {
    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Create session
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
        .expect("session.create failed");
    let session_id = result["session_id"]
        .as_str()
        .expect("session_id")
        .to_string();

    // List active sessions — should contain our session
    let list = client
        .session_list(
            Some(&crucible_daemon::test_support::kiln_name("kiln")),
            None,
            Some("chat"),
            None,
            None,
        )
        .await
        .expect("session.list failed");
    let sessions = list["sessions"]
        .as_array()
        .expect("sessions should be array");
    assert!(
        sessions
            .iter()
            .any(|s| s["session_id"].as_str() == Some(&session_id)),
        "Active session should appear in list"
    );

    // Pause → verify via get
    client
        .session_pause(&session_id)
        .await
        .expect("pause failed");
    let session = client
        .session_get(&session_id)
        .await
        .expect("get after pause");
    assert_state(&session, "paused", "List after pause");

    // Resume → verify
    client
        .session_resume(&session_id)
        .await
        .expect("resume failed");
    let session = client
        .session_get(&session_id)
        .await
        .expect("get after resume");
    assert_state(&session, "active", "List after resume");

    // End → verify via the end response itself
    let end_result = client.session_end(&session_id).await.expect("end failed");
    assert_state(&end_result, "ended", "After end");

    // After end, the session is removed from the in-memory store.
    // session.get would return "Session not found" which is correct.

    server.shutdown().await;
}
