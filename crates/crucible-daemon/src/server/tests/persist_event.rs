use super::*;
use crate::session_manager::SessionError;
use crate::session_storage::SessionStorage;
use async_trait::async_trait;
use crucible_core::session::{SessionSummary, SessionType};

/// Minimal AgentManager for exercising the sweep's cleanup call.
fn sweep_test_agent_manager() -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: Arc::new(SessionManager::new()),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

struct FailingStorage;

#[async_trait]
impl SessionStorage for FailingStorage {
    async fn save(&self, _s: &crucible_core::session::Session) -> Result<(), SessionError> {
        Ok(())
    }
    async fn load(
        &self,
        _id: &str,
        _k: &Path,
    ) -> Result<crucible_core::session::Session, SessionError> {
        Err(SessionError::NotFound("mock".to_string()))
    }
    async fn list(&self, _k: &Path) -> Result<Vec<SessionSummary>, SessionError> {
        Ok(vec![])
    }
    async fn append_event(
        &self,
        _s: &crucible_core::session::Session,
        _e: &str,
    ) -> Result<(), SessionError> {
        Err(SessionError::IoError("simulated disk failure".to_string()))
    }
    async fn append_markdown(
        &self,
        _s: &crucible_core::session::Session,
        _r: &str,
        _c: &str,
    ) -> Result<(), SessionError> {
        Err(SessionError::IoError("simulated disk failure".to_string()))
    }
    async fn load_events(
        &self,
        _id: &str,
        _k: &Path,
        _limit: Option<usize>,
        _offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        Ok(vec![])
    }
    async fn count_events(&self, _id: &str, _k: &Path) -> Result<usize, SessionError> {
        Ok(0)
    }
}

#[tokio::test]
async fn test_persist_event_returns_error_on_storage_failure() {
    let tmp = TempDir::new().unwrap();
    let sm = Arc::new(SessionManager::new());
    let session = sm
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let event = SessionEventMessage::new(
        session.id.clone(),
        "user_message",
        serde_json::json!({"content": "hello"}),
    );

    let storage = FailingStorage;
    let result = persist_event(&event, &sm, &storage).await;
    assert!(
        result.is_err(),
        "persist_event must propagate storage errors, not swallow them"
    );
}

#[tokio::test]
async fn test_persist_event_skips_non_persistent_events() {
    let tmp = TempDir::new().unwrap();
    let sm = Arc::new(SessionManager::new());
    let session = sm
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let event = SessionEventMessage::new(
        session.id.clone(),
        "stream_chunk",
        serde_json::json!({"chunk": "partial"}),
    );

    let storage = FailingStorage;
    let result = persist_event(&event, &sm, &storage).await;
    assert!(
        result.is_ok(),
        "Non-persistent events should be skipped without error"
    );
}

#[tokio::test]
async fn test_should_persist_filters_correctly() {
    let persistent = [
        "user_message",
        "thinking",
        "message_complete",
        "tool_call",
        "tool_result",
        "model_switched",
        "ended",
    ];
    for event_name in &persistent {
        let event = SessionEventMessage::new("test", *event_name, serde_json::json!({}));
        assert!(should_persist(&event), "{} should be persisted", event_name);
    }

    let non_persistent = ["stream_chunk", "status_update", "unknown"];
    for event_name in &non_persistent {
        let event = SessionEventMessage::new("test", *event_name, serde_json::json!({}));
        assert!(
            !should_persist(&event),
            "{} should NOT be persisted",
            event_name
        );
    }

    let mut replay_event = SessionEventMessage::new("test", "user_message", serde_json::json!({}));
    replay_event.msg_type = "replay_event".to_string();
    assert!(
        !should_persist(&replay_event),
        "replay events should not be persisted"
    );
}

#[tokio::test]
async fn test_sweep_and_archive_stale_sessions_archives_inactive_sessions_without_subscribers() {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();

    let kiln_manager = KilnManager::new();
    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &kiln_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    assert!(session_manager.get_session(&session.id).is_none());

    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_sweep_cleans_up_agent_state_for_archived_sessions() {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new();
    let subscription_manager = SubscriptionManager::new();
    let kiln_manager = KilnManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();

    // Simulate per-turn agent state that the sweep must free.
    let agent_manager = sweep_test_agent_manager();
    agent_manager.snapshots.insert(
        session.id.clone(),
        0,
        crate::workspace_snapshot::WorkspaceSnapshot::default(),
    );
    assert!(!agent_manager.snapshots.is_empty());

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &kiln_manager,
        &subscription_manager,
        &agent_manager,
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    assert!(
        agent_manager.snapshots.is_empty(),
        "sweep must free the archived session's agent state"
    );
}

/// The sweep must reach sessions that are only in storage (ended sessions
/// are evicted from memory) — this was the gap that let hundreds of stale
/// ended sessions accumulate in listings forever.
#[tokio::test]
async fn test_sweep_archives_stale_persisted_sessions_not_in_memory() {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();
    session_manager.end_session(&session.id).await.unwrap();
    assert!(
        session_manager.get_session(&session.id).is_none(),
        "precondition: ended session must be evicted from memory"
    );

    let kiln_manager = KilnManager::new();
    kiln_manager.open(tmp.path()).await.unwrap();

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &kiln_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert!(persisted.archived);
}

/// Legacy meta.json files can carry a RELATIVE kiln path ("./docs") from
/// before kiln paths were canonicalized. Archiving with the file's
/// self-reported kiln resolves against the daemon's cwd and misses — the
/// sweep must archive under the kiln directory it actually scanned.
#[tokio::test]
async fn test_sweep_archives_sessions_whose_meta_has_relative_kiln_path() {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();
    session_manager.end_session(&session.id).await.unwrap();

    // Rewrite the persisted meta.json with a legacy relative kiln path.
    let meta_path = tmp
        .path()
        .join(".crucible")
        .join("sessions")
        .join(&session.id)
        .join("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["kiln"] = serde_json::json!("./docs");
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta).unwrap()).unwrap();

    let kiln_manager = KilnManager::new();
    kiln_manager.open(tmp.path()).await.unwrap();

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &kiln_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_sweep_and_archive_stale_sessions_skips_sessions_with_active_subscribers() {
    let tmp = TempDir::new().unwrap();
    let session_manager = SessionManager::new();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();

    let client = ClientId::new();
    subscription_manager.subscribe(client, &session.id);

    let kiln_manager = KilnManager::new();
    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &kiln_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 0);
    let still_active = session_manager.get_session(&session.id).unwrap();
    assert!(!still_active.archived);
}

#[tokio::test]
async fn test_session_create_with_granular_recording_mode() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    client
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{\"recording_mode\":\"granular\"}}\n",
        )
        .await
        .unwrap();

    let mut buf = vec![0u8; 2048];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        response.contains("\"result\""),
        "Should have successful result"
    );
    assert!(
        response.contains("\"session_id\""),
        "Should have session_id in response"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_create_default_no_recording_mode() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();
    // Create session without recording_mode parameter
    client
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{}}\n")
        .await
        .unwrap();

    let mut buf = vec![0u8; 2048];
    let n = client.read(&mut buf).await.unwrap();
    let response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        response.contains("\"result\""),
        "Should have successful result"
    );
    assert!(
        response.contains("\"session_id\""),
        "Should have session_id in response"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_session_get_includes_recording_mode() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let server = Server::bind(&sock_path, None).await.unwrap();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    // First, create a session with granular recording mode
    client
        .write_all(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.create\",\"params\":{\"recording_mode\":\"granular\"}}\n",
        )
        .await
        .unwrap();

    let mut buf = vec![0u8; 2048];
    let n = client.read(&mut buf).await.unwrap();
    let response_str = String::from_utf8_lossy(&buf[..n]);

    // Extract session_id from response
    let response: serde_json::Value =
        serde_json::from_str(&response_str).expect("Failed to parse create response");
    let session_id = response["result"]["session_id"]
        .as_str()
        .expect("No session_id in response");

    // Now get the session and verify recording_mode is in response
    let get_request = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"session.get\",\"params\":{{\"session_id\":\"{}\"}}}}\n",
        session_id
    );
    client.write_all(get_request.as_bytes()).await.unwrap();

    let mut buf = vec![0u8; 2048];
    let n = client.read(&mut buf).await.unwrap();
    let get_response = String::from_utf8_lossy(&buf[..n]);

    assert!(
        get_response.contains("recording_mode"),
        "session.get response should include recording_mode field"
    );
    assert!(
        get_response.contains("granular"),
        "recording_mode should be 'granular'"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_granular_session_creates_recording_file() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}","recording_mode":"granular"}}}}"#,
        kiln_path.display()
    );
    client.write_all(create_req.as_bytes()).await.unwrap();
    client.write_all(b"\n").await.unwrap();

    let mut buf = vec![0u8; 4096];
    let n = client.read(&mut buf).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
    let session_id = response["result"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let event = SessionEventMessage::text_delta(&session_id, "hello world");
    event_tx.send(event).unwrap();

    // Wait for recording writer flush (500ms interval + margin)
    tokio::time::sleep(Duration::from_millis(700)).await;

    let session_dir = kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&session_id);
    let recording_path = session_dir.join("recording.jsonl");

    assert!(
        recording_path.exists(),
        "recording.jsonl should exist for granular session"
    );

    let content = tokio::fs::read_to_string(&recording_path).await.unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert!(
        lines.len() >= 2,
        "Should have header + at least 1 event, got {} lines",
        lines.len()
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_non_granular_session_has_no_recording_file() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}"}}}}"#,
        kiln_path.display()
    );
    client.write_all(create_req.as_bytes()).await.unwrap();
    client.write_all(b"\n").await.unwrap();

    let mut buf = vec![0u8; 4096];
    let n = client.read(&mut buf).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
    let session_id = response["result"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let event = SessionEventMessage::user_message(&session_id, "msg-1", "hello");
    event_tx.send(event).unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let session_dir = kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&session_id);
    let recording_path = session_dir.join("recording.jsonl");

    assert!(
        !recording_path.exists(),
        "recording.jsonl should NOT exist for non-granular session"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}

#[tokio::test]
async fn test_granular_recording_stops_on_session_end() {
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");
    let kiln_path = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln_path).unwrap();

    let server = Server::bind(&sock_path, None).await.unwrap();
    let event_tx = server.event_sender();
    let shutdown_handle = server.shutdown_handle();
    let server_task = tokio::spawn(server.run());

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = UnixStream::connect(&sock_path).await.unwrap();

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kiln":"{}","recording_mode":"granular"}}}}"#,
        kiln_path.display()
    );
    client.write_all(create_req.as_bytes()).await.unwrap();
    client.write_all(b"\n").await.unwrap();

    let mut buf = vec![0u8; 4096];
    let n = client.read(&mut buf).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
    let session_id = response["result"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let event = SessionEventMessage::text_delta(&session_id, "before end");
    event_tx.send(event).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // End the session
    let end_req = format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"session.end","params":{{"session_id":"{}"}}}}"#,
        session_id
    );
    client.write_all(end_req.as_bytes()).await.unwrap();
    client.write_all(b"\n").await.unwrap();

    buf.fill(0);
    let n = client.read(&mut buf).await.unwrap();
    let end_response = String::from_utf8_lossy(&buf[..n]);
    assert!(
        end_response.contains("\"state\":\"ended\""),
        "Session should be ended: {}",
        end_response
    );

    // Wait for writer to flush footer
    tokio::time::sleep(Duration::from_millis(300)).await;

    let session_dir = kiln_path
        .join(".crucible")
        .join("sessions")
        .join(&session_id);
    let recording_path = session_dir.join("recording.jsonl");
    let content = tokio::fs::read_to_string(&recording_path).await.unwrap();
    let lines: Vec<&str> = content.lines().collect();

    // Last line should be footer with total_events
    let last_line = lines.last().unwrap();
    let footer: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert!(
        footer.get("total_events").is_some(),
        "Footer should have total_events field"
    );

    let _ = shutdown_handle.send(());
    let _ = server_task.await;
}
