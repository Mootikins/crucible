//! Attribution: which tool calls account for which composed hunks.
//!
//! Many-to-many and strictly informational. One hunk can compose the work of
//! several calls; one call can spread across several hunks; and a call whose
//! work was later overwritten attributes to nothing at all — which is the
//! superseded signal the ToolCard renders.
//!
//! Attribution is computed by *projecting* an interval's changed line numbers
//! into base and current coordinates, not by matching line content. Content
//! matching would attribute every hunk containing a lone `}` to whichever
//! call last added a `}` somewhere in the file.

use std::collections::HashMap;
use std::path::Path;

use crucible_core::session::{ComposedHunk, Interval, LineRange};
use similar::{DiffOp, TextDiff};

use super::compose::{hunks_between, RootComposition};
use super::error::ReviewResult;
use super::git;

/// Maps line numbers from one version of a file into another, for lines that
/// survive unchanged between them.
///
/// A line that did not survive has no projection, and that is the correct
/// answer: a later edit overwrote it, so the earlier call no longer accounts
/// for anything at that position.
struct LineMap {
    /// `(from_start, to_start, len)`, 1-based, in ascending `from` order.
    spans: Vec<(u32, u32, u32)>,
}

impl LineMap {
    fn between(from: &str, to: &str) -> Self {
        let diff = TextDiff::from_lines(from, to);
        let spans = diff
            .ops()
            .iter()
            .filter_map(|op| match op {
                DiffOp::Equal {
                    old_index,
                    new_index,
                    len,
                } => Some((*old_index as u32 + 1, *new_index as u32 + 1, *len as u32)),
                _ => None,
            })
            .collect();
        Self { spans }
    }

    fn project(&self, line: u32) -> Option<u32> {
        self.spans.iter().find_map(|(from, to, len)| {
            (line >= *from && line < from + len).then(|| to + (line - from))
        })
    }
}

/// Attribute the composed hunks of one root to the intervals that produced
/// them.
///
/// `intervals` must already be filtered to this root. Contested intervals are
/// dropped by the caller: a bracket that overlapped another writer cannot say
/// which of the two made a change, and a confident wrong attribution is worse
/// than none — the hunk simply surfaces as external.
pub(super) async fn attribute_root(
    root: &Path,
    composition: &mut RootComposition,
    intervals: &[&Interval],
) -> ReviewResult<()> {
    // Index hunks by path so each interval only pays for files it touched.
    let mut by_path: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, hunk) in composition.hunks.iter().enumerate() {
        by_path.entry(hunk.path.clone()).or_default().push(idx);
    }
    if by_path.is_empty() {
        return Ok(());
    }

    for interval in intervals {
        let Some(root_interval) = interval.roots_touched.iter().find(|r| *r.root == *root) else {
            continue;
        };
        for (path, kind) in
            git::changed_paths(root, &root_interval.before_tree, &root_interval.after_tree).await?
        {
            let Some(hunk_indices) = by_path.get(&path) else {
                continue;
            };
            let Some((base_text, current_text)) = composition.texts.get(&path) else {
                continue;
            };

            let before =
                git::blob_or_empty(root, &root_interval.before_tree, &path, kind.has_before())
                    .await?;
            let after =
                git::blob_or_empty(root, &root_interval.after_tree, &path, kind.has_after())
                    .await?;
            let (Some(before), Some(after)) = (before, after) else {
                continue;
            };

            // Lines this call wrote, projected forward into the worktree as it
            // stands now, and lines it removed, projected back into the base.
            let changes = hunks_between(&before, &after);
            let forward = LineMap::between(&after, current_text);
            let backward = LineMap::between(&before, base_text);
            let added: Vec<u32> = project_all(&changes, &forward, |h| h.after_range);
            let removed: Vec<u32> = project_all(&changes, &backward, |h| h.before_range);

            for &idx in hunk_indices {
                let hunk = &mut composition.hunks[idx];
                if hits(&added, hunk.current_range) || hits(&removed, hunk.base_range) {
                    record(hunk, &interval.tool_call_id);
                }
            }
        }
    }
    Ok(())
}

/// Project every line of every change range through `map`, sorted.
fn project_all(
    changes: &[super::compose::RawHunk],
    map: &LineMap,
    range_of: impl Fn(&super::compose::RawHunk) -> LineRange,
) -> Vec<u32> {
    let mut out: Vec<u32> = changes
        .iter()
        .flat_map(|h| {
            let r = range_of(h);
            (r.start..r.end).filter_map(|line| map.project(line))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// True when any projected line falls inside `range`.
fn hits(sorted_lines: &[u32], range: LineRange) -> bool {
    if range.is_empty() {
        return false;
    }
    let first_at_or_after = sorted_lines.partition_point(|line| *line < range.start);
    sorted_lines
        .get(first_at_or_after)
        .is_some_and(|line| *line < range.end)
}

fn record(hunk: &mut ComposedHunk, tool_call_id: &str) {
    if !hunk.tool_call_ids.iter().any(|id| id == tool_call_id) {
        hunk.tool_call_ids.push(tool_call_id.to_string());
    }
}
