//! Integration tests for Task 1.2f — `session.create` emits setup events.
//!
//! Verifies that after a session is created, the daemon's setup task emits
//! the expected setup-event sequence over its broadcast channel:
//!
//!   1. `session_initialized` (always first)
//!   2. concurrently: `workspace_indexed`, `kiln_notes_indexed`,
//!      `plugins_discovered`, `mcp_servers_ready`
//!   3. for `agent_type: "internal"` only: `providers_listed` and
//!      (conditionally) `context_limit_resolved`
//!
//! The client subscribes BEFORE calling `session.create` so setup events are
//! not missed. The integration test uses an in-process test server following
//! the same pattern as `rpc_session_e2e.rs`.

use anyhow::Result;
use crucible_daemon::rpc_client::SessionCreateParams;
use crucible_daemon::{DaemonClient, Server, SessionEvent};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

struct TestServer {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

/// Install the rustls CryptoProvider before any TLS usage (provider listing).
fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl TestServer {
    async fn start() -> Result<Self> {
        ensure_crypto_provider();
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let server =
            Server::bind_with_data_home(&socket_path, temp_dir.path().to_path_buf()).await?;
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

/// Collect setup events for `session_id` from `event_rx`, returning early
/// once every event name in `expected` has been observed, or when the
/// timeout elapses.
///
/// We can't simply assert "got all 7" because the setup task runs tasks
/// concurrently (indexers/discovery) and because `context_limit_resolved`
/// is conditional. Instead, callers pass the set of required events and
/// the collector exits as soon as that set is satisfied — avoiding a
/// blanket wall-clock wait. The `timeout` is an upper bound for CI
/// headroom if something genuinely stalls.
async fn collect_setup_events(
    event_rx: &mut mpsc::UnboundedReceiver<SessionEvent>,
    session_id: &str,
    expected: &HashSet<&str>,
    timeout: Duration,
) -> Vec<SessionEvent> {
    let setup_event_names: HashSet<&str> = [
        "session_initialized",
        "workspace_indexed",
        "kiln_notes_indexed",
        "plugins_discovered",
        "mcp_servers_ready",
        "providers_listed",
        "context_limit_resolved",
    ]
    .into_iter()
    .collect();

    let mut events = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        match tokio::time::timeout(deadline - now, event_rx.recv()).await {
            Ok(Some(ev)) => {
                if ev.session_id == session_id && setup_event_names.contains(ev.event_type.as_str())
                {
                    seen.insert(ev.event_type.clone());
                    events.push(ev);
                    if expected.iter().all(|name| seen.contains(*name)) {
                        break;
                    }
                }
            }
            Ok(None) => break, // channel closed
            Err(_) => break,   // timeout
        }
    }
    events
}

/// Internal-agent session: expect all common events + `providers_listed`.
/// `context_limit_resolved` is conditional on having an endpoint and model,
/// which a freshly-created session does not yet have, so we accept its
/// absence.
#[tokio::test]
async fn session_create_emits_setup_events_for_internal_agent() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    std::fs::write(kiln_dir.path().join("note.md"), "# hello\n").expect("write note");

    // Subscribe BEFORE creating the session — the setup task fires the
    // moment `session.create` returns and we must not miss events.
    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("connect_with_events failed");

    // Subscribe to `*` (wildcard) BEFORE calling session.create so the
    // setup task's events — which fire the moment the session is
    // registered — are not missed. The daemon supports `"*"` as a
    // subscribe-to-all sentinel (see server/tests.rs
    // `test_session_subscribe_wildcard`).
    client
        .session_subscribe(&["*"])
        .await
        .expect("pre-subscribe failed");

    let resp = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: Some("internal".to_string()),
        })
        .await
        .expect("session_create failed");

    let session_id = resp["session_id"]
        .as_str()
        .expect("session_id must be string")
        .to_string();

    let expected: HashSet<&str> = [
        "session_initialized",
        "workspace_indexed",
        "kiln_notes_indexed",
        "plugins_discovered",
        "mcp_servers_ready",
        "providers_listed",
    ]
    .into_iter()
    .collect();
    let events = collect_setup_events(
        &mut event_rx,
        &session_id,
        &expected,
        Duration::from_secs(5),
    )
    .await;

    let event_types: Vec<String> = events.iter().map(|e| e.event_type.clone()).collect();
    let event_set: HashSet<&str> = event_types.iter().map(|s| s.as_str()).collect();

    assert!(
        event_set.contains("session_initialized"),
        "missing session_initialized; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("workspace_indexed"),
        "missing workspace_indexed; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("kiln_notes_indexed"),
        "missing kiln_notes_indexed; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("plugins_discovered"),
        "missing plugins_discovered; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("mcp_servers_ready"),
        "missing mcp_servers_ready; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("providers_listed"),
        "internal-agent session must emit providers_listed; got {:?}",
        event_types
    );

    // Ordering: session_initialized must be the first setup event observed.
    assert_eq!(
        event_types.first().map(String::as_str),
        Some("session_initialized"),
        "session_initialized must fire first; got {:?}",
        event_types
    );

    server.shutdown().await;
}

/// ACP session: expect all common events BUT NOT `providers_listed` or
/// `context_limit_resolved` (those are internal-agent only).
#[tokio::test]
async fn session_create_omits_llm_events_for_acp_agent() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("connect_with_events failed");

    client
        .session_subscribe(&["*"])
        .await
        .expect("pre-subscribe failed");

    let resp = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: Some("acp".to_string()),
        })
        .await
        .expect("session_create failed");

    let session_id = resp["session_id"]
        .as_str()
        .expect("session_id must be string")
        .to_string();

    // For ACP we assert on *absence* of LLM events, so we cannot early-exit
    // the moment the common events arrive — we need to drain the full
    // window to catch a regression where `providers_listed` would leak in
    // later. Pass an expected set that will never fully match so the
    // collector waits the whole timeout.
    let never_completes: HashSet<&str> = ["__sentinel_never_fires__"].into_iter().collect();
    let events = collect_setup_events(
        &mut event_rx,
        &session_id,
        &never_completes,
        Duration::from_secs(5),
    )
    .await;
    let event_types: Vec<String> = events.iter().map(|e| e.event_type.clone()).collect();
    let event_set: HashSet<&str> = event_types.iter().map(|s| s.as_str()).collect();

    // Common events: still emitted.
    assert!(
        event_set.contains("session_initialized"),
        "missing session_initialized; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("workspace_indexed"),
        "missing workspace_indexed; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("kiln_notes_indexed"),
        "missing kiln_notes_indexed; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("plugins_discovered"),
        "missing plugins_discovered; got {:?}",
        event_types
    );
    assert!(
        event_set.contains("mcp_servers_ready"),
        "missing mcp_servers_ready; got {:?}",
        event_types
    );

    // LLM-specific events: MUST be absent for ACP.
    assert!(
        !event_set.contains("providers_listed"),
        "ACP session must NOT emit providers_listed; got {:?}",
        event_types
    );
    assert!(
        !event_set.contains("context_limit_resolved"),
        "ACP session must NOT emit context_limit_resolved; got {:?}",
        event_types
    );

    server.shutdown().await;
}
