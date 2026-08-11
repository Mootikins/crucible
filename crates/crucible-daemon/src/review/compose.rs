//! Composed-diff computation: `session_base` → current worktree.
//!
//! The composed diff, not the interval stream, is the review surface. Every
//! hunk it produces is a difference against the session's base tree and is
//! therefore independently revertible — which a single tool call's
//! contribution is not, once a later call has touched the same lines.

use std::collections::HashMap;

use crucible_core::session::{ComposedHunk, HunkId, LineRange, PhysicalRoot, ReviewState, TreeSha};
use similar::{DiffOp, TextDiff};

use super::error::ReviewResult;
use super::git;

/// Split text into lines that keep their terminators, matching how
/// `similar::TextDiff::from_lines` indexes them.
pub(super) fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// One change cluster between two versions of a file.
pub(super) struct RawHunk {
    pub(super) before_range: LineRange,
    pub(super) after_range: LineRange,
    pub(super) before: String,
    pub(super) after: String,
}

/// Change clusters between two versions of a file, with zero context lines.
///
/// Zero context is what makes hunks independently revertible: a hunk that
/// carried surrounding context would fail to apply the moment an adjacent
/// hunk moved, which is precisely the "review state lands on the wrong
/// lines" failure the content-derived identity exists to prevent.
pub(super) fn hunks_between(before: &str, after: &str) -> Vec<RawHunk> {
    let before_lines = lines(before);
    let after_lines = lines(after);
    let diff = TextDiff::from_lines(before, after);

    let mut out = Vec::new();
    for group in diff.grouped_ops(0) {
        // Grouping with n=0 still emits zero-length `Equal` ops as group
        // separators; they carry no change and must not widen the range.
        let changes: Vec<&DiffOp> = group
            .iter()
            .filter(|op| !matches!(op, DiffOp::Equal { .. }))
            .collect();
        let (Some(first), Some(last)) = (changes.first(), changes.last()) else {
            continue;
        };

        let before_range = LineRange::new(
            first.old_range().start as u32 + 1,
            last.old_range().end as u32 + 1,
        );
        let after_range = LineRange::new(
            first.new_range().start as u32 + 1,
            last.new_range().end as u32 + 1,
        );
        out.push(RawHunk {
            before: slice(&before_lines, &before_range),
            after: slice(&after_lines, &after_range),
            before_range,
            after_range,
        });
    }
    out
}

/// Join the 1-based half-open `range` of `all` back into text.
pub(super) fn slice(all: &[&str], range: &LineRange) -> String {
    let start = (range.start.saturating_sub(1)) as usize;
    let end = (range.end.saturating_sub(1) as usize).min(all.len());
    if start >= end {
        return String::new();
    }
    all[start..end].concat()
}

/// The composed diff for one root.
pub(super) struct RootComposition {
    pub(super) hunks: Vec<ComposedHunk>,
    /// `path → (base text, current text)`, kept so attribution can project
    /// interval line numbers into base and current coordinates without
    /// re-reading every blob.
    pub(super) texts: HashMap<String, (String, String)>,
}

/// Compute the composed hunks for one root.
///
/// `root` is a repository top level and every returned `path` is relative to
/// it. Binary (non-UTF-8) files are skipped: they have no line hunks, and a
/// fabricated empty hunk would be indistinguishable from a no-op.
pub(super) async fn compose_root(
    root: &PhysicalRoot,
    base: &TreeSha,
    current: &TreeSha,
) -> ReviewResult<RootComposition> {
    let mut composed = Vec::new();
    let mut texts = HashMap::new();
    for (path, kind) in git::changed_paths(root, base, current).await? {
        let before = match git::blob_or_empty(root, base, &path, kind.has_before()).await? {
            Some(text) => text,
            None => continue,
        };
        let after = match git::blob_or_empty(root, current, &path, kind.has_after()).await? {
            Some(text) => text,
            None => continue,
        };

        for raw in hunks_between(&before, &after) {
            // The base range separates byte-identical repeats within one file
            // without being positional in the worktree: `base` is immutable
            // for the session, so these coordinates do not move under a later
            // edit the way an ordinal among siblings does.
            let id = HunkId::derive(root, &path, &raw.before, &raw.after, raw.before_range);

            composed.push(ComposedHunk {
                id,
                root: root.clone(),
                path: path.clone(),
                base_range: raw.before_range,
                current_range: raw.after_range,
                before_content: raw.before,
                after_content: raw.after,
                tool_call_ids: Vec::new(),
                state: ReviewState::Unreviewed,
                // Both are derived against the states map, which composition
                // deliberately cannot see; `list_hunks` fills them in.
                reapplied: false,
            });
        }
        texts.insert(path, (before, after));
    }
    Ok(RootComposition {
        hunks: composed,
        texts,
    })
}
