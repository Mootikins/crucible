//! Session struct and implementation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::{Path, PathBuf};

use super::agent::SessionAgent;
use super::enums::{RecordingMode, SessionState, SessionType};
use super::id::SessionId;
use crate::config::KilnName;

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
/// (`#[serde(remote = "Self")]`) so that loading can merge every path-shaped
/// on-disk spelling of the kiln set into one place — see [`PersistedKilns`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(remote = "Self")]
pub struct Session {
    /// Unique identifier (e.g., "chat-2025-01-08T1530-abc123")
    pub id: SessionId,

    /// Session type determines logging format and behavior
    pub session_type: SessionType,

    /// Every kiln this session can query, by registry name.
    ///
    /// Flat and appendable: no member is privileged for *scope* — search,
    /// trust classification and prompt assembly treat every entry alike.
    /// Ordering is preserved because one thing still needs exactly one kiln:
    /// [`Session::default_kiln`]. Nothing else may read position.
    ///
    /// **Never serialized, in either direction.** The on-disk spelling is
    /// path-shaped (see [`PersistedKilns`]) and this field is populated only by
    /// the storage layer, which is the only layer holding a kiln registry.
    /// Routing a validating `KilnName` deserializer through this field would
    /// make every `meta.json` written before names existed fail to parse — and
    /// the listing swallows a parse failure — so users' sessions would silently
    /// vanish. It would also run on *wire* input, where turning a
    /// caller-supplied path into a kiln is the attack the registry's floor
    /// exists to stop.
    #[serde(skip)]
    pub kilns: Vec<KilnName>,

    /// Persisted kiln paths that no registry entry claims.
    ///
    /// Kept, verbatim and unreachable, so that a session whose kilns could not
    /// be resolved — a config that failed to parse, an entry the user renamed —
    /// is not *erased* by the next save. They grant nothing: an unresolvable
    /// kiln is not a kiln, so it contributes no root, no classification, no
    /// search source and no prompt line. It is only the record of what the
    /// session used to reach, so re-registering the entry restores it.
    #[serde(skip)]
    unresolved_kiln_paths: Vec<PathBuf>,

    /// Working directory for file operations, when the session has one.
    ///
    /// `None` is a session with no workspace at all — a tools-only agent, a
    /// chat that was never given a project, or one whose workspace was
    /// detached. It used to be spelled `workspace == kilns[0]`, a sentinel set
    /// at construction that could not tell "no project" from "the project
    /// happens to be the kiln", that every consumer re-derived independently
    /// (including the web UI, in its own language), and that stops being
    /// expressible at all once kilns are names rather than paths.
    ///
    /// Never `Some("")`: an empty path names no directory, and every root
    /// builder downstream reads one as a root that encloses everything. The
    /// setters below — and [`Session::normalize_workspace`] on the
    /// deserialization path — are where that is made unrepresentable.
    #[serde(default)]
    pub workspace: Option<PathBuf>,

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

    /// The kiln set as it appears on disk: paths, in all three spellings a
    /// `meta.json` has ever used. See [`PersistedKilns`].
    #[serde(flatten)]
    persisted_kilns: PersistedKilns,
}

/// The path-shaped, on-disk spelling of a session's kiln set.
///
/// Three arms because three shapes are in the wild: `kiln` + `connected_kilns`
/// (pre-flatten), and the flat `kilns` array. All three are paths, and all
/// three are folded into [`Self::kilns`] on load.
///
/// **This stays paths.** A name is meaningless without the registry that
/// resolves it, and the registry lives daemon-side; a `meta.json` that named
/// kilns would be unreadable by anything else and — worse — would make every
/// file written before this change fail to parse. The mapping to
/// [`Session::kilns`] therefore happens in the storage layer, once, on load.
///
/// Flattened into [`Session`] rather than read by a separate migration pass so
/// that every load path gets the merge, including the ones that deserialize a
/// `Session` straight off the wire.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct PersistedKilns {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kiln: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    connected_kilns: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    kilns: Vec<PathBuf>,
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
        session.normalize_workspace();
        Ok(session)
    }
}

impl Session {
    /// Create a new session with the given type and kiln set.
    ///
    /// A session starts with no workspace. Callers that have one say so with
    /// [`Session::with_workspace`].
    pub fn new(session_type: SessionType, kilns: Vec<KilnName>) -> Self {
        let id = SessionId::generate(session_type);

        Self {
            id,
            session_type,
            workspace: None,
            kilns,
            unresolved_kiln_paths: Vec::new(),
            persisted_kilns: PersistedKilns::default(),
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

    /// Set or clear the workspace (where the agent operates).
    #[must_use]
    pub fn with_workspace(mut self, workspace: Option<PathBuf>) -> Self {
        self.set_workspace(workspace);
        self
    }

    /// Set or clear the workspace, in place.
    ///
    /// An empty path clears rather than being stored: it names no directory,
    /// and a `""` root out-ranks every denial in the containment builders at
    /// the shallowest possible depth. Normalizing here means no consumer has
    /// to remember the distinction — see the field docs.
    pub fn set_workspace(&mut self, workspace: Option<PathBuf>) {
        self.workspace = workspace.filter(|w| !w.as_os_str().is_empty());
    }

    /// Set the per-session isolation override (see [`Session::isolation`]).
    #[must_use]
    pub fn with_isolation(mut self, isolation: Option<serde_json::Value>) -> Self {
        self.isolation = isolation;
        self
    }

    /// Add one kiln to the session's kiln set (no-op if already present).
    pub fn with_kiln(mut self, kiln: KilnName) -> Self {
        self.add_kiln(kiln);
        self
    }

    /// Replace the session's kiln set.
    pub fn with_kilns(mut self, kilns: Vec<KilnName>) -> Self {
        self.kilns = kilns;
        self
    }

    /// Add a kiln to the session's kiln set. Returns `false` if it was already
    /// reachable, which callers use to skip the attach-side work.
    pub fn add_kiln(&mut self, kiln: KilnName) -> bool {
        if self.kilns.contains(&kiln) {
            return false;
        }
        self.kilns.push(kiln);
        true
    }

    /// Detach a kiln. Returns `false` if it was not attached.
    pub fn remove_kiln(&mut self, kiln: &KilnName) -> bool {
        let before = self.kilns.len();
        self.kilns.retain(|k| k != kiln);
        self.kilns.len() != before
    }

    /// Fold every on-disk spelling of the kiln set into
    /// [`PersistedKilns::kilns`].
    ///
    /// Primary first, then connected, then the flat array; duplicates collapse
    /// to their first occurrence. A file written before the flatten therefore
    /// loads with exactly the kiln paths it could reach, in the order it could
    /// reach them — which is what [`Session::default_kiln`] reads position
    /// from.
    ///
    /// It deliberately does **not** touch [`Session::kilns`]: turning a path
    /// into a kiln requires the registry, and this runs on wire input too.
    fn absorb_legacy_kilns(&mut self) {
        let persisted = std::mem::take(&mut self.persisted_kilns);
        let mut merged: Vec<PathBuf> = Vec::new();
        for kiln in persisted
            .kiln
            .into_iter()
            .chain(persisted.connected_kilns)
            .chain(persisted.kilns)
        {
            if !merged.contains(&kiln) {
                merged.push(kiln);
            }
        }
        self.persisted_kilns.kilns = merged;
    }

    /// Take the path-shaped kiln set read off disk, leaving none behind.
    ///
    /// The storage layer's half of the name↔path mapping: it is the only layer
    /// holding a registry, so it is the only one that may turn these into
    /// [`Session::kilns`].
    pub fn take_persisted_kiln_paths(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.persisted_kilns.kilns)
    }

    /// Stage the path-shaped kiln set to be written back out.
    pub fn set_persisted_kiln_paths(&mut self, paths: Vec<PathBuf>) {
        self.persisted_kilns.kilns = paths;
    }

    /// Persisted kiln paths that resolved to no registry entry — carried
    /// forward so a save does not erase them. See the field docs.
    pub fn unresolved_kiln_paths(&self) -> &[PathBuf] {
        &self.unresolved_kiln_paths
    }

    /// Record the persisted kiln paths that no registry entry claims.
    pub fn set_unresolved_kiln_paths(&mut self, paths: Vec<PathBuf>) {
        self.unresolved_kiln_paths = paths;
    }

    /// Re-establish the "never `Some("")`" invariant on a deserialized value.
    ///
    /// `Deserialize` writes the field directly, so it is the one door the
    /// setters do not guard — and the empty path is exactly what a kiln-less
    /// session's `meta.json` carries from before the field was optional
    /// (`Session::new` stamped `PathBuf::default()` when there was no first
    /// kiln to copy).
    ///
    /// The OTHER pre-`Option` spelling — `workspace == kilns[0]`, stamped on
    /// sessions that were never given a workspace — is deliberately **not**
    /// collapsed here. On disk it is indistinguishable from a workspace the
    /// user chose, and choosing it is ordinary: a repo that is also your kiln
    /// (`cru chat --kiln .`) writes exactly that. Collapsing it would re-import
    /// the very ambiguity this field's type exists to remove, and would discard
    /// the user's choice on every daemon restart — a live bug, in exchange for
    /// tidying a rare legacy file. Such a session now reads as having a
    /// workspace equal to its kiln, which is what every consumer except the
    /// two sentinel readers already treated it as.
    fn normalize_workspace(&mut self) {
        let workspace = self.workspace.take();
        self.set_workspace(workspace);
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
        self.id.dir_under(sessions_root)
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
    pub fn can_access_kiln(&self, kiln: &KilnName) -> bool {
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
    pub fn default_kiln(&self) -> Option<&KilnName> {
        self.kilns.first()
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
