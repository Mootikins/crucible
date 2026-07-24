//! Session management for the daemon.
//!
//! Manages active sessions and provides CRUD operations. Sessions are stored
//! in their owning kiln's `.crucible/sessions/` directory.

use crate::session_storage::{FileSessionStorage, SessionStorage};
use chrono::{DateTime, Utc};
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
    /// Last-known kiln for every session this manager has seen, kept even
    /// after the session leaves the in-memory `sessions` map (end/eviction).
    /// This is how a transparent revive resolves which kiln's storage to load
    /// from without depending on the kiln being currently open. Cleared only
    /// when a session is deleted.
    session_kilns: DashMap<String, PathBuf>,
    /// Base directory under which per-session scratch workspaces are created
    /// for sessions started without an explicit workspace. When `None`, such
    /// sessions fall back to `workspace == kiln` (the historical behavior).
    /// Resolved and tilde-expanded at construction (see
    /// [`crate::scm::resolve_session_workspace_dir`]).
    session_workspace_dir: Option<PathBuf>,
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
            session_kilns: DashMap::new(),
            session_workspace_dir: None,
        }
    }

    /// Set the base directory for per-session scratch workspaces.
    ///
    /// When set, sessions created without an explicit workspace get a
    /// session-unique `<dir>/<session_id>` workspace instead of falling back to
    /// the kiln path. The directory should already be tilde-expanded.
    #[must_use]
    pub fn with_session_workspace_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.session_workspace_dir = dir;
        self
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
        } else if let Some(base) = &self.session_workspace_dir {
            // No explicit workspace: give the session its own scratch workspace
            // so its filesystem containment boundary is a private, session-unique
            // directory rather than the shared kiln path. Created BEFORE the
            // session is persisted/used so the path canonicalizes when trust and
            // containment are derived. On failure, fall back to the historical
            // `workspace == kiln` behavior — never fail session creation over a
            // scratch directory.
            let scratch = base.join(&session.id);
            match std::fs::create_dir_all(&scratch) {
                Ok(()) => {
                    session = session.with_workspace(scratch);
                }
                Err(e) => {
                    tracing::warn!(
                        path = %scratch.display(),
                        error = %e,
                        "Failed to create session scratch workspace; falling back to kiln path"
                    );
                }
            }
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
        self.session_kilns
            .insert(session_id.clone(), session_clone.kiln.clone());
        self.sessions.insert(session_id.clone(), session);

        info!(session_id = %session_id, session_type = %session_clone.session_type, "Session created");
        Ok(session_clone)
    }

    /// Create a delegated child session of `parent`.
    ///
    /// The child inherits the parent's kiln, workspace, and connected kilns,
    /// carries `parent_session_id`, and is created with its agent config
    /// already set (children never go through `configure_agent`). Children
    /// are full sessions in behavior but are hidden from default listings
    /// and lifecycle-subordinate to their parent.
    pub async fn create_child_session(
        &self,
        parent: &Session,
        agent: crucible_core::session::SessionAgent,
        title: Option<String>,
    ) -> Result<Session, SessionError> {
        let mut session = Session::new(SessionType::Agent, parent.kiln.clone())
            .with_workspace(parent.workspace.clone())
            .with_connected_kilns(parent.connected_kilns.clone())
            .with_parent(parent.id.clone());
        session.agent = Some(agent);
        session.title = title;

        let session_id = session.id.clone();
        self.storage.save(&session).await?;
        let session_clone = session.clone();
        self.session_kilns
            .insert(session_id.clone(), session_clone.kiln.clone());
        self.sessions.insert(session_id.clone(), session);

        info!(
            session_id = %session_id,
            parent_session_id = %parent.id,
            "Child session created"
        );
        Ok(session_clone)
    }

    /// Ids of persisted child sessions of `parent_id` in `kiln`. Used by the
    /// archive/delete cascades: children are lifecycle-subordinate to their
    /// parent and must not outlive it in listings.
    pub async fn child_session_ids(&self, parent_id: &str, kiln: &Path) -> Vec<String> {
        self.storage
            .list(kiln)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.parent_session_id.as_deref() == Some(parent_id))
            .map(|s| s.id)
            .collect()
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

        // Always-resumable: a session loaded from storage becomes live
        // regardless of its persisted lifecycle state. `Session::resume()`
        // only lifts `Paused`, so set the state directly — an `Ended`,
        // `Paused`, or `Compacting` session all revive to `Active`.
        session.state = SessionState::Active;

        // Persist updated state
        self.storage.save(&session).await?;

        // Store in memory
        let session_clone = session.clone();
        self.session_kilns
            .insert(session.id.clone(), session.kiln.clone());
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
        self.session_kilns
            .insert(session.id.clone(), session.kiln.clone());
        self.sessions.insert(session.id.clone(), session);
    }

    pub async fn update_session(&self, session: &Session) -> Result<(), SessionError> {
        self.storage.save(session).await?;
        self.session_kilns
            .insert(session.id.clone(), session.kiln.clone());
        self.sessions.insert(session.id.clone(), session.clone());
        Ok(())
    }

    /// Last-known kiln for a session, even after it has left the in-memory
    /// `sessions` map (ended or evicted). Used by the send path to resolve
    /// which kiln's storage to revive an idle session from. Returns `None`
    /// only for sessions this manager has never seen (e.g. after a daemon
    /// restart), where the caller must fall back to another kiln source.
    pub fn session_kiln(&self, session_id: &str) -> Option<PathBuf> {
        self.session_kilns.get(session_id).map(|r| r.clone())
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
        include_archived: bool,
    ) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .filter(|r| {
                let s = r.value();
                kiln.is_none_or(|k| &s.kiln == k)
                    && workspace.is_none_or(|w| &s.workspace == w)
                    && session_type.is_none_or(|t| s.session_type == t)
                    && state.is_none_or(|st| s.state == st)
                    && (include_archived || !s.archived)
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
        include_archived: bool,
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
                && (include_archived || !s.archived)
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
                        && (include_archived || !summary.archived)
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

    pub async fn delete_session(&self, session_id: &str, kiln: &Path) -> Result<(), SessionError> {
        let was_in_memory = self.sessions.get(session_id).is_some();

        if let Some(session) = self.get_session(session_id) {
            if session.state != SessionState::Ended {
                self.end_session(session_id).await?;
            }
        }

        self.sessions.remove(session_id);
        self.recording_senders.remove(session_id);
        self.session_kilns.remove(session_id);

        let session_dir = FileSessionStorage::sessions_base(kiln).join(session_id);
        let persisted_exists = session_dir.exists();

        if !was_in_memory && !persisted_exists {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        if persisted_exists {
            tokio::fs::remove_dir_all(&session_dir).await?;
        }

        info!(session_id = %session_id, kiln = %kiln.display(), "Session deleted");
        Ok(())
    }

    pub async fn archive_session(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<Session, SessionError> {
        if let Some(session) = self.get_session(session_id) {
            if matches!(session.state, SessionState::Active | SessionState::Paused) {
                self.end_session(session_id).await?;
            }
        }

        let session_dir = FileSessionStorage::sessions_base(kiln).join(session_id);
        let meta_path = session_dir.join("meta.json");
        let legacy_path = session_dir.join("session.json");

        let source_path = if tokio::fs::metadata(&meta_path).await.is_ok() {
            meta_path.clone()
        } else if tokio::fs::metadata(&legacy_path).await.is_ok() {
            legacy_path
        } else {
            return Err(SessionError::NotFound(session_id.to_string()));
        };

        let mut session: Session =
            serde_json::from_str(&tokio::fs::read_to_string(&source_path).await?)?;
        session.archived = true;

        tokio::fs::write(&meta_path, serde_json::to_string_pretty(&session)?).await?;

        self.sessions.remove(session_id);
        self.recording_senders.remove(session_id);

        info!(session_id = %session_id, kiln = %kiln.display(), "Session archived");
        Ok(session)
    }

    pub async fn unarchive_session(
        &self,
        session_id: &str,
        kiln: &Path,
    ) -> Result<Session, SessionError> {
        let session_dir = FileSessionStorage::sessions_base(kiln).join(session_id);
        let meta_path = session_dir.join("meta.json");
        let legacy_path = session_dir.join("session.json");

        let source_path = if tokio::fs::metadata(&meta_path).await.is_ok() {
            meta_path.clone()
        } else if tokio::fs::metadata(&legacy_path).await.is_ok() {
            legacy_path
        } else {
            return Err(SessionError::NotFound(session_id.to_string()));
        };

        let mut session: Session =
            serde_json::from_str(&tokio::fs::read_to_string(&source_path).await?)?;
        session.archived = false;

        tokio::fs::write(&meta_path, serde_json::to_string_pretty(&session)?).await?;

        info!(session_id = %session_id, kiln = %kiln.display(), "Session unarchived");
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

    /// Catch-up titling: persisted, non-archived sessions with content but
    /// no title get the truncation fallback. The LLM title path only fires
    /// on a live `message_complete`, so a daemon restart, a wedged task, or
    /// a pre-feature session would otherwise stay "Untitled" forever.
    ///
    /// Returns how many sessions were titled. Emits `title_changed` per hit.
    pub async fn title_untitled_sessions(
        &self,
        kilns: &[PathBuf],
        event_tx: &tokio::sync::broadcast::Sender<SessionEventMessage>,
    ) -> usize {
        let mut titled = 0;
        let mut seen = std::collections::HashSet::new();
        for kiln in kilns {
            for summary in self
                .list_sessions_filtered_async(Some(kiln), None, None, None, false)
                .await
            {
                if !seen.insert(summary.id.clone()) {
                    continue;
                }
                // Delegated children are titled at creation and hidden from
                // listings — never re-title them.
                if summary.parent_session_id.is_some() {
                    continue;
                }
                if summary
                    .title
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty())
                {
                    continue;
                }
                // First user message from the log; empty sessions stay untitled
                // (the archive sweep owns those).
                let Ok(events) = self
                    .storage
                    .load_events(&summary.id, kiln, Some(200), None)
                    .await
                else {
                    continue;
                };
                let first_user = events.iter().find_map(|e| {
                    if e.get("event").and_then(|v| v.as_str()) != Some("user_message") {
                        return None;
                    }
                    e.get("data")?.get("content")?.as_str().map(str::to_string)
                });
                let Some(first_user) = first_user else {
                    continue;
                };
                let title = crate::agent_manager::title::truncate_to_title(&first_user);

                // In-memory sessions go through set_title (persists too);
                // cold ones get patched directly in storage.
                if self.set_title(&summary.id, title.clone()).await.is_err() {
                    let Ok(mut session) = self.storage.load(&summary.id, kiln).await else {
                        continue;
                    };
                    session.title = Some(title.clone());
                    if self.storage.save(&session).await.is_err() {
                        continue;
                    }
                }
                let _ = event_tx.send(SessionEventMessage::new(
                    &summary.id,
                    "title_changed",
                    serde_json::json!({ "title": title }),
                ));
                info!(session_id = %summary.id, title = %title, "Catch-up title applied");
                titled += 1;
            }
        }
        titled
    }

    pub async fn update_last_activity(
        &self,
        session_id: &str,
        last_activity: DateTime<Utc>,
    ) -> Result<(), SessionError> {
        let session = {
            let mut entry = self
                .sessions
                .get_mut(session_id)
                .ok_or(SessionError::NotFound(session_id.to_string()))?;

            entry.last_activity = Some(last_activity);
            entry.clone()
        };

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
mod tests;
