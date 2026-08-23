//! Parameterized agent handshake integration tests
//!
//! Verifies that `CrucibleAcpClient` completes the full handshake, session
//! creation, and error-handling flow for every mock agent kind (Claude-ACP and
//! OpenCode). Each test runs once per agent via `test-case`, preserving the
//! coverage of the former `claude_acp_integration` and `opencode_integration`
//! modules while sharing one assertion body per behavior.

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};
use test_case::test_case;

/// Build a fresh config for the given agent kind. Used as a `test-case`
/// argument so each test enumerates over both supported agent kinds.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn handshake_completes(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;

    // Mock agents advertise auth/methods but don't enforce them, so the
    // handshake should always succeed for both agent kinds.
    if let Err(ref e) = result {
        eprintln!("Handshake failed with error: {:?}", e);
    }
    assert!(
        result.is_ok(),
        "Should complete handshake successfully: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(!session.id().is_empty(), "Should have valid session ID");
    assert!(
        client.is_connected(),
        "Client should be connected after handshake"
    );
}

/// Initialization (the `initialize` request inside `connect_with_best_mcp`)
/// succeeds for every agent kind.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn initialization_succeeds(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(
        result.is_ok(),
        "Initialization should succeed: {:?}",
        result.err()
    );
}

/// After a successful handshake, the session id carries the mock prefix.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn session_id_has_mock_prefix(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(
        result.is_ok(),
        "Should complete handshake: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(
        session.id().starts_with("mock-session-"),
        "Session ID should have mock prefix, got: {}",
        session.id()
    );
}

/// When the mock agent is configured to inject errors, the handshake fails
/// for every agent kind.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn error_injection_fails_handshake(make_config: fn() -> MockStdioAgentConfig) {
    let mut config = make_config();
    config.inject_errors = true;
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(result.is_err(), "Should fail when errors are injected");
}

// -- session/close (plan W7, decision d) ------------------------------------

/// The client sends `session/close` when the agent advertises the
/// capability, and the agent answers it.
#[tokio::test]
async fn close_is_sent_when_the_agent_advertises_it() {
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut config = MockStdioAgentConfig::opencode();
    config.supports_session_close = true;
    config.method_log = Some(log.clone());
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("handshake succeeds");
    assert!(
        client.agent_supports_session_close(),
        "the initialize reply advertises sessionCapabilities.close"
    );

    client
        .close_session(session.id())
        .await
        .expect("session/close succeeds");
    assert!(
        log.lock().unwrap().iter().any(|m| m == "session/close"),
        "the agent received session/close, got: {:?}",
        log.lock().unwrap()
    );
}

/// An agent without `session/close` answers `-32601`. That answer is a
/// normal shutdown, not an error (Hermes behaves this way).
#[tokio::test]
async fn a_method_not_found_reply_to_close_is_not_an_error() {
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("handshake succeeds");
    assert!(
        !client.agent_supports_session_close(),
        "the default mock does not advertise sessionCapabilities.close"
    );

    client
        .close_session(session.id())
        .await
        .expect("a -32601 reply to session/close is tolerated");
}

// -- session/resume (plan W7, decision d) -----------------------------------

/// With a stored agent session id, the client sends `session/resume` and
/// keeps that session. No `session/new` crosses the wire.
#[tokio::test]
async fn resume_reuses_the_agent_session_when_the_agent_answers_it() {
    use crucible_daemon::acp::session::ResumeDisposition;

    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut config = MockStdioAgentConfig::opencode();
    config.supports_session_resume = true;
    config.method_log = Some(log.clone());
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp_resuming(None, Some("mock-session-prior"))
        .await
        .expect("resume succeeds");

    assert_eq!(session.id(), "mock-session-prior");
    assert_eq!(session.resume(), ResumeDisposition::Resumed);
    let log = log.lock().unwrap();
    assert!(log.iter().any(|m| m == "session/resume"), "log: {log:?}");
    assert!(
        !log.iter().any(|m| m == "session/new"),
        "a resumed session must not also open a new one, log: {log:?}"
    );
}

/// An agent without `session/resume` answers `-32601`. The client falls
/// back to `session/new` and reports the fallback on the session.
#[tokio::test]
async fn resume_falls_back_to_session_new_on_method_not_found() {
    use crucible_daemon::acp::session::ResumeDisposition;

    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut config = MockStdioAgentConfig::opencode();
    config.method_log = Some(log.clone());
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp_resuming(None, Some("mock-session-prior"))
        .await
        .expect("the -32601 reply falls back to session/new");

    assert_ne!(session.id(), "mock-session-prior");
    assert!(session.id().starts_with("mock-session-"));
    assert_eq!(session.resume(), ResumeDisposition::FellBackToNew);
    let log = log.lock().unwrap();
    assert!(log.iter().any(|m| m == "session/resume"), "log: {log:?}");
    assert!(log.iter().any(|m| m == "session/new"), "log: {log:?}");
}

/// A handle built with a stored agent session id resumes that session, and
/// reports the same id back for the daemon to persist again.
#[tokio::test]
async fn a_stored_agent_session_id_is_resumed_on_reconnect() {
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};

    let workspace = tempfile::TempDir::new().expect("temp workspace");
    let agent_path = crate::support::mock_agent_path()
        .to_string_lossy()
        .into_owned();
    let mut agent_config = crate::support::mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_SESSION_RESUME".into(), "1".into());

    let handle = AcpAgentHandle::new(AcpAgentHandleParams {
        resume_acp_session_id: Some("mock-session-carried".into()),
        ..crate::support::mock_handle_params(&agent_config, workspace.path())
    })
    .await
    .expect("ACP handshake succeeds");

    assert_eq!(
        handle.acp_session_id().as_deref(),
        Some("mock-session-carried"),
        "the handle keeps the resumed agent session id"
    );
}

/// When the agent answers `session/resume` with `-32601`, the handle falls
/// back to `session/new` and announces the fallback in the event stream.
#[tokio::test]
async fn resume_fallback_is_announced_in_the_event_stream() {
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};

    let workspace = tempfile::TempDir::new().expect("temp workspace");
    let agent_path = crate::support::mock_agent_path()
        .to_string_lossy()
        .into_owned();
    // No CRU_MOCK_SESSION_RESUME: the binary answers -32601.
    let agent_config = crate::support::mock_session_agent(&agent_path);

    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel(16);
    let handle = AcpAgentHandle::new(AcpAgentHandleParams {
        resume_acp_session_id: Some("mock-session-stale".into()),
        event_tx: Some(event_tx),
        parent_session_id: Some("sess-w7"),
        ..crate::support::mock_handle_params(&agent_config, workspace.path())
    })
    .await
    .expect("the -32601 reply falls back to session/new");

    let new_id = handle
        .acp_session_id()
        .expect("the fallback opened a fresh agent session");
    assert_ne!(new_id, "mock-session-stale");

    let event = event_rx.try_recv().expect("the fallback was announced");
    assert_eq!(event.event, "acp_resume_fallback");
    assert_eq!(event.session_id, "sess-w7");
    assert_eq!(event.data["requested_session_id"], "mock-session-stale");
    assert_eq!(event.data["new_session_id"], new_id);
}

/// When the handle drops, the daemon sends `session/close` before it kills
/// the agent process. The spawned mock binary records the closed session id.
#[tokio::test]
async fn close_is_sent_on_handle_drop_when_the_agent_advertises_it() {
    use crucible_daemon::acp_handle::AcpAgentHandle;

    let workspace = tempfile::TempDir::new().expect("temp workspace");
    let capture = workspace.path().join("close-capture");
    let agent_path = crate::support::mock_agent_path()
        .to_string_lossy()
        .into_owned();
    let mut agent_config = crate::support::mock_session_agent(&agent_path);
    agent_config
        .env_overrides
        .insert("CRU_MOCK_SESSION_CLOSE".into(), "1".into());
    agent_config.env_overrides.insert(
        "CRU_MOCK_CLOSE_CAPTURE".into(),
        capture.to_string_lossy().into_owned(),
    );

    let handle = AcpAgentHandle::new(crate::support::mock_handle_params(
        &agent_config,
        workspace.path(),
    ))
    .await
    .expect("ACP handshake succeeds");
    drop(handle);

    // The drop spawns the goodbye as a task; poll for the capture file.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let session_id = loop {
        if let Ok(content) = std::fs::read_to_string(&capture) {
            break content;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the agent never received session/close"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    };
    assert!(
        session_id.starts_with("mock-session-"),
        "session/close names the agent session, got: {session_id}"
    );
}
