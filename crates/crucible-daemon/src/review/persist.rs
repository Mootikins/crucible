//! The journal side of [`ReviewLedgers`]: restoring a session from
//! `review.jsonl`, appending to it, and keeping the git trees it names alive.
//!
//! Split from [`super`] along the seam that was already there — everything
//! here either reads or writes durable state, and nothing in it decides what a
//! hunk means.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crucible_core::session::{Integrity, Interval, Ledger, Skip, SkipKind};
use tracing::{debug, warn};

use super::backend::RootBackend;
use super::plain_store::PlainStore;
use super::{git, journal, ReviewError, ReviewLedgers, ReviewResult};
use crate::diff::comments::quoted_lines;

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
    /// [`ReviewLedgers::open`].** Only a session with no journal at all sets
    /// `session_base`. An unreadable journal is an error the caller
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
    /// grading there decides which roots the listing marks as degraded.
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
        self.migrate_comments(session_id, restored.comments).await;
        self.integrity
            .insert(session_id.to_string(), restored.integrity);
        self.ledgers.insert(session_id.to_string(), restored.ledger);
        // A restart that lost the keep ref (or a session restored into a repo
        // that never had one) reclaims its trees here, before anything reads
        // them.
        self.refresh_keep_refs(session_id).await;
        Ok(())
    }

    /// Copy the old comments of a session journal to the comment store, one
    /// time only, under the session record diffset of the session.
    ///
    /// A failure leaves the journal as it is and does not stop the restore.
    /// The store has no mark then, so the next restore tries again.
    async fn migrate_comments(&self, session_id: &str, old: Vec<journal::JournalComment>) {
        if old.is_empty() {
            return;
        }
        let diffset = match super::record_diffset(session_id) {
            Ok(diffset) => diffset,
            Err(e) => {
                warn!(session_id, error = %e, "journal comments not migrated");
                return;
            }
        };
        match self.comments.is_migrated(&diffset) {
            Ok(false) => {}
            Ok(true) => return,
            Err(e) => {
                warn!(session_id, error = %format!("{e:#}"), "journal comments not migrated");
                return;
            }
        }
        let mut comments = Vec::with_capacity(old.len());
        for comment in old {
            // An absent or unreadable file quotes nothing. The comment
            // still migrates, and the projection then finds it outdated.
            let text = tokio::fs::read_to_string(comment.root.join(&comment.path))
                .await
                .unwrap_or_default();
            let quoted = quoted_lines(&text, comment.line_range);
            comments.push(comment.into_comment(diffset.clone(), quoted));
        }
        let count = comments.len();
        match self.comments.migrate(&diffset, comments) {
            Ok(true) => debug!(
                session_id,
                count, "journal comments migrated to the comment store"
            ),
            Ok(false) => {}
            Err(e) => {
                warn!(session_id, error = %format!("{e:#}"), "journal comments not migrated");
            }
        }
    }

    /// Register a session whose journal exists and could not be read at all.
    ///
    /// Without this the session simply has no ledger, and *no ledger* is the
    /// same signal a workspace outside git produces: the panel reports an
    /// empty queue. A journal whose *header line* is merely corrupt already
    /// degrades every root through [`crucible_core::session::SkipKind::Session`],
    /// so without this the wholly-unreadable case — strictly more broken —
    /// would be the only one that reports nothing.
    ///
    /// The ledger inserted here has an **empty** `session_base`, which is
    /// exactly the shape `journal::load` produces for a journal whose base
    /// records were all skipped. Nothing is captured: capturing a fresh base is
    /// the data loss the journal exists to prevent, and the point of this is
    /// only to make the session *present* to every reader.
    ///
    /// The journal path is registered too, even though nothing could be read
    /// from it, so that a later append does not go to a different file.
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
            "review journal unreadable; every root of this session is degraded"
        );
        self.integrity.insert(session_id.to_string(), integrity);
        self.journals
            .insert(session_id.to_string(), path.to_path_buf());
        self.ledgers
            .insert(session_id.to_string(), Ledger::new(session_id, Vec::new()));
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
    /// that has already happened — the interval is in memory — and turning a completed action into a failed RPC would leave
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

    /// Re-claim every tracked root's snapshots, through that root's backend.
    ///
    /// Total rather than incremental — see [`RootBackend::keep`]. Failure is a
    /// warning: the ledger is still correct, its snapshots are merely exposed
    /// to the next collection in that store.
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
            let snapshots = ledger.trees_for(&base.root);
            let backend = RootBackend::of(&base.base_tree);
            if let Err(e) = backend
                .keep(&self.plain, &base.root, session_id, &snapshots)
                .await
            {
                warn!(
                    session_id,
                    root = %base.root.display(),
                    error = %e,
                    "review keep claim not updated; this session's snapshots are exposed to collection"
                );
            }
        }
    }
}

/// Release a deleted session's claims: its keep refs, and its claims on
/// plain-store snapshots.
///
/// Called before the session directory is removed, because the journal is the
/// only record of which repositories a session ever touched — once it is gone
/// the refs are unreachable garbage that nothing will ever collect. The plain
/// store needs no journal for this, because every claim it holds is named by
/// the session, but it is released here anyway so the disk comes back at the
/// delete rather than at the next sweep.
///
/// `plain_root` is `None` for a composition root that has no plain store. Its
/// claims are then released by [`sweep_review_refs`] instead, which finds them
/// by the session directory that is no longer there.
pub async fn drop_keep_refs(session_dir: &Path, session_id: &str, plain_root: Option<&Path>) {
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
    if let Some(plain_root) = plain_root {
        if let Err(e) = PlainStore::new(plain_root.to_path_buf())
            .drop_keep(session_id)
            .await
        {
            debug!(
                session_id,
                root = %plain_root.display(),
                error = %e,
                "review snapshot claim not released"
            );
        }
    }
}

/// Release the snapshots left behind by sessions whose directories are gone:
/// keep refs in every repository a surviving journal names, and the claims and
/// files in the plain store.
///
/// The backstop for every path that removes a session directory without going
/// through `delete_session` — a user deleting a kiln by hand, a crash between
/// the two steps, a session dir removed by an older build. Returns how many
/// claims it released: git refs plus the plain-store files
/// [`PlainStore::sweep`] removed.
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
/// tracking. `delete_session` covers the path a user actually takes. The plain
/// store has no such residual: its root is one directory this daemon owns, and
/// the sweep enumerates it rather than reaching it through a journal.
pub async fn sweep_review_refs(sessions_root: &Path, plain_root: &Path) -> usize {
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
    // Same pass, same rule, the other store: a plain snapshot is collected by
    // nothing but this call, so it runs even when no repository was reached.
    dropped += PlainStore::new(plain_root.to_path_buf())
        .sweep(sessions_root)
        .await;
    dropped
}
