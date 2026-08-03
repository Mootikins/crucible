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
use crate::session_manager::SessionManager;
use dashmap::DashSet;
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::Mutex;

/// Shared plugin session-lifecycle enforcement.
///
/// Holds a `Weak<AgentManager>`: the manager owns the `DelegationService`,
/// which holds an `Arc<SessionLifecycle>`, so a strong reference here would
/// close a cycle. Absent (unbound, or the manager dropped) only costs the
/// fast path for the isolation registry — never correctness.
pub struct SessionLifecycle {
    sessions: Arc<SessionManager>,
    plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    agents: OnceLock<Weak<AgentManager>>,
    /// Sessions whose plugin `on_session_end` hooks have already been claimed.
    ///
    /// `get_session` is a check, not a claim: the session is not removed until
    /// the end handler runs, so two concurrent teardowns both see it and both
    /// fire. `insert` returns false for the loser. Grows by one id per ended
    /// session for the daemon's lifetime — bounded and tiny, and it must
    /// outlive the session entry itself, which is what makes it a valid
    /// duplicate guard.
    plugin_end_claimed: DashSet<String>,
}

impl SessionLifecycle {
    pub fn new(
        sessions: Arc<SessionManager>,
        plugin_loader: Arc<Mutex<Option<DaemonPluginLoader>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            sessions,
            plugin_loader,
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
    pub async fn enforce_session_start(&self, session_id: &str) -> anyhow::Result<()> {
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
        Ok(())
    }

    /// Tear down a session being refused: plugin end hooks first (so an
    /// earlier plugin's container is released), then end the session.
    async fn refuse_session(&self, session_id: &str) {
        self.fire_session_end(session_id).await;
        if let Some(agents) = self.agents() {
            agents.context_attach().release(session_id);
        }
        if let Err(end_err) = self.sessions.end_session(session_id).await {
            tracing::error!(
                session_id = %session_id,
                error = %end_err,
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

    /// Fire plugin `on_session_end` hooks, best-effort and exactly once.
    ///
    /// Unlike the start path this never propagates: refusing to *end* a session
    /// strands the user with something they cannot clean up, which is the
    /// opposite of the start-hook tradeoff. Shared by the normal end path, the
    /// create-refusal path and the delegation watcher, so every way a session
    /// ends tears down identically.
    pub async fn fire_session_end(&self, session_id: &str) {
        // Only fire for a session the manager still knows — this rejects
        // made-up ids and sessions already torn down and removed.
        let Some(daemon_session) = self.sessions.get_session(session_id) else {
            tracing::debug!(
                session_id = %session_id,
                "skipping plugin session_end hooks for unknown/already-ended session"
            );
            return;
        };
        // ...but existence is a CHECK, not a CLAIM. End hooks run before
        // `end_session` removes the session, so two concurrent teardowns both
        // pass the guard above and both fire — and plugins are promised they
        // need not be idempotent (a double `oci` teardown removes an
        // already-removed container). Claim atomically; the loser returns.
        if !self.plugin_end_claimed.insert(session_id.to_string()) {
            tracing::debug!(
                session_id = %session_id,
                "plugin session_end hooks already claimed; skipping"
            );
            return;
        }
        let mut guard = self.plugin_loader.lock().await;
        let Some(loader) = guard.as_mut() else { return };
        // Drop the isolation claim with the session. A claim that outlives its
        // container would keep denying tools for a session id that may be
        // reused, and a stale claim is indistinguishable from a live one.
        loader.isolation().release(session_id);
        loader.status().release(session_id);
        let session = crucible_lua::Session::new(session_id.to_string())
            .with_workspace(daemon_session.workspace.to_string_lossy());
        session.bind(Box::new(crate::server::NoopSessionRpc));
        if let Err(e) = loader.fire_session_end(&session).await {
            tracing::warn!(session_id = %session_id, error = %e, "plugin session_end hooks failed");
        }
    }

    async fn fire_session_start(&self, session_id: &str) -> anyhow::Result<()> {
        let mut guard = self.plugin_loader.lock().await;
        let Some(loader) = guard.as_mut() else {
            return Ok(());
        };
        // The real workspace, not a placeholder: `oci` bind-mounts it, so a
        // wrong or absent path means it isolates the wrong directory — and it
        // is also the key the plugin containers by, so a child sharing its
        // parent's workspace registers against the parent's container instead
        // of paying a second cold start.
        let mut session = crucible_lua::Session::new(session_id.to_string());
        if let Some(daemon_session) = self.sessions.get_session(session_id) {
            session = session.with_workspace(daemon_session.workspace.to_string_lossy());
            // The per-session opt-in, forwarded untouched. This is the whole
            // delivery mechanism: no new Lua API, just a field on the object
            // the plugin already gets.
            if let Some(isolation) = daemon_session.isolation {
                session = session.with_isolation(isolation);
            }
        }
        session.bind(Box::new(crate::server::NoopSessionRpc));
        loader.fire_session_start(&session).await
    }
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
