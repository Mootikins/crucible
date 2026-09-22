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
//! and an external ACP agent share. That is also why attribution works for
//! an external agent, whose tools the daemon does not run.
//!
//! **Any write to a [`Ledger`] that does not go through a [`ReviewLedgers`]
//! method is a persistence bug.** The mutators are methods on the value, so a
//! caller holding a `DashMap` guard reaches them and compiles cleanly while
//! never reaching [`journal`]. See [`Ledger::push_interval_in_memory`], which
//! is named for exactly that. The journal append lives beside every mutation
//! here for that reason and no other.

mod attribute;
pub(crate) mod backend;
mod compose;
mod error;
pub(crate) mod git;
mod journal;
mod persist;
mod plain_store;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use crucible_core::session::{
    ChildLedgerRef, Comment, ComposedHunk, Integrity, Interval, Ledger, PhysicalRoot, RootBase,
    RootInterval, RootStatus, SnapshotId,
};
use dashmap::DashMap;
use tracing::{debug, warn};

use backend::RootBackend;
use crucible_core::diff::{DiffFileEntry, DiffFileText, FileStatus, UnreadableRoot};
use crucible_core::types::acp::MAX_DIFF_BYTES;

use crate::diff::branch::FileText;
use crate::diff::comments::CommentStore;
use crucible_core::diff::DiffsetId;
use crucible_core::session::SessionId;

pub use error::{ReviewError, ReviewResult};
pub use persist::{drop_keep_refs, sweep_review_refs};

/// Where a daemon rooted at `data_home` snapshots the review roots that are
/// not in a git repository.
///
/// One spelling, because three callers need the same directory: the manager
/// that captures into it, the delete that releases a session's claim on it,
/// and the maintenance sweep that collects it.
pub fn snapshot_root(data_home: &std::path::Path) -> std::path::PathBuf {
    data_home.join("review-snapshots")
}

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
    before: Vec<(PathBuf, SnapshotId)>,
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

/// Per-session review ledgers.
///
/// Mirrors `SnapshotMap`: a `DashMap` owned by `AgentManager`, cleared on
/// session end. Unlike snapshots, entries are read on demand by RPC handlers
/// rather than consumed once by undo, so they are cloned out rather than
/// removed.
pub struct ReviewLedgers {
    /// Snapshots of the review roots that are not in a git repository.
    ///
    /// A value rather than a `Default`, because the store writes under the
    /// daemon's data root and a default would have to invent one — which, in a
    /// test, means the developer's real `~/.crucible`. The path arrives through
    /// [`crate::agent_manager::AgentManagerParams`] for that reason.
    plain: plain_store::PlainStore,
    ledgers: DashMap<String, Ledger>,
    /// The comments of each diffset. A session keeps no comments of its
    /// own: the comments of its record belong to its session record diffset.
    comments: CommentStore,
    /// Capture brackets currently open per root, across all sessions.
    /// Overlap on a shared root is what makes bracketing unsound (§5), so it
    /// is tracked globally rather than per session — the concurrent writers
    /// are usually a parent and its delegated children.
    open: DashMap<PathBuf, Vec<OpenBracket>>,
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
    /// Here so that an open capture bracket takes a suppression window. Bound
    /// after construction because the watch needs an async constructor and
    /// this type is built by `Default` in `AgentManager::new`.
    external: std::sync::OnceLock<Arc<crate::watch::external_changes::ExternalChangeTracker>>,
    next_handle: AtomicU64,
}

/// The session record diffset of `session_id`: the owner of the comments
/// that the `review.*` methods make.
pub(crate) fn record_diffset(session_id: &str) -> ReviewResult<DiffsetId> {
    SessionId::parse(session_id)
        .map(|session| DiffsetId::for_session(&session))
        .map_err(|e| ReviewError::InvalidSession(e.to_string()))
}

/// A comment store failure is a daemon-side I/O fault.
fn store_error(error: anyhow::Error) -> ReviewError {
    ReviewError::Io(std::io::Error::other(format!("{error:#}")))
}

#[derive(Debug)]
struct OpenBracket {
    id: u64,
    contested: bool,
}

impl ReviewLedgers {
    /// Build the ledgers, storing plain-root snapshots under `plain_root`
    /// and comments in `comments`.
    ///
    /// The daemon passes `<data home>/review-snapshots`; a test passes a
    /// directory it owns. There is no `Default`: a review root outside git is
    /// snapshotted into this directory, and nothing may guess where it is.
    pub fn new(plain_root: PathBuf, comments: CommentStore) -> Self {
        Self {
            plain: plain_store::PlainStore::new(plain_root),
            ledgers: DashMap::new(),
            comments,
            open: DashMap::new(),
            journals: DashMap::new(),
            integrity: DashMap::new(),
            parents: DashMap::new(),
            external: std::sync::OnceLock::new(),
            next_handle: AtomicU64::new(0),
        }
    }

    /// Build the ledgers for a test, with the comment store in `plain_root`.
    ///
    /// The plain snapshot store reads only its own names in `plain_root`, so
    /// the `diff-comments` directory there does not disturb it.
    #[cfg(test)]
    pub(crate) fn for_tests(plain_root: PathBuf) -> Self {
        let comments = CommentStore::new(plain_root.join("diff-comments"));
        Self::new(plain_root, comments)
    }

    /// Open a ledger for a session over the roots it may write to, capturing
    /// `session_base` once.
    ///
    /// A root inside a repository is normalised to its top level and
    /// deduplicated, so a workspace and a kiln inside one repo become a single
    /// tracked root. A root outside one is tracked too, over the plain
    /// snapshot store — a kiln outside git is the expected shape, not an
    /// exotic one. Only a root that is not there at all is skipped.
    ///
    /// Re-opening an already-open session is a no-op. `session_base` is
    /// captured exactly once and never recomputed; see [`Self::restore`].
    pub async fn open(&self, session_id: &str, roots: &[PathBuf]) -> ReviewResult<()> {
        if self.ledgers.contains_key(session_id) {
            return Ok(());
        }

        let mut base: Vec<RootBase> = Vec::new();
        for root in roots {
            let Ok((top, backend)) = RootBackend::detect(root).await else {
                debug!(root = %root.display(), "root cannot be reached; not tracked for review");
                continue;
            };
            if base.iter().any(|b| b.root == top) {
                continue;
            }
            let base_tree = backend.capture(&self.plain, &top).await?;
            base.push(RootBase {
                root: top,
                base_tree,
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
    /// path, so later intervals live only for the daemon's lifetime.
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
            || self.parents.contains_key(session_id)
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
    /// An open capture bracket holds one. Without it the watcher reports
    /// each write of the bracketed call as a change that the user made.
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
        self.parents.remove(session_id);
        // The journal itself stays on disk — teardown is not deletion, and the
        // record has to be there when the session is resumed. Only the in-memory
        // handles go, or every session the daemon ever loads leaks two map
        // entries for its lifetime.
        self.journals.remove(session_id);
        self.integrity.remove(session_id);
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
        // Read off `session_base` rather than `roots()`: the base id is what
        // says which store this root's snapshots live in, and a bracket that
        // guessed would capture into the wrong one.
        let roots: Vec<(PathBuf, RootBackend)> = ledger
            .session_base()
            .iter()
            .map(|base| (base.root.to_path_buf(), RootBackend::of(&base.base_tree)))
            .collect();
        drop(ledger);

        let id = self.next_handle.fetch_add(1, Ordering::Relaxed);
        let mut before = Vec::with_capacity(roots.len());
        for (root, backend) in roots {
            match backend.capture(&self.plain, &root).await {
                Ok(snapshot) => {
                    self.mark_open(&root, id);
                    before.push((root, snapshot));
                }
                Err(e) => {
                    // Leaving the roots captured so far registered would make
                    // every later bracket on them look contested forever.
                    for (opened, _) in &before {
                        self.mark_closed(opened, id);
                    }
                    return Err(e);
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
            let backend = RootBackend::of(&handle.before[i].1);
            match backend.capture(&self.plain, &root).await {
                Ok(snapshot) => {
                    handle.before[i].1 = snapshot;
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
            let backend = RootBackend::of(&before_tree);
            match backend.capture(&self.plain, &root).await {
                Ok(after_tree) if after_tree != before_tree => touched.push(RootInterval {
                    root: PhysicalRoot::from_top_level(root),
                    before_tree,
                    after_tree,
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
        // a turn comparison then reads harvested work as "not yet happened".
        //
        // `0` when the link carries no turn, which is a row written before the
        // field existed. It is the earliest possible turn; the cost is a
        // delegation card that reads as turn 0.
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

    /// The composed diff for a session, attributed to its tool calls.
    pub async fn list_hunks(&self, session_id: &str) -> ReviewResult<Vec<ComposedHunk>> {
        Ok(self.list_hunks_with_status(session_id).await?.0)
    }

    /// The composed diff, plus what the ledger can and cannot vouch for.
    ///
    /// A degraded root contributes **no hunks**, which is why the statuses
    /// have to come back beside them: losing attribution is silent on its own
    /// (see [`crucible_core::session::Integrity`]), so "this root produced
    /// nothing" and "this root cannot be read" must be distinguishable by every
    /// caller.
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
            // The listing then names the root with a reason.
            let backend = RootBackend::of(&base.base_tree);
            if let Some(reason) = degraded_reason(&integrity, base, backend, &self.plain).await {
                warn!(
                    session_id,
                    root = %base.root.display(),
                    reason = %reason,
                    "review ledger cannot account for this root"
                );
                statuses.push(RootStatus::degraded(base.root.clone(), reason));
                continue;
            }
            statuses.push(RootStatus::intact(base.root.clone()));

            let current = backend.capture(&self.plain, &base.root).await?;
            let mut composition =
                compose::compose_root(backend, &self.plain, &base.root, &base.base_tree, &current)
                    .await?;

            let intervals: Vec<&Interval> = ledger
                .intervals()
                .iter()
                .filter(|i| !i.contested)
                .filter(|i| i.roots_touched.iter().any(|r| r.root == base.root))
                .collect();
            attribute::attribute_root(
                backend,
                &self.plain,
                &base.root,
                &mut composition,
                &intervals,
            )
            .await?;
            all.extend(composition.hunks);
        }
        Ok((all, statuses))
    }

    /// The files of the session record: each path that differs between the
    /// session base and the disk, with its line counts and no text.
    ///
    /// A session with no ledger has an empty record, because it has no base
    /// to compare with. A root that the ledger cannot read contributes no
    /// files, as in [`Self::list_hunks_with_status`]. The record names that
    /// root and the reason, so that a client does not show it as complete.
    pub(crate) async fn record_files(&self, session_id: &str) -> ReviewResult<RecordFiles> {
        let mut record = RecordFiles::default();
        let Some(ledger) = self.ledger(session_id) else {
            return Ok(record);
        };
        let integrity = self
            .integrity
            .get(session_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();
        for base in ledger.session_base() {
            let backend = RootBackend::of(&base.base_tree);
            if let Some(reason) = degraded_reason(&integrity, base, backend, &self.plain).await {
                warn!(
                    session_id,
                    root = %base.root.display(),
                    reason = %reason,
                    "the session record leaves out a root that the ledger cannot read"
                );
                record.unreadable_roots.push(UnreadableRoot {
                    root: base.root.clone(),
                    reason,
                });
                continue;
            }
            let current = backend.capture(&self.plain, &base.root).await?;
            let changes = backend
                .changed_paths(&self.plain, &base.root, &base.base_tree, &current)
                .await?;
            for (path, kind) in changes {
                let before = self
                    .snapshot_text(backend, base, &base.base_tree, &path, kind.has_before())
                    .await?;
                let after = self
                    .snapshot_text(backend, base, &current, &path, kind.has_after())
                    .await?;
                record
                    .files
                    .push(record_entry(&base.root, path, kind, &before, &after));
            }
        }
        record
            .files
            .sort_by(|a, b| (&a.root, &a.path).cmp(&(&b.root, &b.path)));
        Ok(record)
    }

    /// The two texts of one file of the session record: the session base
    /// snapshot, and the file on disk.
    ///
    /// `root` must be one of the roots of the ledger. The caller checks that
    /// `path` is a plain relative path that stays inside `root`.
    pub(crate) async fn record_text(
        &self,
        session_id: &str,
        root: &PhysicalRoot,
        path: &str,
    ) -> ReviewResult<DiffFileText> {
        let ledger = self
            .ledger(session_id)
            .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;
        let base = ledger
            .session_base()
            .iter()
            .find(|b| &b.root == root)
            .ok_or_else(|| ReviewError::PathEscapesRoot {
                path: root.to_path_buf(),
            })?;
        let backend = RootBackend::of(&base.base_tree);
        let exists = backend
            .contains_path(&self.plain, &base.root, &base.base_tree, path)
            .await?;
        let before = self
            .snapshot_text(backend, base, &base.base_tree, path, exists)
            .await?;
        let after = crate::diff::branch::disk_text(&base.root.join(path))
            .await
            .map_err(|e| std::io::Error::other(format!("{e:#}")))?;
        Ok(DiffFileText {
            base_text: before.into_shown(),
            current_text: after.into_shown(),
        })
    }

    /// The text of `path` in the snapshot `snap` of one root.
    async fn snapshot_text(
        &self,
        backend: RootBackend,
        base: &RootBase,
        snap: &SnapshotId,
        path: &str,
        exists: bool,
    ) -> ReviewResult<FileText> {
        if !exists {
            return Ok(FileText::Absent);
        }
        Ok(
            match backend.blob(&self.plain, &base.root, snap, path).await? {
                None => FileText::Binary,
                Some(text) if text.len() > MAX_DIFF_BYTES => FileText::TooLarge,
                Some(text) => FileText::Text(text),
            },
        )
    }

    /// The store of the comments of every diffset.
    pub fn comment_store(&self) -> &CommentStore {
        &self.comments
    }

    /// Store a range-anchored comment under its diffset. Build it with
    /// [`Comment::new`].
    pub fn add_comment(&self, comment: &Comment) -> ReviewResult<()> {
        self.comments.add(comment).map_err(store_error)
    }

    /// The comments of the session record of `session_id`.
    pub fn comments(&self, session_id: &str) -> ReviewResult<Vec<Comment>> {
        self.comments
            .list(&record_diffset(session_id)?)
            .map_err(store_error)
    }

    /// Mark a comment of the session record resolved. Unknown ids are an
    /// error rather than a silent success, so a stale client learns its view
    /// is out of date.
    pub fn resolve_comment(&self, session_id: &str, comment_id: &str) -> ReviewResult<()> {
        self.resolve_diffset_comment(&record_diffset(session_id)?, comment_id)
    }

    /// Mark a comment of `diffset` resolved. An unknown id is an error.
    pub fn resolve_diffset_comment(
        &self,
        diffset: &DiffsetId,
        comment_id: &str,
    ) -> ReviewResult<()> {
        if self
            .comments
            .resolve(diffset, comment_id)
            .map_err(store_error)?
        {
            Ok(())
        } else {
            Err(ReviewError::UnknownComment(comment_id.to_string()))
        }
    }

    /// Remove a comment of `diffset` from the store. An unknown id is an error.
    ///
    /// Delete is not resolve: the comment leaves the store, so no listing and
    /// no quickfix line holds it again.
    pub fn delete_diffset_comment(
        &self,
        diffset: &DiffsetId,
        comment_id: &str,
    ) -> ReviewResult<()> {
        if self
            .comments
            .delete(diffset, comment_id)
            .map_err(store_error)?
        {
            Ok(())
        } else {
            Err(ReviewError::UnknownComment(comment_id.to_string()))
        }
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

/// The entry of one file of the session record.
///
/// A binary or too-large side gives no counts, because the daemon does not
/// compare a text that it does not send.
fn record_entry(
    root: &PhysicalRoot,
    path: String,
    kind: git::ChangeKind,
    before: &FileText,
    after: &FileText,
) -> DiffFileEntry {
    let status = match kind {
        git::ChangeKind::Added => FileStatus::Added,
        git::ChangeKind::Modified => FileStatus::Modified,
        git::ChangeKind::Deleted => FileStatus::Deleted,
    };
    let binary = matches!(before, FileText::Binary) || matches!(after, FileText::Binary);
    let too_large = matches!(before, FileText::TooLarge) || matches!(after, FileText::TooLarge);
    let (added, removed) = if binary || too_large {
        (0, 0)
    } else {
        line_counts(text_or_empty(before), text_or_empty(after))
    };
    DiffFileEntry {
        root: root.clone(),
        path,
        status,
        added,
        removed,
        binary,
        too_large,
    }
}

fn text_or_empty(text: &FileText) -> &str {
    match text {
        FileText::Text(text) => text,
        FileText::Absent | FileText::Binary | FileText::TooLarge => "",
    }
}

/// The lines that `after` adds and the lines that it removes, from `before`.
pub(crate) fn line_counts(before: &str, after: &str) -> (u32, u32) {
    let (mut added, mut removed) = (0usize, 0usize);
    for op in similar::TextDiff::from_lines(before, after).ops() {
        match *op {
            similar::DiffOp::Equal { .. } => {}
            similar::DiffOp::Insert { new_len, .. } => added += new_len,
            similar::DiffOp::Delete { old_len, .. } => removed += old_len,
            similar::DiffOp::Replace {
                old_len, new_len, ..
            } => {
                added += new_len;
                removed += old_len;
            }
        }
    }
    let clamp = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    (clamp(added), clamp(removed))
}

/// The files of a session record, and the roots that it leaves out.
#[derive(Debug, Default)]
pub(crate) struct RecordFiles {
    pub(crate) files: Vec<DiffFileEntry>,
    pub(crate) unreadable_roots: Vec<UnreadableRoot>,
}

/// Why one root's attribution cannot be trusted, or `None` when it can.
///
/// Ordered cheapest-first: a journal gap is already known, a missing directory
/// is one `stat`, and only a root that survives both costs a lookup in the
/// store its base snapshot came from.
async fn degraded_reason(
    integrity: &Integrity,
    base: &RootBase,
    backend: RootBackend,
    plain: &plain_store::PlainStore,
) -> Option<String> {
    if integrity.blocks(&base.root) {
        return Some("journal records for this root could not be read".to_string());
    }
    if !base.root.is_dir() {
        return Some("tracked root no longer exists".to_string());
    }
    if !backend
        .snapshot_exists(plain, &base.root, &base.base_tree)
        .await
    {
        return Some(format!(
            "session base snapshot {} is no longer stored",
            base.base_tree
        ));
    }
    None
}
