//! Session summary type for listings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::enums::{SessionState, SessionType};
use super::session::Session;

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
    /// Agent model name (for display)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_model: Option<String>,
    /// Whether this session is archived
    #[serde(default)]
    pub archived: bool,
    /// Last activity timestamp (None for legacy sessions that predate it)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<DateTime<Utc>>,
    /// Parent session id for delegated child sessions. Children are hidden
    /// from default listings; `#[serde(default)]` keeps old meta files valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
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
            agent_model: session.agent.as_ref().map(|a| a.model.clone()),
            archived: session.archived,
            last_activity: session.last_activity,
            parent_session_id: session.parent_session_id.clone(),
        }
    }
}
