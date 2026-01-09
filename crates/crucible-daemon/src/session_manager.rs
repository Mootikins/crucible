//! Session management for the daemon.
//!
//! Manages active sessions and provides CRUD operations. Sessions are stored
//! in their owning kiln's `.crucible/sessions/` directory.

use crucible_core::session::{Session, SessionState, SessionSummary, SessionType};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, info};

/// Manages active sessions in the daemon.
///
/// Sessions can be created, listed, paused, resumed, and ended.
/// The manager tracks all active sessions and their state.
pub struct SessionManager {
    /// Active sessions indexed by session ID
    sessions: RwLock<HashMap<String, Session>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Create a new session.
    ///
    /// # Arguments
    /// * `session_type` - The type of session (Chat, Agent, Workflow)
    /// * `kiln` - The kiln path where the session will be stored
    /// * `workspace` - Optional workspace path (defaults to kiln)
    /// * `connected_kilns` - Additional kilns this session can query
    ///
    /// # Returns
    /// The created session
    pub fn create_session(
        &self,
        session_type: SessionType,
        kiln: PathBuf,
        workspace: Option<PathBuf>,
        connected_kilns: Vec<PathBuf>,
    ) -> Session {
        let mut session = Session::new(session_type, kiln);

        if let Some(ws) = workspace {
            session = session.with_workspace(ws);
        }

        if !connected_kilns.is_empty() {
            session = session.with_connected_kilns(connected_kilns);
        }

        let session_id = session.id.clone();

        // Store in active sessions
        {
            let mut sessions = self.sessions.write().unwrap();
            sessions.insert(session_id.clone(), session.clone());
        }

        info!(session_id = %session_id, session_type = %session.session_type, "Session created");
        session
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

    /// Pause a session.
    ///
    /// Returns the previous state if successful.
    pub fn pause_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
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
        info!(session_id = %session_id, "Session paused");
        Ok(previous)
    }

    /// Resume a paused session.
    ///
    /// Returns the previous state if successful.
    pub fn resume_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
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
        info!(session_id = %session_id, "Session resumed");
        Ok(previous)
    }

    /// End a session.
    ///
    /// The session remains in memory with Ended state until explicitly removed.
    pub fn end_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .get_mut(session_id)
            .ok_or(SessionError::NotFound(session_id.to_string()))?;

        if session.state == SessionState::Ended {
            return Err(SessionError::AlreadyEnded(session_id.to_string()));
        }

        session.end();
        info!(session_id = %session_id, "Session ended");
        Ok(session.clone())
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

    /// Update session title.
    pub fn set_title(&self, session_id: &str, title: String) -> Result<(), SessionError> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .get_mut(session_id)
            .ok_or(SessionError::NotFound(session_id.to_string()))?;

        session.title = Some(title);
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
    use std::path::PathBuf;

    fn test_kiln() -> PathBuf {
        PathBuf::from("/tmp/test-kiln")
    }

    #[test]
    fn test_create_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);

        assert!(session.id.starts_with("chat-"));
        assert_eq!(session.session_type, SessionType::Chat);
        assert_eq!(session.kiln, test_kiln());
        assert_eq!(session.workspace, test_kiln());
        assert!(session.connected_kilns.is_empty());
        assert_eq!(session.state, SessionState::Active);
    }

    #[test]
    fn test_create_session_with_workspace() {
        let manager = SessionManager::new();
        let workspace = PathBuf::from("/tmp/workspace");
        let session = manager.create_session(
            SessionType::Agent,
            test_kiln(),
            Some(workspace.clone()),
            vec![],
        );

        assert!(session.id.starts_with("agent-"));
        assert_eq!(session.kiln, test_kiln());
        assert_eq!(session.workspace, workspace);
    }

    #[test]
    fn test_create_session_with_connected_kilns() {
        let manager = SessionManager::new();
        let extra_kiln = PathBuf::from("/tmp/extra-kiln");
        let session =
            manager.create_session(SessionType::Workflow, test_kiln(), None, vec![extra_kiln.clone()]);

        assert!(session.id.starts_with("workflow-"));
        assert_eq!(session.connected_kilns, vec![extra_kiln]);
    }

    #[test]
    fn test_get_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session_id = session.id.clone();

        let retrieved = manager.get_session(&session_id).unwrap();
        assert_eq!(retrieved.id, session_id);

        assert!(manager.get_session("nonexistent").is_none());
    }

    #[test]
    fn test_list_sessions() {
        let manager = SessionManager::new();
        manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        manager.create_session(SessionType::Agent, test_kiln(), None, vec![]);

        let sessions = manager.list_sessions();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_list_sessions_filtered() {
        let manager = SessionManager::new();
        let kiln1 = PathBuf::from("/tmp/kiln1");
        let kiln2 = PathBuf::from("/tmp/kiln2");

        manager.create_session(SessionType::Chat, kiln1.clone(), None, vec![]);
        manager.create_session(SessionType::Agent, kiln2.clone(), None, vec![]);
        manager.create_session(SessionType::Chat, kiln2.clone(), None, vec![]);

        // Filter by kiln
        let filtered = manager.list_sessions_filtered(Some(&kiln1), None, None, None);
        assert_eq!(filtered.len(), 1);

        // Filter by type
        let filtered = manager.list_sessions_filtered(None, None, Some(SessionType::Chat), None);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_pause_resume_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session_id = session.id.clone();

        // Pause
        let prev = manager.pause_session(&session_id).unwrap();
        assert_eq!(prev, SessionState::Active);

        let session = manager.get_session(&session_id).unwrap();
        assert_eq!(session.state, SessionState::Paused);

        // Resume
        let prev = manager.resume_session(&session_id).unwrap();
        assert_eq!(prev, SessionState::Paused);

        let session = manager.get_session(&session_id).unwrap();
        assert_eq!(session.state, SessionState::Active);
    }

    #[test]
    fn test_pause_invalid_state() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session_id = session.id.clone();

        // Pause once
        manager.pause_session(&session_id).unwrap();

        // Try to pause again
        let err = manager.pause_session(&session_id).unwrap_err();
        assert!(matches!(err, SessionError::InvalidState { .. }));
    }

    #[test]
    fn test_end_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session_id = session.id.clone();

        let ended = manager.end_session(&session_id).unwrap();
        assert_eq!(ended.state, SessionState::Ended);

        // Session still in memory
        assert!(manager.get_session(&session_id).is_some());
    }

    #[test]
    fn test_remove_session() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session_id = session.id.clone();

        // Can't remove active session
        let err = manager.remove_session(&session_id).unwrap_err();
        assert!(matches!(err, SessionError::InvalidState { .. }));

        // End then remove
        manager.end_session(&session_id).unwrap();
        let removed = manager.remove_session(&session_id).unwrap();
        assert_eq!(removed.id, session_id);

        // No longer in memory
        assert!(manager.get_session(&session_id).is_none());
    }

    #[test]
    fn test_counts() {
        let manager = SessionManager::new();

        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.total_count(), 0);

        let session1 = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);
        let session2 = manager.create_session(SessionType::Agent, test_kiln(), None, vec![]);

        assert_eq!(manager.active_count(), 2);
        assert_eq!(manager.total_count(), 2);

        manager.pause_session(&session1.id).unwrap();
        assert_eq!(manager.active_count(), 1);
        assert_eq!(manager.total_count(), 2);

        manager.end_session(&session2.id).unwrap();
        assert_eq!(manager.active_count(), 0);
        assert_eq!(manager.total_count(), 2);
    }

    #[test]
    fn test_set_title() {
        let manager = SessionManager::new();
        let session = manager.create_session(SessionType::Chat, test_kiln(), None, vec![]);

        manager
            .set_title(&session.id, "My Session".to_string())
            .unwrap();

        let updated = manager.get_session(&session.id).unwrap();
        assert_eq!(updated.title, Some("My Session".to_string()));
    }
}
