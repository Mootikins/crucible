//! The review gate: a writing tool call waits while the file it targets still
//! has unreviewed hunks.
//!
//! Split from `tool_call.rs` the way `gate_decision` and `isolation_gate` are,
//! so the rule — which calls are gated, on what — is a pure function with its
//! own tests, and the call site is one line.
//!
//! # Why it does not `match` on the mode
//!
//! The mode is an open string id. Matching on it here is how
//! `agent_factory.rs`'s `if mode == "plan"` came to exist and then had to be
//! mirrored in three more places. The mode resolves to a [`ReviewPolicy`]
//! once, at the top, and everything below reads the policy.
//!
//! That resolution knows the built-in modes only. A mode declared in Lua, in
//! config, or advertised by an ACP agent takes the conservative default —
//! `PreWrite` — so it fails closed, but it cannot yet declare a *weaker*
//! policy for itself. Wiring that means carrying the mode's descriptor to the
//! gate rather than its id; until then `session.list_modes` can report a
//! policy weaker than what is enforced.
//!
//! # Why it blocks rather than denies
//!
//! A denial is a result the model reads, and the model's response to "you may
//! not edit this file yet" is to try again — a loop that burns tokens and ends
//! in the same place. Waiting is what the situation actually is: the call is
//! fine, it is just not its turn. There is deliberately no timeout; the turn's
//! own `execution_timeout_secs` and the user's cancel already bound it, and a
//! blocked agent is a correct agent. It must never look *stalled*, though,
//! which is what the `review_gate` event is for.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crucible_core::session::{GateBlock, ReviewState};
use crucible_core::types::acp::FileDiff;
use crucible_core::types::mode::ReviewPolicy;

use super::super::*;
use crate::review::GateHold;

/// How often a blocked call re-asks the ledger.
///
/// Each poll recomputes the composed diff for one file, which costs a `git
/// diff` against the session base. Half a second is imperceptible to whoever
/// just clicked Accept and cheap enough that a long block is not a busy loop.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// What the gate holds this tool call against, decided before any ledger
/// lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum GateSubject {
    /// Nothing. Let the call through.
    Ungated,
    /// These specific files — the call named them in its arguments.
    Files(Vec<String>),
    /// The whole session, but only against hunks left unreviewed by an
    /// *earlier* turn. For a call whose targets cannot be known.
    EarlierTurns,
}

/// Whether this call is gated, and on what.
///
/// Pure, and separate from the ledger lookup around it, because this is the
/// rule; finding the hunks is not what can be subtly wrong.
pub(super) fn gate_subject(
    policy: ReviewPolicy,
    tool_name: &str,
    write_targets: &[String],
) -> GateSubject {
    if policy != ReviewPolicy::PreWrite {
        return GateSubject::Ungated;
    }
    match tool_name {
        // A delegation writes through a child session, so nothing in its
        // arguments names a file. Falling back to the whole session is the
        // only honest option — but only against hunks from an earlier turn,
        // so a delegation is not blocked by the edits of the turn issuing it.
        "delegate_session" => GateSubject::EarlierTurns,
        // `bash` is equally unknowable and deliberately does NOT take that
        // fallback. Almost every turn ends in a build or test command, so
        // gating bash on the session would block the agent on the edits it
        // just made, every turn, and `normal` mode would feel broken. Bash
        // writes are still captured and attributed by the bracket watcher
        // (needs_review_bracket("bash") is true); they just aren't gated.
        "bash" => GateSubject::Ungated,
        _ => {
            if write_targets.is_empty() {
                GateSubject::Ungated
            } else {
                GateSubject::Files(write_targets.to_vec())
            }
        }
    }
}

/// The files a call is proposing to write, in the order it named them.
///
/// Sourced from the `FileDiff`s the turn already carries, falling back to
/// synthesising them from the call's own name and arguments — which is what a
/// call unwrapped from the `invoke_tool` bridge needs, since the diffs
/// upstream were synthesised against `invoke_tool` and are empty. Reusing
/// `diff_synth` rather than adding a fourth "is this a writing tool" list is
/// the point: it is already mcp-prefix aware, and it is what produced the diff
/// the tool card renders.
pub(super) fn write_targets(
    tool_name: &str,
    args: &serde_json::Value,
    diffs: &[FileDiff],
) -> Vec<String> {
    let synthesized;
    let diffs = if diffs.is_empty() {
        synthesized = crate::tools::diff_synth::synthesize_diffs(tool_name, args);
        &synthesized
    } else {
        diffs
    };
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for diff in diffs {
        if seen.insert(diff.path.as_str()) {
            targets.push(diff.path.clone());
        }
    }
    targets
}

/// Hold the call until nothing it targets is unreviewed.
///
/// Returns once the call may proceed.
///
/// # The fail-closed exception, and its limit
///
/// A query the ledger cannot answer *at all* — no ledger for the session, or a
/// transient `git` failure — lets the call through. Not because fail-open is
/// the rule, but because in that state there is no hunk for anyone to review
/// and therefore no human action that could ever release the hold: refusing
/// does not fail closed, it hangs the turn.
///
/// That is the whole exception. Degradation is otherwise graded — see
/// [`crucible_core::session::Integrity`] for why a lost interval has to block
/// rather than pass. A *structural* failure (a tracked root that is gone, a
/// base tree that has been gc'd) blocks, bounded by `review.rebase`; only the
/// transient case is let through here.
///
/// # Why the block is observable
///
/// While parked, the session carries a [`GateBlock`], so a client that missed
/// the `review_gate` event can still tell a blocked turn from a hung one. The
/// hold is a guard, not a flag: this loop awaits a sleep, so a cancelled or
/// timed-out turn drops the future *while parked*, and only `Drop` clears the
/// block.
pub(super) async fn hold_for_review(
    stream_ctx: &StreamContext,
    tool_name: &str,
    args: &serde_json::Value,
    diffs: &[FileDiff],
) {
    let Some(ledger) = stream_ctx.agent_stream_config.review.clone() else {
        return;
    };
    // A session whose roots are not git-backed has no ledger and nothing to
    // gate on. Checked before the policy so a non-git workspace does not warn
    // its way through every tool call.
    //
    // A session whose journal could not be read *does* have one — an empty
    // ledger carrying an unscoped loss — and that is deliberate: absence here
    // is indistinguishable from "not a git repo", so it would turn the gate off
    // for exactly the session whose evidence was lost.
    if !ledger.is_open(&stream_ctx.session_id) {
        return;
    }
    let policy = ReviewPolicy::for_mode_id(&stream_ctx.session_mode)
        .effective_for(&stream_ctx.agent_stream_config.agent_type);
    // `gate_subject` re-checks this. The early return is what keeps a
    // non-gating session from paying to work out what the call writes.
    if policy != ReviewPolicy::PreWrite {
        return;
    }

    let subject = gate_subject(policy, tool_name, &write_targets(tool_name, args, diffs));
    if subject == GateSubject::Ungated {
        return;
    }

    // Read once: `current` only advances on a User or Agent node, so it is
    // stable for the whole tool batch, and re-reading it inside the loop would
    // hold the tree lock on every poll.
    let this_turn = stream_ctx.conversation_tree.lock().await.current().index();
    // Likewise once: the session's registered directories do not move mid-turn,
    // and this is a map lookup the poll loop has no reason to repeat.
    let bases = relative_bases(stream_ctx);

    let mut hold: Option<GateHold> = None;
    loop {
        let waiting_on = match &subject {
            GateSubject::Ungated => None,
            GateSubject::Files(paths) => blocking_file(&ledger, stream_ctx, &bases, paths).await,
            GateSubject::EarlierTurns => {
                blocking_earlier_turn(&ledger, &stream_ctx.session_id, this_turn).await
            }
        };
        let Some(waiting_on) = waiting_on else {
            break;
        };
        if hold.is_none() {
            info!(
                session_id = %stream_ctx.session_id,
                tool = %tool_name,
                waiting_on = %waiting_on,
                "review gate: holding tool call until its target is reviewed"
            );
            hold = Some(ledger.hold_gate(
                &stream_ctx.session_id,
                GateBlock {
                    tool: tool_name.to_string(),
                    path: waiting_on.clone(),
                },
            ));
            emit_gate_event(stream_ctx, tool_name, true, Some(waiting_on));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Dropped explicitly rather than at end of scope: the block must be gone
    // before the release event goes out, or a client that re-polls on the
    // event still reads the session as waiting.
    if hold.take().is_some() {
        debug!(
            session_id = %stream_ctx.session_id,
            tool = %tool_name,
            "review gate: released"
        );
        emit_gate_event(stream_ctx, tool_name, false, None);
    }
}

/// The first named file that still has unreviewed hunks, if any.
async fn blocking_file(
    ledger: &crate::review::ReviewLedgers,
    stream_ctx: &StreamContext,
    bases: &[PathBuf],
    paths: &[String],
) -> Option<String> {
    let session_id = &stream_ctx.session_id;
    for path in paths {
        let resolved = target_spellings(bases, path);
        match ledger.has_unreviewed_in_file(session_id, &resolved).await {
            // `Degraded` blocks alongside `Unreviewed`: a root the ledger
            // cannot account for produces no hunks at all, so treating its
            // silence as "clear" is the structural-failure case turning the
            // gate off exactly when attribution has stopped working.
            // `review.rebase` is the human-reachable release.
            //
            // What it reports differs, though, and naming the file for both
            // was a small lie with a real cost: a degraded root has no hunk in
            // that file to review, so the panel sent the user somewhere with
            // nothing to act on and no way to tell why the turn would not
            // move. `blocking_earlier_turn` already named the root; this now
            // agrees with it.
            Ok(verdict) if verdict.blocks() => {
                return Some(match verdict.degraded_root() {
                    Some(root) => root.display().to_string(),
                    None => path.clone(),
                })
            }
            Ok(_) => {}
            Err(error) => {
                warn!(
                    session_id = %session_id,
                    path = %path,
                    error = %error,
                    "review gate: could not resolve hunks for target; letting the call through"
                );
            }
        }
    }
    None
}

/// A description of what an earlier turn left unreviewed, if anything.
///
/// External hunks are already excluded below — they are the user's own edits,
/// and blocking the agent on those is nonsense.
///
/// A degraded root is checked *first*, and before the no-intervals
/// short-circuit. A root the ledger cannot account for contributes no hunks and
/// no intervals, so every cheaper test here reads it as "nothing to block on" —
/// which is the same graded degradation that holds a `write_file` letting a
/// `delegate_session` straight through, and the delegated child gets a fresh
/// ledger of its own with nothing inherited. The parent's unreviewable work
/// would be bypassable by delegating.
async fn blocking_earlier_turn(
    ledger: &crate::review::ReviewLedgers,
    session_id: &str,
    this_turn: u32,
) -> Option<String> {
    if ledger.integrity(session_id).blocks_everything() {
        return Some(UNSCOPED_LOSS.to_string());
    }
    // Read before the listing purely so a session with no ledger takes the same
    // silent `None` it always did rather than warning once per poll.
    let session_ledger = ledger.ledger(session_id)?;

    let (hunks, statuses) = match ledger.list_hunks_with_status(session_id).await {
        Ok(listed) => listed,
        Err(error) => {
            warn!(
                session_id = %session_id,
                error = %error,
                "review gate: could not list unreviewed hunks; letting the call through"
            );
            return None;
        }
    };
    if let Some(degraded) = statuses.iter().find(|status| status.is_degraded()) {
        return Some(degraded.root.display().to_string());
    }

    let earlier: HashSet<&str> = session_ledger
        .intervals()
        .iter()
        .filter(|interval| interval.node_id < this_turn)
        .map(|interval| interval.tool_call_id.as_str())
        .collect();
    if earlier.is_empty() {
        return None;
    }

    hunks
        .into_iter()
        .filter(|hunk| hunk.state == ReviewState::Unreviewed && !hunk.is_external())
        .find(|hunk| {
            hunk.tool_call_ids
                .iter()
                .any(|id| earlier.contains(id.as_str()))
        })
        .map(|hunk| hunk.path)
}

/// What a session-wide loss is called in the gate block, since there is no
/// root left to name: the records that would have said which roots the ledger
/// tracked are the ones that could not be read.
const UNSCOPED_LOSS: &str = "every tracked root (this session's review journal could not be read)";

/// Every spelling of one relative tool argument the ledger might recognise.
///
/// A relative path has no single base. `write_file` names one relative to the
/// workspace, `create_note` names one relative to the **kiln** — which is its
/// own tracked root whenever it is its own repository — and a hunk's `path` is
/// relative to its repository top level, a third thing again when one of those
/// directories is nested inside the other. Joining onto the workspace alone
/// matched none of the others, so every note the agent wrote was ungated.
///
/// Over-matching is the safe direction here: a spurious match costs a block a
/// human can clear by reviewing, where a missed one costs the gate entirely.
fn target_spellings(bases: &[PathBuf], path: &str) -> Vec<PathBuf> {
    let raw = PathBuf::from(path);
    if raw.is_absolute() {
        return vec![raw];
    }
    let mut spellings: Vec<PathBuf> = Vec::with_capacity(bases.len() + 1);
    for base in bases {
        let joined = base.join(&raw);
        if !spellings.contains(&joined) {
            spellings.push(joined);
        }
    }
    spellings.push(raw);
    spellings
}

/// The directories a tool's relative path argument could be relative to.
///
/// The same set the send path opens the ledger over, minus the normalisation:
/// these are compared against, never stored, so they stay in the spelling the
/// session was registered with and [`crate::review::paths`] reconciles them at
/// query time.
fn relative_bases(stream_ctx: &StreamContext) -> Vec<PathBuf> {
    let mut bases = vec![stream_ctx.workspace_path.clone()];
    if let Some(session) = stream_ctx
        .session_manager
        .get_session(&stream_ctx.session_id)
    {
        for base in &session.kilns {
            if !bases.contains(base) {
                bases.push(base.clone());
            }
        }
    }
    bases
}

/// Tell subscribers the turn is waiting on a human, and when it stops.
///
/// One event name carrying a boolean rather than two names, so a client tracks
/// a single piece of state: a blocked agent that merely goes quiet is
/// indistinguishable from a hung one.
fn emit_gate_event(
    stream_ctx: &StreamContext,
    tool_name: &str,
    blocked: bool,
    waiting_on: Option<String>,
) {
    emit_event(
        &stream_ctx.event_tx,
        SessionEventMessage::new(
            &stream_ctx.session_id,
            "review_gate",
            serde_json::json!({
                "blocked": blocked,
                "tool": tool_name,
                "path": waiting_on,
            }),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::ReviewLedgers;
    use crucible_core::session::{Ledger, PhysicalRoot, RootBase, TreeSha};
    use serde_json::json;

    /// A root the ledger cannot account for produces no hunks and no
    /// intervals, so every cheap test in the session-wide fallback reads it as
    /// "nothing to block on". The same graded degradation that holds a
    /// `write_file` therefore let a `delegate_session` straight through, and
    /// the child it spawned started with a fresh ledger of its own — the
    /// parent's unreviewable work bypassable by delegating.
    #[tokio::test]
    async fn a_degraded_root_holds_a_delegation_the_way_it_holds_a_write() {
        let ledgers = Arc::new(ReviewLedgers::default());
        // A root that has been moved or deleted: nothing to capture, nothing to
        // diff, and no interval left to make the gate look.
        let gone = PhysicalRoot::from_top_level("/no/such/tracked/root");
        ledgers.restore(Ledger::new(
            "sess",
            vec![RootBase {
                root: gone.clone(),
                base_tree: TreeSha::new("0".repeat(40)),
            }],
        ));

        assert_eq!(
            blocking_earlier_turn(&ledgers, "sess", 7).await,
            Some(gone.display().to_string()),
            "a delegation ran on a session whose attribution had stopped working"
        );
    }

    /// An unscoped loss has no root left to name — the records that would have
    /// said which roots the session tracked are exactly the ones that could not
    /// be read — so the fallback has to block on the session itself.
    #[tokio::test]
    async fn an_unscoped_journal_loss_holds_a_delegation() {
        let dir = tempfile::TempDir::new().unwrap();
        let journal = dir.path().join("review.jsonl");
        // Present to `try_exists`, unreadable to `read_to_string`.
        std::fs::create_dir(&journal).unwrap();

        let ledgers = Arc::new(ReviewLedgers::default());
        let _ = ledgers.restore_from_journal("sess", &journal).await;

        assert_eq!(
            blocking_earlier_turn(&ledgers, "sess", 7).await,
            Some(UNSCOPED_LOSS.to_string())
        );
    }

    fn targets(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_write_to_a_named_file_is_gated_on_that_file() {
        assert_eq!(
            gate_subject(
                ReviewPolicy::PreWrite,
                "edit_file",
                &targets(&["src/foo.rs"])
            ),
            GateSubject::Files(targets(&["src/foo.rs"]))
        );
    }

    /// `plan` asks for no review and `auto` reviews at turn end; neither may
    /// hold a call. Expressed as policies, not mode ids — the gate never sees
    /// a mode.
    #[test]
    fn only_prewrite_gates_anything() {
        for policy in [ReviewPolicy::None, ReviewPolicy::PostTurn] {
            assert_eq!(
                gate_subject(policy, "edit_file", &targets(&["src/foo.rs"])),
                GateSubject::Ungated,
                "{policy:?} must never hold a call"
            );
            assert_eq!(
                gate_subject(policy, "delegate_session", &[]),
                GateSubject::Ungated,
                "{policy:?} must never hold a delegation"
            );
        }
    }

    /// The ACP degradation, at the seam the gate actually reads: an external
    /// agent has already run the tool, so the strongest thing that can be true
    /// of it is PostTurn — and PostTurn gates nothing.
    #[test]
    fn an_external_agents_write_is_never_held() {
        let policy = ReviewPolicy::for_mode_id("normal").effective_for("acp");
        assert_eq!(policy, ReviewPolicy::PostTurn);
        assert_eq!(
            gate_subject(policy, "edit_file", &targets(&["src/foo.rs"])),
            GateSubject::Ungated
        );
    }

    #[test]
    fn the_same_write_is_held_for_an_internal_agent() {
        let policy = ReviewPolicy::for_mode_id("normal").effective_for("internal");
        assert_eq!(
            gate_subject(policy, "edit_file", &targets(&["src/foo.rs"])),
            GateSubject::Files(targets(&["src/foo.rs"]))
        );
    }

    /// Blocking bash on the session would block every turn on its own edits,
    /// because almost every turn ends in a build or test command.
    #[test]
    fn bash_never_takes_the_session_wide_fallback() {
        assert_eq!(
            gate_subject(ReviewPolicy::PreWrite, "bash", &[]),
            GateSubject::Ungated
        );
    }

    /// ...and not even when something upstream managed to name a file for it.
    /// Bash's real writes are unknowable from its arguments; the watcher
    /// backstop owns them.
    #[test]
    fn bash_is_ungated_even_with_a_named_target() {
        assert_eq!(
            gate_subject(ReviewPolicy::PreWrite, "bash", &targets(&["src/foo.rs"])),
            GateSubject::Ungated
        );
    }

    #[test]
    fn a_delegation_falls_back_to_earlier_turns() {
        assert_eq!(
            gate_subject(ReviewPolicy::PreWrite, "delegate_session", &[]),
            GateSubject::EarlierTurns
        );
    }

    #[test]
    fn a_read_only_call_is_ungated() {
        assert_eq!(
            gate_subject(ReviewPolicy::PreWrite, "read_file", &[]),
            GateSubject::Ungated
        );
    }

    #[test]
    fn write_targets_come_from_the_turns_diffs_when_it_has_them() {
        let diffs = vec![FileDiff::new("src/a.rs", "new")];
        assert_eq!(
            write_targets("invoke_tool", &json!({}), &diffs),
            targets(&["src/a.rs"])
        );
    }

    /// An `invoke_tool` bridge call arrives with diffs synthesised against
    /// `invoke_tool` (i.e. none), so the unwrapped name and arguments have to
    /// be re-synthesised or the gate would never see the write.
    #[test]
    fn write_targets_are_synthesized_when_the_turn_carried_none() {
        let args = json!({ "path": "src/b.rs", "old_string": "a", "new_string": "b" });
        assert_eq!(
            write_targets("edit_file", &args, &[]),
            targets(&["src/b.rs"])
        );
    }

    #[test]
    fn write_targets_are_mcp_prefix_aware() {
        let args = json!({ "path": "src/c.rs", "content": "hi" });
        assert_eq!(
            write_targets("mcp__fs__write_file", &args, &[]),
            targets(&["src/c.rs"])
        );
    }

    #[test]
    fn write_targets_dedupe_repeated_paths() {
        let diffs = vec![
            FileDiff::new("src/a.rs", "one"),
            FileDiff::new("src/a.rs", "two"),
            FileDiff::new("src/b.rs", "three"),
        ];
        assert_eq!(
            write_targets("multi_edit", &json!({}), &diffs),
            targets(&["src/a.rs", "src/b.rs"])
        );
    }

    #[test]
    fn an_absolute_target_has_exactly_one_spelling() {
        assert_eq!(
            target_spellings(
                &[PathBuf::from("/w"), PathBuf::from("/k")],
                "/elsewhere/a.rs"
            ),
            vec![PathBuf::from("/elsewhere/a.rs")]
        );
    }

    /// `create_note`'s `path` is KILN-relative, and a kiln that is its own
    /// repository is a tracked root of its own. Joining every relative target
    /// onto the workspace matched neither the kiln's absolute path nor the
    /// root-relative spelling the hunk carries, so kiln writes were ungated.
    #[test]
    fn a_relative_target_is_spelled_against_every_base_and_left_root_relative() {
        assert_eq!(
            target_spellings(&[PathBuf::from("/w"), PathBuf::from("/k")], "Notes/x.md"),
            vec![
                PathBuf::from("/w/Notes/x.md"),
                PathBuf::from("/k/Notes/x.md"),
                PathBuf::from("Notes/x.md"),
            ]
        );
    }

    #[test]
    fn a_kiln_that_is_the_workspace_does_not_duplicate_a_spelling() {
        assert_eq!(
            target_spellings(&[PathBuf::from("/w"), PathBuf::from("/w")], "src/a.rs"),
            vec![PathBuf::from("/w/src/a.rs"), PathBuf::from("src/a.rs")]
        );
    }
}
