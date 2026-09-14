//! The undo side of a reject: a journaled stack of batches, per session.
//!
//! A reject is the daemon rewriting the user's file from the Changes panel.
//! Nothing in the browser can take that back — the hunk left the composed
//! diff when the revert landed — so the undo is a daemon method too, and it
//! is multi-level: a user who rejects three hunks and wants the second one
//! back needs a stack, not a slot.
//!
//! The stack lives in the journal. Every reject appends one `Rejected`
//! record after the `State` records of the hunks it reverted, and every undo
//! rewrites those states as `Unreviewed` and appends one `Undone`. Replay
//! pushes and pops in the same order, so a restart neither resurrects a
//! rejection the user took back nor forgets a batch the user could still
//! take back.

use crucible_core::session::ReviewState;
use tracing::debug;

use super::{compose, journal, BulkOutcome, RejectBatch, ReviewError, ReviewLedgers, ReviewResult};

/// How many batches a session's undo stack keeps. The oldest falls off.
///
/// Older rejects are not lost to the user: their lines are still in the
/// transcript's tool results, and the rejection note names them.
pub const REJECT_STACK_DEPTH: usize = 50;

impl ReviewLedgers {
    /// Push one user action's rejects onto the session's undo stack, and
    /// journal them. An empty batch — a bulk reject that applied nothing —
    /// pushes nothing, so an undo never pops a no-op.
    pub(super) async fn push_reject_batch(&self, session_id: &str, batch: RejectBatch) {
        if batch.is_empty() {
            return;
        }
        {
            let mut stack = self.reject_stack.entry(session_id.to_string()).or_default();
            stack.push(batch.clone());
            if stack.len() > REJECT_STACK_DEPTH {
                stack.remove(0);
            }
        }
        self.append(session_id, journal::Record::Rejected { batch })
            .await;
    }

    /// Take back the most recent reject: write every hunk in that batch back
    /// into its file and return each to the queue as `Unreviewed`.
    ///
    /// The batch is walked in reverse, because each hunk's `start` is in the
    /// coordinates of the file just after its own revert, and undoing the
    /// later reverts first is what puts the file back into those coordinates.
    /// Every write is prepared in memory before any file is touched: a hunk
    /// whose restored lines are no longer there (the user edited them) is
    /// [`ReviewError::Stale`], and one stale hunk refuses the whole batch,
    /// which stays on the stack. Writing the rest would leave the batch half
    /// undone with no record that says so, and the user may yet put the file
    /// back and ask again.
    ///
    /// An empty stack answers an empty outcome, not an error: the panel's
    /// Undo is reachable whether or not anything is left to undo.
    pub async fn undo_reject(&self, session_id: &str) -> ReviewResult<BulkOutcome> {
        if !self.ledgers.contains_key(session_id) {
            return Err(ReviewError::NoLedger(session_id.to_string()));
        }
        let Some(batch) = self
            .reject_stack
            .get(session_id)
            .and_then(|stack| stack.last().cloned())
        else {
            return Ok(BulkOutcome::default());
        };

        // `(absolute path, text)` for every file the batch touches, edited in
        // memory as the walk proceeds so a second hunk in one file sees the
        // first one's lines back in place.
        let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
        let mut outcome = BulkOutcome::default();
        for hunk in batch.iter().rev() {
            let path = hunk.root.join(&hunk.path);
            let slot = match files.iter().position(|(p, _)| *p == path) {
                Some(at) => at,
                None => {
                    // A missing file reads as empty, the way `revert_hunk`
                    // reads it: a revert that restored a deletion may have
                    // emptied it, and an undo of that deletes it again.
                    let text = match tokio::fs::read_to_string(&path).await {
                        Ok(text) => text,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                        Err(_) => {
                            outcome.failed.push((
                                hunk.id.clone(),
                                ReviewError::Stale {
                                    path: hunk.path.clone(),
                                },
                            ));
                            continue;
                        }
                    };
                    files.push((path, text));
                    files.len() - 1
                }
            };
            let current = &files[slot].1;
            match reapply(
                current,
                hunk.start,
                &hunk.before_content,
                &hunk.after_content,
            ) {
                Some(next) => {
                    files[slot].1 = next;
                    outcome.applied.push(hunk.id.clone());
                }
                None => outcome.failed.push((
                    hunk.id.clone(),
                    ReviewError::Stale {
                        path: hunk.path.clone(),
                    },
                )),
            }
        }
        if !outcome.failed.is_empty() {
            debug!(
                session_id,
                stale = outcome.failed.len(),
                "undo refused; the batch stays on the stack"
            );
            outcome.applied.clear();
            return Ok(outcome);
        }

        // Every write in one suppression window: this is the daemon editing
        // the worktree, and the watcher must not announce the undo as the
        // user's own change.
        let suppressed = self.suppress(session_id);
        for (path, text) in &files {
            tokio::fs::write(path, text).await?;
        }
        drop(suppressed);

        // Recorded in the order the hunks were rejected, so the journal reads
        // as the action did. The decision is rewritten first: a reload that
        // saw `Undone` without the state rewrite would list the restored
        // hunk as `reapplied`.
        for hunk in &batch {
            self.record_state(session_id, &hunk.id, ReviewState::Unreviewed)
                .await;
        }
        if let Some(mut stack) = self.reject_stack.get_mut(session_id) {
            stack.pop();
        }
        self.append(session_id, journal::Record::Undone).await;
        Ok(outcome)
    }
}

/// Write `after` over the lines `start..start + lines(before)` of `text`,
/// answering `None` when those lines do not read `before`.
///
/// The same clamp `revert_hunk` applies: an empty `before` matches any empty
/// slice, so a `start` past the end of the file is the evidence the slice
/// check cannot see that the file is no longer the one the revert wrote.
fn reapply(text: &str, start: u32, before: &str, after: &str) -> Option<String> {
    let lines = compose::lines(text);
    let first = start.saturating_sub(1) as usize;
    if first > lines.len() {
        return None;
    }
    let count = compose::lines(before).len();
    let last = (first + count).min(lines.len());
    if lines[first..last].concat() != before {
        return None;
    }
    let mut next = String::with_capacity(text.len() + after.len());
    next.push_str(&lines[..first].concat());
    next.push_str(after);
    next.push_str(&lines[last..].concat());
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::reapply;

    #[test]
    fn reapply_writes_after_over_the_restored_lines() {
        assert_eq!(
            reapply("1\n2\n3\n", 2, "2\n", "two\n").as_deref(),
            Some("1\ntwo\n3\n")
        );
    }

    #[test]
    fn reapply_restores_an_insertion_at_a_seam() {
        assert_eq!(
            reapply("1\n3\n", 2, "", "2\n").as_deref(),
            Some("1\n2\n3\n")
        );
    }

    #[test]
    fn reapply_restores_a_deletion_by_removing_the_lines() {
        assert_eq!(
            reapply("1\n2\n3\n", 2, "2\n", "").as_deref(),
            Some("1\n3\n")
        );
    }

    #[test]
    fn reapply_refuses_lines_that_moved_on() {
        assert_eq!(reapply("1\nELSE\n3\n", 2, "2\n", "two\n"), None);
    }

    #[test]
    fn reapply_refuses_a_start_past_the_end_even_for_an_insertion() {
        assert_eq!(reapply("1\n", 5, "", "x\n"), None);
    }
}
