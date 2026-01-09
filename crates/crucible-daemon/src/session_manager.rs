//! Session management for the daemon.
//!
//! Manages active sessions and provides CRUD operations. Sessions are stored
//! in their owning kiln's `.crucible/sessions/` directory.

use crate::session_storage::{FileSessionStorage, SessionStorage};
use crucible_core::session::{Session, SessionState, SessionSummary, SessionType};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

/// Manages active sessions in the daemon.
///
/// Sessions can be created, listed, paused, resumed, and ended.
/// The manager tracks all active sessions and their state.
/// Sessions are automatically persisted to storage on create and state changes.
pub struct SessionManager {
    /// Active sessions indexed by session ID
    sessions: RwLock<HashMap<String, Session>>,
    /// Storage backend for session persistence
    storage: Arc<dyn SessionStorage>,
}

impl SessionManager {
    /// Create a new session manager with default file-based storage.
    pub fn new() -> Self {
        Self::with_storage(Arc::new(FileSessionStorage::new()))
    }

    /// Create a session manager with a custom storage backend.
    pub fn with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            storage,
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
    ) -> Result<Session, SessionError> {
        let mut session = Session::new(session_type, kiln);

        if let Some(ws) = workspace {
            session = session.with_workspace(ws);
        }

        if !connected_kilns.is_empty() {
            session = session.with_connected_kilns(connected_kilns);
        }

        let session_id = session.id.clone();

        // Persist to storage
        self.storage.save(&session).await?;

        // Store in active sessions
        let session_clone = session.clone();
        {
            let mut sessions = self.sessions.write().unwrap();
            sessions.insert(session_id.clone(), session);
        }

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
        {
            let mut sessions = self.sessions.write().unwrap();
            sessions.insert(session.id.clone(), session);
        }

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
        self.storage.load_events(session_id, kiln, limit, offset).await
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
        let sessions = self.sessions.read().unwrap();
        sessions.get(session_id).cloned()
    }

    /// List all active sessions.
    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        let sessions = self.sessions.read().unwrap();
        sessions.values().map(SessionSummary::from).collect()
    }

    /// List sessions filtered by criteria.
    pub fn list_sessions_filtered(
        &self,
        kiln: Option<&PathBuf>,
        workspace: Option<&PathBuf>,
        session_type: Option<SessionType>,
        state: Option<SessionState>,
    ) -> Vec<SessionSummary> {
        let sessions = self.sessions.read().unwrap();
        sessions
            .values()
            .filter(|s| {
                kiln.map_or(true, |k| &s.kiln == k)
                    && workspace.map_or(true, |w| &s.workspace == w)
                    && session_type.map_or(true, |t| s.session_type == t)
                    && state.map_or(true, |st| s.state == st)
            })
            .map(SessionSummary::from)
            .collect()
    }

    /// Pause a session and persist the state change.
    ///
    /// Returns the previous state if successful.
    pub async fn pause_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
        let (previous, session) = {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if session.state != SessionState::Active {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Active,
                    actual: session.state,
                });
            }

            let previous = session.state;
            session.pause();
            (previous, session.clone())
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
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if session.state != SessionState::Paused {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Paused,
                    actual: session.state,
                });
            }

            let previous = session.state;
            session.resume();
            (previous, session.clone())
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Session resumed");
        Ok(previous)
    }

    /// End a session and persist the state change.
    ///
    /// The session remains in memory with Ended state until explicitly removed.
    pub async fn end_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if session.state == SessionState::Ended {
                return Err(SessionError::AlreadyEnded(session_id.to_string()));
            }

            session.end();
            session.clone()
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Session ended");
        Ok(session)
    }

    /// Request compaction for a session.
    ///
    /// Sets the session state to Compacting. The actual compaction
    /// (summarizing events) is performed by the agent when it sees this state.
    pub async fn request_compaction(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            if session.state != SessionState::Active {
                return Err(SessionError::InvalidState {
                    expected: SessionState::Active,
                    actual: session.state,
                });
            }

            session.state = SessionState::Compacting;
            session.clone()
        };

        // Persist updated state
        self.storage.save(&session).await?;

        info!(session_id = %session_id, "Compaction requested");
        Ok(session)
    }

    /// Remove an ended session from memory.
    ///
    /// Returns the session if it was found and ended.
    pub fn remove_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions.get(session_id).cloned();

        match session {
            Some(s) if s.state == SessionState::Ended => {
                sessions.remove(session_id);
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
    pub fn active_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap();
        sessions
            .values()
            .filter(|s| s.state == SessionState::Active)
            .count()
    }

    /// Get the total count of sessions (including paused/ended).
    pub fn total_count(&self) -> usize {
        let sessions = self.sessions.read().unwrap();
        sessions.len()
    }

    /// Update session title and persist the change.
    pub async fn set_title(&self, session_id: &str, title: String) -> Result<(), SessionError> {
        let session = {
            let mut sessions = self.sessions.write().unwrap();
            let session = sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            session.title = Some(title);
            session.clone()
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_create_session() {
        let tmp = TempDir::new().unwrap();
        let manager = SessionManager::new();
        let session = manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();
        manager
            .create_session(SessionType::Agent, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, kiln1.clone(), None, vec![])
            .await
            .unwrap();
        manager
            .create_session(SessionType::Agent, kiln2.clone(), None, vec![])
            .await
            .unwrap();
        manager
            .create_session(SessionType::Chat, kiln2.clone(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();
        let session_id = session.id.clone();

        let ended = manager.end_session(&session_id).await.unwrap();
        assert_eq!(ended.state, SessionState::Ended);

        // Session still in memory
        assert!(manager.get_session(&session_id).is_some());
    }

    #[tokio::test]
    async fn test_remove_session() {
        let tmp = TempDir::new().unwrap();
        let manager = SessionManager::new();
        let session = manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();
        let session_id = session.id.clone();

        // Can't remove active session
        let err = manager.remove_session(&session_id).unwrap_err();
        assert!(matches!(err, SessionError::InvalidState { .. }));

        // End then remove
        manager.end_session(&session_id).await.unwrap();
        let removed = manager.remove_session(&session_id).unwrap();
        assert_eq!(removed.id, session_id);

        // No longer in memory
        assert!(manager.get_session(&session_id).is_none());
    }

    #[tokio::test]
    async fn test_counts() {
        let tmp = TempDir::new().unwrap();
        let manager = SessionManager::new();

        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.total_count(), 0);

        let session1 = manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();
        let session2 = manager
            .create_session(SessionType::Agent, tmp.path().to_path_buf(), None, vec![])
            .await
            .unwrap();

        assert_eq!(manager.active_count(), 2);
        assert_eq!(manager.total_count(), 2);

        manager.pause_session(&session1.id).await.unwrap();
        assert_eq!(manager.active_count(), 1);
        assert_eq!(manager.total_count(), 2);

        manager.end_session(&session2.id).await.unwrap();
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.total_count(), 2);
    }

    #[tokio::test]
    async fn test_set_title() {
        let tmp = TempDir::new().unwrap();
        let manager = SessionManager::new();
        let session = manager
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
            .create_session(SessionType::Chat, tmp.path().to_path_buf(), None, vec![])
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
}
