//! Attribution and bracket lifecycle: which tool call a composed hunk belongs
//! to, and what the capture bracket can and cannot witness.

use super::*;

/// The property the whole design rests on: a hunk keyed positionally would
/// change identity — and lose its review state — the moment the agent edited
/// anything above it.
#[tokio::test]
async fn hunk_keeps_identity_when_unrelated_lines_shift_above_it() {
    let fx = Fixture::new("one\ntwo\nthree\nfour\nfive\n").await;

    fx.call("call-1", 1, "one\ntwo\nEDITED\nfour\nfive\n").await;
    let before_shift = find(&fx.hunks().await, "EDITED\n").id.clone();

    // An unrelated insertion four lines above moves the hunk down.
    fx.call("call-2", 1, "zero\none\ntwo\nEDITED\nfour\nfive\n")
        .await;
    let hunks = fx.hunks().await;
    let shifted = find(&hunks, "EDITED\n");

    assert_eq!(
        shifted.id, before_shift,
        "identity moved with the line number"
    );
    assert_eq!(
        shifted.current_range.start, 4,
        "the hunk really did shift down"
    );
}

#[tokio::test]
async fn identical_hunks_in_one_file_get_distinct_identities() {
    let fx = Fixture::new("a\nkeep\na\n").await;
    fx.call("call-1", 1, "b\nkeep\nb\n").await;

    let hunks = fx.hunks().await;
    let ids: Vec<&HunkId> = hunks.iter().map(|h| &h.id).collect();
    assert_eq!(hunks.len(), 2, "expected two separate hunks: {hunks:#?}");
    assert_eq!(hunks[0].before_content, hunks[1].before_content);
    assert_eq!(hunks[0].after_content, hunks[1].after_content);
    assert_ne!(ids[0], ids[1], "identical repeats collided");
}

#[tokio::test]
async fn review_state_survives_an_edit_above_the_hunk() {
    let fx = Fixture::new("one\ntwo\nthree\n").await;
    fx.call("call-1", 1, "one\ntwo\nEDITED\n").await;
    let id = find(&fx.hunks().await, "EDITED\n").id.clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Accepted)
        .await
        .unwrap();

    fx.call("call-2", 1, "zero\none\ntwo\nEDITED\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(find(&hunks, "EDITED\n").state, ReviewState::Accepted);
    assert_eq!(
        find(&hunks, "zero\n").state,
        ReviewState::Unreviewed,
        "the new hunk must not inherit a decision"
    );
}

#[tokio::test]
async fn unrecognised_identity_is_unreviewed_and_rejected_by_set_state() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "two\n").await;

    let stranger = HunkId::derive(
        &PhysicalRoot::from_top_level(fx.dir.path()),
        "a.txt",
        "nothing\n",
        "like it\n",
        LineRange::new(1, 2),
    );
    let err = fx
        .ledgers
        .set_state(&fx.session, &stranger, ReviewState::Accepted)
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::UnknownHunk(_)), "{err:?}");

    // And the real hunk is still unreviewed — nothing leaked onto it.
    assert_eq!(fx.hunks().await[0].state, ReviewState::Unreviewed);
}

#[tokio::test]
async fn each_hunk_attributes_to_the_call_that_made_it() {
    let fx = Fixture::new("one\ntwo\nthree\nfour\nfive\n").await;
    fx.call("call-1", 1, "ONE\ntwo\nthree\nfour\nfive\n").await;
    fx.call("call-2", 2, "ONE\ntwo\nthree\nfour\nFIVE\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(find(&hunks, "ONE\n").tool_call_ids, ["call-1"]);
    assert_eq!(find(&hunks, "FIVE\n").tool_call_ids, ["call-2"]);
}

/// The superseded signal: a call whose written lines no longer exist
/// attributes to nothing, and its ToolCard loses its accept/reject buttons
/// because the action unit is gone.
#[tokio::test]
async fn a_call_whose_written_lines_are_gone_attributes_to_nothing() {
    let fx = Fixture::new("one\ntwo\n").await;
    fx.call("call-1", 1, "one\nFIRST ATTEMPT\ntwo\n").await;
    fx.call("call-2", 2, "one\nSECOND ATTEMPT\ntwo\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        hunks[0].tool_call_ids,
        ["call-2"],
        "the superseded call must not still claim the hunk"
    );
}

/// Attribution is many-to-many. Rewriting a line the first call already
/// rewrote leaves one composed hunk that both calls genuinely contributed to
/// — the first deleted the base line, the second wrote what stands there now.
#[tokio::test]
async fn a_hunk_both_calls_contributed_to_names_both() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "first attempt\n").await;
    fx.call("call-2", 2, "second attempt\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].tool_call_ids, ["call-1", "call-2"]);
}

#[tokio::test]
async fn a_call_that_wrote_nothing_records_no_interval() {
    let fx = Fixture::new("one\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    let recorded = fx
        .ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    assert!(!recorded);
    assert!(fx
        .ledgers
        .ledger(&fx.session)
        .unwrap()
        .intervals()
        .is_empty());
}

/// Where the bracket opens is the entire mechanism, so that is what this
/// pins: an edit landing before the open point is external, the tool's own
/// write inside it is not.
///
/// The predecessor of this test asserted that a human edit made *inside* an
/// open bracket stayed external. `ReviewLedgers` cannot answer that and must
/// not pretend to — its only evidence is a pair of tree SHAs, and a human
/// write and a tool write inside one window are byte-identical evidence. A
/// ledger that called the first one external would call the second one
/// external too. The fix for the human-edit-during-the-gate defect is
/// therefore in the caller, at the open point (see
/// `handle_tool_call_in_stream`), and the manager-level test for it is
/// `agent_manager::tests::review_capture`.
#[tokio::test]
async fn an_edit_that_lands_before_the_bracket_opens_is_external() {
    let fx = Fixture::new("one\n").await;
    // The human edits in their own editor while the review gate holds the
    // call — which is now before the bracket exists at all.
    std::fs::write(fx.dir.path().join("b.txt"), "typed by hand\n").unwrap();

    // Gate releases, bracket opens, the tool writes its own file.
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.write("written by the tool\n");
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    let b = hunks.iter().find(|h| h.path.contains("b.txt")).unwrap();
    assert!(
        b.is_external(),
        "the user's own edit was attributed to tool call {:?}",
        b.tool_call_ids
    );
    let a = hunks.iter().find(|h| h.path.contains("a.txt")).unwrap();
    assert_eq!(
        a.tool_call_ids,
        vec!["call-1".to_string()],
        "narrowing must not cost the call its own attribution"
    );
}

/// The bracket must take the suppression window itself.
///
/// The watcher's own tests took one by hand, so the *mechanism* was covered
/// and the *wiring* was not — `Ownership::Bracketed` was unreachable in
/// production, every agent write was classified external, and each one cost
/// every connected client a full recompose. Attribution was unharmed, since
/// `is_external()` reads `tool_call_ids` rather than the watcher, which is
/// precisely why nothing failed.
#[tokio::test]
async fn a_bracketed_call_suppresses_the_worktree_watch() {
    use crate::watch::external_changes::{ExternalChangeTracker, Ownership};

    let fx = Fixture::new("one\n").await;
    let tracker = Arc::new(ExternalChangeTracker::default());
    let root = fx.dir.path().to_path_buf();
    tracker.track(&fx.session, std::slice::from_ref(&root));
    fx.ledgers.set_external_tracker(Arc::clone(&tracker));

    assert_eq!(
        tracker.observe(&root.join("a.txt")),
        Ownership::External,
        "with no bracket open, a write is the user's"
    );

    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    assert_eq!(
        tracker.observe(&root.join("a.txt")),
        Ownership::Bracketed,
        "a write inside a bracket is the ledger's to attribute"
    );

    // Deliberately NOT asserting that dropping the handle re-arms detection
    // immediately: `SUPPRESSION_LINGER` keeps the window open past the last
    // bracket so a formatter or LSP writing just after the call returns is
    // still the ledger's. Releasing is the tracker's contract, tested there.
    drop(handle);
}

/// The re-baseline the permission prompt needs: a bracket whose baseline has
/// been moved forward disowns everything written before the move.
#[tokio::test]
async fn a_rebased_bracket_disowns_what_preceded_the_rebase() {
    let fx = Fixture::new("one\n").await;
    let mut handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();

    // The human, while the permission prompt is on screen.
    std::fs::write(fx.dir.path().join("b.txt"), "typed by hand\n").unwrap();
    // They answer it; the interval starts here instead.
    fx.ledgers.rebase(&mut handle).await;
    fx.write("written by the tool\n");
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    let b = hunks.iter().find(|h| h.path.contains("b.txt")).unwrap();
    assert!(
        b.is_external(),
        "an edit before the re-baseline was attributed to tool call {:?}",
        b.tool_call_ids
    );
    let a = hunks.iter().find(|h| h.path.contains("a.txt")).unwrap();
    assert_eq!(a.tool_call_ids, vec!["call-1".to_string()]);
}

/// Re-baselining must not disarm the overlap registry: the bracket is still
/// open, so a later one on the same root is still contested, and the handle's
/// `Drop` must still deregister it.
#[tokio::test]
async fn a_rebased_bracket_stays_registered() {
    let fx = Fixture::new("one\n").await;
    let mut first = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.ledgers.rebase(&mut first).await;

    let second = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.write("two\n");
    fx.ledgers
        .close(&fx.session, second, "second", 1)
        .await
        .unwrap();
    drop(first);

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    assert!(
        ledger.intervals()[0].contested,
        "a re-baselined bracket is still an open writer on the root"
    );

    fx.call("later", 2, "three\n").await;
    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    let later = ledger
        .intervals()
        .iter()
        .find(|i| i.tool_call_id == "later")
        .unwrap();
    assert!(
        !later.contested,
        "the re-baselined bracket's Drop never deregistered its root"
    );
}

#[tokio::test]
async fn an_edit_no_bracket_saw_is_external() {
    let fx = Fixture::new("one\n").await;
    // The user, or a formatter landing after Bash returned.
    fx.write("edited by hand\n");

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0].is_external());
    assert!(
        fx.ledgers
            .unreviewed_hunks(&fx.session)
            .await
            .unwrap()
            .is_empty(),
        "external hunks must never block the agent"
    );
}

#[tokio::test]
async fn external_hunks_are_not_revertible() {
    let fx = Fixture::new("one\n").await;
    fx.write("edited by hand\n");
    let id = fx.hunks().await[0].id.clone();

    let err = fx.ledgers.revert_hunk(&fx.session, &id).await.unwrap_err();
    assert!(matches!(err, ReviewError::ExternalHunk(_)), "{err:?}");
    assert_eq!(
        fx.read(),
        "edited by hand\n",
        "the user's edit was destroyed"
    );
}

/// Concurrent delegated sessions share a workspace by default, so overlapping
/// brackets are the normal path, not an edge case.
#[tokio::test]
async fn overlapping_brackets_degrade_to_external_rather_than_guess() {
    let fx = Fixture::new("one\ntwo\nthree\n").await;

    let outer = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    let inner = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.write("ONE\ntwo\nthree\n");
    fx.ledgers
        .close(&fx.session, inner, "inner", 1)
        .await
        .unwrap();
    fx.write("ONE\ntwo\nTHREE\n");
    fx.ledgers
        .close(&fx.session, outer, "outer", 1)
        .await
        .unwrap();

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    assert!(
        ledger.intervals().iter().all(|i| i.contested),
        "both brackets overlapped and both must say so"
    );
    let hunks = fx.hunks().await;
    assert!(
        hunks.iter().all(ComposedHunk::is_external),
        "contested intervals must not produce confident attribution"
    );
}

#[tokio::test]
async fn a_later_bracket_after_an_overlap_is_not_contested() {
    let fx = Fixture::new("one\n").await;
    let a = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    let b = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.write("two\n");
    fx.ledgers.close(&fx.session, b, "b", 1).await.unwrap();
    fx.ledgers.close(&fx.session, a, "a", 1).await.unwrap();

    fx.call("clean", 2, "three\n").await;

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    let clean = ledger
        .intervals()
        .iter()
        .find(|i| i.tool_call_id == "clean")
        .unwrap();
    assert!(!clean.contested, "the overlap registry leaked");
}

/// A cancelled turn and an expired `execution_timeout_secs` both drop the turn
/// future mid-tool-call, so the bracket never reaches `close`. Before the
/// handle's `Drop` backstop that poisoned the root for the daemon's whole
/// lifetime: every later bracket on it, in every session, read as contested.
#[tokio::test]
async fn a_bracket_dropped_without_closing_does_not_poison_its_roots() {
    let fx = Fixture::new("one\n").await;

    drop(fx.ledgers.open_bracket(&fx.session).await.unwrap());

    fx.call("after-cancel", 1, "two\n").await;

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    let interval = &ledger.intervals()[0];
    assert!(
        !interval.contested,
        "a dropped bracket left its root registered"
    );
    assert!(
        fx.hunks().await.iter().all(|h| !h.is_external()),
        "attribution must survive a cancelled turn"
    );
}

#[tokio::test]
async fn reverting_restores_the_base_content_and_records_the_rejection() {
    let fx = Fixture::new("one\ntwo\nthree\n").await;
    fx.call("call-1", 1, "one\nEDITED\nthree\n").await;
    let id = find(&fx.hunks().await, "EDITED\n").id.clone();

    fx.ledgers.revert_hunk(&fx.session, &id).await.unwrap();

    assert_eq!(fx.read(), "one\ntwo\nthree\n");
    assert!(
        fx.hunks().await.is_empty(),
        "a reverted hunk is gone from the composed diff"
    );
}

/// Rejecting a hunk whose file has shrunk underneath must refuse and write
/// nothing.
///
/// The shrink is caught here by identity: `revert_hunk` re-lists first, so a
/// moved file yields a different `HunkId` and `UnknownHunk`. That leaves the
/// `start > lines.len()` clamp in `revert_hunk` as a backstop for the window
/// this test cannot reach — the worktree moving *between* that re-list and the
/// read, which an ungated `bash` call can do while a human is clicking Reject.
/// The clamp is not decoration: a deletion hunk has an empty `current_range`
/// and empty `after_content`, so the staleness guard passes (`slice` answers
/// `""` for any empty range), and `lines[..start]` would then index past the
/// end. The release profile aborts rather than unwinds, taking every live
/// session with it.
#[tokio::test]
async fn reverting_a_hunk_whose_file_shrank_refuses_and_writes_nothing() {
    let fx = Fixture::new("one\ntwo\nthree\nfour\nfive\n").await;
    fx.call("call-1", 1, "one\ntwo\n").await;
    let id = find(&fx.hunks().await, "").id.clone();

    fx.write("one\n");

    let err = fx
        .ledgers
        .revert_hunk(&fx.session, &id)
        .await
        .expect_err("a shrunk file must refuse");
    assert!(
        matches!(err, ReviewError::UnknownHunk(_) | ReviewError::Stale { .. }),
        "{err:?}"
    );
    assert_eq!(fx.read(), "one\n", "a refused revert writes nothing");
}

/// The counterpart, because the fix is one `?` away from breaking it: reverting
/// the agent's deletion of an entire file depends on the read failing and being
/// read as empty, so `NotFound` must stay distinct from a real read error.
#[tokio::test]
async fn reverting_a_whole_file_deletion_recreates_the_file() {
    let fx = Fixture::new("one\ntwo\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    std::fs::remove_file(fx.dir.path().join("a.txt")).unwrap();
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    let id = hunks.first().expect("a deletion hunk").id.clone();
    fx.ledgers.revert_hunk(&fx.session, &id).await.unwrap();

    assert_eq!(fx.read(), "one\ntwo\n");
}

#[tokio::test]
async fn reverting_one_hunk_leaves_the_others_alone() {
    let fx = Fixture::new("one\ntwo\nthree\nfour\nfive\n").await;
    fx.call("call-1", 1, "ONE\ntwo\nthree\nfour\nFIVE\n").await;
    let hunks = fx.hunks().await;
    let first = find(&hunks, "ONE\n").id.clone();

    fx.ledgers.revert_hunk(&fx.session, &first).await.unwrap();

    assert_eq!(fx.read(), "one\ntwo\nthree\nfour\nFIVE\n");
    let remaining = fx.hunks().await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].after_content, "FIVE\n");
}

#[tokio::test]
async fn rejecting_reverts_rather_than_only_recording() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();

    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();

    assert_eq!(
        fx.read(),
        "one\n",
        "a recorded rejection that did not revert"
    );
}

#[tokio::test]
async fn the_gate_query_is_scoped_to_the_file() {
    let fx = Fixture::new("one\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    fx.write("EDITED\n");
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();
    assert_eq!(
        fx.ledgers
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Unreviewed
    );
    assert_eq!(
        fx.ledgers
            .has_unreviewed_in_file(&fx.session, &only(root.join("elsewhere.txt")))
            .await
            .unwrap(),
        Verdict::Clear
    );
}

#[tokio::test]
async fn session_base_is_captured_once_and_never_moves() {
    let fx = Fixture::new("one\n").await;
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .base_tree
        .clone();

    fx.call("call-1", 1, "two\n").await;
    // Re-opening must not re-derive the base — that would silently empty the
    // composed diff of everything done so far.
    fx.ledgers
        .open(&fx.session, &[fx.dir.path().to_path_buf()])
        .await
        .unwrap();

    assert_eq!(
        fx.ledgers.ledger(&fx.session).unwrap().session_base()[0].base_tree,
        base
    );
    assert_eq!(fx.hunks().await.len(), 1);
}

#[tokio::test]
async fn a_root_outside_git_is_not_trackable() {
    let dir = TempDir::new().unwrap();
    let ledgers = ReviewLedgers::default();
    let err = ledgers
        .open("sess", &[dir.path().to_path_buf()])
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::NoTrackableRoots(_)), "{err:?}");
    assert!(!ledgers.is_open("sess"));
}

#[tokio::test]
async fn roots_inside_one_repo_collapse_to_a_single_tracked_root() {
    let fx = Fixture::new("one\n").await;
    let nested = fx.dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();

    let ledgers = ReviewLedgers::default();
    ledgers
        .open("multi", &[fx.dir.path().to_path_buf(), nested])
        .await
        .unwrap();

    let base = ledgers.ledger("multi").unwrap();
    assert_eq!(base.session_base().len(), 1);
}

#[tokio::test]
async fn clearing_a_session_drops_its_ledger_and_state() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "two\n").await;

    fx.ledgers.clear_session(&fx.session);

    assert!(!fx.ledgers.is_open(&fx.session));
    let err = fx.ledgers.list_hunks(&fx.session).await.unwrap_err();
    assert!(matches!(err, ReviewError::NoLedger(_)), "{err:?}");
}

#[tokio::test]
async fn a_new_file_is_one_hunk_attributed_to_its_call() {
    let fx = Fixture::new("one\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    std::fs::write(fx.dir.path().join("new.txt"), "brand new\n").unwrap();
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].path, "new.txt");
    assert!(hunks[0].before_content.is_empty());
    assert_eq!(hunks[0].tool_call_ids, ["call-1"]);
}

#[tokio::test]
async fn a_deleted_file_is_one_hunk_attributed_to_its_call() {
    let fx = Fixture::new("one\ntwo\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    std::fs::remove_file(fx.dir.path().join("a.txt")).unwrap();
    fx.ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert!(hunks[0].after_content.is_empty());
    assert_eq!(hunks[0].before_content, "one\ntwo\n");
    assert_eq!(hunks[0].tool_call_ids, ["call-1"]);
}
