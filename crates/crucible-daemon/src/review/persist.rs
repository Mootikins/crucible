//! The journal side of [`ReviewLedgers`]: restoring a session from
//! `review.jsonl`, appending to it, rebasing off it, and keeping the git trees
//! it names alive.
//!
//! Split from [`super`] for the 1000-line module budget, along the seam that
//! was already there — everything here either reads or writes durable state,
//! and nothing in it decides what a hunk means.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crucible_core::session::{
    HunkId, Integrity, Interval, Ledger, ReviewState, RootBase, RootStatus, Skip, SkipKind, TreeSha,
};
use tracing::{debug, warn};

use super::{git, journal, ReviewError, ReviewLedgers, ReviewResult};
use crate::workspace_snapshot;

impl ReviewLedgers {
    /// Open a session's ledger, restoring it from `review.jsonl` under `dir`
    /// when one is there.
    ///
    /// This is the entry point the send path uses, and the whole reason the
    /// journal exists: without it a daemon restart re-derived `session_base`
    /// from the current worktree, which reports that the agent changed nothing
    /// and empties the review queue of everything done before the restart.
    ///
    /// **A journal that exists and cannot be read never falls through to
    /// [`ReviewLedgers::open`].** Only two things ever set `session_base`: a
    /// session with no journal at all, and an explicit
    /// [`Self::rebase_session`]. An unreadable journal is an error the caller
    /// must see, because the alternative — capturing a fresh base — is the same
    /// data loss wearing a different hat.
    pub async fn open_or_restore(
        &self,
        session_id: &str,
        dir: &Path,
        roots: &[PathBuf],
    ) -> ReviewResult<()> {
        if self.ledgers.contains_key(session_id) {
            return Ok(());
        }
        let path = dir.join(journal::FILE);
        match tokio::fs::try_exists(&path).await {
            Ok(true) => return self.restore_from_journal(session_id, &path).await,
            Ok(false) => {}
            // Cannot prove there is no journal, so must not act as if there is
            // none.
            Err(e) => {
                return Err(ReviewError::Journal {
                    path,
                    reason: e.to_string(),
                })
            }
        }

        self.open(session_id, roots).await?;
        self.journals.insert(session_id.to_string(), path.clone());

        let Some(ledger) = self.ledger(session_id) else {
            return Ok(());
        };
        self.append(session_id, journal::header(session_id)).await;
        for base in ledger.session_base() {
            self.append(
                session_id,
                journal::Record::Base {
                    root: base.root.to_path_buf(),
                    base_tree: base.base_tree.clone(),
                },
            )
            .await;
        }
        self.refresh_keep_refs(session_id).await;
        Ok(())
    }

    /// Replay a session's journal into memory.
    ///
    /// Records the lenient parser could not use are recorded on the session's
    /// [`crucible_core::session::Integrity`] rather than dropped, and the
    /// grading there is what decides whether the gate blocks — see
    /// [`ReviewLedgers::has_unreviewed_in_file`].
    ///
    /// A journal that cannot be read *at all* is [`Self::poison`]ed before the
    /// error propagates, so the failure lands on the same graded path as a
    /// journal one of whose lines would not parse.
    pub async fn restore_from_journal(&self, session_id: &str, path: &Path) -> ReviewResult<()> {
        let restored = match journal::load(path, session_id).await {
            Ok(restored) => restored,
            Err(e) => {
                self.poison(session_id, path, &e);
                return Err(e);
            }
        };
        if !restored.integrity.is_intact() {
            warn!(
                session_id,
                path = %path.display(),
                skipped = restored.integrity.skips().len(),
                "review journal restored with gaps; attribution is incomplete"
            );
        }
        self.journals
            .insert(session_id.to_string(), path.to_path_buf());
        self.states
            .insert(session_id.to_string(), restored.states.clone());
        self.comments
            .insert(session_id.to_string(), restored.comments.clone());
        self.integrity
            .insert(session_id.to_string(), restored.integrity);
        self.ledgers.insert(session_id.to_string(), restored.ledger);
        // A restart that lost the keep ref (or a session restored into a repo
        // that never had one) reclaims its trees here, before anything reads
        // them.
        self.refresh_keep_refs(session_id).await;
        Ok(())
    }

    /// Register a session whose journal exists and could not be read at all.
    ///
    /// Without this the session simply has no ledger, and *no ledger* is the
    /// same signal a workspace outside git produces: the gate returns before it
    /// queries anything, every write proceeds unheld, and the panel reports an
    /// empty queue. A journal whose *header line* is merely corrupt already
    /// fails closed through [`crucible_core::session::SkipKind::Session`], so
    /// without this the wholly-unreadable case — strictly more broken — would
    /// be the only one that blocks nothing.
    ///
    /// The ledger inserted here has an **empty** `session_base`, which is
    /// exactly the shape `journal::load` produces for a journal whose base
    /// records were all skipped. Nothing is captured: capturing a fresh base is
    /// the data loss the journal exists to prevent, and the point of this is
    /// only to make the session *present* to every reader.
    ///
    /// The journal path is registered too, even though nothing could be read
    /// from it. [`Self::rebase_session`] is the human-reachable release, and
    /// without a registered path its records — and the keep refs that protect
    /// its trees — would silently go nowhere.
    fn poison(&self, session_id: &str, path: &Path, error: &ReviewError) {
        let mut integrity = Integrity::default();
        integrity.record(Skip {
            record: SkipKind::Session,
            line: 0,
            reason: error.to_string(),
        });
        warn!(
            session_id,
            path = %path.display(),
            error = %error,
            "review journal unreadable; every write in this session is held until a rebase"
        );
        self.integrity.insert(session_id.to_string(), integrity);
        self.journals
            .insert(session_id.to_string(), path.to_path_buf());
        self.ledgers
            .insert(session_id.to_string(), Ledger::new(session_id, Vec::new()));
    }

    /// Make sure `session_id` has somewhere to append to, without disturbing a
    /// journal it already has.
    ///
    /// A fresh file gets a header, so the arithmetic every later decision was
    /// made under is on record from the first line.
    async fn ensure_journal(&self, session_id: &str, dir: &Path) {
        if self.journals.contains_key(session_id) {
            return;
        }
        let path = dir.join(journal::FILE);
        let fresh = !tokio::fs::try_exists(&path).await.unwrap_or(false);
        self.journals.insert(session_id.to_string(), path);
        if fresh {
            self.append(session_id, journal::header(session_id)).await;
        }
    }

    /// Move `session_base` to the worktree as it is now, over `roots`.
    ///
    /// The release valve for a block no review can clear: a base tree that has
    /// been gc'd, a root that has moved, a journal whose records could not be
    /// read. Structural failure fails closed precisely *because* this exists —
    /// without it, blocking on a broken ledger would be an unreleasable hang
    /// rather than a bounded one.
    ///
    /// Everything the old base described stops being in the composed diff, so
    /// this is destructive to the queue and is only ever a deliberate human
    /// action. Decisions survive: they are keyed by content, not by base, and
    /// [`ReviewLedgers::list_hunks`] will re-apply any that still name a live
    /// hunk.
    ///
    /// `roots` is re-derived by the caller rather than read from the ledger,
    /// so a session whose journal lost its base records — the case with no
    /// roots left to recapture — is still recoverable. `dir` is the session's
    /// storage directory, and is needed for the same reason: a rebase is
    /// reachable on a session that has never run a turn, and therefore has no
    /// journal registered yet.
    pub async fn rebase_session(
        &self,
        session_id: &str,
        dir: &Path,
        roots: &[PathBuf],
    ) -> ReviewResult<Vec<RootStatus>> {
        let existing = self.ledger(session_id);
        let mut captured: Vec<RootBase> = Vec::new();
        let mut kept: Vec<RootBase> = Vec::new();
        let mut statuses: Vec<RootStatus> = Vec::new();
        for root in roots {
            let top = match git::top_level(root).await {
                Ok(top) => top,
                // A root that has stopped being a repository takes the same
                // keep-and-report path as one that merely failed to capture,
                // and for the same reason: dropping it removes it from
                // `session_base`, so it contributes neither hunks nor a status
                // and the gate answers `Clear` for it. Silently narrowing the
                // gate is not what the user asked for when they reached for
                // the release valve.
                Err(e) => {
                    if let Some(base) = existing.as_ref().and_then(|ledger| {
                        ledger
                            .session_base()
                            .iter()
                            .find(|base| root.starts_with(base.root.as_path()))
                            .cloned()
                    }) {
                        let reported = base.root.clone();
                        kept.push(base);
                        statuses.push(RootStatus::degraded(reported, e.to_string()));
                    }
                    continue;
                }
            };
            if statuses.iter().any(|s| s.root == top) {
                continue;
            }
            match workspace_snapshot::capture_tree(&top).await {
                Ok(tree) => {
                    captured.push(RootBase {
                        root: top.clone(),
                        base_tree: TreeSha::new(tree),
                    });
                    statuses.push(RootStatus::intact(top));
                }
                // Reported rather than skipped: a root that cannot be captured
                // is exactly the root the user is trying to unblock, and
                // silently dropping it would leave them rebasing forever.
                //
                // And *kept* rather than dropped, which is the other half.
                // `Ledger::rebase` replaces `session_base` wholesale, so a root
                // missing from the new set stops existing for the ledger — it
                // contributes neither hunks nor a `RootStatus`, so the gate
                // answers `Clear` for it the moment the failure clears, with
                // its changes gone from the composed diff and only another
                // rebase able to put them back. Capture failure here is
                // transient by construction (`git::top_level` already
                // succeeded, so the root is a repository), so the base it had
                // is still the right one.
                Err(e) => {
                    if let Some(base) = existing.as_ref().and_then(|l| l.base_tree(&top)) {
                        kept.push(RootBase {
                            root: top.clone(),
                            base_tree: base.clone(),
                        });
                    }
                    statuses.push(RootStatus::degraded(top, e.to_string()));
                }
            }
        }
        if statuses.is_empty() {
            return Err(ReviewError::NoTrackableRoots(
                roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }

        let bases: Vec<RootBase> = captured.iter().cloned().chain(kept).collect();
        match self.ledgers.get_mut(session_id) {
            Some(mut ledger) => ledger.rebase(bases.clone()),
            None => {
                self.ledgers
                    .insert(session_id.to_string(), Ledger::new(session_id, bases));
            }
        }
        // Cleared only when nothing was left behind. A partial rebase has not
        // answered what the journal lost under the root it could not capture,
        // and retrying once that root recovers is the release.
        if statuses.iter().all(|s| !s.is_degraded()) {
            self.integrity.remove(session_id);
        }

        // Before the first append, and after the `statuses.is_empty()` refusal
        // so a rebase that found no repository leaves no journal behind.
        self.ensure_journal(session_id, dir).await;
        // Only the roots whose base actually moved. A `Rebase` record voids
        // every interval under its root on replay, so writing one for a root
        // this call could not recapture would throw away the attribution the
        // `kept` branch above exists to preserve.
        for base in &captured {
            self.append(
                session_id,
                journal::Record::Rebase {
                    root: base.root.to_path_buf(),
                    base_tree: base.base_tree.clone(),
                },
            )
            .await;
        }
        self.refresh_keep_refs(session_id).await;
        Ok(statuses)
    }

    pub(super) async fn record_state(
        &self,
        session_id: &str,
        hunk_id: &HunkId,
        state: ReviewState,
    ) {
        self.states
            .entry(session_id.to_string())
            .or_default()
            .insert(hunk_id.clone(), state);
        self.append(session_id, journal::state(hunk_id, state))
            .await;
    }

    /// Add an interval to a ledger and to its journal, as one operation.
    ///
    /// **The only live path that may add an interval.** Both halves live here
    /// so a second writer cannot quietly drop the durable one; see the module
    /// docs in [`super`] for why the in-memory half alone compiles.
    ///
    /// Returns whether an in-memory ledger was present. The append happens
    /// either way, so a harvest into a parent whose ledger was already
    /// dropped still survives a restart.
    pub(super) async fn record_interval(&self, session_id: &str, interval: Interval) -> bool {
        let present = match self.ledgers.get_mut(session_id) {
            Some(mut ledger) => {
                ledger.push_interval_in_memory(interval.clone());
                true
            }
            None => false,
        };
        self.append(session_id, journal::Record::Interval(interval))
            .await;
        present
    }

    /// Append one record to the session's journal, if it has one.
    ///
    /// Best-effort and loud rather than fallible: every caller is a mutation
    /// that has already happened — the revert is on disk, the interval is in
    /// memory — and turning a completed action into a failed RPC would leave
    /// the caller believing it did not happen.
    pub(super) async fn append(&self, session_id: &str, record: journal::Record) {
        let Some(path) = self.journals.get(session_id).map(|r| r.value().clone()) else {
            return;
        };
        if let Err(e) = journal::append(&path, &record).await {
            warn!(
                session_id,
                path = %path.display(),
                error = %e,
                "review journal append failed; this will not survive a restart"
            );
        }
    }

    /// Re-point every tracked root's keep ref at this ledger's current trees.
    ///
    /// Total rather than incremental — see [`git::update_keep`]. Failure is a
    /// warning: the ledger is still correct, its trees are merely exposed to
    /// the next `git gc` in that repository.
    pub(super) async fn refresh_keep_refs(&self, session_id: &str) {
        // Only for a session that has a journal. Both release paths —
        // `drop_keep_refs` and `sweep_review_refs` — find a session's
        // repositories by reading its journal, so a ref claimed without one
        // could never be released and would pin trees in a user's repository
        // for good.
        if !self.journals.contains_key(session_id) {
            return;
        }
        let Some(ledger) = self.ledger(session_id) else {
            return;
        };
        for base in ledger.session_base() {
            let trees = ledger.trees_for(&base.root);
            if let Err(e) = git::update_keep(&base.root, session_id, &trees).await {
                warn!(
                    session_id,
                    root = %base.root.display(),
                    error = %e,
                    "review keep ref not updated; this session's trees are exposed to git gc"
                );
            }
        }
    }
}

/// Release a deleted session's keep refs, using its journal to find the
/// repositories they live in.
///
/// Called before the session directory is removed, because the journal is the
/// only record of which repositories a session ever touched — once it is gone
/// the refs are unreachable garbage that nothing will ever collect.
pub async fn drop_keep_refs(session_dir: &Path, session_id: &str) {
    for root in journal::roots_in(&session_dir.join(journal::FILE)).await {
        if let Err(e) = git::drop_keep(&root, session_id).await {
            debug!(
                session_id,
                root = %root.display(),
                error = %e,
                "review keep ref not released"
            );
        }
    }
}

/// Drop keep refs left behind by sessions whose journals are gone.
///
/// The backstop for every path that removes a session directory without going
/// through `delete_session` — a user deleting a kiln by hand, a crash between
/// the two steps, a session dir removed by an older build. Returns how many
/// refs it released.
///
/// Deliberately *not* time-based: a keep ref whose journal still exists is
/// still doing its job, however old, and expiring it would delete the trees a
/// long-running review depends on. The only thing that makes a ref garbage is
/// its session no longer existing.
///
/// Known residual: a repository is only visited if some surviving journal
/// still names it, so if *every* session over one repository has its directory
/// removed out of band, that repository's refs are never reached. Nothing
/// enumerates repositories independently — the journals are the only index —
/// and the cost is a handful of pinned trees in a repo the daemon has stopped
/// tracking. `delete_session` covers the path a user actually takes.
pub async fn sweep_review_refs(sessions_root: &Path) -> usize {
    // (repository → session ids that still have a journal there). One
    // repository is commonly a root for sessions in several kilns, and with a
    // single sessions root every one of them is in this scan — which is what
    // makes "no journal names it" safe to read as "nothing needs it".
    let mut live: HashMap<PathBuf, Vec<String>> = HashMap::new();
    if let Ok(mut entries) = tokio::fs::read_dir(sessions_root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let session_id = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path().join(journal::FILE);
            for root in journal::roots_in(&path).await {
                live.entry(root).or_default().push(session_id.clone());
            }
        }
    }

    let mut dropped = 0;
    for (root, sessions) in &live {
        let Ok(held) = git::keep_refs(root).await else {
            continue;
        };
        for (stale, name) in held.iter().filter(|(id, _)| !sessions.contains(id)) {
            match git::drop_ref(root, name).await {
                Ok(()) => dropped += 1,
                Err(e) => debug!(
                    session_id = %stale,
                    reference = %name,
                    root = %root.display(),
                    error = %e,
                    "stale keep ref not released"
                ),
            }
        }
    }
    dropped
}
