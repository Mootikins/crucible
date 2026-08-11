//! `review.jsonl`: the ledger's persistence.
//!
//! A sibling of `session.jsonl` under `{kiln}/.crucible/sessions/{id}/`,
//! append-only, one tagged JSON object per line, read with the same
//! skip-and-warn leniency.
//!
//! # Why a file and not SQLite
//!
//! `storage/sqlite/` is the note and knowledge-graph store. A `sessions` table
//! beside it would be exactly the parallel store the architecture exists to
//! avoid, and sessions already have a directory with an append-only log in it.
//!
//! # Why appends are eager
//!
//! Every mutation writes immediately. Rewriting the whole blob is O(n²) over a
//! session and a crash mid-rewrite truncates the base record — the one record
//! whose loss is unrecoverable. Flushing only at turn boundaries loses exactly
//! the turn you most wanted, because the daemon dies mid-turn far more often
//! than between turns.
//!
//! # Reading is not the same as capturing
//!
//! A journal that exists and cannot be read must never fall through to
//! capturing a fresh base. Only two things ever set `session_base`: a session
//! with no journal at all, and an explicit rebase. Everything else degrades.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use crucible_core::session::{
    ChildLedgerRef, Comment, HunkId, Integrity, Interval, Ledger, LineRange, PhysicalRoot,
    ReviewState, RootBase, Skip, SkipKind, TreeSha,
};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use super::error::{ReviewError, ReviewResult};

/// The journal's name inside a session's storage directory.
pub(super) const FILE: &str = "review.jsonl";

/// One journal line.
///
/// Internally tagged on `t`, so an unknown record from a newer daemon is still
/// classifiable by tag even when its body is not decodable — which is what
/// lets degradation be graded rather than all-or-nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(super) enum Record {
    /// Written once, first. Carries the fingerprint of the algorithms every
    /// later record's meaning depends on.
    Header {
        session_id: String,
        alg: String,
        created_at: DateTime<Utc>,
    },
    /// A tracked root and the tree the session started from.
    Base {
        root: PathBuf,
        base_tree: TreeSha,
    },
    /// A new base for a root, superseding every `Base` and `Rebase` above it
    /// and voiding the intervals measured from the old one.
    Rebase {
        root: PathBuf,
        base_tree: TreeSha,
    },
    Interval(Interval),
    Child(ChildLedgerRef),
    /// A review decision. Carries the `alg` it was made under: an identity
    /// derived by different arithmetic names different lines, so applying the
    /// decision would be applying it to code the user never saw.
    State {
        hunk: HunkId,
        state: ReviewState,
        alg: String,
    },
    Comment(Comment),
    CommentResolved {
        comment: String,
    },
}

/// Everything one journal replays to.
pub(super) struct Restored {
    pub(super) ledger: Ledger,
    pub(super) states: HashMap<HunkId, ReviewState>,
    pub(super) comments: Vec<Comment>,
    pub(super) integrity: Integrity,
}

/// Append one record.
pub(super) async fn append(path: &Path, record: &Record) -> ReviewResult<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut line = serde_json::to_string(record)
        .map_err(|e| ReviewError::Git(format!("review journal encode failed: {e}")))?;
    line.push('\n');

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line.as_bytes()).await?;
    file.flush().await?;
    Ok(())
}

/// Replay a journal.
///
/// Returns `Err` only when the file itself cannot be read — a case that must
/// stop the caller rather than degrade it. A line that will not parse is
/// recorded on [`Integrity`] and skipped, never dropped from disk: a decision
/// excluded by an `alg` mismatch comes back if the change that caused the
/// mismatch is rolled back.
pub(super) async fn load(path: &Path, session_id: &str) -> ReviewResult<Restored> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| ReviewError::Journal {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;

    let mut bases: Vec<RootBase> = Vec::new();
    let mut intervals: Vec<Interval> = Vec::new();
    let mut children: Vec<ChildLedgerRef> = Vec::new();
    let mut states: HashMap<HunkId, ReviewState> = HashMap::new();
    let mut comments: Vec<Comment> = Vec::new();
    let mut integrity = Integrity::default();

    for (index, line) in text.lines().enumerate() {
        let number = index as u32 + 1;
        if line.trim().is_empty() {
            continue;
        }
        let record = match serde_json::from_str::<Record>(line) {
            Ok(record) => record,
            Err(e) => {
                let skip = Skip {
                    record: classify(line),
                    line: number,
                    reason: e.to_string(),
                };
                warn!(
                    path = %path.display(),
                    line = number,
                    kind = ?skip.record,
                    error = %e,
                    "review journal record skipped"
                );
                integrity.record(skip);
                continue;
            }
        };

        match record {
            Record::Header {
                session_id: recorded,
                alg: recorded_alg,
                ..
            } => {
                if recorded != session_id {
                    warn!(
                        path = %path.display(),
                        recorded = %recorded,
                        expected = %session_id,
                        "review journal names a different session; reading it anyway"
                    );
                }
                if recorded_alg != alg() {
                    debug!(
                        path = %path.display(),
                        recorded = %recorded_alg,
                        current = %alg(),
                        "review journal was written under different hunk arithmetic"
                    );
                }
            }
            Record::Base { root, base_tree } => set_base(&mut bases, root, base_tree),
            Record::Rebase { root, base_tree } => {
                intervals.retain(|i| !i.roots_touched.iter().any(|r| *r.root == *root));
                // An explicit rebase is the user saying "the worktree as it is
                // now is the baseline". Records the loader could not read
                // describe history that no longer bears on the review surface,
                // so the block they imposed is exactly what the rebase is for
                // — but only for the root it names. Clearing wholesale let a
                // partial rebase's block evaporate on the next restart, while
                // the in-memory `rebase_session` kept it, so replay and live
                // state disagreed about whether the gate should hold.
                integrity.clear_root(&root);
                set_base(&mut bases, root, base_tree);
            }
            Record::Interval(interval) => intervals.push(interval),
            Record::Child(child) => children.push(child),
            Record::State {
                hunk,
                state,
                alg: recorded,
            } => {
                if recorded == alg() {
                    states.insert(hunk, state);
                } else {
                    // Fail closed and leave it on disk: the hunk returns to the
                    // queue, and rolling the change back restores the decision.
                    integrity.record(Skip {
                        record: SkipKind::Informational,
                        line: number,
                        reason: format!(
                            "decision recorded under hunk arithmetic {recorded}, now {}",
                            alg()
                        ),
                    });
                }
            }
            Record::Comment(comment) => comments.push(comment),
            Record::CommentResolved { comment } => {
                match comments.iter_mut().find(|c| c.id == comment) {
                    Some(found) => found.resolved = true,
                    // The comment itself was skipped above; the resolution has
                    // nothing to attach to and costs nothing.
                    None => debug!(
                        comment,
                        "resolution for a comment this journal does not hold"
                    ),
                }
            }
        }
    }

    if bases.is_empty() {
        // No base means no composed diff and no attribution, but capturing a
        // fresh one here would report that the agent changed nothing. Say so
        // instead, loudly enough that the gate blocks.
        integrity.record(Skip {
            record: SkipKind::Session,
            line: 0,
            reason: "journal holds no session base".to_string(),
        });
    }

    let mut ledger = Ledger::new(session_id, bases);
    for interval in intervals {
        ledger.push_interval_in_memory(interval);
    }
    for child in children {
        ledger.link_child(child);
    }

    Ok(Restored {
        ledger,
        states,
        comments,
        integrity,
    })
}

/// The roots a journal names, without replaying it.
///
/// The delete and sweep paths need the repositories a session claimed keep
/// refs in, and they run when there is no ledger in memory to ask. Skipped
/// lines are not this function's problem: a root it cannot read is a keep ref
/// that survives one more sweep, which is the harmless direction.
pub(super) async fn roots_in(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = tokio::fs::read_to_string(path).await else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    for line in text.lines() {
        if let Ok(Record::Base { root, .. }) | Ok(Record::Rebase { root, .. }) =
            serde_json::from_str::<Record>(line)
        {
            if !roots.contains(&root) {
                roots.push(root);
            }
        }
    }
    roots
}

/// The header for a fresh journal.
pub(super) fn header(session_id: &str) -> Record {
    Record::Header {
        session_id: session_id.to_string(),
        alg: alg().to_string(),
        created_at: Utc::now(),
    }
}

/// A decision, stamped with the arithmetic it was made under.
pub(super) fn state(hunk: &HunkId, state: ReviewState) -> Record {
    Record::State {
        hunk: hunk.clone(),
        state,
        alg: alg().to_string(),
    }
}

/// Last writer wins, per root.
fn set_base(bases: &mut Vec<RootBase>, root: PathBuf, base_tree: TreeSha) {
    let root = PhysicalRoot::from_top_level(root);
    match bases.iter_mut().find(|b| *b.root == *root) {
        Some(existing) => existing.base_tree = base_tree,
        None => bases.push(RootBase { root, base_tree }),
    }
}

/// What a line the typed parser rejected costs, from whatever of it is still
/// legible.
///
/// Scoped from the raw text rather than from a re-parse, because the dominant
/// corruption for a file written one line at a time is a truncated append —
/// which leaves the head of the record, including its tag, perfectly readable
/// while defeating any JSON parser. A half-written interval is still
/// recognisably an interval over a named root, and scoping the block to that
/// root is the difference between holding one repository and holding the whole
/// session.
fn classify(line: &str) -> SkipKind {
    match scan_string(line, "t").as_deref() {
        Some("child" | "state" | "comment" | "comment_resolved") => SkipKind::Informational,
        // Scoped to the interval's root only when it names exactly one. An
        // interval over several roots truncated partway through them would
        // otherwise block its FIRST root and leave the rest unblocked, which
        // by `Integrity`'s rule goes quiet on the very root whose evidence was
        // lost. `SkipKind` scopes to one root, so more than one legible root
        // falls back to blocking the session.
        Some("interval") => match scan_strings(line, "root").as_slice() {
            [root] => SkipKind::Root {
                root: PhysicalRoot::from_top_level(root),
            },
            _ => SkipKind::Session,
        },
        // `header`, `base`, `rebase`, a line with no legible tag at all, and
        // anything a newer daemon writes that this one has never heard of.
        // Unknown is unscoped on purpose: an unrecognised record could be an
        // interval by another name, and calling it informational would let its
        // hunks go external — which is the gate turning itself off.
        _ => SkipKind::Session,
    }
}

/// The value of a top-level string field, read out of a line no parser will
/// accept.
///
/// The candidate is re-parsed as a JSON string from its opening quote, so
/// escapes are honoured and a value that is itself truncated — no closing
/// quote — correctly yields `None` rather than a mangled path. Field order
/// matters only in that serde writes the tag first, which is why `t` is found
/// before any string value could contain a lookalike.
/// Every legible value of `field` in `line`, in order.
///
/// The count is the point, not just the first value: an interval naming two
/// roots cannot be scoped to one of them, and knowing that requires seeing
/// both.
fn scan_strings(line: &str, field: &str) -> Vec<String> {
    let needle = format!("\"{field}\":");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = line[from..].find(&needle) {
        let start = from + at;
        if let Some(value) = scan_string(&line[start..], field) {
            found.push(value);
        }
        from = start + needle.len();
    }
    found
}

fn scan_string(line: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\":");
    let rest = line[line.find(&needle)? + needle.len()..].trim_start();
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut end = 1;
    while end < bytes.len() {
        match bytes[end] {
            b'\\' => end += 2,
            b'"' => return serde_json::from_str(&rest[..=end]).ok(),
            _ => end += 1,
        }
    }
    None
}

/// Fingerprint of everything a recorded decision's meaning depends on: how a
/// hunk's identity is derived, and which lines the diff engine groups into a
/// hunk in the first place.
///
/// Computed by *running* both on a fixed input rather than by naming versions
/// in a const. A version string goes stale the moment someone bumps `similar`
/// without touching this file, and a stale fingerprint silently re-activates
/// decisions made under different arithmetic — which is the failure this
/// exists to prevent, not a cosmetic one.
fn alg() -> &'static str {
    static ALG: OnceLock<String> = OnceLock::new();
    ALG.get_or_init(|| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(
            HunkId::derive(
                &PhysicalRoot::from_top_level("/alg"),
                "alg.txt",
                "before\n",
                "after\n",
                LineRange::new(1, 2),
            )
            .as_str()
            .as_bytes(),
        );
        // A pair chosen to exercise grouping, not just content: two separate
        // change clusters with an unchanged run between them, so a change in
        // how `similar` splits or merges hunks moves the fingerprint.
        for raw in super::compose::hunks_between("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n") {
            hasher.update(&raw.before_range.start.to_le_bytes());
            hasher.update(&raw.before_range.end.to_le_bytes());
            hasher.update(&raw.after_range.start.to_le_bytes());
            hasher.update(&raw.after_range.end.to_le_bytes());
        }
        hex::encode(&hasher.finalize().as_bytes()[..8])
    })
}
