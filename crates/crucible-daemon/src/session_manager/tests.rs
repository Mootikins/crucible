//! Tests for the session manager.
//!
//! Split out of session_manager.rs: the tests were 811 of its 1569 lines,
//! putting the file over the 1500-line ceiling CI enforces.

use super::*;
use tempfile::TempDir;

#[tokio::test]
async fn title_sweep_titles_untitled_sessions_with_content() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().to_path_buf();
    let manager = SessionManager::new();
    let session = manager
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();
    manager
        .storage
        .append_event(
            &session,
            r#"{"type":"event","event":"user_message","data":{"content":"how do wikilinks resolve?"},"seq":1}"#,
        )
        .await
        .unwrap();
    // An empty session in the same kiln must stay untitled.
    let empty = manager
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();

    let (tx, mut rx) = tokio::sync::broadcast::channel(8);
    let titled = manager.title_untitled_sessions(&[kiln], &tx).await;

    assert_eq!(titled, 1);
    let updated = manager.get_session(&session.id).unwrap();
    assert_eq!(updated.title.as_deref(), Some("how do wikilinks resolve?"));
    assert!(manager
        .get_session(&empty.id)
        .unwrap()
        .title
        .as_deref()
        .unwrap_or("")
        .is_empty());
    let event = rx.try_recv().unwrap();
    assert_eq!(event.event, "title_changed");
    assert_eq!(event.session_id, session.id);
}

#[tokio::test]
async fn test_create_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    assert!(session.id.starts_with("chat-"));
    assert_eq!(session.session_type, SessionType::Chat);
    assert_eq!(session.kiln, tmp.path());
    assert_eq!(session.workspace, tmp.path());
    assert!(session.connected_kilns.is_empty());
    assert_eq!(session.state, SessionState::Active);
}

#[tokio::test]
async fn test_create_session_with_workspace() {
    let tmp = TempDir::new().unwrap();
    let workspace = tmp.path().join("workspace");
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Agent,
            tmp.path().to_path_buf(),
            Some(workspace.clone()),
            vec![],
            None,
        )
        .await
        .unwrap();

    assert!(session.id.starts_with("agent-"));
    assert_eq!(session.kiln, tmp.path());
    assert_eq!(session.workspace, workspace);
}

#[tokio::test]
async fn test_create_session_with_connected_kilns() {
    let tmp = TempDir::new().unwrap();
    let extra_kiln = tmp.path().join("extra-kiln");
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Workflow,
            tmp.path().to_path_buf(),
            None,
            vec![extra_kiln.clone()],
            None,
        )
        .await
        .unwrap();

    assert!(session.id.starts_with("workflow-"));
    assert_eq!(session.connected_kilns, vec![extra_kiln]);
}

#[tokio::test]
async fn test_create_session_no_workspace_gets_scratch_dir() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();
    let scratch_base = tmp.path().join("workspaces");

    let manager = SessionManager::new().with_session_workspace_dir(Some(scratch_base.clone()));
    let session = manager
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();

    // Workspace is a session-unique scratch dir, not the kiln.
    assert_ne!(session.workspace, kiln);
    assert_eq!(session.workspace, scratch_base.join(&session.id));
    assert!(session.workspace.ends_with(&session.id));
    assert!(session.workspace.is_dir(), "scratch dir should be created");
}

#[tokio::test]
async fn test_create_session_explicit_workspace_ignores_scratch_dir() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    let workspace = tmp.path().join("explicit-workspace");
    let scratch_base = tmp.path().join("workspaces");

    let manager = SessionManager::new().with_session_workspace_dir(Some(scratch_base.clone()));
    let session = manager
        .create_session(
            SessionType::Agent,
            kiln.clone(),
            Some(workspace.clone()),
            vec![],
            None,
        )
        .await
        .unwrap();

    // Explicit workspace wins; no scratch dir is created.
    assert_eq!(session.workspace, workspace);
    assert!(!scratch_base.exists());
}

#[tokio::test]
async fn test_create_session_scratch_dir_failure_falls_back_to_kiln() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().join("kiln");
    std::fs::create_dir_all(&kiln).unwrap();

    // Point the scratch base under a regular file so `create_dir_all`
    // cannot succeed; creation must fall back to `workspace == kiln`.
    let blocker = tmp.path().join("not-a-dir");
    std::fs::write(&blocker, b"x").unwrap();
    let scratch_base = blocker.join("workspaces");

    let manager = SessionManager::new().with_session_workspace_dir(Some(scratch_base));
    let session = manager
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();

    assert_eq!(session.workspace, kiln);
}

#[tokio::test]
async fn test_get_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    let retrieved = manager.get_session(&session_id).unwrap();
    assert_eq!(retrieved.id, session_id);

    assert!(manager.get_session("nonexistent").is_none());
}

#[tokio::test]
async fn test_list_sessions() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    manager
        .create_session(
            SessionType::Agent,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let sessions = manager.list_sessions();
    assert_eq!(sessions.len(), 2);
}

#[tokio::test]
async fn test_list_sessions_filtered() {
    let tmp = TempDir::new().unwrap();
    let kiln1 = tmp.path().join("kiln1");
    let kiln2 = tmp.path().join("kiln2");
    std::fs::create_dir_all(&kiln1).unwrap();
    std::fs::create_dir_all(&kiln2).unwrap();

    let manager = SessionManager::new();
    manager
        .create_session(SessionType::Chat, kiln1.clone(), None, vec![], None)
        .await
        .unwrap();
    manager
        .create_session(SessionType::Agent, kiln2.clone(), None, vec![], None)
        .await
        .unwrap();
    manager
        .create_session(SessionType::Chat, kiln2.clone(), None, vec![], None)
        .await
        .unwrap();

    // Filter by kiln
    let filtered = manager.list_sessions_filtered(Some(&kiln1), None, None, None, true);
    assert_eq!(filtered.len(), 1);

    // Filter by type
    let filtered = manager.list_sessions_filtered(None, None, Some(SessionType::Chat), None, true);
    assert_eq!(filtered.len(), 2);
}

#[tokio::test]
async fn test_pause_resume_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    // Pause
    let prev = manager.pause_session(&session_id).await.unwrap();
    assert_eq!(prev, SessionState::Active);

    let session = manager.get_session(&session_id).unwrap();
    assert_eq!(session.state, SessionState::Paused);

    // Resume
    let prev = manager.resume_session(&session_id).await.unwrap();
    assert_eq!(prev, SessionState::Paused);

    let session = manager.get_session(&session_id).unwrap();
    assert_eq!(session.state, SessionState::Active);
}

#[tokio::test]
async fn test_pause_invalid_state() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    // Pause once
    manager.pause_session(&session_id).await.unwrap();

    // Try to pause again
    let err = manager.pause_session(&session_id).await.unwrap_err();
    assert!(matches!(err, SessionError::InvalidState { .. }));
}

#[tokio::test]
async fn test_end_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    let ended = manager.end_session(&session_id).await.unwrap();
    assert_eq!(ended.state, SessionState::Ended);

    // Session removed from memory after end
    assert!(manager.get_session(&session_id).is_none());
}

#[tokio::test]
async fn test_remove_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    let err = manager.remove_session(&session_id).unwrap_err();
    assert!(matches!(err, SessionError::InvalidState { .. }));

    manager.end_session(&session_id).await.unwrap();

    // end_session already removes from memory
    assert!(manager.get_session(&session_id).is_none());
    let err = manager.remove_session(&session_id).unwrap_err();
    assert!(matches!(err, SessionError::NotFound(_)));
}

#[tokio::test]
async fn test_counts() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();

    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.total_count(), 0);

    let session1 = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session2 = manager
        .create_session(
            SessionType::Agent,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    assert_eq!(manager.active_count(), 2);
    assert_eq!(manager.total_count(), 2);

    manager.pause_session(&session1.id).await.unwrap();
    assert_eq!(manager.active_count(), 1);
    assert_eq!(manager.total_count(), 2);

    manager.end_session(&session2.id).await.unwrap();
    assert_eq!(manager.active_count(), 0);
    assert_eq!(manager.total_count(), 1);
}

#[tokio::test]
async fn test_set_title() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    manager
        .set_title(&session.id, "My Session".to_string())
        .await
        .unwrap();

    let updated = manager.get_session(&session.id).unwrap();
    assert_eq!(updated.title, Some("My Session".to_string()));
}

#[tokio::test]
async fn test_update_last_activity_updates_and_persists_timestamp() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let ts = chrono::Utc::now() + chrono::Duration::hours(1);
    manager.update_last_activity(&session.id, ts).await.unwrap();

    let updated = manager.get_session(&session.id).unwrap();
    assert_eq!(updated.last_activity, Some(ts));

    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert_eq!(persisted.last_activity, Some(ts));
}

#[tokio::test]
async fn test_delete_session() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let session_dir = FileSessionStorage::sessions_base(tmp.path()).join(&session.id);
    assert!(session_dir.exists());

    manager
        .delete_session(&session.id, tmp.path())
        .await
        .unwrap();

    assert!(manager.get_session(&session.id).is_none());
    assert!(!session_dir.exists());
}

#[tokio::test]
async fn test_delete_session_not_found() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();

    let err = manager
        .delete_session("missing-session", tmp.path())
        .await
        .unwrap_err();

    assert!(matches!(err, SessionError::NotFound(_)));
}

#[tokio::test]
async fn test_archive_session_sets_archived_and_keeps_files() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let session_dir = FileSessionStorage::sessions_base(tmp.path()).join(&session.id);
    assert!(session_dir.exists());

    let archived = manager
        .archive_session(&session.id, tmp.path())
        .await
        .unwrap();

    assert!(archived.archived);
    assert!(session_dir.exists());
    assert!(manager.get_session(&session.id).is_none());

    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert!(persisted.archived);
}

#[tokio::test]
async fn test_unarchive_session_sets_archived_false() {
    let tmp = TempDir::new().unwrap();
    let manager = SessionManager::new();
    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    manager
        .archive_session(&session.id, tmp.path())
        .await
        .unwrap();

    let unarchived = manager
        .unarchive_session(&session.id, tmp.path())
        .await
        .unwrap();

    assert!(!unarchived.archived);

    let persisted = FileSessionStorage::new()
        .load(&session.id, tmp.path())
        .await
        .unwrap();
    assert!(!persisted.archived);
}

#[tokio::test]
async fn test_session_manager_persists_on_create() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let manager = SessionManager::with_storage(storage.clone());

    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    // Verify it was persisted
    let loaded = storage.load(&session.id, tmp.path()).await.unwrap();
    assert_eq!(loaded.id, session.id);
}

#[tokio::test]
async fn test_session_manager_resume_from_storage() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());

    // Create a session and save it directly to storage
    let session = Session::new(SessionType::Chat, tmp.path().to_path_buf());
    let session_id = session.id.clone();
    storage.save(&session).await.unwrap();

    // Create manager and resume
    let manager = SessionManager::with_storage(storage);
    let resumed = manager
        .resume_session_from_storage(&session_id, tmp.path())
        .await
        .unwrap();

    assert_eq!(resumed.id, session_id);
    assert_eq!(resumed.state, SessionState::Active);

    // Also available in memory
    assert!(manager.get_session(&session_id).is_some());
}

#[tokio::test]
async fn test_session_manager_persists_state_changes() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());
    let manager = SessionManager::with_storage(storage.clone());

    let session = manager
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    // Pause and verify persisted
    manager.pause_session(&session_id).await.unwrap();
    let loaded = storage.load(&session_id, tmp.path()).await.unwrap();
    assert_eq!(loaded.state, SessionState::Paused);

    // Resume and verify persisted
    manager.resume_session(&session_id).await.unwrap();
    let loaded = storage.load(&session_id, tmp.path()).await.unwrap();
    assert_eq!(loaded.state, SessionState::Active);

    // End and verify persisted
    manager.end_session(&session_id).await.unwrap();
    let loaded = storage.load(&session_id, tmp.path()).await.unwrap();
    assert_eq!(loaded.state, SessionState::Ended);
}

#[tokio::test]
async fn test_list_sessions_includes_persisted_sessions() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());

    let manager1 = SessionManager::with_storage(storage.clone());
    let session = manager1
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    let session_id = session.id.clone();

    manager1.pause_session(&session_id).await.unwrap();
    drop(manager1);

    let manager2 = SessionManager::with_storage(storage);
    let sessions = manager2
        .list_sessions_filtered_async(Some(&tmp.path().to_path_buf()), None, None, None, true)
        .await;

    assert_eq!(
        sessions.len(),
        1,
        "Persisted session should be visible after daemon restart"
    );
    assert_eq!(sessions[0].id, session_id);
}

#[tokio::test]
async fn test_list_sessions_storage_includes_all_states() {
    let tmp = TempDir::new().unwrap();
    let storage = Arc::new(FileSessionStorage::new());

    let manager1 = SessionManager::with_storage(storage.clone());

    let _active_session = manager1
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();

    let paused_session = manager1
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    manager1.pause_session(&paused_session.id).await.unwrap();

    let _ended_session = manager1
        .create_session(
            SessionType::Chat,
            tmp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .unwrap();
    manager1.end_session(&_ended_session.id).await.unwrap();

    drop(manager1);
    let manager2 = SessionManager::with_storage(storage);

    let sessions = manager2
        .list_sessions_filtered_async(Some(&tmp.path().to_path_buf()), None, None, None, true)
        .await;
    assert_eq!(
        sessions.len(),
        3,
        "All persisted sessions should be visible"
    );

    let paused = manager2
        .list_sessions_filtered_async(
            Some(&tmp.path().to_path_buf()),
            None,
            None,
            Some(SessionState::Paused),
            true,
        )
        .await;
    assert_eq!(paused.len(), 1);
    assert_eq!(paused[0].id, paused_session.id);
}

use async_trait::async_trait;

struct FailingSaveStorage;

#[async_trait]
impl SessionStorage for FailingSaveStorage {
    async fn save(&self, _session: &Session) -> Result<(), SessionError> {
        Err(SessionError::IoError("simulated disk failure".to_string()))
    }
    async fn load(&self, _id: &str, _kiln: &Path) -> Result<Session, SessionError> {
        Err(SessionError::NotFound("not impl".to_string()))
    }
    async fn list(&self, _kiln: &Path) -> Result<Vec<SessionSummary>, SessionError> {
        Ok(vec![])
    }
    async fn append_event(&self, _s: &Session, _e: &str) -> Result<(), SessionError> {
        Ok(())
    }
    async fn append_markdown(&self, _s: &Session, _r: &str, _c: &str) -> Result<(), SessionError> {
        Ok(())
    }
    async fn load_events(
        &self,
        _id: &str,
        _kiln: &Path,
        _limit: Option<usize>,
        _offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        Ok(vec![])
    }
    async fn count_events(&self, _id: &str, _kiln: &Path) -> Result<usize, SessionError> {
        Ok(0)
    }
}

#[tokio::test]
async fn test_update_session_does_not_modify_memory_on_storage_failure() {
    let storage = Arc::new(FailingSaveStorage);
    let manager = SessionManager::with_storage(storage);

    let mut session = Session::new(SessionType::Chat, PathBuf::from("/tmp/test-kiln"));
    session.title = Some("Original Title".to_string());
    let session_id = session.id.clone();
    manager.sessions.insert(session_id.clone(), session.clone());

    let mut modified = session.clone();
    modified.title = Some("Updated Title".to_string());

    let result = manager.update_session(&modified).await;
    assert!(
        result.is_err(),
        "update_session should fail when storage fails"
    );

    let in_memory = manager.get_session(&session_id).unwrap();
    assert_eq!(
        in_memory.title,
        Some("Original Title".to_string()),
        "In-memory session should retain original title when storage fails"
    );
}

/// Storage that records the order of `save` calls and can hold one of them
/// open, so the interleaving under test is forced rather than raced for.
struct GatedSaveStorage {
    inner: FileSessionStorage,
    /// Saves observed, in completion order, as `(session_id, state)`.
    order: std::sync::Mutex<Vec<(String, SessionState)>>,
    /// Blocks the first save that arrives while it is `Some`.
    gate: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

#[async_trait]
impl SessionStorage for GatedSaveStorage {
    async fn save(&self, session: &Session) -> Result<(), SessionError> {
        let held = self.gate.lock().unwrap().take();
        if let Some(rx) = held {
            let _ = rx.await;
        }
        self.inner.save(session).await?;
        self.order
            .lock()
            .unwrap()
            .push((session.id.clone(), session.state));
        Ok(())
    }
    async fn load(&self, id: &str, kiln: &Path) -> Result<Session, SessionError> {
        self.inner.load(id, kiln).await
    }
    async fn list(&self, kiln: &Path) -> Result<Vec<SessionSummary>, SessionError> {
        self.inner.list(kiln).await
    }
    async fn append_event(&self, s: &Session, e: &str) -> Result<(), SessionError> {
        self.inner.append_event(s, e).await
    }
    async fn append_markdown(&self, s: &Session, r: &str, c: &str) -> Result<(), SessionError> {
        self.inner.append_markdown(s, r, c).await
    }
    async fn load_events(
        &self,
        id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        self.inner.load_events(id, kiln, limit, offset).await
    }
    async fn count_events(&self, id: &str, kiln: &Path) -> Result<usize, SessionError> {
        self.inner.count_events(id, kiln).await
    }
}

/// The persist task refreshes `last_activity` on every session event, and it
/// clones the session out of the map before awaiting the write. When
/// `session.end` lands in that window, the stale `Active` clone must not be
/// the last thing written to `meta.json` — otherwise `session.get` reports the
/// session gone while `session.list` keeps offering it as active.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ending_a_session_outlasts_a_concurrent_last_activity_persist() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path().to_path_buf();
    let (release, gate) = tokio::sync::oneshot::channel();
    let storage = Arc::new(GatedSaveStorage {
        inner: FileSessionStorage::new(),
        order: std::sync::Mutex::new(Vec::new()),
        gate: std::sync::Mutex::new(None),
    });
    let manager = Arc::new(SessionManager::with_storage(storage.clone()));

    let session = manager
        .create_session(SessionType::Chat, kiln.clone(), None, vec![], None)
        .await
        .unwrap();
    let id = session.id.clone();

    // Arm the gate only now, so it catches the last-activity save rather than
    // the create.
    *storage.gate.lock().unwrap() = Some(gate);

    let activity = tokio::spawn({
        let manager = manager.clone();
        let id = id.clone();
        async move { manager.update_last_activity(&id, Utc::now()).await }
    });
    // Let the persist task reach the gate holding its Active clone.
    while storage.gate.lock().unwrap().is_some() {
        tokio::task::yield_now().await;
    }

    let ending = tokio::spawn({
        let manager = manager.clone();
        let id = id.clone();
        async move { manager.end_session(&id).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let _ = release.send(());

    activity.await.unwrap().unwrap();
    ending.await.unwrap().unwrap();

    let persisted = manager.storage.load(&id, &kiln).await.unwrap();
    assert_eq!(
        persisted.state,
        SessionState::Ended,
        "last write to meta.json was {:?}",
        storage.order.lock().unwrap()
    );

    let active = manager
        .list_sessions_filtered_async(
            Some(&kiln),
            None,
            Some(SessionType::Chat),
            Some(SessionState::Active),
            false,
        )
        .await;
    assert!(
        !active.iter().any(|s| s.id == id),
        "ended session reappeared in the active list"
    );
}
