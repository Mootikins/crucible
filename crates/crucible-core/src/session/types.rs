//! Core session types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Generate a session ID with the given type prefix.
///
/// Format: `{type}-{YYYY-MM-DDTHHMM}-{random6}`
/// Example: `chat-2025-01-08T1530-a1b2c3`
fn generate_session_id(type_prefix: &str) -> String {
    use rand::Rng;
    let timestamp = Utc::now().format("%Y-%m-%dT%H%M");
    let mut rng = rand::rng();
    let random: String = (0..6)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("{}-{}-{}", type_prefix, timestamp, random)
}

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

    /// Optional title/description for the session
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
            title: None,
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

    /// Set the session title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Get the storage path for this session.
    ///
    /// Returns: `{kiln}/.crucible/sessions/{session_id}/`
    pub fn storage_path(&self) -> PathBuf {
        self.kiln
            .join(".crucible")
            .join("sessions")
            .join(&self.id)
    }

    /// Get the path to the markdown log file.
    pub fn log_path(&self) -> PathBuf {
        self.storage_path().join("session.md")
    }

    /// Get the path to the JSONL event log.
    pub fn jsonl_path(&self) -> PathBuf {
        self.storage_path().join("session.jsonl")
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

/// Type of session, determines logging format and behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
    /// User/assistant conversation (interactive chat)
    Chat,
    /// Autonomous agent actions (may run without user input)
    Agent,
    /// Programmatic workflow execution
    Workflow,
}

impl SessionType {
    /// Get the string prefix used in session IDs.
    pub fn as_prefix(&self) -> &'static str {
        match self {
            SessionType::Chat => "chat",
            SessionType::Agent => "agent",
            SessionType::Workflow => "workflow",
        }
    }
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_prefix())
    }
}

/// Current state of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Session is actively processing
    #[default]
    Active,
    /// Session is paused (not processing new events)
    Paused,
    /// Session is compacting old context
    Compacting,
    /// Session has ended
    Ended,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionState::Active => write!(f, "active"),
            SessionState::Paused => write!(f, "paused"),
            SessionState::Compacting => write!(f, "compacting"),
            SessionState::Ended => write!(f, "ended"),
        }
    }
}

/// Summary of a session for listing.
///
/// A lighter-weight version of Session without full event history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Session ID
    pub id: String,
    /// Session type
    pub session_type: SessionType,
    /// Owning kiln
    pub kiln: PathBuf,
    /// Workspace
    pub workspace: PathBuf,
    /// Current state
    pub state: SessionState,
    /// When started
    pub started_at: DateTime<Utc>,
    /// Optional title
    pub title: Option<String>,
    /// Number of events in the session
    pub event_count: usize,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            session_type: session.session_type,
            kiln: session.kiln.clone(),
            workspace: session.workspace.clone(),
            state: session.state,
            started_at: session.started_at,
            title: session.title.clone(),
            event_count: 0, // Would be populated from storage
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let kiln = PathBuf::from("/home/user/notes");
        let session = Session::new(SessionType::Chat, kiln.clone());

        assert!(session.id.starts_with("chat-"));
        assert_eq!(session.session_type, SessionType::Chat);
        assert_eq!(session.kiln, kiln);
        assert_eq!(session.workspace, kiln); // defaults to kiln
        assert!(session.connected_kilns.is_empty());
        assert_eq!(session.state, SessionState::Active);
    }

    #[test]
    fn test_session_with_workspace() {
        let kiln = PathBuf::from("/home/user/notes");
        let workspace = PathBuf::from("/home/user/project");
        let session = Session::new(SessionType::Agent, kiln.clone()).with_workspace(workspace.clone());

        assert_eq!(session.kiln, kiln);
        assert_eq!(session.workspace, workspace);
    }

    #[test]
    fn test_session_with_connected_kilns() {
        let kiln = PathBuf::from("/home/user/notes");
        let reference = PathBuf::from("/home/user/reference");
        let session = Session::new(SessionType::Chat, kiln.clone()).with_connected_kiln(reference.clone());

        assert!(session.can_access_kiln(&kiln));
        assert!(session.can_access_kiln(&reference));
        assert!(!session.can_access_kiln(&PathBuf::from("/other")));
    }

    #[test]
    fn test_session_storage_paths() {
        let kiln = PathBuf::from("/home/user/notes");
        let session = Session::new(SessionType::Chat, kiln);

        let storage = session.storage_path();
        assert!(storage.to_string_lossy().contains(".crucible/sessions/chat-"));
        assert!(session.log_path().ends_with("session.md"));
        assert!(session.jsonl_path().ends_with("session.jsonl"));
        assert!(session.artifacts_path().ends_with("artifacts"));
    }

    #[test]
    fn test_session_state_transitions() {
        let kiln = PathBuf::from("/home/user/notes");
        let mut session = Session::new(SessionType::Chat, kiln);

        assert!(session.is_active());

        session.pause();
        assert_eq!(session.state, SessionState::Paused);
        assert!(!session.is_active());

        session.resume();
        assert_eq!(session.state, SessionState::Active);
        assert!(session.is_active());

        session.end();
        assert_eq!(session.state, SessionState::Ended);
        assert!(!session.is_active());
    }

    #[test]
    fn test_session_serialization() {
        let kiln = PathBuf::from("/home/user/notes");
        let session = Session::new(SessionType::Chat, kiln).with_title("Test session");

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("\"session_type\":\"chat\""));
        assert!(json.contains("\"state\":\"active\""));
        assert!(json.contains("\"title\":\"Test session\""));

        let parsed: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.session_type, session.session_type);
        assert_eq!(parsed.title, session.title);
    }
}
