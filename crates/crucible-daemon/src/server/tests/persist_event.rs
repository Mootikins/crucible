use super::*;
use crate::session_manager::SessionError;
use crate::session_storage::SessionStorage;
use crate::test_support::temp_session_manager;
use async_trait::async_trait;
use crucible_core::session::{SessionSummary, SessionType};

/// Minimal AgentManager for exercising the sweep's cleanup call.
fn sweep_test_agent_manager() -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    let session_manager = temp_session_manager();
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: None,
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

struct FailingStorage(PathBuf);

#[async_trait]
impl SessionStorage for FailingStorage {
    fn sessions_root(&self) -> &Path {
        &self.0
    }
    async fn save(&self, _s: &crucible_core::session::Session) -> Result<(), SessionError> {
        Ok(())
    }
    async fn load(&self, _id: &str) -> Result<crucible_core::session::Session, SessionError> {
        Err(SessionError::NotFound("mock".to_string()))
    }
    async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
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
        _limit: Option<usize>,
        _offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        Ok(vec![])
    }
    async fn count_events(&self, _id: &str) -> Result<usize, SessionError> {
        Ok(0)
    }
}

#[tokio::test]
async fn test_persist_event_returns_error_on_storage_failure() {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let event = SessionEventMessage::new(
        session.id.clone(),
        "user_message",
        serde_json::json!({"content": "hello"}),
    );

    let storage = FailingStorage(sm.sessions_root().to_path_buf());
    let result = persist_event(&event, &sm, &storage).await;
    assert!(
        result.is_err(),
        "persist_event must propagate storage errors, not swallow them"
    );
}

#[tokio::test]
async fn test_persist_event_skips_non_persistent_events() {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let event = SessionEventMessage::new(
        session.id.clone(),
        "stream_chunk",
        serde_json::json!({"chunk": "partial"}),
    );

    let storage = FailingStorage(sm.sessions_root().to_path_buf());
    let result = persist_event(&event, &sm, &storage).await;
    assert!(
        result.is_ok(),
        "Non-persistent events should be skipped without error"
    );
}

/// Recording that precognition *ran* is useless without what it injected —
/// the badge lists the notes by name. Pins the whole payload through the
/// storage round-trip.
#[tokio::test]
async fn test_persist_event_keeps_precognition_notes() {
    use std::sync::Mutex;

    struct CapturingStorage(Mutex<Vec<String>>, PathBuf);

    #[async_trait]
    impl SessionStorage for CapturingStorage {
        fn sessions_root(&self) -> &Path {
            &self.1
        }
        async fn save(&self, _s: &crucible_core::session::Session) -> Result<(), SessionError> {
            Ok(())
        }
        async fn load(&self, _id: &str) -> Result<crucible_core::session::Session, SessionError> {
            Err(SessionError::NotFound("mock".to_string()))
        }
        async fn list(&self) -> Result<Vec<SessionSummary>, SessionError> {
            Ok(vec![])
        }
        async fn append_event(
            &self,
            _s: &crucible_core::session::Session,
            e: &str,
        ) -> Result<(), SessionError> {
            self.0.lock().unwrap().push(e.to_string());
            Ok(())
        }
        async fn append_markdown(
            &self,
            _s: &crucible_core::session::Session,
            _r: &str,
            _c: &str,
        ) -> Result<(), SessionError> {
            Ok(())
        }
        async fn load_events(
            &self,
            _id: &str,
            _limit: Option<usize>,
            _offset: Option<usize>,
        ) -> Result<Vec<serde_json::Value>, SessionError> {
            Ok(vec![])
        }
        async fn count_events(&self, _id: &str) -> Result<usize, SessionError> {
            Ok(0)
        }
    }

    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    let event = SessionEventMessage::new(
        session.id.clone(),
        "precognition_complete",
        serde_json::json!({
            "notes_count": 2,
            "query_summary": "tell me about the kiln",
            "notes": [
                {"title": "Kilns", "kiln_label": "docs", "score": 0.91},
                {"title": "Wikilinks", "kiln_label": "docs", "score": 0.72},
            ],
        }),
    );

    let storage = CapturingStorage(Mutex::new(Vec::new()), sm.sessions_root().to_path_buf());
    persist_event(&event, &sm, &storage).await.unwrap();

    let written = storage.0.lock().unwrap();
    assert_eq!(written.len(), 1, "precognition_complete must be written");
    let parsed: serde_json::Value = serde_json::from_str(&written[0]).unwrap();
    assert_eq!(parsed["event"], "precognition_complete");
    assert_eq!(parsed["data"]["notes_count"], 2);
    let notes = parsed["data"]["notes"].as_array().expect("notes survive");
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0]["title"], "Kilns");
    assert_eq!(notes[0]["kiln_label"], "docs");
}

#[tokio::test]
async fn test_should_persist_filters_correctly() {
    let persistent = [
        "user_message",
        "thinking",
        "segment_complete",
        "message_complete",
        "tool_call",
        "tool_result",
        "model_switched",
        "ended",
        // Without this the precognition badge cannot survive a reload: the
        // event is broadcast live and then lost, so a resumed transcript has
        // no record of what context was injected.
        "precognition_complete",
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

/// A session's starting model is emitted (`session_initialized.model`) but was
/// never persisted, while `model_switched` was — so a session that switched
/// models had an attributable second half and an unattributable first half.
///
/// It is persisted only once the model is known. The setup task runs before
/// `session.configure_agent` and "almost always observes `None`", so
/// `assets/fixtures/demo.jsonl` records `"model":""`; persisting that would look
/// like an answer.
#[tokio::test]
async fn a_session_initialized_is_persisted_once_the_model_is_known() {
    let payload = |model: &str| {
        serde_json::json!({
            "model": model,
            "mode": "normal",
            "agent_name": null,
            "kiln_path": "/k",
            "workspace_path": "/w",
        })
    };
    assert!(should_persist(&SessionEventMessage::new(
        "test",
        "session_initialized",
        payload("glm-5"),
    )));
    assert!(!should_persist(&SessionEventMessage::new(
        "test",
        "session_initialized",
        payload(""),
    )));
}

#[tokio::test]
async fn test_sweep_and_archive_stale_sessions_archives_inactive_sessions_without_subscribers() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    assert!(session_manager.get_session(&session.id).is_none());

    let persisted = FileSessionStorage::new(session_manager.sessions_root().to_path_buf())
        .load(&session.id)
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_sweep_cleans_up_agent_state_for_archived_sessions() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
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
/// The sweep is what frees an ended session now, so it has to actually collect
/// one.
///
/// `end_session` deliberately leaves the session resident — evicting there
/// dropped the turn's in-flight events — which means nothing reclaims it except
/// this sweep. Without this test the retention has no proven upper bound: a
/// candidate filter that skipped `Ended` would turn "resident until the sweep"
/// into "resident forever" and no test would notice.
#[tokio::test]
async fn the_sweep_reclaims_a_stale_ended_session_that_is_still_resident() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
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
        session_manager.get_session(&session.id).is_some(),
        "precondition: ending keeps the session resident"
    );

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1, "a stale ended session must be archived");
    assert!(
        session_manager.get_session(&session.id).is_none(),
        "and evicted: the sweep is the only thing that frees it now"
    );
    let persisted = FileSessionStorage::new(session_manager.sessions_root().to_path_buf())
        .load(&session.id)
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_sweep_archives_stale_persisted_sessions_not_in_memory() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();
    session_manager
        .update_last_activity(&session.id, Utc::now() - ChronoDuration::hours(80))
        .await
        .unwrap();
    session_manager.end_session(&session.id).await.unwrap();
    // Ending no longer evicts (it dropped in-flight events); `remove_session` is
    // the eviction verb, and evicting is what this test is about.
    session_manager
        .remove_session(&session.id)
        .expect("an ended session may be evicted");
    assert!(
        session_manager.get_session(&session.id).is_none(),
        "precondition: this test is about a session that is not in memory"
    );

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
        &subscription_manager,
        &sweep_test_agent_manager(),
        72,
    )
    .await
    .unwrap();

    assert_eq!(archived, 1);
    let persisted = FileSessionStorage::new(session_manager.sessions_root().to_path_buf())
        .load(&session.id)
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_sweep_and_archive_stale_sessions_skips_sessions_with_active_subscribers() {
    let tmp = TempDir::new().unwrap();
    let session_manager = temp_session_manager();
    let subscription_manager = SubscriptionManager::new();

    let session = session_manager
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
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

    let archived = sweep_and_archive_stale_sessions(
        &session_manager,
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
    let server = TestServer::start().await;
    let mut client = server.connect().await;
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

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_create_default_no_recording_mode() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;
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

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_get_includes_recording_mode() {
    let server = TestServer::start().await;
    let mut client = server.connect().await;

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

    server.shutdown().await;
}

#[tokio::test]
async fn test_granular_session_creates_recording_file() {
    use std::time::Duration;

    let server = TestServer::start().await;
    let kiln_path = server.kiln_path.clone();
    let event_tx = server.event_tx.clone();
    let mut client = server.connect().await;

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kilns":["{}"],"recording_mode":"granular"}}}}"#,
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

    let session_dir = server.sessions_root().join(&session_id);
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

    server.shutdown().await;
}

#[tokio::test]
async fn test_non_granular_session_has_no_recording_file() {
    use std::time::Duration;

    let server = TestServer::start().await;
    let kiln_path = server.kiln_path.clone();
    let event_tx = server.event_tx.clone();
    let mut client = server.connect().await;

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kilns":["{}"]}}}}"#,
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

    let session_dir = server.sessions_root().join(&session_id);
    let recording_path = session_dir.join("recording.jsonl");

    assert!(
        !recording_path.exists(),
        "recording.jsonl should NOT exist for non-granular session"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_granular_recording_stops_on_session_end() {
    use std::time::Duration;

    let server = TestServer::start().await;
    let kiln_path = server.kiln_path.clone();
    let event_tx = server.event_tx.clone();
    let mut client = server.connect().await;

    let create_req = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"session.create","params":{{"type":"chat","kilns":["{}"],"recording_mode":"granular"}}}}"#,
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

    let session_dir = server.sessions_root().join(&session_id);
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

    server.shutdown().await;
}

/// The turn's last events arrive after `session.end` has already returned.
///
/// This is the flake that made `just test gated` red about one run in ten under
/// CPU contention and never once in isolation: `end_session` used to evict the
/// session from the in-memory map as soon as its RPC completed, and the persist
/// task — one `await` behind — then found `get_session` empty and returned
/// `Ok(())` without writing. The transcript came out missing whichever events had
/// not been drained yet, so `cru session show` reported a turn that never
/// grounded itself, and once the whole `session.jsonl` was absent.
///
/// Asserted at the seam rather than through a real turn, because the race is only
/// reachable by timing: driving the daemon cannot pin it, and a test that needs
/// load to fail is the flake again with extra steps. Ending the session first
/// reproduces the *state* the race produced, deterministically.
#[tokio::test]
async fn an_event_that_arrives_after_the_session_ended_is_still_written() {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();

    sm.end_session(&session.id).await.unwrap();

    let storage = FileSessionStorage::new(sm.sessions_root().to_path_buf());
    let event = SessionEventMessage::new(
        session.id.clone(),
        "precognition_complete",
        serde_json::json!({"notes_count": 1, "query_summary": "what is a kiln?"}),
    );
    persist_event(&event, &sm, &storage).await.unwrap();

    let persisted = storage.load_events(&session.id, None, None).await.unwrap();
    let events: Vec<&str> = persisted
        .iter()
        .filter_map(|e| e["event"].as_str())
        .collect();
    assert!(
        events.contains(&"precognition_complete"),
        "a late event must still reach session.jsonl; got {events:?}"
    );
}

/// …but only for a session this daemon can still account for. A deleted
/// session's retained kiln is cleared, and re-creating its transcript from a
/// straggling event would resurrect a file the user asked to be gone.
#[tokio::test]
async fn an_event_for_a_deleted_session_is_dropped_rather_than_recreating_it() {
    let tmp = TempDir::new().unwrap();
    let sm = temp_session_manager();
    let session = sm
        .create_session(
            SessionType::Chat,
            vec![tmp.path().to_path_buf()],
            None,
            None,
        )
        .await
        .unwrap();
    sm.delete_session(&session.id).await.unwrap();

    let storage = FileSessionStorage::new(sm.sessions_root().to_path_buf());
    let event = SessionEventMessage::new(
        session.id.clone(),
        "precognition_complete",
        serde_json::json!({"notes_count": 1}),
    );
    persist_event(&event, &sm, &storage).await.unwrap();

    let persisted = storage.load_events(&session.id, None, None).await.unwrap();
    assert!(
        persisted.is_empty(),
        "a deleted session must stay deleted; got {persisted:?}"
    );
}
