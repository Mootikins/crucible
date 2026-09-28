//! Session management for the daemon.
//!
//! Manages active sessions and provides CRUD operations. Sessions are stored
//! under the daemon's sessions root (see [`crate::session_storage`]).

use crate::session_storage::{FileSessionStorage, SessionStorage};
use chrono::{DateTime, Utc};
use crucible_core::config::KilnName;
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::{
    RecordingMode, Session, SessionId, SessionState, SessionSummary, SessionType,
};
use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

/// How a listing constrains the session's kiln set.
///
/// [`Self::Kilnless`] exists because zero kilns is a legitimate session shape
/// — a tools-only agent — and it matches no [`Self::Attached`] filter. Without
/// an explicit variant such a session vanished from every listing: the
/// no-kiln-argument path in `handle_session_list` fans out over the open kilns
/// and the data home, and a kiln-less session belongs to none of them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KilnFilter<'a> {
    /// No constraint on the kiln set.
    #[default]
    Any,
    /// Sessions that have this kiln attached.
    Attached(&'a KilnName),
    /// Sessions with an empty kiln set.
    Kilnless,
}

impl KilnFilter<'_> {
    #[must_use]
    pub fn matches(&self, kilns: &[KilnName]) -> bool {
        match self {
            Self::Any => true,
            Self::Attached(kiln) => kilns.iter().any(|k| k == *kiln),
            Self::Kilnless => kilns.is_empty(),
        }
    }
}

/// A caller's own kiln set, and the reach it grants over the flat backlog.
///
/// One root now holds every session on the box, so the per-kiln directory that
/// used to bound a handler is gone and scope has to be stated instead. The rule
/// is **kiln-set overlap**: a session is in a caller's reach iff the two sets
/// share at least one member. `session.search`, `session.list`,
/// `session.list_persisted` and `session.cleanup` all answer to this one
/// predicate — they are the four handlers that read or delete across the root,
/// and a second spelling of the rule is a second place for it to be wrong.
///
/// It cannot be pushed into [`KilnFilter`], which tests one kiln at a time:
/// overlap is a property of the *pair* of sets, and testing `kilns[0]` alone
/// tests a fraction of the caller's reach — a caller on `[a, b]` misses every
/// session that shares only `b`.
///
/// An empty scope overlaps nothing, which is the fail-closed answer *and* the
/// right one for a kiln-less tools-only session (§4.1: an empty kiln set
/// degrades capabilities). Each handler decides for itself what an empty scope
/// means — an empty result, or a refusal for a destructive verb.
///
/// Names, not paths, and typed rather than raw JSON on purpose: the untyped
/// version parsed each element with `PathBuf::from`, which accepts every string
/// — so `"vault"` was a perfectly good scope member that matched nothing at
/// all, silently. A [`KilnName`] either parses or is refused at the handler's
/// parse site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KilnScope(Vec<KilnName>);

impl KilnScope {
    #[must_use]
    pub fn new(kilns: Vec<KilnName>) -> Self {
        Self(kilns)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `kilns` shares at least one member with this scope.
    #[must_use]
    pub fn overlaps(&self, kilns: &[KilnName]) -> bool {
        kilns.iter().any(|k| self.0.contains(k))
    }

    #[must_use]
    pub fn kilns(&self) -> &[KilnName] {
        &self.0
    }
}

/// Remove exactly `{sessions_root}/{session_id}` and refuse anything else.
///
/// The last gate in front of `remove_dir_all`, and deliberately not a
/// `starts_with(sessions_root)` test. §7 of the containment design: "beneath a
/// root" is not the property that matters, because both destructive escapes
/// adversarial review demonstrated produced a path that *was* beneath a root
/// and was still the wrong directory. The assertion is **equality** — after
/// resolving every symlink, what we are about to delete has to be the
/// directory the validated id names, sitting directly in the resolved sessions
/// root.
///
/// What that catches beyond [`SessionId`]'s own validation: a symlink at
/// `{sessions_root}/{id}` pointing at someone's home directory. The id is a
/// perfectly ordinary component, the path is beneath the root by every lexical
/// test, and the target is not ours. Comparing canonical forms is the only
/// check that sees it.
///
/// It is not TOCTOU-proof — `canonicalize` and `remove_dir_all` are two
/// syscalls — and it is not claimed to be. That is what §2's capability handle
/// (`cap-std`, one `openat2` with `RESOLVE_BENEATH`) is for; this is the
/// userspace layer under it.
pub(crate) async fn remove_session_dir(
    sessions_root: &Path,
    session_id: &SessionId,
) -> Result<(), SessionError> {
    let expected = session_id.dir_under(sessions_root);

    let resolved_root = tokio::fs::canonicalize(sessions_root).await?;
    let resolved = tokio::fs::canonicalize(&expected).await?;

    if resolved != session_id.dir_under(&resolved_root) {
        return Err(SessionError::IoError(format!(
            "refusing to delete {}: it resolves to {}, which is not {}/{session_id}",
            expected.display(),
            resolved.display(),
            resolved_root.display(),
        )));
    }

    tokio::fs::remove_dir_all(&resolved).await?;
    Ok(())
}

/// Manages active sessions in the daemon.
///
/// Sessions can be created, listed, paused, resumed, and ended.
/// The manager tracks all active sessions and their state.
/// Sessions are automatically persisted to storage on create and state changes.
pub struct SessionManager {
    sessions: DashMap<SessionId, Session>,
    storage: Arc<dyn SessionStorage>,
    recording_senders: DashMap<SessionId, mpsc::Sender<SessionEventMessage>>,
    /// Base directory under which per-session scratch workspaces are created
    /// for sessions started without an explicit workspace. When `None`, such
    /// sessions simply have no workspace.
    /// Resolved and tilde-expanded at construction (see
    /// [`crate::scm::resolve_session_scratch_dir`]).
    session_workspace_dir: Option<PathBuf>,
    /// Serializes each session's read-modify-write cycle against `meta.json`.
    ///
    /// Every mutator here clones the session out of `sessions` and only then
    /// awaits `storage.save`. Without this lock two of them interleave and the
    /// loser's stale clone lands last: the persist task's `update_last_activity`
    /// captures the session while it is still `Active`, `end_session` writes
    /// `Ended` and drops it from the map, and the pending `Active` write then
    /// overwrites it — an ended session that `session.get` cannot find but
    /// `session.list` reports as active forever. The interleaved non-atomic
    /// writes can also leave `meta.json` unparseable.
    session_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>>,
    /// Where the review's plain-store snapshots live, so deleting a session
    /// can release its claim on them at once rather than at the next sweep.
    ///
    /// `None` for a manager built without one: the sweep still finds those
    /// claims, by the session directory that is no longer there.
    review_snapshot_root: Option<PathBuf>,
    /// Name → directory for kilns.
    ///
    /// Held here rather than reached for through the storage trait because
    /// every consumer of a session's scope — containment, trust
    /// classification, retrieval, the prompt — already has a `SessionManager`
    /// and needs the same mapping. Empty by default, which denies every name.
    kiln_registry: Arc<crate::kiln_registry::KilnRegistry>,
    /// The journal that the persist task drains into each session's log.
    /// A reader of a log waits on it first, so that it sees each event that
    /// was already published. Empty for a manager without a daemon.
    journal: crate::lossless_queue::Waiter,
    /// The bus that stamps each event of a session with its seq. A resume
    /// seeds the counter of the session from its log. `None` for a manager
    /// without a daemon, which stamps nothing.
    events: Option<crate::EventBus>,
}

/// The one listing predicate, over the fields `Session` and `SessionSummary` share.
#[allow(clippy::too_many_arguments)]
fn session_matches(
    kiln: &KilnFilter<'_>,
    workspace: Option<&PathBuf>,
    session_type: Option<SessionType>,
    state: Option<SessionState>,
    include_archived: bool,
    kilns: &[KilnName],
    session_workspace: Option<&PathBuf>,
    actual_type: SessionType,
    actual_state: SessionState,
    archived: bool,
) -> bool {
    kiln.matches(kilns)
        && workspace.is_none_or(|w| session_workspace == Some(w))
        && session_type.is_none_or(|t| actual_type == t)
        && state.is_none_or(|st| actual_state == st)
        && (include_archived || !archived)
}

impl SessionManager {
    /// Create a new session manager with file-based storage rooted at
    /// `sessions_root` (see [`FileSessionStorage::root_for`]).
    pub fn new(sessions_root: PathBuf) -> Self {
        Self::with_storage(Arc::new(FileSessionStorage::new(sessions_root)))
    }

    /// Resolve kiln names — the session's, and every caller's — against
    /// `registry`.
    ///
    /// Also handed to the storage layer by the composition root, which is what
    /// maps a persisted path back onto a name.
    #[must_use]
    pub fn with_kiln_registry(mut self, registry: Arc<crate::kiln_registry::KilnRegistry>) -> Self {
        self.kiln_registry = registry;
        self
    }

    /// Release a deleted session's claim on the plain review store under
    /// `root`. See [`crate::review::drop_keep_refs`].
    #[must_use]
    pub fn with_review_snapshot_root(mut self, root: PathBuf) -> Self {
        self.review_snapshot_root = Some(root);
        self
    }

    /// The name → directory mapping every scope consumer resolves through.
    pub fn kiln_registry(&self) -> &Arc<crate::kiln_registry::KilnRegistry> {
        &self.kiln_registry
    }

    /// The directories a session's kilns reach. See
    /// [`KilnRegistry::paths_for`](crate::kiln_registry::KilnRegistry::paths_for)
    /// for why an unresolvable name contributes nothing.
    pub fn kiln_paths(&self, kilns: &[KilnName]) -> Vec<PathBuf> {
        self.kiln_registry.paths_for(kilns)
    }

    /// The storage backend every session write goes through.
    pub fn storage(&self) -> &Arc<dyn SessionStorage> {
        &self.storage
    }

    /// Root directory holding every session's storage.
    pub fn sessions_root(&self) -> &Path {
        self.storage.sessions_root()
    }

    /// Storage directory for one session: `{sessions_root}/{session_id}`.
    ///
    /// Exists so callers that write beside the transcript (workflow snapshots,
    /// recordings, review journals) do not each re-derive the layout.
    pub fn session_dir(&self, session_id: &SessionId) -> PathBuf {
        session_id.dir_under(self.sessions_root())
    }

    /// The session-owned workspace folder that holds `path`, when one does.
    ///
    /// A session created with no project gets `<session_workspace_dir>/<id>`
    /// as its workspace (see [`SessionManager::create_session`]). That folder
    /// is where the session's work goes, so the file surfaces that admit a
    /// registered project must admit it too — it used to answer "root is not
    /// a registered project" to the file tree of every project-less session.
    ///
    /// Admitted by shape AND by record: the canonical `path` must sit at or
    /// under a direct child of the scratch base, and that child must be the
    /// stored workspace of a session this manager can read. A folder someone
    /// created under the base by hand names no session and is refused, and
    /// nothing outside the base is ever a session folder. Canonical paths on
    /// both sides, so a symlink cannot present the shape.
    pub async fn session_workspace_containing(&self, path: &Path) -> Option<PathBuf> {
        let base = self.session_workspace_dir.as_ref()?.canonicalize().ok()?;
        let canon = path.canonicalize().ok()?;
        let rel = canon.strip_prefix(&base).ok()?;
        let id = rel.components().next()?.as_os_str().to_str()?;
        let folder = base.join(id);
        let session = self.read_session(id).await.ok().flatten()?;
        let stored = session.workspace.as_deref()?.canonicalize().ok()?;
        (stored == folder).then_some(folder)
    }

    /// Create a session manager with a custom storage backend.
    pub fn with_storage(storage: Arc<dyn SessionStorage>) -> Self {
        let sessions_root = storage.sessions_root().to_path_buf();
        Self {
            sessions: DashMap::new(),
            storage,
            recording_senders: DashMap::new(),
            session_workspace_dir: None,
            review_snapshot_root: None,
            session_locks: DashMap::new(),
            journal: crate::lossless_queue::Waiter::default(),
            events: None,
            kiln_registry: Arc::new(crate::kiln_registry::KilnRegistry::empty(
                crate::kiln_registry::KilnRegistryContext::new(
                    sessions_root.clone(),
                    None,
                    sessions_root,
                ),
            )),
        }
    }

    /// Let the log readers wait on the journal of `events`, and let a resume
    /// seed the seq counter of its session. See [`Self::settle_history`].
    #[must_use]
    pub(crate) fn with_event_bus(mut self, events: crate::EventBus) -> Self {
        self.journal = events.journal_waiter();
        self.events = Some(events);
        self
    }

    /// Wait until each event published before this call is in its session's
    /// log.
    ///
    /// The broadcast reaches a client before the persist task writes the
    /// event. A client that saw an event and then reads the history must
    /// find it there, so every reader of a session log calls this first.
    pub async fn settle_history(&self) {
        self.journal.wait().await;
    }

    /// Guard for a session's mutate-then-persist cycle. See `session_locks`.
    ///
    /// Never hold this across a call to another method that takes it — the
    /// mutex is not reentrant. `archive_session`/`delete_session` therefore
    /// call `end_session` before acquiring their own guard.
    async fn persist_guard(&self, session_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self
            .session_locks
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        lock.lock_owned().await
    }

    /// Set the base directory for per-session scratch workspaces.
    ///
    /// When set, sessions created without an explicit workspace get a
    /// session-unique `<dir>/<session_id>` workspace instead of having none at
    /// all. The directory should already be tilde-expanded.
    #[must_use]
    pub fn with_session_workspace_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.session_workspace_dir = dir;
        self
    }

    /// Create a new session and persist it to storage.
    ///
    /// # Arguments
    /// * `session_type` - The type of session (Chat, Agent, Workflow)
    /// * `kilns` - Every kiln this session can query; flat and order-insensitive
    /// * `workspace` - Optional workspace path; absent means the session has
    ///   none, unless a session workspace base is configured (see
    ///   [`SessionManager::with_session_workspace_dir`])
    ///
    /// # Returns
    /// The created session, or an error if persistence fails
    pub async fn create_session(
        &self,
        session_type: SessionType,
        kilns: Vec<KilnName>,
        workspace: Option<PathBuf>,
        recording_mode: Option<RecordingMode>,
    ) -> Result<Session, SessionError> {
        let mut session = Session::new(session_type, kilns);

        if let Some(ws) = workspace {
            session = session.with_workspace(Some(ws));
        } else if let Some(base) = &self.session_workspace_dir {
            // No explicit workspace: give the session its own scratch workspace
            // so its filesystem containment boundary is a private, session-unique
            // directory rather than the shared kiln path. Created BEFORE the
            // session is persisted/used so the path canonicalizes when trust and
            // containment are derived. On failure the session simply has no
            // workspace — never fail session creation over a scratch directory,
            // and never quietly substitute the kiln, which is a corpus rather
            // than a place to act.
            let scratch = session.id.dir_under(base);
            match std::fs::create_dir_all(&scratch) {
                Ok(()) => {
                    session = session.with_workspace(Some(scratch));
                }
                Err(e) => {
                    tracing::warn!(
                        path = %scratch.display(),
                        error = %e,
                        "Failed to create session scratch workspace; the session has no workspace"
                    );
                }
            }
        }

        if let Some(mode) = recording_mode {
            session = session.with_recording_mode(mode);
        }

        let session_id = session.id.clone();

        // Persist to storage
        self.storage.save(&session).await?;
        // A new session has an empty log, so its live fold starts empty and
        // is the fold of the log. `load_transcript` reads it.
        if let Some(events) = &self.events {
            events.seed_transcript(session_id.as_str(), Default::default);
        }

        // Store in active sessions
        let session_clone = session.clone();
        self.sessions.insert(session_id.clone(), session);

        info!(session_id = %session_id, session_type = %session_clone.session_type, "Session created");
        Ok(session_clone)
    }

    /// Storage half of AgentManager's fork admission; do not expose it on a wire.
    pub(crate) async fn copy_session(
        &self,
        parent: Session,
        up_to: Option<u64>,
    ) -> Result<(Session, u64), SessionError> {
        use crate::observe::events::{
            injection_payload, replay_session_log, stored_line, LogEvent,
        };
        use crucible_core::protocol::SessionEventMessage;

        let jsonl = match tokio::fs::read_to_string(parent.jsonl_path(self.sessions_root())).await {
            Ok(jsonl) => jsonl,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error.into()),
        };
        let mut child = Session::new(parent.session_type, parent.kilns)
            .with_workspace(parent.workspace)
            .with_isolation(parent.isolation)
            .with_isolation_record(parent.isolation_record);
        child.agent = parent.agent;
        child.variables = parent.variables;
        let child_id = child.id.to_string();

        // Copy the conversation in wire shape. A row that came from a wire
        // line is that line, under the child's id. Accepted context keeps its
        // role and provenance but not its anchor: the anchor names a parent
        // turn, and the copy places the context where the parent's replay
        // read it. A row that an older daemon wrote as a view line is written
        // again as the wire event with the same meaning.
        let mut rows = Vec::new();
        let mut copied = 0u64;
        let mut model: Option<String> = None;
        let mut last_turn: Option<String> = None;
        for row in replay_session_log(&jsonl) {
            if up_to.is_some_and(|limit| copied >= limit) {
                break;
            }
            if !matches!(
                row.event,
                LogEvent::User { .. } | LogEvent::Assistant { .. } | LogEvent::System { .. }
            ) {
                continue;
            }
            copied += 1;
            let ts = row.event.timestamp();
            if row.injected {
                if let Some(payload) = injection_payload(&row.event, None) {
                    rows.push(stored_line(
                        SessionEventMessage::typed(child_id.as_str(), payload),
                        ts,
                    )?);
                }
                continue;
            }
            // The model is announced once and carried forward, so a copied
            // answer under another model needs its own announcement.
            if let LogEvent::Assistant { model: Some(m), .. } = &row.event {
                if model.as_ref() != Some(m) {
                    model = Some(m.clone());
                    let switched =
                        SessionEventMessage::model_switched(child_id.as_str(), m.as_str(), "");
                    rows.push(stored_line(switched, ts)?);
                }
            }
            if let Some(mut wire) = row.wire {
                if let LogEvent::User { .. } = row.event {
                    last_turn = wire
                        .data
                        .get("message_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                }
                wire.session_id = child_id.clone();
                rows.push(serde_json::to_string(&wire)?);
                continue;
            }
            match row.event {
                LogEvent::User {
                    content, plugin, ..
                } => {
                    let message_id = uuid::Uuid::new_v4().to_string();
                    last_turn = Some(message_id.clone());
                    let payload =
                        crucible_core::protocol::session_events::TurnPayload::UserMessage {
                            message_id,
                            content,
                            origin: plugin.map(crucible_core::turn::TurnOrigin::Plugin),
                        };
                    rows.push(stored_line(
                        SessionEventMessage::typed(child_id.as_str(), payload),
                        ts,
                    )?);
                }
                LogEvent::Assistant {
                    content, tokens, ..
                } => {
                    let message_id = last_turn
                        .clone()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let complete = SessionEventMessage::message_complete(
                        child_id.as_str(),
                        message_id,
                        content,
                        tokens.as_ref(),
                        None,
                    );
                    rows.push(stored_line(complete, ts)?);
                }
                // A plain system row from an older daemon has no wire event
                // with its meaning, so it stays a view line.
                event @ LogEvent::System { .. } => rows.push(serde_json::to_string(&event)?),
                _ => {}
            }
        }
        // Like delegation, configuration is inherited as a value. No provider
        // connection or lifecycle hook is needed to copy a session.
        let result = async {
            for row in &rows {
                self.storage.append_event(&child, row).await?;
            }
            self.storage.save(&child).await
        }
        .await;
        if let Err(error) = result {
            if child.storage_path(self.sessions_root()).exists() {
                remove_session_dir(self.sessions_root(), &child.id).await?;
            }
            return Err(error);
        }
        self.sessions.insert(child.id.clone(), child.clone());
        Ok((child, copied))
    }

    /// Create a delegated child session of `parent`.
    ///
    /// The child inherits the parent's kiln, workspace, connected kilns and
    /// isolation override, carries `parent_session_id`, and is created with its
    /// agent config already set (children never go through `configure_agent`).
    /// Children are full sessions in behavior but are hidden from default
    /// listings and lifecycle-subordinate to their parent.
    ///
    /// Isolation, and the record that a plugin isolated the parent, are
    /// inherited for the same reason the workspace is: a child
    /// runs the parent's tools against the parent's directory. Letting it
    /// resolve isolation independently would put a sandboxed parent's subagent
    /// on the host — the delegation escape, reopened through the resolution
    /// order instead of through the lifecycle hooks.
    pub async fn create_child_session(
        &self,
        parent: &Session,
        agent: crucible_core::session::SessionAgent,
        title: Option<String>,
    ) -> Result<Session, SessionError> {
        let mut session = Session::new(SessionType::Agent, parent.kilns.clone())
            .with_workspace(parent.workspace.clone())
            .with_isolation(parent.isolation.clone())
            .with_isolation_record(parent.isolation_record.clone())
            .with_parent(parent.id.clone());
        session.agent = Some(agent);
        session.title = title;

        let session_id = session.id.clone();
        self.storage.save(&session).await?;
        let session_clone = session.clone();
        self.sessions.insert(session_id.clone(), session);

        info!(
            session_id = %session_id,
            parent_session_id = %parent.id,
            "Child session created"
        );
        Ok(session_clone)
    }

    /// Ids of persisted child sessions of `parent_id`. Used by the
    /// archive/delete cascades: children are lifecycle-subordinate to their
    /// parent and must not outlive it in listings.
    pub async fn child_session_ids(&self, parent_id: &str) -> Vec<SessionId> {
        self.storage
            .list()
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
    ///
    /// # Returns
    /// The resumed session with state set to Active
    pub async fn resume_session_from_storage(
        &self,
        session_id: &SessionId,
    ) -> Result<Session, SessionError> {
        // Load from storage
        let mut session = self.storage.load(session_id).await?;
        self.seed_seq(session_id).await?;

        // Always-resumable: a session loaded from storage becomes live
        // regardless of its persisted lifecycle state. `Session::resume()`
        // only lifts `Paused`, so set the state directly — an `Ended`,
        // `Paused`, or `Compacting` session all revive to `Active`.
        session.state = SessionState::Active;

        // Persist updated state
        self.storage.save(&session).await?;

        // Store in memory
        let session_clone = session.clone();
        self.sessions.insert(session.id.clone(), session);

        info!(session_id = %session_id, "Session resumed from storage");
        Ok(session_clone)
    }

    /// Continue the seq of `session_id` above the highest seq in its log.
    ///
    /// A daemon restart, or the cleanup of an ended session, drops the
    /// counter. A client cursor and `session.events_after` compare seqs, so a
    /// counter that started at 1 again would hide each new event from a
    /// client that already read the log. The journal settles first, so the
    /// log holds each event that this process already stamped.
    async fn seed_seq(&self, session_id: &SessionId) -> Result<(), SessionError> {
        let Some(events) = &self.events else {
            return Ok(());
        };
        self.settle_history().await;
        let lines = self.storage.load_events(session_id, None, None).await?;
        let persisted = lines
            .iter()
            .filter_map(|line| line.get("seq").and_then(serde_json::Value::as_u64))
            .max()
            .unwrap_or(0);
        // The live fold continues the fold of the log, which is the snapshot
        // a client reads. It starts before the counter, because the counter
        // makes an entry for the session.
        events.seed_transcript(session_id.as_str(), || {
            let stored = crate::observe::stored_events(session_id.as_str(), lines);
            crucible_core::transcript::TranscriptFold::from_events(&stored)
        });
        events.seed_session(session_id.as_str(), persisted);
        Ok(())
    }

    /// Load events from storage with pagination.
    ///
    /// Returns events in chronological order (oldest first), in their
    /// current form. The TUI and the web read the history from here, so
    /// neither keeps a copy of the old event forms.
    pub async fn load_session_events(
        &self,
        session_id: &SessionId,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<serde_json::Value>, SessionError> {
        self.settle_history().await;
        let events = self.storage.load_events(session_id, limit, offset).await?;
        Ok(crucible_core::protocol::session_events::migrate_history(
            events,
        ))
    }

    /// The transcript of a session: what a client draws.
    ///
    /// A resident session answers the live fold of the event bus. It holds
    /// the text that a running turn streamed, which the log does not store,
    /// so the next live ops fit it. Any other session folds its whole stored
    /// log. The fold reads every event, not one page: a page can start in
    /// the middle of a turn. An old view line becomes the wire event with its
    /// meaning ([`crate::observe::stored_events`]).
    pub async fn load_transcript(
        &self,
        session_id: &SessionId,
    ) -> Result<crucible_core::transcript::Transcript, SessionError> {
        if let Some(live) = self
            .events
            .as_ref()
            .and_then(|events| events.transcript(session_id.as_str()))
        {
            return Ok(live);
        }
        self.settle_history().await;
        let lines = self.storage.load_events(session_id, None, None).await?;
        let events = crate::observe::stored_events(session_id.as_str(), lines);
        Ok(crucible_core::transcript::TranscriptFold::of_events(
            &events,
        ))
    }

    /// Count total events for a session.
    pub async fn count_session_events(
        &self,
        session_id: &SessionId,
    ) -> Result<usize, SessionError> {
        self.settle_history().await;
        self.storage.count_events(session_id).await
    }

    /// Get a session by ID.
    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.sessions.get(session_id).map(|r| r.clone())
    }

    /// Read history without reviving a session or running its lifecycle hooks.
    pub async fn read_session(&self, session_id: &str) -> Result<Option<Session>, SessionError> {
        if let Some(session) = self.get_session(session_id) {
            return Ok(Some(session));
        }
        let id = SessionId::parse(session_id).map_err(|e| SessionError::IoError(e.to_string()))?;
        match self.storage.load(&id).await {
            Ok(session) => Ok(Some(session)),
            Err(SessionError::NotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn register_transient(&self, session: Session) {
        self.sessions.insert(session.id.clone(), session);
    }

    /// Change a session under its persist guard, then persist it.
    ///
    /// `change` returns whether it changed anything; `false` skips the write
    /// and returns `None`. The change reads the live entry after the guard is
    /// held, so it sees every earlier writer. A caller that saved a whole copy
    /// it read before the guard put back each field another writer changed in
    /// that gap: a title set just after create came back as `None`.
    ///
    /// Memory changes only after the save succeeds. The write-back skips an
    /// entry that is gone, because `delete_session` evicts without the guard.
    pub async fn modify_session(
        &self,
        session_id: &str,
        change: impl FnOnce(&mut Session) -> bool,
    ) -> Result<Option<Session>, SessionError> {
        let _guard = self.persist_guard(session_id).await;
        let mut session = self
            .read_session(session_id)
            .await?
            .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?;
        if !change(&mut session) {
            return Ok(None);
        }
        self.storage.save(&session).await?;
        if let Some(mut live) = self.sessions.get_mut(session_id) {
            *live = session.clone();
        }
        Ok(Some(session))
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
    pub fn list_sessions_filtered(
        &self,
        kiln: KilnFilter<'_>,
        workspace: Option<&PathBuf>,
        session_type: Option<SessionType>,
        state: Option<SessionState>,
        include_archived: bool,
    ) -> Vec<SessionSummary> {
        self.sessions
            .iter()
            .filter(|r| {
                let s = r.value();
                session_matches(
                    &kiln,
                    workspace,
                    session_type,
                    state,
                    include_archived,
                    &s.kilns,
                    s.workspace.as_ref(),
                    s.session_type,
                    s.state,
                    s.archived,
                )
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
        kiln: KilnFilter<'_>,
        workspace: Option<&PathBuf>,
        session_type: Option<SessionType>,
        state: Option<SessionState>,
        include_archived: bool,
    ) -> Vec<SessionSummary> {
        use std::collections::HashSet;

        let mut results = Vec::new();
        let mut seen_ids: HashSet<SessionId> = HashSet::new();

        // First, collect in-memory sessions (they have the latest state)
        for entry in self.sessions.iter() {
            let s = entry.value();
            if session_matches(
                &kiln,
                workspace,
                session_type,
                state,
                include_archived,
                &s.kilns,
                s.workspace.as_ref(),
                s.session_type,
                s.state,
                s.archived,
            ) {
                seen_ids.insert(s.id.clone());
                results.push(SessionSummary::from(s));
            }
        }

        // Then, persisted sessions. One root holds them all, so `kiln` is a
        // filter over each summary's kiln set rather than a directory to scan.
        if let Ok(persisted) = self.storage.list().await {
            for summary in persisted {
                if seen_ids.contains(&summary.id) {
                    continue;
                }
                if session_matches(
                    &kiln,
                    workspace,
                    session_type,
                    state,
                    include_archived,
                    &summary.kilns,
                    summary.workspace.as_ref(),
                    summary.session_type,
                    summary.state,
                    summary.archived,
                ) {
                    results.push(summary);
                }
            }
        }

        results
    }

    /// Pause a session and persist the state change.
    ///
    /// Returns the previous state if successful.
    pub async fn pause_session(&self, session_id: &str) -> Result<SessionState, SessionError> {
        let _guard = self.persist_guard(session_id).await;
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
        let _guard = self.persist_guard(session_id).await;
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

    pub fn set_recording_sender(
        &self,
        session_id: &SessionId,
        tx: mpsc::Sender<SessionEventMessage>,
    ) {
        self.recording_senders.insert(session_id.clone(), tx);
    }

    pub fn get_recording_sender(
        &self,
        session_id: &str,
    ) -> Option<mpsc::Sender<SessionEventMessage>> {
        self.recording_senders.get(session_id).map(|r| r.clone())
    }
    pub async fn end_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let _guard = self.persist_guard(session_id).await;
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

        // The session STAYS resident. Ending is a lifecycle transition; evicting
        // is cache reclamation, and doing the second here made the first lossy.
        //
        // `session.end` arrives microseconds after a turn's last events are
        // broadcast, while the persist task is still draining them. Evicting on
        // end meant those events resolved to no session, and `persist_event`
        // answered `Ok(())` without writing — so a turn's transcript came out
        // missing whichever events had not been drained yet: usually none,
        // sometimes `precognition_complete` alone, and under load occasionally
        // the entire `session.jsonl`. That was a one-in-ten flake in
        // `just test gated` and unreproducible in isolation.
        //
        // Nothing today should emit an event for a session that is not resident,
        // so the fix is to keep the invariant true rather than to teach the
        // writer to tolerate breaking it. (An external trigger — a webhook, a
        // POST — would be a *reason* to revive a session into memory, and when
        // that exists it should revive it, not persist behind its back.)
        //
        // Reclamation is the archive sweep's job and already was: it archives
        // sessions idle past `auto_archive_hours` (72h default, every 30min),
        // `archive_session` evicts, and it refuses to touch a session with
        // connected subscribers. `total_count`'s own doc has always said the map
        // holds "paused/ended" sessions; the eviction here was added later, for
        // memory growth, and contradicted it.
        info!(session_id = %session_id, "Session ended");
        Ok(session)
    }

    pub async fn delete_session(&self, session_id: &SessionId) -> Result<(), SessionError> {
        let was_in_memory = self.sessions.get(session_id).is_some();

        if let Some(session) = self.get_session(session_id) {
            if session.state != SessionState::Ended {
                self.end_session(session_id).await?;
            }
        }

        self.sessions.remove(session_id);
        self.recording_senders.remove(session_id);
        self.session_locks.remove(session_id.as_str());

        let session_dir = self.session_dir(session_id);
        let persisted_exists = session_dir.exists();

        if !was_in_memory && !persisted_exists {
            return Err(SessionError::NotFound(session_id.to_string()));
        }

        if persisted_exists {
            // Before the directory goes: `review.jsonl` is the only record of
            // which repositories this session claimed keep refs in, so once it
            // is deleted those refs pin trees that nothing will ever collect.
            crate::review::drop_keep_refs(
                &session_dir,
                session_id,
                self.review_snapshot_root.as_deref(),
            )
            .await;
            remove_session_dir(self.sessions_root(), session_id).await?;
        }

        info!(session_id = %session_id, "Session deleted");
        Ok(())
    }

    pub async fn archive_session(&self, session_id: &SessionId) -> Result<Session, SessionError> {
        if let Some(session) = self.get_session(session_id) {
            if matches!(session.state, SessionState::Active | SessionState::Paused) {
                self.end_session(session_id).await?;
            }
        }

        // After `end_session` above, never before: the guard is not reentrant.
        let _guard = self.persist_guard(session_id).await;

        let session = self.set_archived(session_id, true).await?;

        self.sessions.remove(session_id);
        self.recording_senders.remove(session_id);

        info!(session_id = %session_id, "Session archived");
        Ok(session)
    }

    /// Rewrite the persisted `archived` flag. The caller holds the persist guard.
    async fn set_archived(
        &self,
        session_id: &SessionId,
        archived: bool,
    ) -> Result<Session, SessionError> {
        let session_dir = self.session_dir(session_id);
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
        session.archived = archived;

        tokio::fs::write(&meta_path, serde_json::to_string_pretty(&session)?).await?;
        Ok(session)
    }

    pub async fn unarchive_session(&self, session_id: &SessionId) -> Result<Session, SessionError> {
        let _guard = self.persist_guard(session_id).await;

        let session = self.set_archived(session_id, false).await?;

        info!(session_id = %session_id, "Session unarchived");
        Ok(session)
    }

    /// Request compaction for a session.
    ///
    /// Sets the session state to Compacting. The actual compaction
    /// (summarizing events) is performed by the agent when it sees this state.
    pub async fn request_compaction(&self, session_id: &str) -> Result<Session, SessionError> {
        let _guard = self.persist_guard(session_id).await;
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
    #[cfg(test)] // the eviction verb the in-process tests use
    pub fn remove_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let session = self.sessions.get(session_id).map(|r| r.clone());

        match session {
            Some(s) if s.state == SessionState::Ended => {
                self.sessions.remove(session_id);
                tracing::debug!(session_id = %session_id, "Session removed from memory");
                Ok(s)
            }
            Some(s) => Err(SessionError::InvalidState {
                expected: SessionState::Ended,
                actual: s.state,
            }),
            None => Err(SessionError::NotFound(session_id.to_string())),
        }
    }

    /// Update session title and persist the change.
    pub async fn set_title(&self, session_id: &str, title: String) -> Result<(), SessionError> {
        let _guard = self.persist_guard(session_id).await;
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
    pub async fn title_untitled_sessions(&self, event_tx: &crate::EventBus) -> usize {
        let mut titled = 0;
        for summary in self
            .list_sessions_filtered_async(KilnFilter::Any, None, None, None, false)
            .await
        {
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
            let Ok(events) = self.storage.load_events(&summary.id, Some(200), None).await else {
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
                let Ok(mut session) = self.storage.load(&summary.id).await else {
                    continue;
                };
                session.title = Some(title.clone());
                if self.storage.save(&session).await.is_err() {
                    continue;
                }
            }
            event_tx.emit(SessionEventMessage::typed(
                &summary.id,
                crucible_core::protocol::SettingsPayload::TitleChanged {
                    title: title.clone(),
                },
            ));
            info!(session_id = %summary.id, title = %title, "Catch-up title applied");
            titled += 1;
        }
        titled
    }

    pub async fn update_last_activity(
        &self,
        session_id: &str,
        last_activity: DateTime<Utc>,
    ) -> Result<(), SessionError> {
        let _guard = self.persist_guard(session_id).await;
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

    /// A caller offered a message as context that no turn can read as
    /// context. Only a system, user or assistant message can be context.
    #[error("not a context message: {0}")]
    NotContext(String),
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
