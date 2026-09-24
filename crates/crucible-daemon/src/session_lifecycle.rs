//! Plugin session lifecycle — the enforcement point every path that makes a
//! session live has to go through.
//!
//! This used to be a private method on the RPC dispatch type, and that is
//! precisely why the delegation escape existed:
//! [`SessionManager::create_child_session`](crate::session_manager::SessionManager::create_child_session)
//! never routes through RPC, so a delegated child fired no plugin
//! `on_session_start`, acquired no container and no isolation claim, and ran
//! every tool on the host. It was masked only because `delegate_session` was
//! itself denied inside a claimed session — a mask that classifying tools by
//! [`ToolSurface`](crucible_core::traits::tools::ToolSurface) removes.
//!
//! Living here instead, the invariant is one function two callers share:
//!
//! > A live session has had its plugin start hooks fired and its isolation
//! > claim checked, or it does not exist.
//!
//! Notably [`plugin_end_claimed`](SessionLifecycle) moves with it. The
//! once-only teardown claim was on `RpcContext`, invisible to
//! `DelegationService`; a child ended by the delegation watcher whose parent
//! also ends would otherwise double-fire teardown, and plugins are explicitly
//! promised they need not be idempotent.

use crate::agent_manager::AgentManager;
use crate::daemon_plugins::DaemonPluginLoader;
use crate::session_manager::{SessionError, SessionManager};
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::{
    IsolationRecord, IsolationRequirement, Session, SessionId, SessionState,
};
use dashmap::DashSet;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::{broadcast, Mutex};

/// Why a session stops. The cause selects the steps that
/// [`SessionLifecycle::stop`] runs, and it names the stop in the
/// `session:ended` event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum StopCause {
    /// `session.pause` or the Lua `pause`. The conversation stays, so a resume
    /// continues it.
    Pause,
    /// `session.end` or the Lua `end_session`.
    End,
    /// `session.archive`. The session leaves memory, and its children go too.
    Archive,
    /// The sweep archived a session that nobody used for a long time.
    AutoArchive,
    /// `session.delete`. The session leaves memory and storage, and its
    /// children go too.
    Delete,
    /// The start checks refused the session.
    Refuse,
    /// A delegated child finished its one turn, or its turn did not start.
    ChildDone,
}

impl StopCause {
    /// The `reason` field of the `session:ended` event.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Pause => "paused",
            Self::End => "ended",
            Self::Archive => "archived",
            Self::AutoArchive => "auto_archived",
            Self::Delete => "deleted",
            Self::Refuse => "refused",
            Self::ChildDone => "child_done",
        }
    }

    /// Whether the session keeps its conversation for a later resume. Such a
    /// stop keeps the context attachment and the agent state, because a
    /// resumed session must not get a new budget and an empty dedup set.
    const fn keeps_conversation(self) -> bool {
        match self {
            Self::Pause => true,
            Self::End
            | Self::Archive
            | Self::AutoArchive
            | Self::Delete
            | Self::Refuse
            | Self::ChildDone => false,
        }
    }

    /// Whether the delegated children of the session stop with it.
    const fn stops_children(self) -> bool {
        match self {
            Self::Archive | Self::Delete => true,
            Self::Pause | Self::End | Self::AutoArchive | Self::Refuse | Self::ChildDone => false,
        }
    }
}

/// What the state change of a stop returned.
#[derive(Debug)]
pub enum Stopped {
    /// The session is paused. `previous` is the state it left.
    Paused { previous: SessionState },
    /// The session ended. It stays resident.
    Ended(Session),
    /// The session is archived and out of memory.
    Archived(Session),
    /// The session is gone from memory and storage.
    Deleted,
}

/// Why a stop did not change the session.
#[derive(Debug, thiserror::Error)]
pub enum StopError {
    /// A turn runs in the session, and the cause keeps the conversation, so
    /// the stop cannot cancel the turn. The end hooks release the isolation
    /// claim, and the isolation gate reads only the claim: a pause during a
    /// turn would let the rest of that turn run its tools on the host.
    #[error(
        "session {0} has a turn that runs. Its isolation claim must stay until the turn \
         is over. Cancel the turn, or wait for it to finish, then pause the session"
    )]
    TurnRunning(String),
    /// The session manager refused the state change, or storage failed.
    #[error(transparent)]
    Session(#[from] SessionError),
}

/// Whether the end stage can run for a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EndStage {
    /// The start hooks of the session ran, so the end hooks must run.
    Run,
    /// No start hook ran in this start, and the plugin runtime is held by
    /// the Lua that asked. There is nothing plugin-owned to release.
    NeverStarted,
}

/// Shared plugin session-lifecycle enforcement.
///
/// Holds a `Weak<AgentManager>`: the manager owns the `DelegationService`,
/// which holds an `Arc<SessionLifecycle>`, so a strong reference here would
/// close a cycle. Absent (unbound, or the manager dropped) only costs the
/// fast path for the isolation registry — never correctness.
pub struct SessionLifecycle {
    sessions: Arc<SessionManager>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    /// The daemon bus. A stop announces itself here after its steps are done.
    event_tx: broadcast::Sender<SessionEventMessage>,
    agents: OnceLock<Weak<AgentManager>>,
    /// Sessions whose plugin `on_session_end` hooks have already been claimed
    /// since their start hooks last fired.
    ///
    /// `get_session` is a check, not a claim: the session is not removed until
    /// the end handler runs, so two concurrent teardowns both see it and both
    /// fire. `insert` returns false for the loser. It must outlive the session
    /// entry itself, which is what makes it a valid duplicate guard.
    ///
    /// The start hooks remove the id. Each firing of the start hooks is paired
    /// with one firing of the end hooks: a session that ends, revives and ends
    /// again claimed isolation twice, and the second end must release it too.
    plugin_end_claimed: DashSet<String>,
}

impl SessionLifecycle {
    pub fn new(
        sessions: Arc<SessionManager>,
        plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
        event_tx: broadcast::Sender<SessionEventMessage>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions,
            plugin_loader,
            event_tx,
            agents: OnceLock::new(),
            plugin_end_claimed: DashSet::new(),
        })
    }

    /// Bind the (Arc'd) agent manager. Idempotent; the first call wins.
    pub fn bind_agent_manager(&self, manager: &Arc<AgentManager>) {
        let _ = self.agents.set(Arc::downgrade(manager));
    }

    fn agents(&self) -> Option<Arc<AgentManager>> {
        self.agents.get().and_then(Weak::upgrade)
    }

    /// Fire plugin start hooks for a session that just became live, and refuse
    /// it if a `required` hook failed.
    ///
    /// Shared by create, resume, resume-from-storage and delegated-child
    /// spawn. The invariant is "a live session is sandboxed" — if only create
    /// enforced it, resuming a session (or delegating from one) would silently
    /// run every tool on the host, which is exactly the ambiguity the
    /// fail-closed design removes.
    ///
    /// On refusal the session is torn down (end hooks first, so an earlier
    /// plugin's container is released) and ended, so a refused session leaves
    /// nothing behind. The `Err` describes *why* it was refused; callers shape
    /// it into their own error type.
    ///
    /// Every path that makes a stored session live again calls this too:
    /// revive-on-send, `session.resume`, `session.resume_from_storage` and the
    /// Lua `resume`. The isolation registry is memory, so a restart or an RPC
    /// pause drops the claim, and only the start hooks put it back. The Lua
    /// `create` calls it as the RPC `session.create` does.
    pub async fn enforce_session_start(&self, session_id: &str) -> anyhow::Result<()> {
        // The hooks need the plugin-loader mutex, and it is not reentrant. A
        // caller that already holds it would wait for itself forever, so the
        // session is refused instead. There were no start hooks, so there is
        // nothing for end hooks to release, and they need the mutex too.
        if this_task_holds_plugin_loader() {
            tracing::error!(session_id = %session_id, "refusing a session start inside plugin Lua");
            if let Err(e) = self
                .stop_steps(session_id, StopCause::Refuse, EndStage::NeverStarted)
                .await
            {
                tracing::error!(session_id = %session_id, error = %e, "refused session could not be ended");
            }
            anyhow::bail!(
                "session {session_id} cannot start or resume here: its start hooks and its \
                 isolation claim need the plugin runtime, and the Lua that asked holds the \
                 plugin runtime (a plugin session hook, lua.eval, or a plugin that loads). \
                 Create or resume the session from Lua that does not hold it, for example in \
                 a task that cru.timer.spawn starts, or with the session.resume RPC or a \
                 later message"
            );
        }

        if let Err(e) = self.fire_session_start(session_id).await {
            tracing::error!(
                session_id = %session_id,
                error = %e,
                "required plugin session_start hook failed; refusing the session"
            );
            self.refuse_session(session_id).await;
            anyhow::bail!("a plugin's session_start hook failed: {e}");
        }

        // A claim the daemon cannot enforce is worse than no claim: the
        // session would LOOK sandboxed while every tool runs on the host.
        if let Some(reason) = self.unenforceable_isolation(session_id).await {
            tracing::error!(session_id = %session_id, %reason, "refusing session");
            self.refuse_session(session_id).await;
            anyhow::bail!("{reason}");
        }

        // The persisted fields are the source, not the registry: the request,
        // and the record of an earlier claim. An empty registry after a
        // restart is not proof that the session never needed a sandbox, so a
        // requirement that no plugin claimed refuses the session.
        if let Some(reason) = self.unclaimed_isolation(session_id).await {
            tracing::error!(session_id = %session_id, %reason, "refusing session");
            self.refuse_session(session_id).await;
            anyhow::bail!("{reason}");
        }

        // The claim is memory, so the fact that it was made goes to storage.
        // Without the record, a session that the plugin configuration
        // isolated looks after a restart like a session that asked for nothing.
        if let Err(e) = self.record_isolation(session_id).await {
            tracing::error!(session_id = %session_id, error = %e, "refusing session");
            self.refuse_session(session_id).await;
            anyhow::bail!(
                "a plugin isolated this session, but the daemon could not store that \
                 fact with the session, so a restart could run it on the host: {e}"
            );
        }
        Ok(())
    }

    /// The reason a session's persisted isolation requirement has no claim,
    /// or `None` when it has no requirement or a plugin claimed it.
    async fn unclaimed_isolation(&self, session_id: &str) -> Option<String> {
        let session = self.sessions.get_session(session_id)?;
        let requirement = required_isolation(&session)?;
        if self.isolation_claim(session_id).await.is_some() {
            return None;
        }
        Some(unclaimed_isolation_reason(&requirement))
    }

    /// Store the record that a plugin isolated the session, when a plugin
    /// claims isolation and the session has no record yet.
    ///
    /// The first claim writes the record, and later claims keep it. The record
    /// is a requirement, so a later start without a claim is refused.
    async fn record_isolation(&self, session_id: &str) -> anyhow::Result<()> {
        let Some(claim) = self.isolation_claim(session_id).await else {
            return Ok(());
        };
        self.sessions
            .modify_session(session_id, |session| {
                if session.isolation_record.is_some() {
                    return false;
                }
                let requirement = if requested_isolation(session).is_some() {
                    IsolationRequirement::Requested
                } else {
                    IsolationRequirement::Configured
                };
                session.isolation_record = Some(IsolationRecord {
                    plugin: claim.plugin.clone(),
                    requirement,
                });
                true
            })
            .await?;
        Ok(())
    }

    /// Tear down a session being refused: the stop runs the end hooks first,
    /// so an earlier plugin's container is released, then it ends the session.
    async fn refuse_session(&self, session_id: &str) {
        if let Err(e) = self.stop(session_id, StopCause::Refuse).await {
            tracing::error!(
                session_id = %session_id,
                error = %e,
                "refused session could not be ended; it may be orphaned"
            );
        }
    }

    /// The isolation registry without waiting on the loader mutex — which is
    /// held across session-start hook execution (container builds included).
    /// Falls back to the loader for contexts the server didn't wire.
    pub async fn isolation_registry(&self) -> Option<crucible_lua::IsolationRegistry> {
        if let Some(registry) = self.agents().and_then(|a| a.isolation()) {
            return Some(registry);
        }
        let guard = self.plugin_loader.lock().await;
        guard.as_ref().map(|l| l.isolation())
    }

    /// The plugin claiming isolation for `session_id`, if any.
    pub async fn isolation_claim(&self, session_id: &str) -> Option<crucible_lua::IsolationClaim> {
        self.isolation_registry().await?.get(session_id)
    }

    /// The reason a session's isolation claim cannot be enforced, or `None`.
    ///
    /// For an internal agent the daemon dispatches every tool, so
    /// `pre_tool_call` handlers and the default-deny gate sit *before*
    /// execution and the claim is enforceable by construction.
    ///
    /// An ACP agent executes tools in its own process and only reports them,
    /// so interception arrives after the fact and stops nothing. That leaves
    /// two cases, and they are not the same:
    ///
    /// * The plugin offered a [`SandboxExec`](crucible_lua::SandboxExec),
    ///   so the agent process is launched *inside* the sandbox. Its tools are
    ///   confined by where the process runs — there is nothing left to
    ///   intercept, and refusing would be refusing a session that is in fact
    ///   sandboxed.
    /// * It did not, so the agent would run on the host while the session
    ///   claims to be isolated. That is the case this refuses, and it is
    ///   refused rather than downgraded because a session that merely *looks*
    ///   sandboxed is worse than one that admits it is not.
    pub(crate) async fn unenforceable_isolation(&self, session_id: &str) -> Option<String> {
        let claim = self.isolation_claim(session_id).await?;
        let session = self.sessions.get_session(session_id)?;
        let agent_type = session.agent.as_ref().map(|a| a.agent_type.clone())?;
        unenforceable_reason(&claim, &agent_type)
    }

    /// Stop a session: the one owner of every way a session leaves service.
    ///
    /// The steps run in this order, and the cause selects which run:
    ///
    /// 1. The end stage: the isolation claim and the status slots go, and the
    ///    plugin `on_session_end` hooks run, once for each start. It releases
    ///    the container, so it runs before the session leaves memory.
    /// 2. The context attachment is released (not on a pause).
    /// 3. The session manager changes the state: pause, end, archive or delete.
    /// 4. `cleanup_session` frees the agent state (not on a pause).
    /// 5. The `session:ended` observers that the session registered for its
    ///    own end run. Then the handlers that the session activated and its
    ///    statusline values are swept, after the hooks, which can write both.
    /// 6. One `session:ended` event goes to the system session, when the stop
    ///    took a session out of service. Its `reason` names the cause. The
    ///    daemon-wide observers get it from the bus.
    /// 7. For an archive or a delete, each delegated child stops the same way.
    ///
    /// Steps 1 to 5 run with the plugin-loader mutex held. A start that waits
    /// for the mutex runs after this stop, so the stop cannot release what a
    /// new start claims. Every step before the event is a direct call: the
    /// bus can drop an event, and these releases must not be lost.
    ///
    /// Lua that holds the plugin runtime must call
    /// [`Self::stop_from_lua`], which cannot wait for the mutex.
    pub async fn stop(&self, session_id: &str, cause: StopCause) -> Result<Stopped, StopError> {
        let stopped = self.stop_steps(session_id, cause, EndStage::Run).await;
        if cause.stops_children() {
            for child in self.sessions.child_session_ids(session_id).await {
                // Boxed: a child stop is a stop, and an async fn cannot hold
                // itself by value.
                if let Err(e) = Box::pin(self.stop(child.as_str(), cause)).await {
                    tracing::warn!(child_id = %child, error = %e, "a child session did not stop");
                }
            }
        }
        stopped
    }

    /// Stop a session for plugin Lua.
    ///
    /// Lua that holds the plugin runtime (a session hook, `lua.eval`) cannot
    /// wait for it. There the checks that can refuse run at once, and the
    /// stop runs on another task, which gets the runtime when that Lua
    /// returns. The steps are not skipped and not reordered: the start hooks
    /// of the session claimed isolation, and only the end hooks release it.
    pub(crate) async fn stop_from_lua(
        self: &Arc<Self>,
        session_id: &str,
        cause: StopCause,
    ) -> Result<(), StopError> {
        if !this_task_holds_plugin_loader() {
            return self.stop(session_id, cause).await.map(|_| ());
        }
        self.check_stoppable(session_id, cause)?;
        if cause.keeps_conversation() && self.agents().is_some_and(|a| a.turn_running(session_id)) {
            return Err(StopError::TurnRunning(session_id.to_string()));
        }
        let lifecycle = Arc::clone(self);
        let session_id = session_id.to_string();
        tokio::spawn(async move {
            if let Err(e) = lifecycle.stop(&session_id, cause).await {
                tracing::warn!(session_id = %session_id, error = %e, "a deferred session stop failed");
            }
        });
        Ok(())
    }

    /// Refuse a stop that the session manager would refuse, before any step
    /// runs. The state change checks again under its own guard.
    fn check_stoppable(&self, session_id: &str, cause: StopCause) -> Result<(), SessionError> {
        let resident = self.sessions.get_session(session_id);
        match cause {
            StopCause::Pause => match resident {
                None => Err(SessionError::NotFound(session_id.to_string())),
                Some(s) if s.state != SessionState::Active => Err(SessionError::InvalidState {
                    expected: SessionState::Active,
                    actual: s.state,
                }),
                Some(_) => Ok(()),
            },
            StopCause::End | StopCause::Refuse | StopCause::ChildDone => match resident {
                None => Err(SessionError::NotFound(session_id.to_string())),
                Some(s) if s.state == SessionState::Ended => {
                    Err(SessionError::AlreadyEnded(session_id.to_string()))
                }
                Some(_) => Ok(()),
            },
            // Archive and delete also act on a session that is only in storage.
            StopCause::Archive | StopCause::AutoArchive | StopCause::Delete => Ok(()),
        }
    }

    /// Steps 1 to 6 of [`Self::stop`], for one session.
    async fn stop_steps(
        &self,
        session_id: &str,
        cause: StopCause,
        stage: EndStage,
    ) -> Result<Stopped, StopError> {
        self.check_stoppable(session_id, cause)?;
        let agents = self.agents();
        // Step 0, the turn. The end stage releases the isolation claim, and
        // the isolation gate reads only the claim, so no turn may run from
        // here on. A pause keeps the conversation, so it cannot cancel the
        // turn: it is refused while one runs. Every other stop cancels the
        // turn first. The hold keeps a new turn from starting in the gap.
        let _turn_hold = match &agents {
            Some(agents) if cause.keeps_conversation() => Some(
                agents
                    .hold_turn_slot(session_id)
                    .ok_or_else(|| StopError::TurnRunning(session_id.to_string()))?,
            ),
            Some(agents) => {
                if agents.turn_running(session_id) {
                    agents.cancel(session_id).await;
                }
                let hold = agents.hold_turn_slot(session_id);
                if hold.is_none() {
                    tracing::warn!(
                        session_id = %session_id,
                        "a turn outlived its cancel; the session stops beside it"
                    );
                }
                hold
            }
            None => None,
        };
        // The session goes out of service only when it was in service.
        let in_service = self
            .sessions
            .get_session(session_id)
            .is_some_and(|s| s.state != SessionState::Ended);

        let mut guard = match stage {
            EndStage::Run => Some(self.plugin_loader.lock().await),
            EndStage::NeverStarted => None,
        };
        let mut loader = guard.as_mut().and_then(|g| g.as_mut());

        if let Some(loader) = loader.as_deref_mut() {
            self.run_end_stage(loader, session_id, cause).await;
        }
        if !cause.keeps_conversation() {
            if let Some(agents) = &agents {
                agents.context_attach().release(session_id);
            }
        }
        let changed = self.change_state(session_id, cause).await;
        if !cause.keeps_conversation() {
            if let Some(agents) = &agents {
                agents.cleanup_session(session_id);
            }
        }
        let ended = (in_service && changed.is_ok())
            .then(|| crate::event_map::session_ended(session_id, cause.reason()));
        if let (Some(loader), Some(ended)) = (loader.as_deref(), ended.as_ref()) {
            run_scoped_observers(loader, session_id, ended).await;
        }
        sweep_session(loader.as_deref(), agents.as_deref(), session_id);
        drop(guard);

        if let Some(ended) = ended {
            crate::event_emitter::emit_event(&self.event_tx, ended);
        }
        changed.map_err(StopError::from)
    }

    /// Step 3 of [`Self::stop`]: the state change in the session manager.
    async fn change_state(
        &self,
        session_id: &str,
        cause: StopCause,
    ) -> Result<Stopped, SessionError> {
        match cause {
            StopCause::Pause => self
                .sessions
                .pause_session(session_id)
                .await
                .map(|previous| Stopped::Paused { previous }),
            StopCause::End | StopCause::Refuse | StopCause::ChildDone => self
                .sessions
                .end_session(session_id)
                .await
                .map(Stopped::Ended),
            StopCause::Archive | StopCause::AutoArchive => self
                .sessions
                .archive_session(&stored_id(session_id)?)
                .await
                .map(Stopped::Archived),
            StopCause::Delete => self
                .sessions
                .delete_session(&stored_id(session_id)?)
                .await
                .map(|()| Stopped::Deleted),
        }
    }

    /// Step 1 of [`Self::stop`]: the end stage, best-effort and once for each
    /// start. The caller holds the plugin-loader mutex.
    ///
    /// Unlike the start path this never propagates: refusing to *end* a session
    /// strands the user with something they cannot clean up, which is the
    /// opposite of the start-hook tradeoff.
    async fn run_end_stage(
        &self,
        loader: &mut DaemonPluginLoader,
        session_id: &str,
        cause: StopCause,
    ) {
        // Only fire for a session the manager still knows — this rejects
        // made-up ids and sessions already torn down and removed.
        let Some(daemon_session) = self.sessions.get_session(session_id) else {
            tracing::debug!(
                session_id = %session_id,
                "skipping plugin session_end hooks for unknown/already-ended session"
            );
            return;
        };
        // ...but existence is a CHECK, not a CLAIM. Two teardowns of one
        // start both find the session, and plugins are promised they need
        // not be idempotent (a double `oci` teardown removes an
        // already-removed container). Claim atomically; the loser returns.
        if !self.plugin_end_claimed.insert(session_id.to_string()) {
            tracing::debug!(
                session_id = %session_id,
                "plugin session_end hooks already claimed; skipping"
            );
            return;
        }
        // Drop the isolation claim with the session. A claim that outlives its
        // container would keep denying tools for a session id that may be
        // reused, and a stale claim is indistinguishable from a live one.
        loader.isolation().release(session_id);
        loader.status().release(session_id);
        // The cause goes to the hooks: a plugin that reviews a finished
        // session (reflection) must not run on a pause.
        let mut session =
            crucible_lua::Session::new(session_id.to_string()).with_end_reason(cause.reason());
        if let Some(workspace) = &daemon_session.workspace {
            session = session.with_workspace(workspace.to_string_lossy());
        }
        session.bind(Box::new(crucible_lua::UnsupportedSessionRpc));
        if let Err(e) = holding_plugin_loader(loader.fire_session_end(&session)).await {
            tracing::warn!(session_id = %session_id, error = %e, "plugin session_end hooks failed");
        }
    }

    async fn fire_session_start(&self, session_id: &str) -> anyhow::Result<()> {
        let mut guard = self.plugin_loader.lock().await;
        // A start opens a new pair, so the next end runs the end hooks again.
        // Without this, the first end's mark stays, and a revived session
        // keeps its claim and its container when it ends a second time.
        // Under the loader lock, so an end that waits for the lock runs after
        // this start, and releases what this start claims.
        self.plugin_end_claimed.remove(session_id);
        let Some(loader) = guard.as_mut() else {
            return Ok(());
        };
        fire_start_hooks(loader, self.agents().as_deref(), &self.sessions, session_id).await
    }
}

/// Fire `on_session_start` for one session. The ONE fire site.
///
/// It used to fire here AND at the session's first turn, once the two VMs
/// became one — so every hook ran twice, with a different session binding each
/// time, and `session.system_prompt = session.system_prompt .. "…"` (the idiom
/// the shipped defaults file documents) raised on `nil` at this one.
///
/// A free function rather than a method because the tests drive it too: a
/// second copy of this wiring in a test harness is a second thing to keep in
/// step with the plugin loader's `required = true` refusal.
pub(crate) async fn fire_start_hooks(
    loader: &mut DaemonPluginLoader,
    agents: Option<&AgentManager>,
    sessions: &SessionManager,
    session_id: &str,
) -> anyhow::Result<()> {
    // The real workspace, not a placeholder: `oci` bind-mounts it, so a wrong
    // or absent path means it isolates the wrong directory — and it is also
    // the key the plugin containers by, so a child sharing its parent's
    // workspace registers against the parent's container instead of paying a
    // second cold start.
    let mut session = crucible_lua::Session::new(session_id.to_string());
    if let Some(daemon_session) = sessions.get_session(session_id) {
        if let Some(workspace) = &daemon_session.workspace {
            session = session.with_workspace(workspace.to_string_lossy());
        }
        // The per-session opt-in, forwarded untouched. This is the whole
        // delivery mechanism: no new Lua API, just a field on the object the
        // plugin already gets.
        if let Some(isolation) = daemon_session.isolation {
            session = session.with_isolation(isolation);
        }
    }

    // The scope the hooks write into. `apply_session_defaults` reads it back.
    let scope = agents.map(|agents| {
        let (scope, variables) = agents.start_hook_scope(session_id);
        session.bind(Box::new(
            crucible_lua::SessionStartScopeRpc::new(scope.clone()).with_variables(variables),
        ));
        scope
    });
    if scope.is_none() {
        // No manager bound: nothing reads the scope, so a hook that writes
        // `session.x` should say so rather than appear to work.
        session.bind(Box::new(crucible_lua::UnsupportedSessionRpc));
    }

    let result = holding_plugin_loader(loader.fire_session_start(&session)).await;

    // Record what the hooks chose even when one of them failed: the others
    // ran, and their writes are as real as a clean run's.
    if let (Some(scope), Some(agents)) = (scope, agents) {
        agents.commit_start_hook_scope(session_id, &scope);
    }
    result
}

/// Step 5 of [`SessionLifecycle::stop`]: drop what the session registered in
/// stores that have no unregister.
///
/// AFTER the end hooks, not before, for two reasons. A `session:end` handler
/// scoped to this session is one of the rows swept, and it has to run first.
/// And a `session:end` handler may set or clear a statusline value (the bar
/// showing "shutting down"), so a sweep first would let the hook put the map
/// straight back.
fn sweep_session(
    loader: Option<&DaemonPluginLoader>,
    agents: Option<&AgentManager>,
    session_id: &str,
) {
    // A plugin turned on for one session registers a row for it, and the
    // store has no unregister: without this, every session that ever enabled
    // a plugin leaves a row behind for the life of the daemon.
    if let Some(loader) = loader {
        let dropped = loader.plugin_handlers().clear_session(session_id);
        if dropped > 0 {
            tracing::debug!(session_id = %session_id, dropped, "swept session-scoped plugin handlers");
        }
    }
    // The same shape of leak one store over: the map is keyed by session and
    // had no other release.
    if let Some(agents) = agents {
        let forgotten = agents.statusline_exprs().release_session(session_id);
        if forgotten > 0 {
            tracing::debug!(session_id = %session_id, forgotten, "swept session statusline expression values");
        }
    }
}

/// Run the observers of `event` that the stopping session registered for
/// itself, before [`sweep_session`] takes them.
///
/// The bus delivers the event to the Lua dispatcher later, and by then the
/// sweep removed these rows: a `session:ended` handler that code inside the
/// session registered for its own end, the documented teardown observer,
/// never ran. The rows run here instead, once. The dispatcher still runs the
/// daemon-wide rows, and it cannot run these a second time, because they are
/// gone when it reads the event.
async fn run_scoped_observers(
    loader: &DaemonPluginLoader,
    session_id: &str,
    event: &SessionEventMessage,
) {
    let Some(hooked) = crate::event_map::decode(event) else {
        return;
    };
    let handlers = loader.plugin_handlers();
    let own_scope = crucible_lua::SessionScope::Session(session_id.to_string());
    let matched: Vec<_> = handlers
        .runtime_handlers_for(
            hooked.hook.as_str(),
            hooked.identifier.as_deref(),
            crucible_lua::Firing::InSession(session_id),
        )
        .into_iter()
        .filter(|row| row.scope == own_scope)
        .collect();
    if matched.is_empty() {
        return;
    }
    let lua = loader.plugin_lua();
    holding_plugin_loader(crate::server::run_handlers(
        &handlers,
        &lua,
        matched,
        hooked.hook,
        &hooked.event,
        Some(session_id),
    ))
    .await;
}

/// The id of a stored session. Archive and delete read the session directory,
/// so the id must be one path component.
fn stored_id(session_id: &str) -> Result<SessionId, SessionError> {
    SessionId::parse(session_id).map_err(|e| SessionError::IoError(e.to_string()))
}

tokio::task_local! {
    /// Present while this task runs plugin Lua and holds the plugin-loader
    /// mutex. Lua awaits the session bridge on the same task, so the bridge
    /// sees it.
    static HOLDS_PLUGIN_LOADER: ();
}

/// Run `lua`, which the caller runs with the plugin-loader mutex held.
///
/// [`SessionLifecycle::enforce_session_start`] reads the mark. Without it, Lua
/// that revives a session from inside a session hook or `lua.eval` waits for
/// the mutex that its own caller holds, and the daemon hangs.
pub(crate) async fn holding_plugin_loader<F: std::future::Future>(lua: F) -> F::Output {
    HOLDS_PLUGIN_LOADER.scope((), lua).await
}

fn this_task_holds_plugin_loader() -> bool {
    HOLDS_PLUGIN_LOADER.try_with(|()| ()).is_ok()
}

/// The isolation a session asked for, as it persisted the request.
///
/// `None` when it asked for none: the field is absent, `null`, or `false`.
/// `false` is an explicit opt out, not a request.
fn requested_isolation(session: &Session) -> Option<&serde_json::Value> {
    session
        .isolation
        .as_ref()
        .filter(|value| !value.is_null() && **value != serde_json::Value::Bool(false))
}

/// The isolation that a session's persisted fields require, as text for a
/// refusal, or `None` when they require none.
///
/// Two fields can require it: the session's own `isolation` request, and the
/// record that a plugin isolated the session before. A session with neither
/// keeps the old behavior: it starts with or without a claim. That includes
/// every session that a daemon wrote before the record existed.
pub(crate) fn required_isolation(session: &Session) -> Option<String> {
    if let Some(requested) = requested_isolation(session) {
        return Some(format!("isolation {requested}"));
    }
    let record = session.isolation_record.as_ref()?;
    Some(match record.requirement {
        IsolationRequirement::Configured => format!(
            "the isolation that plugin '{}' gave it from the plugin configuration",
            record.plugin
        ),
        IsolationRequirement::Requested => {
            format!("the isolation that plugin '{}' gave it", record.plugin)
        }
    })
}

/// The refusal for a session whose isolation requirement no plugin claimed.
pub(crate) fn unclaimed_isolation_reason(requirement: &str) -> String {
    format!(
        "this session requires {requirement}, but no plugin claimed isolation for it: \
         the isolating plugin is not loaded, or it declined the session. The session \
         would run on the host, so it is refused. Load and configure the isolating plugin, \
         then send or resume again"
    )
}

/// Whether a claim can actually be enforced for an agent of this type.
///
/// Pure, and separate from the lookup around it, because this is the rule —
/// the plumbing that finds the claim and the session is not what can be
/// subtly wrong. See [`SessionLifecycle::unenforceable_isolation`].
pub(crate) fn unenforceable_reason(
    claim: &crucible_lua::IsolationClaim,
    agent_type: &str,
) -> Option<String> {
    if agent_type == "internal" || !claim.exec.is_empty() {
        return None;
    }
    Some(format!(
        "plugin '{}' claims isolation for this session, but its agent is external \
         (agent_type '{agent_type}') and the plugin offered no way to launch a process \
         inside the sandbox: the agent would execute its own tools on the host, so the \
         sandbox cannot be enforced",
        claim.plugin
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::IsolationClaim;

    fn claim(exec_prefix: &[&str]) -> IsolationClaim {
        let exec = crucible_lua::SandboxExec {
            prefix: exec_prefix.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        IsolationClaim {
            plugin: "oci".to_string(),
            exempt: Default::default(),
            exec,
        }
    }

    /// The daemon dispatches an internal agent's tools, so the gate sits
    /// before execution and nothing has to be relocated.
    #[test]
    fn an_internal_agent_needs_no_way_into_the_sandbox() {
        assert!(unenforceable_reason(&claim(&[]), "internal").is_none());
    }

    /// An ACP agent runs its own tools in its own process, so a claim the
    /// plugin cannot launch into is a session that only LOOKS sandboxed.
    #[test]
    fn an_external_agent_with_no_way_in_is_refused() {
        let reason = unenforceable_reason(&claim(&[]), "acp")
            .expect("an unreachable sandbox must refuse the session");
        assert!(reason.contains("oci"), "{reason}");
        assert!(reason.contains("on the host"), "{reason}");
    }

    /// ...but with a prefix the agent process starts inside the container, so
    /// its tools are confined by where it runs. Refusing here would refuse a
    /// session that is genuinely sandboxed — the whole point of the prefix.
    #[test]
    fn an_external_agent_launched_into_the_sandbox_is_allowed() {
        assert!(
            unenforceable_reason(&claim(&["podman", "exec", "-i", "crucible-s1"]), "acp").is_none()
        );
    }
}
