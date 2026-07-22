//! Session struct and implementation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::agent::{generate_session_id, SessionAgent};
use super::enums::{RecordingMode, SessionState, SessionType};

/// A session is a continuous sequence of agent actions in a workspace.
///
/// Sessions are the fundamental unit of agent interaction in Crucible.
/// They track conversation history, agent reasoning, tool calls, and
/// can be persisted, resumed, and searched.
///
/// # Storage
///
/// Sessions are stored in their owning kiln at:
/// `{kiln}/.crucible/sessions/{session_id}/`
///
/// Contents:
/// - `session.md` - Human-readable markdown log
/// - `session.jsonl` - Machine-readable event log
/// - `artifacts/` - Generated files, fetched content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    /// Unique identifier (e.g., "chat-2025-01-08T1530-abc123")
    pub id: String,

    /// Session type determines logging format and behavior
    pub session_type: SessionType,

    /// The kiln that owns/stores this session
    pub kiln: PathBuf,

    /// Working directory for file operations (may differ from kiln)
    pub workspace: PathBuf,

    /// Additional kilns this session can query (beyond the owning kiln)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connected_kilns: Vec<PathBuf>,

    /// Current state
    pub state: SessionState,

    /// When the session started
    pub started_at: DateTime<Utc>,

    /// Optional continuation from previous session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,

    /// Optional parent session ID for delegation linking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,

    /// Optional title/description for the session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Agent configuration for this session (persisted for resume)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<SessionAgent>,

    /// Recording mode for this session (coarse or granular)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_mode: Option<RecordingMode>,

    /// Notification queue for this session
    #[serde(
        default,
        skip_serializing_if = "crate::types::NotificationQueue::is_empty"
    )]
    pub notifications: crate::types::NotificationQueue,

    /// Whether this session is archived
    #[serde(default)]
    pub archived: bool,

    /// Last time this session had activity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<DateTime<Utc>>,
}

impl Session {
    /// Create a new session with the given type and owning kiln.
    ///
    /// The workspace defaults to the kiln path.
    pub fn new(session_type: SessionType, kiln: PathBuf) -> Self {
        let type_prefix = session_type.as_prefix();
        let id = generate_session_id(type_prefix);

        Self {
            id,
            session_type,
            workspace: kiln.clone(),
            kiln,
            connected_kilns: Vec::new(),
            state: SessionState::Active,
            started_at: Utc::now(),
            continued_from: None,
            parent_session_id: None,
            title: None,
            agent: None,
            recording_mode: None,
            notifications: crate::types::NotificationQueue::new(),
            archived: false,
            last_activity: Some(Utc::now()),
        }
    }

    /// Set the workspace (where agent operates).
    pub fn with_workspace(mut self, workspace: PathBuf) -> Self {
        self.workspace = workspace;
        self
    }

    /// Add a connected kiln for knowledge queries.
    pub fn with_connected_kiln(mut self, kiln: PathBuf) -> Self {
        self.connected_kilns.push(kiln);
        self
    }

    /// Set multiple connected kilns.
    pub fn with_connected_kilns(mut self, kilns: Vec<PathBuf>) -> Self {
        self.connected_kilns = kilns;
        self
    }

    /// Set the session as a continuation of another.
    pub fn continued_from(mut self, session_id: impl Into<String>) -> Self {
        self.continued_from = Some(session_id.into());
        self
    }

    /// Link this session to a parent session (delegation). Sessions with a
    /// parent are "child sessions": full sessions in behavior, but hidden
    /// from default listings and lifecycle-subordinate to their parent.
    pub fn with_parent(mut self, parent_session_id: impl Into<String>) -> Self {
        self.parent_session_id = Some(parent_session_id.into());
        self
    }

    /// Set the session title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the agent configuration.
    pub fn with_agent(mut self, agent: SessionAgent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Set the recording mode for this session.
    pub fn with_recording_mode(mut self, mode: RecordingMode) -> Self {
        self.recording_mode = Some(mode);
        self
    }

    /// Get the storage path for this session.
    ///
    /// When the kiln is the crucible home (`~/.crucible/`), returns
    /// `~/.crucible/sessions/{id}` to avoid double-nesting `.crucible/.crucible/`.
    /// Otherwise returns `{kiln}/.crucible/sessions/{session_id}/`.
    pub fn storage_path(&self) -> PathBuf {
        if crate::config::is_crucible_home(&self.kiln) {
            self.kiln.join("sessions").join(&self.id)
        } else {
            self.kiln.join(".crucible").join("sessions").join(&self.id)
        }
    }

    /// Get the path to the markdown log file.
    pub fn log_path(&self) -> PathBuf {
        self.storage_path().join("session.md")
    }

    /// Get the path to the JSONL event log.
    pub fn jsonl_path(&self) -> PathBuf {
        self.storage_path().join("session.jsonl")
    }

    /// Get the path to the granular recording JSONL file.
    pub fn recording_jsonl_path(&self) -> &'static str {
        "recording.jsonl"
    }

    /// Check if this session is in granular recording mode.
    pub fn is_granular(&self) -> bool {
        matches!(self.recording_mode, Some(RecordingMode::Granular))
    }

    /// Get the artifacts directory path.
    pub fn artifacts_path(&self) -> PathBuf {
        self.storage_path().join("artifacts")
    }

    /// Check if this session can access a given kiln.
    pub fn can_access_kiln(&self, kiln: &PathBuf) -> bool {
        &self.kiln == kiln || self.connected_kilns.contains(kiln)
    }

    /// Pause the session.
    pub fn pause(&mut self) {
        if self.state == SessionState::Active {
            self.state = SessionState::Paused;
        }
    }

    /// Resume a paused session.
    pub fn resume(&mut self) {
        if self.state == SessionState::Paused {
            self.state = SessionState::Active;
        }
    }

    /// End the session.
    pub fn end(&mut self) {
        self.state = SessionState::Ended;
    }

    /// Check if the session is active.
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }
}
