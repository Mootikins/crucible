//! Everything scoped to one live session, in one place.
//!
//! `AgentManager` used to keep a session's state in twelve independently-keyed
//! maps held consistent by convention. Teardown was twelve removes, and the
//! failure mode of forgetting one ranged from a few MiB per turn
//! (`snapshots`) to a turn left running against a session that no longer
//! exists (`request_state`). One map of one `Arc` per session makes teardown a
//! single `remove`, and [`AgentManager::session_residue`] makes a forgotten
//! store a compile error rather than a leak.

use super::cache_stats::CacheStats;
use super::interaction::PendingInteraction;
use super::{BoxedAgentHandle, PendingPermission, PermissionId};
use crate::tool_dispatch::ToolDispatcher;
use crucible_core::interaction::{InteractionRequest, PermRequest};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Everything scoped to one live session, so teardown is one `remove` and a
/// forgotten field is impossible rather than merely unlikely.
///
/// Values are behind their own locks, not the map's: the agent handle is held
/// across a whole turn and `permissions` is mutated in place (its
/// `oneshot::Sender`s are not `Clone`), so a single outer
/// `RwLock<HashMap<..>>` would make a permission insert on session A block a
/// dispatcher read on session B. The read path here is one DashMap shard lock,
/// held just long enough to clone an `Arc`, never across an `await`.
#[derive(Default)]
pub(crate) struct SessionSlot {
    /// Agent handle and tool dispatcher, behind ONE lock because they are
    /// invalidated together and having them separate was a bug: see
    /// [`Self::install_agent`].
    build: Mutex<BuildCache>,
    /// This session's Lua VM, built on first use.
    ///
    /// A `OnceLock` rather than a map entry because that is the honest type:
    /// the old `DashMap` was check-then-insert, so two concurrent first uses
    /// each built a VM and one was discarded — along with any handler
    /// registered on it. `get_or_init` builds exactly once. The builder is
    /// synchronous (it loads files and runs `on_session_start`), which is why
    /// this is `std::sync::OnceLock` and the tree below is not.
    pub(in crate::agent_manager) lua:
        std::sync::OnceLock<Arc<tokio::sync::Mutex<super::SessionEventState>>>,
    /// Scheduler-owned conversation tree, rebuilt from the session's JSONL on
    /// first use. `tokio::sync::OnceCell` because that rebuild is async.
    pub(in crate::agent_manager) tree:
        tokio::sync::OnceCell<Arc<tokio::sync::Mutex<crucible_core::turn::ConversationTree>>>,
    /// This session's starting values, captured when its VM ran its
    /// `on_session_start` hooks. `None` until the VM has run — a manager whose
    /// VM construction failed outright falls back to the raw globals, which is
    /// a different answer from "the VM ran and captured nothing".
    overrides: Mutex<Option<crucible_lua::SessionDefaultValues>>,
    /// A mode change deferred because the handle was busy serving a turn. The
    /// dispatch path *drains* this (take, not read) at the start of the next
    /// turn; `set_mode` is its only writer.
    pending_mode: Mutex<Option<String>>,
    /// Permission prompts this session is waiting on answers to.
    ///
    /// Mutated in place, never cloned out: `PendingPermission` holds a
    /// `oneshot::Sender`, which is not `Clone`. That is also why the whole slot
    /// is a `DashMap` value rather than one big `RwLock<HashMap<..>>` — under
    /// one outer lock, inserting a prompt here would block a dispatcher read on
    /// an unrelated session.
    permissions: Mutex<HashMap<PermissionId, PendingPermission>>,
    /// Non-permission interactions this session is waiting on answers to.
    ///
    /// Kept apart from `permissions` rather than folded into it because the
    /// two have different lifetimes and different failure answers: an
    /// unanswered permission must resolve to *deny*, which is a real decision
    /// the gate acts on, while an unanswered question resolves to *cancelled*,
    /// which the asker decides what to do with. One map would have to carry
    /// both, and the type that expresses "deny" and "cancelled" as the same
    /// value does not exist.
    interactions: Mutex<HashMap<PermissionId, PendingInteraction>>,
    /// Per-session prompt-cache aggregate, updated on every
    /// `message_complete` that carries usage data.
    cache_stats: Mutex<CacheStats>,
}

/// The two values a turn builds from the session's config, and the generation
/// that says whether a build in flight is still building the right thing.
///
/// Both bake their config in at build time — the agent its system prompt and
/// model, the dispatcher its workspace, kilns and MCP tools — so every scope or
/// model change invalidates them, and it invalidates them together.
#[derive(Default)]
struct BuildCache {
    agent: Option<Arc<tokio::sync::Mutex<BoxedAgentHandle>>>,
    dispatcher: Option<Arc<dyn ToolDispatcher>>,
    /// Bumped on every invalidation. A build that started at generation N
    /// installs nothing if the generation moved while it was awaiting —
    /// otherwise a `switch_model` landing mid-build is silently lost and the
    /// session serves the old model while reporting the new one.
    ///
    /// The request slot does not cover this: `switch_model` reads that slot
    /// without claiming it, so it can pass the check *before* a turn claims it
    /// and then finish inside that turn's build. `get_or_create_agent`'s doc
    /// comment has the interleaving step by step, and
    /// `tests/build_race.rs` reproduces it.
    generation: u64,
}

/// What a caller finds when it asks for the cached agent handle.
pub(crate) enum CachedAgent {
    /// A handle a previous turn built and nothing has invalidated since.
    Hit(Arc<tokio::sync::Mutex<BoxedAgentHandle>>),
    /// Nothing cached, so the caller must build. Carries the generation to hand
    /// back to [`SessionSlot::install_agent`], which is what makes the build
    /// notice an invalidation that landed while it was awaiting.
    Miss { generation: u64 },
}

impl SessionSlot {
    /// The cached agent handle, or the generation a build must install against.
    ///
    /// The two are read under one lock because they have to be: the generation
    /// only means anything relative to the cache state observed with it.
    pub(crate) fn agent_or_generation(&self) -> CachedAgent {
        let build = self.lock_build();
        match &build.agent {
            Some(agent) => CachedAgent::Hit(Arc::clone(agent)),
            None => CachedAgent::Miss {
                generation: build.generation,
            },
        }
    }

    /// The cached agent handle, if a previous turn built one.
    pub(crate) fn cached_agent(&self) -> Option<Arc<tokio::sync::Mutex<BoxedAgentHandle>>> {
        self.lock_build().agent.clone()
    }

    /// Cache a freshly built agent handle, unless the world moved while it was
    /// being built. Returns whether it was installed.
    ///
    /// The loser branch is deliberately not an error: the caller already holds a
    /// valid handle for the config it read, so its turn can proceed uncached and
    /// the next turn rebuilds from the new config. Failing the turn instead would
    /// punish the user for switching models at an unlucky moment.
    #[must_use]
    pub(crate) fn install_agent(
        &self,
        generation: u64,
        agent: &Arc<tokio::sync::Mutex<BoxedAgentHandle>>,
    ) -> bool {
        let mut build = self.lock_build();
        if build.generation != generation {
            return false;
        }
        build.agent = Some(Arc::clone(agent));
        true
    }

    /// The cached tool dispatcher, or the generation a build must install against.
    pub(crate) fn dispatcher_or_generation(&self) -> Result<Arc<dyn ToolDispatcher>, u64> {
        let build = self.lock_build();
        match &build.dispatcher {
            Some(dispatcher) => Ok(Arc::clone(dispatcher)),
            None => Err(build.generation),
        }
    }

    /// Cache a freshly built tool dispatcher, under the same generation rule as
    /// [`Self::install_agent`] — a scope mutation invalidates both, so a
    /// dispatcher built from the pre-mutation session is just as stale.
    pub(crate) fn install_dispatcher(&self, generation: u64, dispatcher: &Arc<dyn ToolDispatcher>) {
        let mut build = self.lock_build();
        if build.generation == generation {
            build.dispatcher = Some(Arc::clone(dispatcher));
        }
    }

    /// Drop the cached agent handle, keeping the dispatcher.
    ///
    /// A model switch changes what the handle answers as and nothing about what
    /// tools the session can reach, so rebuilding the dispatcher would be waste
    /// — it re-opens the kiln and the embedding provider. The generation still
    /// moves for both: a dispatcher build in flight cannot be told apart from
    /// one that started after this, and rebuilding one turn's dispatcher costs
    /// less than serving a stale one.
    pub(crate) fn invalidate_agent(&self) {
        let mut build = self.lock_build();
        build.agent = None;
        build.generation += 1;
    }

    /// Drop both, in one lock acquisition.
    ///
    /// A scope mutation (connected kilns, workspace) changes inputs to both, so
    /// this is genuinely atomic where two `DashMap::remove`s were two windows.
    pub(crate) fn invalidate_build(&self) {
        let mut build = self.lock_build();
        build.agent = None;
        build.dispatcher = None;
        build.generation += 1;
    }

    /// Test-support: seed the build cache as if a previous turn had filled it,
    /// bypassing the generation check (there is no build in flight to lose to).
    #[cfg(test)]
    pub(crate) fn seed_build_for_test(
        &self,
        agent: Option<&Arc<tokio::sync::Mutex<BoxedAgentHandle>>>,
        dispatcher: Option<&Arc<dyn ToolDispatcher>>,
    ) {
        let mut build = self.lock_build();
        if let Some(agent) = agent {
            build.agent = Some(Arc::clone(agent));
        }
        if let Some(dispatcher) = dispatcher {
            build.dispatcher = Some(Arc::clone(dispatcher));
        }
    }

    /// Whether a handle is cached. `system_prompt` is locked once one is (it is
    /// baked into the handle), and tests assert on eviction.
    pub(crate) fn has_agent(&self) -> bool {
        self.lock_build().agent.is_some()
    }

    /// A poisoned build lock means a thread panicked while swapping an `Option`,
    /// which cannot leave a torn value — recovering beats poisoning every later
    /// turn on the session.
    fn lock_build(&self) -> std::sync::MutexGuard<'_, BuildCache> {
        self.build
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// This session's captured starting values, or `None` if its VM never ran.
    pub(crate) fn overrides(&self) -> Option<crucible_lua::SessionDefaultValues> {
        self.overrides
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Record what `on_session_start` left in the session's scope.
    pub(crate) fn set_overrides(&self, values: crucible_lua::SessionDefaultValues) {
        *self
            .overrides
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(values);
    }

    /// Take the deferred mode change, if one is staged. Draining rather than
    /// reading is the contract: a deferral applies to exactly one turn.
    pub(crate) fn take_pending_mode(&self) -> Option<String> {
        self.lock_pending_mode().take()
    }

    /// Stage a mode change for the next turn, replacing any earlier one.
    pub(crate) fn set_pending_mode(&self, mode_id: &str) {
        *self.lock_pending_mode() = Some(mode_id.to_string());
    }

    /// Discard any staged mode change. `set_mode` clears before it decides
    /// whether to apply or defer, so a superseded deferral cannot be drained
    /// onto a later turn and silently revert the live mode.
    pub(crate) fn clear_pending_mode(&self) {
        *self.lock_pending_mode() = None;
    }

    /// Read the staged mode change without consuming it. Tests only: the
    /// production path drains, and a peek that production also used would make
    /// "applies to exactly one turn" unenforceable.
    #[cfg(test)]
    pub(crate) fn peek_pending_mode(&self) -> Option<String> {
        self.lock_pending_mode().clone()
    }

    fn lock_pending_mode(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.pending_mode
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Register a prompt this session is waiting on.
    pub(crate) fn insert_permission(&self, id: PermissionId, pending: PendingPermission) {
        self.lock_permissions().insert(id, pending);
    }

    /// Take a prompt out, to answer it or to abandon it. The caller owns the
    /// `oneshot::Sender` afterwards: sending answers the waiter, dropping makes
    /// its receiver error out immediately.
    /// Whether the permission registry — not the interaction one — owns `id`.
    ///
    /// The routing predicate: a reply belongs to whichever map holds its id,
    /// which is a fact about the registries and not about the reply.
    pub(crate) fn holds_permission(&self, id: &str) -> bool {
        self.lock_permissions().contains_key(id)
    }

    pub(crate) fn take_permission(&self, id: &str) -> Option<PendingPermission> {
        self.lock_permissions().remove(id)
    }

    /// What a pending prompt is asking, without consuming it.
    pub(crate) fn permission_request(&self, id: &str) -> Option<PermRequest> {
        self.lock_permissions()
            .get(id)
            .map(|pending| pending.request.clone())
    }

    /// Every prompt this session is waiting on.
    pub(crate) fn list_permissions(&self) -> Vec<(PermissionId, PermRequest)> {
        self.lock_permissions()
            .iter()
            .map(|(id, pending)| (id.clone(), pending.request.clone()))
            .collect()
    }

    /// Drop every pending prompt's sender, returning how many there were.
    ///
    /// The teardown that matters on cancel: each dropped sender makes its
    /// receiver `Err` at once, releasing callers parked inside
    /// `PermissionSerializer::run`. Without it a partial cancel leaves prompts
    /// dangling for the full 300 s timeout with the serializer lock held.
    pub(crate) fn drop_permissions(&self) -> usize {
        let mut permissions = self.lock_permissions();
        let count = permissions.len();
        permissions.clear();
        count
    }

    fn lock_permissions(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<PermissionId, PendingPermission>> {
        self.permissions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Register a non-permission interaction this session is waiting on.
    pub(crate) fn insert_interaction(&self, id: PermissionId, pending: PendingInteraction) {
        self.lock_interactions().insert(id, pending);
    }

    /// Take an interaction out, to answer it or to abandon it. Same ownership
    /// rule as [`Self::take_permission`]: dropping the returned sender makes
    /// the waiter's receiver error out at once.
    pub(crate) fn take_interaction(&self, id: &str) -> Option<PendingInteraction> {
        self.lock_interactions().remove(id)
    }

    /// Every non-permission interaction this session is waiting on.
    pub(crate) fn list_interactions(&self) -> Vec<(PermissionId, InteractionRequest)> {
        self.lock_interactions()
            .iter()
            .map(|(id, pending)| (id.clone(), pending.request.clone()))
            .collect()
    }

    /// Drop every pending interaction's sender, returning how many there were.
    ///
    /// Called from the same teardown as [`Self::drop_permissions`] and for the
    /// same reason: a dropped sender releases its waiter immediately instead of
    /// parking it for the full timeout.
    pub(crate) fn drop_interactions(&self) -> usize {
        let mut interactions = self.lock_interactions();
        let count = interactions.len();
        interactions.clear();
        count
    }

    fn lock_interactions(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<PermissionId, PendingInteraction>> {
        self.interactions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Snapshot the prompt-cache aggregate.
    pub(crate) fn cache_stats(&self) -> CacheStats {
        self.lock_cache_stats().clone()
    }

    /// Fold one completion's usage into the aggregate.
    pub(crate) fn record_usage(&self, usage: &crucible_core::traits::llm::TokenUsage) {
        self.lock_cache_stats().record(usage);
    }

    /// A poisoned lock here means another thread panicked mid-update, which
    /// leaves the counters merely inaccurate — recovering is strictly better
    /// than propagating the panic into an unrelated turn.
    fn lock_cache_stats(&self) -> std::sync::MutexGuard<'_, CacheStats> {
        self.cache_stats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
