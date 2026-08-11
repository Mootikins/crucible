//! The review ledger: per-tool-call change attribution over a session's
//! workspace roots.
//!
//! Two things live here and must not be conflated.
//!
//! The **ledger** is append-only evidence — an [`Interval`] per bracketed
//! tool call recording the tree on either side of it. The **composed diff**
//! is the review surface: `session_base` → current worktree, recomputed on
//! demand. Attribution intersects the two.
//!
//! Everything is keyed on tree SHAs rather than on "did this call report an
//! edit", because the filesystem is the only witness both an internal agent
//! and an external ACP agent share. That is also why attribution keeps
//! working for agents the gate cannot block.
//!
//! **Any write to a [`Ledger`] that does not go through a [`ReviewLedgers`]
//! method is a persistence bug.** The mutators are methods on the value, so a
//! caller holding a `DashMap` guard reaches them and compiles cleanly while
//! never reaching [`journal`]. See [`Ledger::push_interval_in_memory`], which
//! is named for exactly that. The journal append lives beside every mutation
//! here for that reason and no other.

mod attribute;
mod compose;
mod error;
mod git;
mod journal;
pub(crate) mod paths;
mod persist;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use crucible_core::session::{
    ChildLedgerRef, Comment, ComposedHunk, GateBlock, HunkId, Integrity, Interval, Ledger,
    PhysicalRoot, ReviewState, RootBase, RootInterval, RootStatus, TreeSha, Verdict,
};
use dashmap::DashMap;
use tracing::{debug, warn};

pub use error::{ReviewError, ReviewResult};
pub use persist::{drop_keep_refs, sweep_review_refs};

use crate::workspace_snapshot;

/// An open capture bracket. Held across a tool call's dispatch and closed
/// with [`ReviewLedgers::close`].
///
/// Not `Copy` or `Clone`: a bracket left open leaks an entry in the
/// contested-overlap registry and makes every concurrent interval on that
/// root look contested forever.
///
/// `Drop` is the backstop for the exit paths that never reach `close` — the
/// turn's cancel arm and its execution timeout both drop the whole turn future
/// mid-tool-call. Without it a cancelled turn poisons its roots for the
/// daemon's remaining lifetime, in every session, and `clear_session` cannot
/// undo it because the overlap registry is keyed by root rather than session.
#[derive(Debug)]
pub struct CaptureHandle {
    id: u64,
    before: Vec<(PathBuf, TreeSha)>,
    /// Weak so a handle that outlives its manager cannot keep the ledgers
    /// alive; a dead registry has nothing left to deregister from.
    ledgers: Weak<ReviewLedgers>,
    /// Suppresses the worktree watch for as long as the bracket is open.
    ///
    /// Everything a bracketed call writes is the ledger's to attribute, so
    /// announcing it as an external change is both wrong and expensive: each
    /// announcement costs every connected client a full recompose. Held here
    /// rather than taken at the call site so the two lifetimes cannot drift —
    /// the same `Drop` that deregisters the roots releases the window, on the
    /// cancelled and timed-out paths as much as the ordinary one.
    _window: Option<crate::watch::external_changes::CaptureWindow>,
}

impl Drop for CaptureHandle {
    fn drop(&mut self) {
        // `close` pops each root as it deregisters it, so this fires for
        // exactly the roots it never reached — all of them on the paths that
        // never called it, the tail on a `close` dropped mid-loop.
        if self.before.is_empty() {
            return;
        }
        let Some(ledgers) = self.ledgers.upgrade() else {
            return;
        };
        for (root, _) in &self.before {
            ledgers.mark_closed(root, self.id);
        }
    }
}

/// A live review-gate block: the session it names is parked waiting for a
/// human, until this is dropped.
///
/// Mirrors [`CaptureHandle`]'s backstop, and for the same reason rather than
/// for symmetry. `hold_for_review` awaits `sleep(POLL_INTERVAL)`, which is a
/// yield point, so a turn cancelled or timed out while parked drops the whole
/// future *there* and never reaches the release. Without `Drop` that turns a
/// transient invisible block into a permanent visible lie — a session chip
/// claiming to wait on a review with no turn running behind it, and no action
/// a user can take to clear it.
///
/// Not `Clone`: two holds for one session would race on release and the loser
/// would clear a block that is still live.
#[derive(Debug)]
pub struct GateHold {
    session_id: String,
    /// Weak for the same reason as [`CaptureHandle::ledgers`]: a hold that
    /// outlives its manager has nothing left to release.
    ledgers: Weak<ReviewLedgers>,
}

impl Drop for GateHold {
    fn drop(&mut self) {
        if let Some(ledgers) = self.ledgers.upgrade() {
            ledgers.gate.remove(&self.session_id);
        }
    }
}

/// Per-session review ledgers.
///
/// Mirrors `SnapshotMap`: a `DashMap` owned by `AgentManager`, cleared on
/// session end. Unlike snapshots, entries are read on demand by RPC handlers
/// rather than consumed once by undo, so they are cloned out rather than
/// removed.
#[derive(Default)]
pub struct ReviewLedgers {
    ledgers: DashMap<String, Ledger>,
    /// Review decisions, per session. A hunk absent from this map is
    /// [`ReviewState::Unreviewed`] — the fail-closed answer, and the only
    /// safe one: an identity we do not recognise must never inherit a
    /// decision made about different lines.
    states: DashMap<String, HashMap<HunkId, ReviewState>>,
    comments: DashMap<String, Vec<Comment>>,
    /// Capture brackets currently open per root, across all sessions.
    /// Overlap on a shared root is what makes bracketing unsound (§5), so it
    /// is tracked globally rather than per session — the concurrent writers
    /// are usually a parent and its delegated children.
    open: DashMap<PathBuf, Vec<OpenBracket>>,
    /// Sessions whose turn is currently parked on the review gate.
    ///
    /// Here rather than on `AgentManager` or `session.status` because this map
    /// is already per-session, already keyed the same way, already drained by
    /// [`Self::clear_session`], and already reachable from the gate — the gate
    /// holds a [`ReviewLedgers`] and nothing else. At most one entry per
    /// session: a turn runs one tool batch at a time, and the gate holds at
    /// most one call in it.
    ///
    /// In memory only. See [`GateBlock`] for why persisting it would be wrong.
    gate: DashMap<String, GateBlock>,
    /// Where each session's `review.jsonl` lives.
    ///
    /// A session absent from this map is not persisted at all — the ledger
    /// still works for the daemon's lifetime, which is what a test fixture and
    /// a session with no storage directory get. Absence is therefore checked,
    /// never assumed.
    journals: DashMap<String, PathBuf>,
    /// What each session's journal could not be read back as.
    integrity: DashMap<String, Integrity>,
    /// Delegated child → the parent that must absorb its intervals when it
    /// ends. Registered from the child's first turn off
    /// `Session::parent_session_id`, which is the only place both ids are in
    /// hand without threading a tool call id through `DelegationRequest`.
    ///
    /// A child whose parent has already gone is simply not in this map by the
    /// time it ends, and its work degrades to `external` in nobody's diff —
    /// the parent it would have belonged to no longer has one.
    parents: DashMap<String, String>,
    /// The worktree watch's tracker, when the daemon started one.
    ///
    /// Here so that *every* path that reverts takes a suppression window,
    /// including [`Self::set_state`] with [`ReviewState::Rejected`]. Bound
    /// after construction because the watch needs an async constructor and
    /// this type is built by `Default` in `AgentManager::new`.
    external: std::sync::OnceLock<Arc<crate::watch::external_changes::ExternalChangeTracker>>,
    next_handle: AtomicU64,
}

#[derive(Debug)]
struct OpenBracket {
    id: u64,
    contested: bool,
}

impl ReviewLedgers {
    /// Open a ledger for a session over the roots it may write to, capturing
    /// `session_base` once.
    ///
    /// Roots are normalised to their repository top level and deduplicated,
    /// so a workspace and a kiln inside one repo become a single tracked
    /// root. Roots outside git are skipped: there is nothing to diff and the
    /// alternative — walking the tree per tool call — costs more than the
    /// feature is worth.
    ///
    /// Re-opening an already-open session is a no-op. `session_base` is
    /// captured exactly once and never recomputed; see [`Self::restore`].
    pub async fn open(&self, session_id: &str, roots: &[PathBuf]) -> ReviewResult<()> {
        if self.ledgers.contains_key(session_id) {
            return Ok(());
        }

        let mut base: Vec<RootBase> = Vec::new();
        for root in roots {
            let Ok(top) = git::top_level(root).await else {
                debug!(root = %root.display(), "root is not in a git repo; not tracked for review");
                continue;
            };
            if base.iter().any(|b| b.root == top) {
                continue;
            }
            let tree = workspace_snapshot::capture_tree(&top).await?;
            base.push(RootBase {
                root: top,
                base_tree: TreeSha::new(tree),
            });
        }

        if base.is_empty() {
            return Err(ReviewError::NoTrackableRoots(
                roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }

        self.ledgers
            .insert(session_id.to_string(), Ledger::new(session_id, base));
        Ok(())
    }

    /// Reinstate a ledger directly, without a journal.
    ///
    /// For callers that already hold a `Ledger` value. Note that a session
    /// restored this way is **not** persisted: nothing registers a journal
    /// path, so later decisions live only for the daemon's lifetime.
    #[cfg(test)]
    pub fn restore(&self, ledger: Ledger) {
        self.ledgers.insert(ledger.session_id().to_string(), ledger);
    }

    /// The ledger for a session, cloned for persistence or inspection.
    pub fn ledger(&self, session_id: &str) -> Option<Ledger> {
        self.ledgers.get(session_id).map(|r| r.value().clone())
    }

    pub fn is_open(&self, session_id: &str) -> bool {
        self.ledgers.contains_key(session_id)
    }

    /// Whether any of these maps still holds something for `session_id`.
    ///
    /// The post-teardown check `AgentManager::session_residue` runs. Broader
    /// than [`Self::is_open`] on purpose: teardown of a delegated child is
    /// [`Self::harvest_and_clear`], which drops the `parents` entry *and* the
    /// ledger, and a `parents` entry left behind would keep the child pointing
    /// at a parent that may since have been reissued.
    pub fn has_session(&self, session_id: &str) -> bool {
        self.ledgers.contains_key(session_id)
            || self.states.contains_key(session_id)
            || self.comments.contains_key(session_id)
            || self.parents.contains_key(session_id)
            || self.gate.contains_key(session_id)
            || self.journals.contains_key(session_id)
            || self.integrity.contains_key(session_id)
    }

    /// Bind the worktree watch's tracker, so daemon-side writes can suppress
    /// their own detection. Idempotent; the daemon binds it once at startup
    /// and nothing else has one.
    pub fn set_external_tracker(
        &self,
        tracker: Arc<crate::watch::external_changes::ExternalChangeTracker>,
    ) {
        let _ = self.external.set(tracker);
    }

    /// Suppress external-change detection for `session_id`'s roots while the
    /// returned guard lives. `None` when no watch is running.
    ///
    /// Every daemon-side write to a watched worktree has to hold one. Without
    /// it the watcher sees the daemon's own revert land, reports it as a
    /// change the user made, and announces that the composed diff moved for
    /// the file the user just finished reviewing.
    fn suppress(&self, session_id: &str) -> Option<crate::watch::external_changes::CaptureWindow> {
        self.external.get().map(|t| t.capture(session_id))
    }

    /// Record that `child` is a delegated session whose intervals belong to
    /// `parent`. See [`Self::absorb_child_intervals`].
    pub fn set_parent(&self, child: &str, parent: &str) {
        self.parents.insert(child.to_string(), parent.to_string());
    }

    /// The parent a delegated session's intervals must be harvested into.
    pub fn parent_of(&self, session_id: &str) -> Option<String> {
        self.parents.get(session_id).map(|r| r.value().clone())
    }

    /// Harvest a delegated child's attribution into its parent, then tear the
    /// child's ledger down.
    ///
    /// Ordered, and therefore async where [`Self::clear_session`] is not:
    /// [`Self::absorb_child_intervals`] reads the child's intervals and
    /// journals them into the parent, and `clear_session` destroys the first
    /// half of that. A session with no registered parent is exactly
    /// `clear_session`.
    pub async fn harvest_and_clear(&self, session_id: &str) {
        if let Some((_, parent)) = self.parents.remove(session_id) {
            let absorbed = self.absorb_child_intervals(&parent, session_id).await;
            debug!(
                child = session_id,
                parent = %parent,
                absorbed,
                "harvested a delegated child's intervals into its parent"
            );
        }
        self.clear_session(session_id);
    }

    /// Drop everything for a session. Called at session end.
    ///
    /// Delegated children go through [`Self::harvest_and_clear`] instead —
    /// this drops the very intervals the harvest needs.
    pub fn clear_session(&self, session_id: &str) {
        self.ledgers.remove(session_id);
        self.states.remove(session_id);
        self.comments.remove(session_id);
        self.parents.remove(session_id);
        // Belt to the `GateHold` braces: a session torn down while a turn is
        // parked must not leave a block behind for an id that may be reissued.
        self.gate.remove(session_id);
        // The journal itself stays on disk — teardown is not deletion, and the
        // queue has to be there when the session is resumed. Only the in-memory
        // handles go, or every session the daemon ever loads leaks two map
        // entries for its lifetime.
        self.journals.remove(session_id);
        self.integrity.remove(session_id);
    }

    /// Mark the session as parked on the review gate.
    ///
    /// The returned [`GateHold`] releases it on drop, which is the only thing
    /// that covers a turn cancelled or timed out while the gate loop sits on
    /// its sleep. Hold it for exactly as long as the call is blocked.
    pub fn hold_gate(self: &Arc<Self>, session_id: &str, block: GateBlock) -> GateHold {
        self.gate.insert(session_id.to_string(), block);
        GateHold {
            session_id: session_id.to_string(),
            ledgers: Arc::downgrade(self),
        }
    }

    /// What this session's turn is waiting on, if it is waiting at all.
    pub fn gate_block(&self, session_id: &str) -> Option<GateBlock> {
        self.gate.get(session_id).map(|r| r.value().clone())
    }

    /// Open a capture bracket: record every tracked root's current tree.
    ///
    /// Call immediately before dispatching a tool call that could write, and
    /// pair with [`Self::close`] on every exit path. Taking `&Arc<Self>` is
    /// what lets the returned handle deregister itself if it is dropped
    /// instead — see [`CaptureHandle`].
    pub async fn open_bracket(self: &Arc<Self>, session_id: &str) -> ReviewResult<CaptureHandle> {
        let ledger = self
            .ledgers
            .get(session_id)
            .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;
        let roots: Vec<PathBuf> = ledger.roots().map(Path::to_path_buf).collect();
        drop(ledger);

        let id = self.next_handle.fetch_add(1, Ordering::Relaxed);
        let mut before = Vec::with_capacity(roots.len());
        for root in roots {
            match workspace_snapshot::capture_tree(&root).await {
                Ok(tree) => {
                    self.mark_open(&root, id);
                    before.push((root, TreeSha::new(tree)));
                }
                Err(e) => {
                    // Leaving the roots captured so far registered would make
                    // every later bracket on them look contested forever.
                    for (opened, _) in &before {
                        self.mark_closed(opened, id);
                    }
                    return Err(e.into());
                }
            }
        }
        Ok(CaptureHandle {
            id,
            before,
            ledgers: Arc::downgrade(self),
            _window: self.suppress(session_id),
        })
    }

    /// Re-baseline an open bracket: recapture every root and overwrite the
    /// tree the interval will be measured from.
    ///
    /// The bracket cannot be opened late enough to avoid every wait. A writer
    /// — the `pre_tool_call` handlers, which can run bash inside a container
    /// over the same bind-mounted workspace — sits *above* a waiter, the
    /// permission prompt. Opening after the writer would report its edits as
    /// `external`; opening before the waiter folds whatever the human types
    /// while deciding into the agent's interval. Re-baselining resolves it in
    /// the only direction that adds no close edge: the handle stays the
    /// caller's local, so cancellation still unwinds to one `Drop`.
    ///
    /// A root whose recapture fails is dropped from the handle rather than
    /// left on its stale tree. A stale tree would attribute the human's edits
    /// to this call, which is the whole thing this exists to prevent; dropping
    /// it deregisters the root and leaves the call unattributed there — the
    /// same policy [`Self::close`] applies to a capture failure.
    pub async fn rebase(&self, handle: &mut CaptureHandle) {
        let mut i = 0;
        while i < handle.before.len() {
            let root = handle.before[i].0.clone();
            match workspace_snapshot::capture_tree(&root).await {
                Ok(sha) => {
                    handle.before[i].1 = TreeSha::new(sha);
                    i += 1;
                }
                Err(e) => {
                    // Removed before deregistering, and with no await between,
                    // so the `Drop` backstop can neither miss this root nor
                    // see it twice.
                    handle.before.remove(i);
                    self.mark_closed(&root, handle.id);
                    warn!(
                        root = %root.display(),
                        error = %e,
                        "review re-baseline failed; tool call left unattributed for this root"
                    );
                }
            }
        }
    }

    /// Close a bracket and record an interval if anything changed.
    ///
    /// Returns `true` when an interval was appended. A tool call that wrote
    /// nothing leaves every tree SHA equal and produces no interval and no
    /// card — which is why dedupe is on the tree and not on whether the call
    /// *claimed* to write.
    pub async fn close(
        &self,
        session_id: &str,
        mut handle: CaptureHandle,
        tool_call_id: &str,
        node_id: u32,
    ) -> ReviewResult<bool> {
        let mut touched = Vec::new();
        let mut contested = false;
        // Consumed root by root, deregistering *before* the await. Taking the
        // whole vec up front would disarm the handle's `Drop` backstop for
        // every root while only the first had been processed, so dropping the
        // turn future mid-loop — its cancel arm, its execution timeout — left
        // the remainder registered for the daemon's lifetime. Popping keeps
        // `Drop` armed for exactly the roots this loop has not reached, and
        // the deregistration point is the interval's true end anyway: the
        // bracket stops being able to witness a concurrent writer once its
        // last `write-tree` is in flight.
        while let Some((root, before_tree)) = handle.before.pop() {
            contested |= self.mark_closed(&root, handle.id);
            match workspace_snapshot::capture_tree(&root).await {
                Ok(sha) if sha != before_tree.as_str() => touched.push(RootInterval {
                    root: PhysicalRoot::from_top_level(root),
                    before_tree,
                    after_tree: TreeSha::new(sha),
                }),
                Ok(_) => {}
                // A capture failure means we cannot say what this call did to
                // this root. Recording a half-interval would attribute the
                // root's later hunks to the wrong call, so drop it and let
                // them surface as external.
                Err(e) => warn!(
                    root = %root.display(),
                    error = %e,
                    "review capture failed; tool call left unattributed for this root"
                ),
            }
        }

        if touched.is_empty() {
            return Ok(false);
        }
        let interval = Interval {
            tool_call_id: tool_call_id.to_string(),
            node_id,
            roots_touched: touched,
            contested,
            child_session_id: None,
        };
        if !self.ledgers.contains_key(session_id) {
            return Err(ReviewError::NoLedger(session_id.to_string()));
        }
        self.record_interval(session_id, interval).await;
        // The interval's two trees are `write-tree` output, reachable from no
        // ref, so they are gc bait until this call claims them.
        self.refresh_keep_refs(session_id).await;
        Ok(true)
    }

    /// Record that a `delegate_session` call spawned a child with its own
    /// ledger. Attribution depth follows session depth.
    ///
    /// `node_id` is the parent-side turn the delegation was issued from, and
    /// is `Option` all the way down — see [`ChildLedgerRef::node_id`].
    pub async fn link_child(
        &self,
        session_id: &str,
        tool_call_id: &str,
        child_session_id: &str,
        node_id: Option<u32>,
    ) {
        let child = ChildLedgerRef {
            tool_call_id: tool_call_id.to_string(),
            child_session_id: child_session_id.to_string(),
            node_id,
        };
        {
            let Some(mut ledger) = self.ledgers.get_mut(session_id) else {
                return;
            };
            ledger.link_child(child.clone());
        }
        self.append(session_id, journal::Record::Child(child)).await;
    }

    /// Fold a delegated child's intervals into its parent's ledger.
    ///
    /// The seam the delegation harvest must use. `delegate_session` is
    /// deliberately not bracketed — a parent bracket would overlap every one of
    /// the child's and mark both contested, turning the whole delegation
    /// `external` — so the child's own intervals are the only attribution that
    /// exists for delegated work, and they have to be copied up.
    ///
    /// A method here rather than something the caller does with a guard, for
    /// the reason in the module docs: harvesting the obvious way would look
    /// right until the next restart, reintroducing the persistence defect for
    /// delegated work specifically.
    ///
    /// Only intervals over roots the parent also tracks are taken: a child in
    /// its own worktree changed nothing the parent's composed diff can show.
    /// Returns how many were absorbed.
    pub async fn absorb_child_intervals(&self, parent: &str, child: &str) -> usize {
        let (Some(child_ledger), Some(parent_ledger)) = (self.ledger(child), self.ledger(parent))
        else {
            return 0;
        };
        let parent_roots: Vec<PathBuf> = parent_ledger.roots().map(Path::to_path_buf).collect();
        // The parent-side turn the delegation was issued from, recorded by
        // `link_child`. A `node_id` indexes a `ConversationTree` and is
        // meaningless in another one, so carrying the child's own index up
        // compares two unrelated coordinate systems: a child that ran twenty
        // turns produces indices larger than the parent's current turn, and
        // `blocking_earlier_turn`'s `node_id < this_turn` then reads harvested
        // work as "not yet happened" and stops gating the parent's next
        // delegation — a bypass in the one fallback the delegation arm has.
        //
        // `0` when the link carries no turn, which is a row written before the
        // field existed. It is the earliest possible turn, so the harvest still
        // gates; the cost is a delegation card that reads as turn 0, and a
        // wrong display coordinate is the cheaper of the two errors.
        let node_id = parent_ledger
            .children()
            .iter()
            .find(|link| link.child_session_id == child)
            .and_then(|link| link.node_id)
            .unwrap_or(0);
        let mut absorbed = 0;
        for interval in child_ledger.intervals() {
            let roots_touched: Vec<RootInterval> = interval
                .roots_touched
                .iter()
                .filter(|r| parent_roots.iter().any(|p| *p == *r.root))
                .cloned()
                .collect();
            if roots_touched.is_empty() {
                continue;
            }
            // Same `tool_call_id` on both sides: the call is the child's, and
            // minting a parent-side id would make the delegation card point at
            // a call that never appears in either transcript.
            if parent_ledger
                .intervals()
                .iter()
                .any(|i| i.tool_call_id == interval.tool_call_id)
            {
                continue;
            }
            // Stamped with the child it came from, not rewritten to look like
            // the parent's own work: `tool_call_id` still names a call in the
            // child's transcript, and only this field says where to look it
            // up.
            let absorbed_interval = Interval {
                roots_touched,
                node_id,
                child_session_id: Some(child.to_string()),
                ..interval.clone()
            };
            self.record_interval(parent, absorbed_interval).await;
            absorbed += 1;
        }
        if absorbed > 0 {
            self.refresh_keep_refs(parent).await;
        }
        absorbed
    }

    /// The composed diff for a session, attributed and carrying review state.
    pub async fn list_hunks(&self, session_id: &str) -> ReviewResult<Vec<ComposedHunk>> {
        Ok(self.list_hunks_with_status(session_id).await?.0)
    }

    /// The composed diff, plus what the ledger can and cannot vouch for.
    ///
    /// A degraded root contributes **no hunks**, which is why the statuses
    /// have to come back beside them: losing attribution fails open on its own
    /// (see [`crucible_core::session::Integrity`]), so "this root produced
    /// nothing" and "this root cannot be read" must be distinguishable by every
    /// caller that gates on the answer.
    ///
    /// Read-only, in the strongest sense: this is the gate poller's half-second
    /// `&self` loop, and it prunes nothing. A decision recorded for a hunk with
    /// no live counterpart round-trips unconditionally, because the `reapplied`
    /// derivation below is defined by that decision still being there.
    pub async fn list_hunks_with_status(
        &self,
        session_id: &str,
    ) -> ReviewResult<(Vec<ComposedHunk>, Vec<RootStatus>)> {
        // Clone the ledger out rather than holding a DashMap guard across the
        // git awaits below — a guard held across an await is a deadlock
        // waiting for the next writer.
        let ledger = self
            .ledger(session_id)
            .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;
        let states = self.states.get(session_id).map(|r| r.value().clone());
        let integrity = self
            .integrity
            .get(session_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let mut all = Vec::new();
        let mut statuses = Vec::new();
        for base in ledger.session_base() {
            // Structural failure — a root that is gone, a base tree gc'd out
            // from under us — is evidence that attribution is broken rather
            // than merely unavailable, so it degrades rather than erroring.
            // Erroring would take today's transient fail-open path and let the
            // write through. `review.rebase` is what bounds the block.
            if let Some(reason) = degraded_reason(&integrity, base).await {
                warn!(
                    session_id,
                    root = %base.root.display(),
                    reason = %reason,
                    "review ledger cannot account for this root; writes under it are held"
                );
                statuses.push(RootStatus::degraded(base.root.clone(), reason));
                continue;
            }
            statuses.push(RootStatus::intact(base.root.clone()));

            let current = TreeSha::new(workspace_snapshot::capture_tree(&base.root).await?);
            let mut composition =
                compose::compose_root(&base.root, &base.base_tree, &current).await?;

            let intervals: Vec<&Interval> = ledger
                .intervals()
                .iter()
                .filter(|i| !i.contested)
                .filter(|i| i.roots_touched.iter().any(|r| r.root == base.root))
                .collect();
            attribute::attribute_root(&base.root, &mut composition, &intervals).await?;

            for mut hunk in composition.hunks {
                let recorded = states
                    .as_ref()
                    .and_then(|s| s.get(&hunk.id))
                    .copied()
                    .unwrap_or_default();
                // A rejection reverts, so an identity recorded `Rejected` and
                // present in the composed diff anyway means the agent applied
                // the same change again. Resolving that to `Rejected` is what
                // let a live worktree change read as reviewed and stopped the
                // gate blocking on it; the decision is split off the history
                // instead, and the history is derived rather than stored so
                // this read path deletes nothing.
                hunk.reapplied = recorded == ReviewState::Rejected;
                hunk.state = if hunk.reapplied {
                    ReviewState::Unreviewed
                } else {
                    recorded
                };
                all.push(hunk);
            }
        }
        Ok((all, statuses))
    }

    /// Hunks the gate may block on: unreviewed and owned by the ledger.
    ///
    /// External hunks are excluded on purpose. They are the user's own edits
    /// (or writes no bracket saw), and refusing to let the agent work until
    /// the user reviews their own typing is nonsense.
    pub async fn unreviewed_hunks(&self, session_id: &str) -> ReviewResult<Vec<ComposedHunk>> {
        Ok(self
            .list_hunks(session_id)
            .await?
            .into_iter()
            .filter(|h| h.state == ReviewState::Unreviewed && !h.is_external())
            .collect())
    }

    /// Whether a write to one file may proceed — the per-write gate query.
    ///
    /// `spellings` are the candidate spellings of a **single** file, absolute
    /// or root-relative, and a match on any of them answers for all. There is
    /// more than one because a tool's relative path argument has no single
    /// base: `write_file` names a workspace-relative path and `create_note`
    /// names a kiln-relative one, while a hunk's own `path` is relative to its
    /// repository top level — which is a third thing again whenever one of
    /// those directories contains the other. Resolving against one base alone
    /// matched nothing for the rest, and a gate query that matches nothing is
    /// a gate that never blocks.
    ///
    /// An absolute spelling is normalised before it is compared. Roots are
    /// stored as `git rev-parse --show-toplevel` printed them — physical —
    /// while the caller resolves against directories as the session was
    /// *registered*, so a workspace reached through a symlink produces two
    /// spellings of one file. Normalising here and not at ingest is
    /// deliberate: see [`paths`].
    ///
    /// Three-valued because "nothing unreviewed here" and "I cannot tell" are
    /// different facts with opposite safety properties — see [`Verdict`]. A
    /// file under no tracked root is [`Verdict::Clear`] and always has been:
    /// no hunk exists there, so no human action could release a hold, and
    /// blocking would be an unreleasable hang rather than a safe default. The
    /// [`Verdict::Degraded`] cases are the ones `review.rebase` bounds.
    pub async fn has_unreviewed_in_file(
        &self,
        session_id: &str,
        spellings: &[PathBuf],
    ) -> ReviewResult<Verdict> {
        // An unscoped loss is checked before anything else: it can mean the
        // ledger no longer knows which roots it tracked, so there is no root
        // list left to compare `file` against.
        if self
            .integrity
            .get(session_id)
            .is_some_and(|i| i.blocks_everything())
        {
            return Ok(Verdict::Degraded { root: None });
        }

        let (hunks, statuses) = self.list_hunks_with_status(session_id).await?;
        // Resolved once rather than per hunk: each is a syscall, and the answer
        // does not depend on which hunk it is compared against.
        let resolved: Vec<PathBuf> = spellings
            .iter()
            .filter(|s| s.is_absolute())
            .map(|s| paths::resolve(s))
            .collect();
        let unreviewed = hunks
            .iter()
            .filter(|h| h.state == ReviewState::Unreviewed && !h.is_external())
            .any(|h| {
                let absolute = h.absolute_path();
                resolved.contains(&absolute)
                    || spellings
                        .iter()
                        .any(|s| *s == absolute || Path::new(&h.path) == s.as_path())
            });
        if unreviewed {
            return Ok(Verdict::Unreviewed);
        }

        let mut degraded = statuses.iter().filter(|s| s.is_degraded());
        let held = if resolved.is_empty() {
            // No absolute spelling, so no root to compare against and no
            // telling whether the target lands in the degraded one. Fail
            // closed.
            degraded.next()
        } else {
            degraded.find(|s| resolved.iter().any(|r| r.starts_with(&s.root)))
        };
        Ok(match held {
            Some(status) => Verdict::Degraded {
                root: Some(status.root.clone()),
            },
            None => Verdict::Clear,
        })
    }

    /// Record a decision about a hunk.
    ///
    /// [`ReviewState::Rejected`] reverts through [`Self::revert_hunk`]: a
    /// rejection recorded without the revert having happened is a lie about
    /// the state of the worktree, and §3 requires the write to land
    /// immediately rather than be deferred into a conflict-prone queue.
    pub async fn set_state(
        &self,
        session_id: &str,
        hunk_id: &HunkId,
        state: ReviewState,
    ) -> ReviewResult<()> {
        if state == ReviewState::Rejected {
            return self.revert_hunk(session_id, hunk_id).await;
        }
        let hunks = self.list_hunks(session_id).await?;
        if !hunks.iter().any(|h| &h.id == hunk_id) {
            return Err(ReviewError::UnknownHunk(hunk_id.clone()));
        }
        self.record_state(session_id, hunk_id, state).await;
        Ok(())
    }

    /// Revert one composed hunk in the worktree and mark it rejected.
    ///
    /// The revert is against `session_base`, which is what makes it safe: the
    /// hunk's base text is restored regardless of how many tool calls
    /// contributed to it, with no three-way merge.
    pub async fn revert_hunk(&self, session_id: &str, hunk_id: &HunkId) -> ReviewResult<()> {
        let hunk = self
            .list_hunks(session_id)
            .await?
            .into_iter()
            .find(|h| &h.id == hunk_id)
            .ok_or_else(|| ReviewError::UnknownHunk(hunk_id.clone()))?;
        if hunk.is_external() {
            return Err(ReviewError::ExternalHunk(hunk_id.clone()));
        }

        let path = hunk.absolute_path();
        // A *missing* file reads as empty on purpose: that is what reverting the
        // agent's deletion of a whole file looks like, and the write below
        // recreates it from `before_content`. Any other read failure is not
        // evidence of emptiness — treating a permission error as "no lines"
        // would pass the staleness guard for a deletion hunk and then overwrite
        // a file we could not read.
        let current = match tokio::fs::read_to_string(&path).await {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(_) => {
                return Err(ReviewError::Stale {
                    path: hunk.path.clone(),
                })
            }
        };
        let lines = compose::lines(&current);
        // The worktree may have moved since the hunk was computed. Rewriting
        // lines we can no longer recognise would silently destroy whatever is
        // there now, so refuse and let the caller re-list.
        if compose::slice(&lines, &hunk.current_range) != hunk.after_content {
            return Err(ReviewError::Stale {
                path: hunk.path.clone(),
            });
        }

        let start = hunk.current_range.start.saturating_sub(1) as usize;
        // The guard above cannot see this case. A pure deletion has an EMPTY
        // `current_range` and `after_content == ""`, and `slice` answers `""`
        // for any empty range — so `"" == ""` passes however far the file has
        // shrunk underneath. Unclamped, `lines[..start]` then panics, and the
        // release profile aborts rather than unwinds, taking every live session
        // with it. A start past the end is the same evidence the guard looks
        // for: the file is no longer the one this hunk was computed against.
        if start > lines.len() {
            return Err(ReviewError::Stale {
                path: hunk.path.clone(),
            });
        }
        let end = (hunk.current_range.end.saturating_sub(1) as usize).min(lines.len());
        let mut next = String::with_capacity(current.len());
        next.push_str(&lines[..start].concat());
        next.push_str(&hunk.before_content);
        next.push_str(&lines[end..].concat());
        // Held across the write and released after it: this is the daemon
        // editing the user's worktree, and the watcher cannot tell it apart
        // from the user doing so. Unsuppressed, rejecting a hunk announces an
        // external change for the file it just reverted — the queue reporting
        // its own drain as new work.
        let suppressed = self.suppress(session_id);
        tokio::fs::write(&path, next).await?;
        drop(suppressed);

        self.record_state(session_id, hunk_id, ReviewState::Rejected)
            .await;
        Ok(())
    }

    /// Store a range-anchored comment. Build it with [`Comment::new`].
    pub async fn add_comment(&self, session_id: &str, comment: Comment) {
        self.comments
            .entry(session_id.to_string())
            .or_default()
            .push(comment.clone());
        self.append(session_id, journal::Record::Comment(comment))
            .await;
    }

    pub fn comments(&self, session_id: &str) -> Vec<Comment> {
        self.comments
            .get(session_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Mark a comment resolved. Unknown ids are an error rather than a
    /// silent success, so a stale client learns its view is out of date.
    pub async fn resolve_comment(&self, session_id: &str, comment_id: &str) -> ReviewResult<()> {
        {
            let mut comments = self
                .comments
                .get_mut(session_id)
                .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;
            let Some(comment) = comments.iter_mut().find(|c| c.id == comment_id) else {
                return Err(ReviewError::UnknownComment(comment_id.to_string()));
            };
            comment.resolved = true;
        }
        self.append(
            session_id,
            journal::Record::CommentResolved {
                comment: comment_id.to_string(),
            },
        )
        .await;
        Ok(())
    }

    /// What the ledger could not read back for this session.
    pub fn integrity(&self, session_id: &str) -> Integrity {
        self.integrity
            .get(session_id)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Register an open bracket on `root`, marking every bracket now open
    /// there as contested.
    fn mark_open(&self, root: &Path, id: u64) {
        let mut brackets = self.open.entry(root.to_path_buf()).or_default();
        brackets.push(OpenBracket {
            id,
            contested: false,
        });
        if brackets.len() > 1 {
            // Mark all of them, not just the newcomer: the bracket that was
            // already open is equally unable to say which writer made a
            // change during the overlap.
            for bracket in brackets.iter_mut() {
                bracket.contested = true;
            }
        }
    }

    /// Deregister a bracket, returning whether it was ever contested.
    fn mark_closed(&self, root: &Path, id: u64) -> bool {
        let Some(mut brackets) = self.open.get_mut(root) else {
            return false;
        };
        let Some(pos) = brackets.iter().position(|b| b.id == id) else {
            return false;
        };
        brackets.remove(pos).contested
    }
}

/// Why one root's attribution cannot be trusted, or `None` when it can.
///
/// Ordered cheapest-first: a journal gap is already known, a missing directory
/// is one `stat`, and only a root that survives both costs a `git cat-file`.
async fn degraded_reason(integrity: &Integrity, base: &RootBase) -> Option<String> {
    if integrity.blocks(&base.root) {
        return Some("journal records for this root could not be read".to_string());
    }
    if !base.root.is_dir() {
        return Some("tracked root no longer exists".to_string());
    }
    if !git::tree_exists(&base.root, &base.base_tree).await {
        return Some(format!(
            "session base tree {} is no longer in the object store",
            base.base_tree
        ));
    }
    None
}
