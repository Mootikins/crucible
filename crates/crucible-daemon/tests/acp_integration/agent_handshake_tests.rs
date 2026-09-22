//! Parameterized agent handshake integration tests
//!
//! Verifies that `CrucibleAcpClient` completes the full handshake, session
//! creation, and error-handling flow for every mock agent kind (Claude-ACP and
//! OpenCode). Each test runs once per agent via `test-case`, preserving the
//! coverage of the former `claude_acp_integration` and `opencode_integration`
//! modules while sharing one assertion body per behavior.

use crate::support::{connect, logged, MockScript, Resume};
use crucible_daemon::acp::ClientError;
use test_case::test_case;

/// The Claude-ACP mock: HTTP and SSE MCP.
fn claude_acp_script() -> MockScript {
    MockScript {
        name: "mock-claude-acp".into(),
        mcp_sse: true,
        ..MockScript::default()
    }
}

/// The OpenCode mock: HTTP MCP only.
fn opencode_script() -> MockScript {
    MockScript {
        name: "mock-opencode".into(),
        ..MockScript::default()
    }
}

/// Build a fresh config for the given agent kind. Used as a `test-case`
/// argument so each test enumerates over both supported agent kinds.
#[test_case(claude_acp_script as fn() -> MockScript; "claude_acp")]
#[test_case(opencode_script as fn() -> MockScript; "opencode")]
#[tokio::test]
async fn handshake_completes(make_script: fn() -> MockScript) {
    let (mut client, _agent) = connect(make_script(), None, None).await;

    let result = client.handshake(None, None).await;

    // The mock agents advertise no auth method, so the handshake succeeds
    // for both agent kinds.
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
}

/// Initialization (the `initialize` request inside `handshake`)
/// succeeds for every agent kind.
#[test_case(claude_acp_script as fn() -> MockScript; "claude_acp")]
#[test_case(opencode_script as fn() -> MockScript; "opencode")]
#[tokio::test]
async fn initialization_succeeds(make_script: fn() -> MockScript) {
    let (mut client, _agent) = connect(make_script(), None, None).await;

    let result = client.handshake(None, None).await;
    assert!(
        result.is_ok(),
        "Initialization should succeed: {:?}",
        result.err()
    );
}

/// After a successful handshake, the session id carries the mock prefix.
#[test_case(claude_acp_script as fn() -> MockScript; "claude_acp")]
#[test_case(opencode_script as fn() -> MockScript; "opencode")]
#[tokio::test]
async fn session_id_has_mock_prefix(make_script: fn() -> MockScript) {
    let (mut client, _agent) = connect(make_script(), None, None).await;

    let result = client.handshake(None, None).await;
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

/// When the mock agent fails `initialize`, the handshake fails
/// for every agent kind.
#[test_case(claude_acp_script as fn() -> MockScript; "claude_acp")]
#[test_case(opencode_script as fn() -> MockScript; "opencode")]
#[tokio::test]
async fn error_injection_fails_handshake(make_script: fn() -> MockScript) {
    let script = MockScript {
        fail_initialize: true,
        ..make_script()
    };
    let (mut client, _agent) = connect(script, None, None).await;

    // The mock answers `initialize` with a JSON-RPC error. The handshake
    // must fail on that answer, promptly, and not by waiting out a timeout.
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        client.handshake(None, None),
    )
    .await
    .expect("an error reply must fail the handshake, not stall it");
    // The agent's own text reaches the caller, so the user can act on it.
    match result {
        Err(ClientError::Session(message)) => {
            assert_eq!(message, "initialize failed: Simulated initialization error")
        }
        other => {
            panic!("an injected error must fail the handshake as a session error, got: {other:?}")
        }
    }
}

// -- session/close (plan W7, decision d) ------------------------------------

/// The client sends `session/close` when the agent advertises the
/// capability, and the agent answers it.
#[tokio::test]
async fn close_is_sent_when_the_agent_advertises_it() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let log = dir.path().join("log");
    let script = MockScript {
        session_close: true,
        log: Some(log.clone()),
        ..opencode_script()
    };
    let (mut client, _agent) = connect(script, None, None).await;

    let session = client
        .handshake(None, None)
        .await
        .expect("handshake succeeds");
    assert!(
        client.agent_supports_session_close(),
        "the initialize reply advertises sessionCapabilities.close"
    );

    client
        .close(session.id())
        .await
        .expect("session/close succeeds");
    assert_eq!(
        logged(&log, "session/close").len(),
        1,
        "the agent received session/close"
    );
}

/// An agent without `session/close` answers `-32601`. That answer is a
/// normal shutdown, not an error (Hermes behaves this way).
#[tokio::test]
async fn a_method_not_found_reply_to_close_is_not_an_error() {
    let (mut client, _agent) = connect(opencode_script(), None, None).await;

    let session = client
        .handshake(None, None)
        .await
        .expect("handshake succeeds");
    assert!(
        !client.agent_supports_session_close(),
        "the default mock does not advertise sessionCapabilities.close"
    );

    client
        .close(session.id())
        .await
        .expect("a -32601 reply to session/close is tolerated");
}

// -- session/resume (plan W7, decision d) -----------------------------------

/// With a stored agent session id, the client sends `session/resume` and
/// keeps that session. No `session/new` crosses the wire.
#[tokio::test]
async fn resume_reuses_the_agent_session_when_the_agent_answers_it() {
    use crucible_daemon::acp::session::ResumeDisposition;

    let dir = tempfile::TempDir::new().expect("temp dir");
    let log = dir.path().join("log");
    let script = MockScript {
        session_resume: Some(Resume::Adopt),
        log: Some(log.clone()),
        ..opencode_script()
    };
    let (mut client, _agent) = connect(script, None, None).await;

    let session = client
        .handshake(None, Some("mock-session-prior"))
        .await
        .expect("resume succeeds");

    assert_eq!(session.id(), "mock-session-prior");
    assert_eq!(session.resume(), ResumeDisposition::Resumed);
    assert_eq!(logged(&log, "session/resume").len(), 1);
    assert!(
        logged(&log, "session/new").is_empty(),
        "a resumed session must not also open a new one"
    );
}

/// An agent without `session/resume` answers `-32601`. The client falls
/// back to `session/new` and reports the fallback on the session.
#[tokio::test]
async fn resume_falls_back_to_session_new_on_method_not_found() {
    use crucible_daemon::acp::session::ResumeDisposition;

    let dir = tempfile::TempDir::new().expect("temp dir");
    let log = dir.path().join("log");
    let script = MockScript {
        log: Some(log.clone()),
        ..opencode_script()
    };
    let (mut client, _agent) = connect(script, None, None).await;

    let session = client
        .handshake(None, Some("mock-session-prior"))
        .await
        .expect("the -32601 reply falls back to session/new");

    assert_ne!(session.id(), "mock-session-prior");
    assert!(session.id().starts_with("mock-session-"));
    assert_eq!(session.resume(), ResumeDisposition::FellBackToNew);
    assert_eq!(logged(&log, "session/resume").len(), 1);
    assert_eq!(logged(&log, "session/new").len(), 1);
}

/// A handle built with a stored agent session id resumes that session, and
/// reports the same id back for the daemon to persist again. The agent's
/// frame log proves the carried id crossed the wire on `session/resume`,
/// and that no `session/new` opened a second session.
#[tokio::test]
async fn a_stored_agent_session_id_is_resumed_on_reconnect() {
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::acp_handle::{AcpAgentHandle, AcpAgentHandleParams};

    let workspace = tempfile::TempDir::new().expect("temp workspace");
    let agent_path = crate::support::mock_agent_path()
        .to_string_lossy()
        .into_owned();
    let mut agent_config = crate::support::mock_session_agent(&agent_path);
    let log = workspace.path().join("log");
    let (key, value) = MockScript {
        session_resume: Some(Resume::Adopt),
        log: Some(log.clone()),
        ..MockScript::default()
    }
    .env();
    agent_config.env_overrides.insert(key, value);

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

    // The agent logs each request as it arrives, before it answers, so the
    // log is complete once the handshake returns.
    let resumes = logged(&log, "session/resume");
    assert!(
        resumes
            .iter()
            .any(|params| params["sessionId"] == "mock-session-carried"),
        "the carried id must reach the agent on session/resume, log: {resumes:?}"
    );
    assert!(
        logged(&log, "session/new").is_empty(),
        "a resumed session must not also open a new one"
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
    // The default script sets no `session_resume`: the binary answers -32601.
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
/// the agent process. The spawned mock binary logs the closed session id.
#[tokio::test]
async fn close_is_sent_on_handle_drop_when_the_agent_advertises_it() {
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::acp_handle::AcpAgentHandle;

    let workspace = tempfile::TempDir::new().expect("temp workspace");
    let log = workspace.path().join("log");
    let agent_path = crate::support::mock_agent_path()
        .to_string_lossy()
        .into_owned();
    let mut agent_config = crate::support::mock_session_agent(&agent_path);
    let (key, value) = MockScript {
        session_close: true,
        log: Some(log.clone()),
        ..MockScript::default()
    }
    .env();
    agent_config.env_overrides.insert(key, value);

    let handle = AcpAgentHandle::new(crate::support::mock_handle_params(
        &agent_config,
        workspace.path(),
    ))
    .await
    .expect("ACP handshake succeeds");
    let expected = handle
        .acp_session_id()
        .expect("the handshake opened an agent session");
    drop(handle);

    // The drop spawns the goodbye as a task, so poll the log.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let closes = logged(&log, "session/close");
        if closes.iter().any(|params| params["sessionId"] == expected) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "session/close never named the agent session {expected:?}; log holds {closes:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
