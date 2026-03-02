//! Session management for the daemon.
//!
//! Manages active sessions and provides CRUD operations. Sessions are stored
//! in their owning kiln's `.crucible/sessions/` directory.

use crate::session_storage::{FileSessionStorage, SessionStorage};
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::{RecordingMode, Session, SessionState, SessionSummary, SessionType};
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Manages active sessions in the daemon.
///
/// Sessions can be created, listed, paused, resumed, and ended.
/// The manager tracks all active sessions and their state.
/// Sessions are automatically persisted to storage on create and state changes.
pub struct SessionManager {
    sessions: DashMap<String, Session>,
    storage: Arc<dyn SessionStorage>,
    recording_senders: DashMap<String, mpsc::Sender<SessionEventMessage>>,
}

impl SessionManager {
    /// Create a new session manager with default file-based storage.
    pub fn new() -> Self {
        Self::with_storage(Arc::new(FileSessionStorage::new()))
    }

    /// Create a session manager with a custom storage backend.
    pub fn with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            sessions: DashMap::new(),
            storage,
            recording_senders: DashMap::new(),
        }
    }

    /// Create a new session and persist it to storage.
    ///
    /// # Arguments
    /// * `session_type` - The type of session (Chat, Agent, Workflow)
    /// * `kiln` - The kiln path where the session will be stored
    /// * `workspace` - Optional workspace path (defaults to kiln)
    /// * `connected_kilns` - Additional kilns this session can query
    ///
    /// # Returns
    /// The created session, or an error if persistence fails
    pub async fn create_session(
        &self,
        session_type: SessionType,
        kiln: PathBuf,
        workspace: Option<PathBuf>,
        connected_kilns: Vec<PathBuf>,
        recording_mode: Option<RecordingMode>,
    ) -> Result<Session, SessionError> {
        let mut session = Session::new(session_type, kiln);

        if let Some(ws) = workspace {
            session = session.with_workspace(ws);
        }

        if !connected_kilns.is_empty() {
            session = session.with_connected_kilns(connected_kilns);
        }

        if let Some(mode) = recording_mode {
            session = session.with_recording_mode(mode);
        }

        let session_id = session.id.clone();

        // Persist to storage
        self.storage.save(&session).await?;

        // Store in active sessions
        let session_clone = session.clone();
        self.sessions.insert(session_id.clone(), session);

        info!(session_id = %session_id, session_type = %session_clone.session_type, "Session created");
        Ok(session_clone)
    }

    /// Resume a session from storage.
    ///
    /// Loads the session from disk and sets its state to Active.
    /// The session is added to the in-memory session map.
    ///
    /// # Arguments
    /// * `session_id` - The ID of the session to resume
    /// * `kiln` - The kiln path where the session is stored
    ///
    /// # Returns
    /// The resumed session with state set to Active
    pub async fn resume_session_from_storage(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<Session, SessionError> {
        // Load from storage
        let mut session = self.storage.load(session_id, kiln).await?;

        // Update state to Active
        session.resume();

        // Persist updated state
        self.storage.save(&session).await?;

        // Store in memory
        let session_clone = session.clone();
        self.sessions.insert(session.id.clone(), session);

        info!(session_id = %session_id, "Session resumed from storage");
        Ok(session_clone)
    }

    /// Load events from storage with pagination.
    ///
    /// Returns events in chronological order (oldest first).
    pub async fn load_session_events(
        &self,
        session_id: &str,
        kiln: &Path,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        self.storage
            .load_events(session_id, kiln, limit, offset)
            .await
    }

    /// Count total events for a session.
    pub async fn count_session_events(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<usize, SessionError> {
        self.storage.count_events(session_id, kiln).await
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.sessions.get(session_id).map(|r| r.clone())
    }

    pub fn register_transient(&self, session: Session) {
        self.sessions.insert(session.id.clone(), session);
    }

    pub async fn update_session(&self, session: &Session) -> Result<(), SessionError> {
        self.storage.save(session).await?;
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    /// List all active sessions.
    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .map(|r| SessionSummary::from(r.value()))
            .collect()
    }

    /// List sessions filtered by criteria (in-memory only).
    ///
    /// For listing that includes persisted sessions, use `list_sessions_filtered_async`.
    #[allow(dead_code)] // sync counterpart of list_sessions_filtered_async, exercised by tests
    pub fn list_sessions_filtered(
        &self,
        kiln: Option<&PathBuf>,
        workspace: Option<&PathBuf>,
        session_type: Option<SessionType>,
        state: Option<SessionState>,
    ) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .filter(|r| {
                let s = r.value();
                kiln.is_none_or(|k| &s.kiln == k)
                    && workspace.is_none_or(|w| &s.workspace == w)
                    && session_type.is_none_or(|t| s.session_type == t)
                    && state.is_none_or(|st| s.state == st)
            })
            .map(|r| SessionSummary::from(r.value()))
            .collect()
    }

    /// List sessions filtered by criteria, including persisted sessions from storage.
    ///
    /// This merges in-memory sessions with persisted sessions from storage.
    /// In-memory sessions take precedence over storage (they have the latest state).
    pub async fn list_sessions_filtered_async(
        &self,
        kiln: Option<&PathBuf>,
        workspace: Option<&PathBuf>,
        session_type: Option<SessionType>,
        state: Option<SessionState>,
    ) -> Vec<SessionSummary> {
        use std::collections::HashSet;

        let mut results = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        // First, collect in-memory sessions (they have the latest state)
        for entry in self.sessions.iter() {
            let s = entry.value();
            if kiln.is_none_or(|k| &s.kiln == k)
                && workspace.is_none_or(|w| &s.workspace == w)
                && session_type.is_none_or(|t| s.session_type == t)
                && state.is_none_or(|st| s.state == st)
            {
                seen_ids.insert(s.id.clone());
                results.push(SessionSummary::from(s));
            }
        }

        // Then, load persisted sessions from storage (if kiln is specified)
        if let Some(kiln_path) = kiln {
            if let Ok(persisted) = self.storage.list(kiln_path).await {
                for summary in persisted {
                    if seen_ids.contains(&summary.id) {
                        continue;
                    }
                    if workspace.is_none_or(|w| &summary.workspace == w)
                        && session_type.is_none_or(|t| summary.session_type == t)
                        && state.is_none_or(|st| summary.state == st)
                    {
                        results.push(summary);
                    }
                }
            }
        }

        results
    }

    /// Pause a session and persist the state change.
    ///
    /// Returns the previous state if successful.
    pub async fn pause_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
        let (previous, session) = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if entry.state != SessionState::Active {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Active,
                    actual: entry.state,
                });
            }

            let previous = entry.state;
            entry.pause();
            (previous, entry.clone())
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Session paused");
        Ok(previous)
    }

    /// Resume a paused session and persist the state change.
    ///
    /// Returns the previous state if successful.
    pub async fn resume_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
        let (previous, session) = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if entry.state != SessionState::Paused {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Paused,
                    actual: entry.state,
                });
            }

            let previous = entry.state;
            entry.resume();
            (previous, entry.clone())
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Session resumed");
        Ok(previous)
    }

    pub fn set_recording_sender(&self, session_id: &str, tx: mpsc::Sender<SessionEventMessage>) {
        self.recording_senders.insert(session_id.to_string(), tx);
    }

    pub fn get_recording_sender(
        &self,
        session_id: &str,
    ) -> Option<mpsc::Sender<SessionEventMessage>> {
        self.recording_senders.get(session_id).map(|r| r.clone())
    }
    pub async fn end_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if entry.state == SessionState::Ended {
                return Err(SessionError::AlreadyEnded(session_id.to_string()));
            }

            entry.end();
            entry.clone()
        };

        self.storage.save(&session).await?;

        // Drop recording sender to trigger graceful writer shutdown
        self.recording_senders.remove(session_id);
        self.sessions.remove(session_id);
        info!(session_id = %session_id, "Session ended and removed from memory");
        Ok(session)
    }

    /// Request compaction for a session.
    ///
    /// Sets the session state to Compacting. The actual compaction
    /// (summarizing events) is performed by the agent when it sees this state.
    pub async fn request_compaction(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if entry.state != SessionState::Active {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Active,
                    actual: entry.state,
                });
            }

            entry.state = SessionState::Compacting;
            entry.clone()
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Compaction requested");
        Ok(session)
    }

    /// Remove an ended session from memory.
    ///
    /// Returns the session if it was found and ended.
    #[allow(dead_code)] // session lifecycle API, exercised by tests
    pub fn remove_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = self.sessions.get(session_id).map(|r| r.clone());

        match session {
            Some(s) if s.state == SessionState::Ended => {
                self.sessions.remove(session_id);
                debug!(session_id = %session_id, "Session removed from memory");
                Ok(s)
            }
            Some(s) => Err(SessionError::InvalidState {
                expected: SessionState::Ended,
                actual: s.state,
            }),
            None => Err(SessionError::NotFound(session_id.to_string())),
        }
    }

    /// Get the count of active sessions.
    #[allow(dead_code)] // diagnostic API, exercised by tests
    pub fn active_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|r| r.value().state == SessionState::Active)
            .count()
    }

    /// Get the total count of sessions (including paused/ended).
    #[allow(dead_code)] // diagnostic API, exercised by tests
    pub fn total_count(&self) -> usize {
        self.sessions.len()
    }

    /// Update session title and persist the change.
    pub async fn set_title(&self, session_id: &str, title: String) -> Result<(), SessionError> {
        let session = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            entry.title = Some(title);
            entry.clone()
        };

        // Persist updated state
        self.storage.save(&session).await?;
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors that can occur during session operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session already ended: {0}")]
    AlreadyEnded(String),

    #[error("Invalid session state: expected {expected}, got {actual}")]
    InvalidState {
        expected: SessionState,
        actual: SessionState,
    },

    #[error("IO error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for SessionError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err.to_string())
    }
}

impl From<serde_json::Error> for SessionError {
    fn from(err: serde_json::Error) -> Self {
        Self::IoError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
        let filtered = manager.list_sessions_filtered(Some(&kiln1), None, None, None);
        assert_eq!(filtered.len(), 1);

        // Filter by type
        let filtered = manager.list_sessions_filtered(None, None, Some(SessionType::Chat), None);
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
            .list_sessions_filtered_async(Some(&tmp.path().to_path_buf()), None, None, None)
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
            .list_sessions_filtered_async(Some(&tmp.path().to_path_buf()), None, None, None)
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
        async fn append_markdown(
            &self,
            _s: &Session,
            _r: &str,
            _c: &str,
        ) -> Result<(), SessionError> {
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
}
