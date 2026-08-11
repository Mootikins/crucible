//! Attributed-diff review types.
//!
//! These cross the RPC boundary (`review.*` methods, `review_changed`
//! events), so every field name here is a wire contract.
//!
//! The model has two halves that must not be confused:
//!
//! * The **ledger** is append-only bookkeeping: which tool call moved the
//!   workspace from which tree to which tree. It is evidence, never the
//!   review surface.
//! * The **composed hunk** is the review surface and the only action unit:
//!   a difference between the session's base tree and the current worktree.
//!   Every composed hunk is independently revertible against `session_base`
//!   by construction, which is exactly what a per-tool-call contribution is
//!   not — reverting an early call's edit after later calls touched the same
//!   lines is a three-way merge that fails hardest where it matters most.
//!
//! Attribution links the two, many-to-many, and stays informational.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A git tree SHA.
///
/// Distinct from a commit SHA on purpose: `git write-tree` and
/// `git commit-tree` both return 40 hex characters and the two are freely
/// substitutable in most git invocations but mean different things. The
/// ledger diffs trees; mixing a commit in silently compares the wrong pair
/// of objects for a tree with more than one parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TreeSha(String);

impl TreeSha {
    pub fn new(sha: impl Into<String>) -> Self {
        Self(sha.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TreeSha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A half-open range of 1-based line numbers.
///
/// `start == end` is a valid empty range and is how a pure insertion is
/// expressed on the *before* side (and a pure deletion on the *after* side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    /// First line, 1-based, inclusive.
    pub start: u32,
    /// One past the last line, 1-based, exclusive.
    pub end: u32,
}

impl LineRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }

    pub fn contains(&self, line: u32) -> bool {
        line >= self.start && line < self.end
    }

    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
}

/// Content-derived identity for a composed hunk.
///
/// **Never positional in the worktree.** The composed diff is recomputed on
/// every worktree change; a `(path, line)` key moves under an edit made
/// anywhere above it, so review state either evaporates or — far worse —
/// lands on a different hunk and waves unreviewed agent code through the
/// gate.
///
/// Derived from `(root, path, before_content, after_content, base_range)`.
///
/// The **root** is part of the identity because a session tracks several
/// roots (workspace, kiln, connected kilns) and hunk paths are root-relative.
/// Two roots holding the same relative path with the same change would
/// otherwise share one id, so accepting the workspace's hunk would silently
/// accept the kiln's and reverting one would revert whichever the composed
/// diff happened to list first.
///
/// The **base range** — the hunk's [`LineRange`] in `session_base`
/// coordinates — is what separates byte-identical repeats within one file. It
/// is the one positional input that is safe to hash: `session_base` is
/// immutable for the session's lifetime, so base coordinates do not move
/// under any worktree edit. The rejected alternative, an ordinal among
/// identical siblings, is positional in the *current* worktree — removing an
/// earlier sibling renumbers the survivor onto the removed hunk's identity
/// and hands it that hunk's decision.
///
/// A re-alignment inside a genuinely ambiguous region (base `x\nx\n` →
/// `x\n`) can still move a hunk's base range and so change its id. That is
/// fail-closed: an identity the ledger does not recognise resolves to
/// [`ReviewState::Unreviewed`], never to an inherited decision.
///
/// Known residual: one repository reached through two bind mounts is two
/// roots and therefore two identities, so accepting through one path leaves
/// the other unreviewed. Inherent to anchoring identity on an absolute path —
/// unlike a symlink, which `git rev-parse --show-toplevel` resolves to a
/// single physical path, a bind mount has no single correct answer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HunkId(String);

impl HunkId {
    /// Derive the identity of a hunk.
    ///
    /// `root` is hashed as raw OS bytes, not `to_string_lossy`: lossy
    /// conversion folds every un-decodable byte onto the same replacement
    /// character, which is exactly how two distinct roots would collide on
    /// one identity. It is hashed *as `RootBase.root` already holds it* and is
    /// never canonicalised here — a syscall inside a pure hash would make
    /// identity depend on filesystem state at derive time rather than at
    /// ledger-open time, and would change every id the moment a mount did.
    ///
    /// The variable-width fields are length-prefixed so that no rearrangement
    /// of the same bytes across fields can collide (`path="a", before="bc"`
    /// must not hash the same as `path="ab", before="c"`). `base` is
    /// fixed-width and therefore self-delimiting.
    pub fn derive(
        root: &PhysicalRoot,
        path: &str,
        before: &str,
        after: &str,
        base: LineRange,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        for field in [
            root.as_os_str().as_encoded_bytes(),
            path.as_bytes(),
            before.as_bytes(),
            after.as_bytes(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field);
        }
        hasher.update(&base.start.to_le_bytes());
        hasher.update(&base.end.to_le_bytes());
        // 128 bits: collision-proof for a session's worth of hunks, and half
        // the wire size of the full digest in a panel listing hundreds.
        Self(hex::encode(&hasher.finalize().as_bytes()[..16]))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for HunkId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Review decision for a single composed hunk.
///
/// [`Self::Unreviewed`] is the default *and* the fail-closed answer for an
/// identity the ledger does not recognise.
///
/// This is the user's *decision*, and nothing else. Whether the same change
/// has been through the queue before is a separate fact about history and
/// lives on [`ComposedHunk::reapplied`]; conflating the two is how a recorded
/// [`Self::Rejected`] came to resolve a change that is sitting in the worktree
/// right now, which reads as reviewed and stops the gate blocking on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    #[default]
    Unreviewed,
    Accepted,
    Rejected,
}

/// A repository top level, as `git rev-parse --show-toplevel` printed it.
///
/// Distinct from a plain `PathBuf` because the difference is invisible and
/// load-bearing: this spelling is physical (symlinks resolved) and is the
/// first field hashed into a [`HunkId`], whereas the paths a caller supplies
/// — `session.workspace`, a tool's target, an RPC argument — are whatever
/// spelling that caller happened to use. Mixing the two gives one change two
/// identities, or a gate query that silently matches nothing.
///
/// Only [`Self::from_top_level`] mints one, so the compiler asks where a root
/// came from at every construction site. Reads go through `Deref`, so
/// `root.join(path)` and friends are unaffected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhysicalRoot(PathBuf);

impl PhysicalRoot {
    /// Wrap the output of `git rev-parse --show-toplevel`.
    ///
    /// The name is the contract: if what you are holding did not come from
    /// git's own idea of the top level, resolve it first rather than calling
    /// this to quiet the compiler.
    pub fn from_top_level(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl std::ops::Deref for PhysicalRoot {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for PhysicalRoot {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl std::fmt::Display for PhysicalRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// One root's tree pair inside an [`Interval`].
///
/// A session writes to several roots (workspace, kiln, connected kilns), and
/// a tree SHA is only meaningful relative to the repository that produced it,
/// so the pair has to be per-root rather than per-interval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootInterval {
    /// Repository top level, not the requested root. `git write-tree` always
    /// emits repo-root-relative paths regardless of where it is invoked, so
    /// every path in this interval is relative to this directory.
    pub root: PhysicalRoot,
    pub before_tree: TreeSha,
    pub after_tree: TreeSha,
}

/// One bracketed write window: a tool call, and what the worktree looked like
/// on either side of it.
///
/// Recorded only when at least one root's tree actually changed — a tool call
/// that wrote nothing produces no interval and no card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Interval {
    /// `ToolCall::call_id` — the existing per-call identity. Not reproducible
    /// across a resume when the model omitted an id and the daemon minted one.
    pub tool_call_id: String,
    /// `ConversationTree` node that was current when the interval opened.
    /// Turn-granular: every tool call in one batch shares it, so this is a
    /// turn coordinate for display, never an identity.
    pub node_id: u32,
    /// Roots whose tree changed across this call. Never empty.
    pub roots_touched: Vec<RootInterval>,
    /// True when another interval was open on a shared root for part of this
    /// window, making the bracket unsound. Attribution skips contested
    /// intervals rather than emitting confident wrong attribution; their
    /// hunks surface as external.
    #[serde(default)]
    pub contested: bool,
    /// Set when this interval was harvested out of a delegated child's ledger
    /// rather than recorded by a bracket of this session's own.
    ///
    /// `tool_call_id` then names a call in the *child's* transcript, which is
    /// why the provenance has to travel with it: without this, a hunk in the
    /// parent's composed diff is attributed to a call id that appears nowhere
    /// in the parent's conversation, and the delegation card it belongs under
    /// (matched through [`ChildLedgerRef::child_session_id`]) cannot be found.
    #[serde(default)]
    pub child_session_id: Option<String>,
}

/// A delegated session's ledger, referenced rather than inlined.
///
/// Attribution depth follows session depth: expanding a `delegate_session`
/// card expands into the child's own tool calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildLedgerRef {
    /// The parent-side `delegate_session` call this child hangs off.
    pub tool_call_id: String,
    pub child_session_id: String,
    /// Parent-side `ConversationTree` node the delegation was issued from.
    ///
    /// `Option`, and never a `u32` defaulted to zero: `0` is a real node — the
    /// root — so a journal row written before this field existed would render
    /// every delegation confidently under turn 0 instead of under the turn
    /// that issued it. Absence is the honest answer; a confident wrong turn
    /// number is worse than no turn number.
    #[serde(default)]
    pub node_id: Option<u32>,
}

/// A root and the tree the session started from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootBase {
    /// Repository top level. See [`RootInterval::root`].
    pub root: PhysicalRoot,
    pub base_tree: TreeSha,
}

/// Append-only per-session record of what the agent did to the filesystem.
///
/// `session_base` is captured once when the ledger opens and is persisted
/// with the session. There is deliberately no setter: re-deriving it on
/// resume would silently empty the composed diff of everything done before
/// the restart, which reads as "the agent changed nothing" rather than as an
/// error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    session_id: String,
    session_base: Vec<RootBase>,
    intervals: Vec<Interval>,
    children: Vec<ChildLedgerRef>,
}

impl Ledger {
    /// Open a ledger over the roots a session may write to.
    pub fn new(session_id: impl Into<String>, session_base: Vec<RootBase>) -> Self {
        Self {
            session_id: session_id.into(),
            session_base,
            intervals: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn session_base(&self) -> &[RootBase] {
        &self.session_base
    }

    /// The tree this session started from for `root`, or `None` when the root
    /// is not one this ledger tracks.
    pub fn base_tree(&self, root: &Path) -> Option<&TreeSha> {
        self.session_base
            .iter()
            .find(|b| *b.root == *root)
            .map(|b| &b.base_tree)
    }

    pub fn roots(&self) -> impl Iterator<Item = &Path> {
        self.session_base.iter().map(|b| b.root.as_path())
    }

    pub fn intervals(&self) -> &[Interval] {
        &self.intervals
    }

    /// Add an interval to the in-memory ledger **without journalling it**.
    ///
    /// Named for what it omits, because the omission is invisible at the
    /// call site: this compiles fine against a `DashMap` guard, and an
    /// interval added this way is gone after a restart. Journal replay is
    /// the only caller that legitimately wants it; every live path goes
    /// through `ReviewLedgers::record_interval`, which does both halves.
    pub fn push_interval_in_memory(&mut self, interval: Interval) {
        self.intervals.push(interval);
    }

    pub fn children(&self) -> &[ChildLedgerRef] {
        &self.children
    }

    pub fn link_child(&mut self, child: ChildLedgerRef) {
        self.children.push(child);
    }

    /// Every tree under `root` that must stay reachable for this ledger to
    /// remain readable: the session base, plus both sides of every interval.
    ///
    /// Trees written by `git write-tree` are referenced by nothing, so git's
    /// own `gc --prune=now` deletes them and the composed diff turns into a
    /// hard error against a missing object. This is the list a keep ref has to
    /// protect.
    pub fn trees_for(&self, root: &Path) -> Vec<TreeSha> {
        let mut trees: Vec<TreeSha> = self
            .session_base
            .iter()
            .filter(|b| *b.root == *root)
            .map(|b| b.base_tree.clone())
            .collect();
        for interval in &self.intervals {
            for touched in &interval.roots_touched {
                if *touched.root == *root {
                    trees.push(touched.before_tree.clone());
                    trees.push(touched.after_tree.clone());
                }
            }
        }
        trees.sort();
        trees.dedup();
        trees
    }

    /// Replace `session_base` wholesale and void the intervals measured from
    /// the old one.
    ///
    /// The **only** sanctioned way to move a base after the ledger is open,
    /// and the reason the field otherwise has no setter. It exists to bound the
    /// block a structural failure imposes — a base tree that has been gc'd
    /// makes the composed diff uncomputable, and without a way to say "start
    /// again from what is on disk now" that block has no human-reachable
    /// release.
    ///
    /// Intervals go with it because a tree pair measured against a base that
    /// no longer exists can no longer intersect anything: keeping them would
    /// leave the ledger claiming attribution it cannot compute. Review
    /// *decisions* are not touched here — they are keyed by content, and the
    /// caller keeps them so a rollback restores them.
    ///
    /// **Per root, not wholesale.** A rebase can fail to recapture one root
    /// while succeeding on the others, and that root keeps the base it had —
    /// so its intervals still measure against a live base and dropping them
    /// would make every hunk under it external, which is the gate turning
    /// itself off. This mirrors the journal, where a `Rebase` record voids the
    /// intervals of the one root it names.
    pub fn rebase(&mut self, session_base: Vec<RootBase>) {
        let moved: Vec<PhysicalRoot> = session_base
            .iter()
            .filter(|new| self.base_tree(&new.root) != Some(&new.base_tree))
            .map(|new| new.root.clone())
            .collect();
        self.intervals.retain_mut(|interval| {
            interval
                .roots_touched
                .retain(|touched| !moved.contains(&touched.root));
            !interval.roots_touched.is_empty()
        });
        self.session_base = session_base;
    }
}

/// What the ledger could not read back from its journal, and therefore cannot
/// vouch for.
///
/// Degradation is **graded**, and the grading is not cosmetic: losing
/// attribution fails *open*. A hunk no interval accounts for is
/// [`ComposedHunk::is_external`], `unreviewed_hunks` excludes external hunks,
/// and the gate silently stops blocking. So a lost interval has to be louder
/// than a lost comment, or a corrupt journal quietly disables the safety
/// property the journal exists to preserve.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integrity {
    skips: Vec<Skip>,
}

impl Integrity {
    pub fn record(&mut self, skip: Skip) {
        self.skips.push(skip);
    }

    pub fn skips(&self) -> &[Skip] {
        &self.skips
    }

    pub fn is_intact(&self) -> bool {
        self.skips.is_empty()
    }

    /// Whether attribution under `root` is incomplete enough that a write
    /// there must be refused.
    pub fn blocks(&self, root: &Path) -> bool {
        self.skips.iter().any(|s| s.record.blocks(root))
    }

    /// Whether the loss is unscoped — no root can be trusted, including roots
    /// the ledger no longer knows it had.
    pub fn blocks_everything(&self) -> bool {
        self.skips
            .iter()
            .any(|s| matches!(s.record, SkipKind::Session))
    }

    /// Forget everything recorded so far.
    ///
    /// Only [`Ledger::rebase`]'s caller may do this, and only together with a
    /// rebase: after the base moves to what is on disk now, records the loader
    /// could not read describe history that no longer bears on the review
    /// surface. Clearing without rebasing would be laundering.
    pub fn clear(&mut self) {
        self.skips.clear();
    }

    /// Drop only the skips scoped to `root`.
    ///
    /// A rebase names one root, so it can only vouch for that one. Clearing
    /// wholesale also discards another root's block and every unscoped
    /// [`SkipKind::Session`] loss, which by the rule above stops the gate
    /// holding writes under a root nobody rebased.
    pub fn clear_root(&mut self, root: &Path) {
        self.skips.retain(
            |skip| !matches!(&skip.record, SkipKind::Root { root: scoped } if **scoped == *root),
        );
    }
}

/// One journal record the loader had to skip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skip {
    pub record: SkipKind,
    /// 1-based line number in `review.jsonl`, so an operator can find it.
    pub line: u32,
    /// The parse failure, verbatim.
    pub reason: String,
}

/// What a skipped record costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SkipKind {
    /// The header, a base record, or a line the loader could not classify at
    /// all. Nothing about the session's base is trustworthy, so every root
    /// blocks — including ones this ledger may not even know it tracked.
    Session,
    /// An interval under a root the loader could still identify. Blocks writes
    /// under that root only; the rest of the session is unaffected.
    Root { root: PhysicalRoot },
    /// A child link, a decision, or a comment. The queue is poorer, not wrong:
    /// a lost decision fails closed on its own (the hunk returns as
    /// unreviewed) and a lost comment costs no safety property.
    Informational,
}

impl SkipKind {
    pub fn blocks(&self, root: &Path) -> bool {
        match self {
            SkipKind::Session => true,
            SkipKind::Root { root: scoped } => **scoped == *root,
            SkipKind::Informational => false,
        }
    }
}

/// Whether the ledger can still account for one tracked root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootStatus {
    /// Repository top level. See [`RootInterval::root`].
    pub root: PhysicalRoot,
    /// Why this root's attribution cannot be trusted; `None` is intact.
    ///
    /// A reason rather than a bool because the only action available to the
    /// user is `review.rebase`, and they should be able to tell a gc'd base
    /// tree from a root that has been deleted before they take it.
    pub degraded: Option<String>,
}

impl RootStatus {
    pub fn intact(root: PhysicalRoot) -> Self {
        Self {
            root,
            degraded: None,
        }
    }

    pub fn degraded(root: PhysicalRoot, reason: impl Into<String>) -> Self {
        Self {
            root,
            degraded: Some(reason.into()),
        }
    }

    pub fn is_degraded(&self) -> bool {
        self.degraded.is_some()
    }
}

/// The gate's answer about one file.
///
/// Three-valued rather than a `bool`, because "no unreviewed hunk here" and
/// "I cannot tell whether there is an unreviewed hunk here" must not be the
/// same answer. Collapsing them is exactly how a structural failure turns the
/// gate off instead of turning it up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing unreviewed, and the ledger can account for the root. Proceed.
    Clear,
    /// A live unreviewed hunk in this file. A human can clear it by reviewing.
    Unreviewed,
    /// The ledger cannot account for this file's root, so silence is not
    /// evidence. A human clears it with `review.rebase`, which is what keeps
    /// this from being an unreleasable hang.
    ///
    /// Carries the root rather than just the fact, because the two block for
    /// different reasons and the user is told which. Reporting the *file* here
    /// — which is what collapsing this into a bare "blocks" did — sends them to
    /// review a file containing no reviewable hunk, with nothing there to act
    /// on. `None` is an unscoped loss: the ledger no longer knows which roots
    /// it tracked, so there is no root to name.
    Degraded { root: Option<PhysicalRoot> },
}

impl Verdict {
    pub fn blocks(&self) -> bool {
        !matches!(self, Verdict::Clear)
    }

    /// What to tell the user they are waiting on, if this blocks.
    ///
    /// A degraded root names itself; an unreviewed hunk is answered by the
    /// caller, which knows the path it asked about.
    pub fn degraded_root(&self) -> Option<&PhysicalRoot> {
        match self {
            Verdict::Degraded { root } => root.as_ref(),
            _ => None,
        }
    }
}

/// A difference between `session_base` and the current worktree: the review
/// surface, and the only unit that can be accepted, rejected or reverted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposedHunk {
    pub id: HunkId,
    /// Repository top level. See [`RootInterval::root`].
    pub root: PhysicalRoot,
    /// Path relative to [`Self::root`].
    pub path: String,
    /// Lines this hunk replaces, in `session_base` coordinates.
    pub base_range: LineRange,
    /// Lines this hunk occupies, in current-worktree coordinates.
    pub current_range: LineRange,
    /// Base-side text, newline-terminated. Empty for a pure insertion.
    pub before_content: String,
    /// Current-side text, newline-terminated. Empty for a pure deletion.
    pub after_content: String,
    /// Tool calls whose writes survive into this hunk, in ledger order.
    /// Many-to-many and informational — a hunk may compose the work of
    /// several calls, and one call may spread across several hunks.
    pub tool_call_ids: Vec<String>,
    pub state: ReviewState,
    /// True when this hunk's identity already carries a recorded
    /// [`ReviewState::Rejected`] and the change is in the composed diff
    /// anyway.
    ///
    /// A rejection reverts, so a rejected identity reappearing can mean
    /// exactly one thing: the agent applied the same change again. That is new
    /// work, never reviewed where it now sits, so [`Self::state`] comes back
    /// as [`ReviewState::Unreviewed`] and the gate holds — re-applying costs
    /// the agent a block, never a bypass.
    ///
    /// Derived on every listing rather than stored, and it deletes nothing:
    /// `unreviewed_hunks` → `list_hunks` is the gate poller's half-second read
    /// loop, so dropping the recorded decision there would make a hot `&self`
    /// path irreversibly discard the user's answers. The flag is what makes a
    /// grind *visible* instead of letting the user accept out of fatigue.
    #[serde(default)]
    pub reapplied: bool,
}

impl ComposedHunk {
    /// True when no tool call in the ledger accounts for this hunk.
    ///
    /// Covers three cases that must all behave identically: the user edited
    /// the file themselves, an unbracketed writer (a `Bash` formatter, a Lua
    /// plugin) landed it, or the only candidate intervals were contested.
    /// External hunks are shown so the composed diff stays honest, never
    /// block the agent, and are not revertible — undoing one would destroy
    /// the user's own edit while reporting that an agent edit was undone.
    pub fn is_external(&self) -> bool {
        self.tool_call_ids.is_empty()
    }

    /// Absolute path to the file this hunk lives in.
    pub fn absolute_path(&self) -> PathBuf {
        self.root.join(&self.path)
    }
}

/// A turn parked on the review gate, waiting for a human.
///
/// **Live daemon state, never a persisted event.** It describes a turn that is
/// running *right now*; a resumed session replaying a stored `blocked: true`
/// would show a chip claiming to wait on a review that no turn is waiting for
/// and no action can clear. It is therefore excluded from `should_persist` and
/// carried only in memory, dropped with the hold that created it.
///
/// The field names are the `review_gate` event's payload, minus its `blocked`
/// discriminator, so a client that missed the event and one that polls the
/// state decode the same shape. A blocked agent that merely goes quiet is
/// indistinguishable from a hung one, which is the whole reason this is
/// observable at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateBlock {
    /// The tool call being held.
    pub tool: String,
    /// The first target still unreviewed — what a human must answer to
    /// release the turn.
    pub path: String,
}

/// Who wrote a review comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentAuthor {
    Human,
    Agent,
}

/// A review comment anchored to a line range.
///
/// Ranges rather than hunks: a hunk comment is just a comment whose range
/// equals a hunk, and ranges additionally allow commenting on unchanged code
/// and on spans crossing several hunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    /// Repository top level. See [`RootInterval::root`].
    pub root: PhysicalRoot,
    /// Path relative to [`Self::root`].
    pub path: String,
    /// Tree the range is anchored in. Later diffs re-project the range
    /// forward from here; a range that no longer projects is outdated.
    pub base_tree: TreeSha,
    pub line_range: LineRange,
    pub body: String,
    pub author: CommentAuthor,
    pub resolved: bool,
    pub created_at: DateTime<Utc>,
}

impl Comment {
    /// Anchor a new, unresolved comment. The id is minted here so every
    /// producer — RPC handler, Lua tool, agent — gets the same shape.
    pub fn new(
        root: PhysicalRoot,
        path: impl Into<String>,
        base_tree: TreeSha,
        line_range: LineRange,
        body: impl Into<String>,
        author: CommentAuthor,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            root,
            path: path.into(),
            base_tree,
            line_range,
            body: body.into(),
            author,
            resolved: false,
            created_at: Utc::now(),
        }
    }
}
