//! Session struct and implementation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

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
/// Sessions are stored under a daemon-owned sessions root, one directory per
/// session id — see [`Session::storage_path`]. Contents:
/// - `session.md` - Human-readable markdown log
/// - `session.jsonl` - Machine-readable event log
/// - `artifacts/` - Generated files, fetched content
///
/// `Serialize`/`Deserialize` are hand-written wrappers around the derived impls
/// (`#[serde(remote = "Self")]`) so that loading can merge the pre-flatten
/// `kiln` + `connected_kilns` spelling into [`Session::kilns`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(remote = "Self")]
pub struct Session {
    /// Unique identifier (e.g., "chat-2025-01-08T1530-abc123")
    pub id: String,

    /// Session type determines logging format and behavior
    pub session_type: SessionType,

    /// Every kiln this session can query.
    ///
    /// Flat and appendable: no member is privileged for *scope* — search,
    /// trust classification and prompt assembly treat every entry alike.
    /// Ordering is preserved because two things still need exactly one kiln:
    /// the workspace sentinel (`workspace == kilns[0]`, set at construction)
    /// and [`Session::default_kiln`]. Nothing else may read position.
    #[serde(default)]
    pub kilns: Vec<PathBuf>,

    /// Working directory for file operations (may differ from the kilns)
    pub workspace: PathBuf,

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

    /// Per-session isolation override, forwarded to plugins untouched.
    ///
    /// Opaque on purpose: the daemon knows nothing about containers, and the
    /// value's shape (`false`, a profile name, or an environment object) is the
    /// isolating plugin's vocabulary. It reaches the plugin as a field on the
    /// [`Session`](crucible_lua::Session) handed to `on_session_start`.
    ///
    /// `None` is *not* the same as `Some(false)`: `None` means the caller said
    /// nothing and the plugin resolves normally, while `Some(false)` is an
    /// explicit opt out of a container the project would otherwise produce.
    /// Persisted, so a resumed session is isolated exactly as it was created —
    /// a sandbox that silently disappears on resume is the ambiguity the
    /// fail-closed design exists to remove.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<serde_json::Value>,

    /// The pre-flatten on-disk spelling, captured only so a `meta.json` written
    /// before `kilns` existed still loads. `Deserialize` merges it into `kilns`
    /// and clears it, so it is never written back.
    #[serde(flatten)]
    legacy_kilns: LegacyKilns,
}

/// `kiln` + `connected_kilns` as they appear in a pre-flatten `meta.json`.
///
/// Flattened into [`Session`] rather than read by a separate migration pass so
/// that every load path gets the merge, including the ones that deserialize a
/// `Session` straight off the wire.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct LegacyKilns {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kiln: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connected_kilns: Vec<PathBuf>,
}

impl Serialize for Session {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Session::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Session {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut session = Session::deserialize(deserializer)?;
        session.absorb_legacy_kilns();
        Ok(session)
    }
}

impl Session {
    /// Create a new session with the given type and kiln set.
    ///
    /// The workspace defaults to the first kiln — the daemon reads
    /// `workspace == kilns[0]` as its "no workspace" sentinel.
    pub fn new(session_type: SessionType, kilns: Vec<PathBuf>) -> Self {
        let type_prefix = session_type.as_prefix();
        let id = generate_session_id(type_prefix);

        Self {
            id,
            session_type,
            workspace: kilns.first().cloned().unwrap_or_default(),
            kilns,
            legacy_kilns: LegacyKilns::default(),
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
            isolation: None,
        }
    }

    /// Set the workspace (where agent operates).
    pub fn with_workspace(mut self, workspace: PathBuf) -> Self {
        self.workspace = workspace;
        self
    }

    /// Set the per-session isolation override (see [`Session::isolation`]).
    #[must_use]
    pub fn with_isolation(mut self, isolation: Option<serde_json::Value>) -> Self {
        self.isolation = isolation;
        self
    }

    /// Add one kiln to the session's kiln set (no-op if already present).
    pub fn with_kiln(mut self, kiln: PathBuf) -> Self {
        self.add_kiln(kiln);
        self
    }

    /// Replace the session's kiln set.
    pub fn with_kilns(mut self, kilns: Vec<PathBuf>) -> Self {
        self.kilns = kilns;
        self
    }

    /// Add a kiln to the session's kiln set. Returns `false` if it was already
    /// reachable, which callers use to skip the attach-side work.
    pub fn add_kiln(&mut self, kiln: PathBuf) -> bool {
        if self.kilns.contains(&kiln) {
            return false;
        }
        self.kilns.push(kiln);
        true
    }

    /// Fold the pre-flatten `kiln`/`connected_kilns` fields into [`Self::kilns`].
    ///
    /// Primary first, then connected, then anything already in `kilns`;
    /// duplicates collapse to their first occurrence. A file written before the
    /// flatten therefore loads with exactly the kilns it could reach.
    fn absorb_legacy_kilns(&mut self) {
        let legacy = std::mem::take(&mut self.legacy_kilns);
        let mut merged: Vec<PathBuf> = Vec::new();
        for kiln in legacy
            .kiln
            .into_iter()
            .chain(legacy.connected_kilns)
            .chain(std::mem::take(&mut self.kilns))
        {
            if !merged.contains(&kiln) {
                merged.push(kiln);
            }
        }
        self.kilns = merged;
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

    /// Get the storage path for this session: `{sessions_root}/{session_id}/`.
    ///
    /// `sessions_root` is passed in rather than derived from a kiln or from the
    /// process-global crucible home. Sessions no longer live inside a kiln, and
    /// a global read here made every caller — tests included — depend on the
    /// developer's real `~/.crucible`.
    pub fn storage_path(&self, sessions_root: &Path) -> PathBuf {
        sessions_root.join(&self.id)
    }

    /// Get the path to the markdown log file.
    pub fn log_path(&self, sessions_root: &Path) -> PathBuf {
        self.storage_path(sessions_root).join("session.md")
    }

    /// Get the path to the JSONL event log.
    pub fn jsonl_path(&self, sessions_root: &Path) -> PathBuf {
        self.storage_path(sessions_root).join("session.jsonl")
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
    pub fn artifacts_path(&self, sessions_root: &Path) -> PathBuf {
        self.storage_path(sessions_root).join("artifacts")
    }

    /// Check if this session can access a given kiln.
    pub fn can_access_kiln(&self, kiln: &PathBuf) -> bool {
        self.kilns.contains(kiln)
    }

    /// The kiln for consumers that need exactly one: note writes, agent-card
    /// discovery, and the single `kiln_path` the MCP server is built around.
    ///
    /// Flattening removed the privileged kiln from *scope*, but writing a note
    /// still needs one destination. Until those consumers grow an explicit
    /// target, it is the first attached kiln — for a migrated session that is
    /// the old `session.kiln`, which `absorb_legacy_kilns` puts first. `None`
    /// when the session has no kilns at all, and every caller must handle it:
    /// a kiln-less session has nowhere to write.
    pub fn default_kiln(&self) -> Option<&Path> {
        self.kilns.first().map(PathBuf::as_path)
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
