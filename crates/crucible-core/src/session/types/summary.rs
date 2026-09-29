//! Session summary and detail types for the wire.
//!
//! [`SessionSummary`] is the one core listing shape for a session:
//! `session.create` and `session.list` both answer it, in full — every
//! field the session record always has is a required field here, not an
//! `Option` the caller might or might not have filled. Only a field the
//! record can genuinely lack (`title`, `agent_model`, `last_activity`,
//! `parent_session_id`) is `Option`.
//!
//! [`SessionDetail`] is what `session.get` answers: a `SessionSummary`
//! (flattened onto the wire, so the keys sit alongside the summary's own),
//! plus the fields only a full record carries (`agent`, `continued_from`,
//! `plugin_approvals`, `plugin_turn_limit`, `recording_mode`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::enums::{RecordingMode, SessionState, SessionType};
use super::id::SessionId;
use super::session::{PluginApproval, Session};
use crate::session::types::agent::SessionAgent;

/// Summary of a session for listing. The one core reply shape for
/// `session.list` and `session.create`.
///
/// A lighter-weight version of Session without full event history.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionSummary {
    /// Session ID. The wire name is `session_id`, never `id`.
    #[serde(rename = "session_id")]
    pub id: SessionId,
    /// Session type, sent as its lowercase prefix (`"chat"`, `"agent"`, …).
    #[serde(rename = "type")]
    pub session_type: SessionType,
    /// Kilns this session can query, by registry name. See [`Session::kilns`].
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
    pub kilns: Vec<crate::config::KilnName>,
    /// Workspace, when the session has one. See [`Session::workspace`].
    #[serde(default)]
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub workspace: Option<PathBuf>,
    /// Current state
    pub state: SessionState,
    /// When the session started. Every session has one.
    pub started_at: DateTime<Utc>,
    /// The session title, when one has been set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Number of events in the session.
    pub event_count: usize,
    /// Agent model name (for display), when the session has an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_model: Option<String>,
    /// Whether this session is archived.
    pub archived: bool,
    /// Last activity timestamp, when the session has run at least once.
    /// Absent for legacy sessions that predate the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<DateTime<Utc>>,
    /// Parent session id for delegated child sessions, when this session is
    /// one. `#[serde(default)]` keeps old meta files valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            session_type: session.session_type,
            kilns: session.kilns.clone(),
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

/// The full session record, as `session.get` answers it: every
/// [`SessionSummary`] field, flattened onto the wire, plus the fields a
/// caller needs to start a session like this one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SessionDetail {
    #[serde(flatten)]
    pub summary: SessionSummary,
    /// The session this one continues, when it continues one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_from: Option<String>,
    /// The session's full agent record, when the session has an agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<SessionAgent>,
    /// Per-plugin approval state. Every session record has this map, empty
    /// or not.
    #[serde(default)]
    pub plugin_approvals: BTreeMap<String, PluginApproval>,
    /// Turns a plugin-started session may run before the daemon stops it.
    /// Every session record has a value, defaulted if never set.
    pub plugin_turn_limit: u32,
    /// How the session records its transcript, when it has a recording mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recording_mode: Option<RecordingMode>,
}

/// Read a [`SessionSummary`] field on a [`SessionDetail`] without going
/// through `.summary`: the flatten makes them one object on the wire, and
/// this makes them one object in Rust too.
impl std::ops::Deref for SessionDetail {
    type Target = SessionSummary;
    fn deref(&self) -> &SessionSummary {
        &self.summary
    }
}

impl From<&Session> for SessionDetail {
    fn from(session: &Session) -> Self {
        Self {
            summary: SessionSummary::from(session),
            continued_from: session.continued_from.clone(),
            agent: session.agent.clone(),
            plugin_approvals: session.plugin_approvals.clone(),
            plugin_turn_limit: session.plugin_turn_limit,
            recording_mode: session.recording_mode,
        }
    }
}
