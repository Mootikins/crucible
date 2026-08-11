//! The per-write gate: which queries match a live hunk, and the block a parked
//! turn publishes while it waits for a human.

use super::*;

/// The two axes the old code conflated, separated: `state` is the user's
/// decision and `reapplied` is a fact about history. The re-applied change is
/// unreviewed *and* visibly a repeat, which is what lets a panel show the
/// grind rather than letting the user accept out of fatigue.
#[tokio::test]
async fn a_reapplied_hunk_is_flagged_as_reapplied() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();
    assert!(
        !fx.hunks().await[0].reapplied,
        "a hunk nobody has decided about reported itself as a repeat"
    );

    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();
    fx.call("call-2", 2, "EDITED\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        hunks[0].id, id,
        "the same change derived a different identity"
    );
    assert!(
        hunks[0].reapplied,
        "the re-applied change looked like first-time work"
    );
}

/// `reapplied` must key on a *rejection*, which is the only decision that
/// reverts. An accepted hunk sits in the worktree by design and is not a
/// repeat of anything.
#[tokio::test]
async fn an_accepted_hunk_is_not_flagged_as_reapplied() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Accepted)
        .await
        .unwrap();

    let hunks = fx.hunks().await;
    assert_eq!(hunks[0].state, ReviewState::Accepted);
    assert!(!hunks[0].reapplied);
}

/// `unreviewed_hunks` -> `list_hunks` is the gate poller's half-second `&self`
/// read loop, so pruning the states map there would make a hot read path
/// irreversibly delete the user's answers — and the composed diff genuinely
/// shrinks and regrows under the user's own `git stash` or `checkout`. Listing
/// repeatedly must therefore change nothing.
#[tokio::test]
async fn listing_the_queue_repeatedly_never_forgets_a_rejection() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();

    // The gate polls while the queue is empty. Nothing here may consume the
    // recorded decision.
    for _ in 0..5 {
        assert!(fx.hunks().await.is_empty());
        assert!(fx
            .ledgers
            .unreviewed_hunks(&fx.session)
            .await
            .unwrap()
            .is_empty());
    }

    fx.call("call-2", 2, "EDITED\n").await;
    assert!(
        fx.hunks().await[0].reapplied,
        "polling the empty queue forgot that this change had been rejected"
    );
}

/// The gate blocks a *re-applied* change rather than waving it through. The
/// cost of re-applying is a block, never a bypass.
#[tokio::test]
async fn a_reapplied_hunk_still_blocks_the_gate() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();
    fx.call("call-2", 2, "EDITED\n").await;

    assert_eq!(
        fx.ledgers
            .has_unreviewed_in_file(&fx.session, &only(fx.dir.path().join("a.txt")))
            .await
            .unwrap(),
        Verdict::Unreviewed,
        "the gate stopped blocking on a change that is live in the worktree"
    );
}

/// The file the gate is asked about need not exist: a deletion hunk describes
/// a path that is gone, and a `write_file` names one not created yet.
/// Canonicalising the query whole answers `Err` for both, so the query has to
/// resolve through its deepest existing ancestor instead.
#[tokio::test]
async fn the_gate_query_matches_a_deleted_file_named_through_a_symlink() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    repo(&real, &[("a.txt", "one\n"), ("b.txt", "one\n")]).await;
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("sym", std::slice::from_ref(&link))
        .await
        .unwrap();
    let handle = ledgers.open_bracket("sym").await.unwrap();
    std::fs::remove_file(real.join("a.txt")).unwrap();
    ledgers.close("sym", handle, "call-1", 1).await.unwrap();

    assert!(
        !link.join("a.txt").exists(),
        "the deleted path must not exist, or this proves nothing"
    );
    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sym", &only(link.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Unreviewed,
        "the gate did not recognise a file it had just deleted"
    );
    // A real "no", not a spelling mismatch dressed up as one.
    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sym", &only(link.join("b.txt")))
            .await
            .unwrap(),
        Verdict::Clear
    );
}

/// Normalisation is query-side only. `RootBase.root` is the first field hashed
/// into every `HunkId`, so rewriting it at ingest would silently invalidate
/// every decision across a resume — the identity must stay pinned to the
/// physical spelling git printed even when the session was opened through a
/// link.
#[tokio::test]
async fn opening_through_a_symlink_still_stores_the_physical_root() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    repo(&real, &[("a.txt", "one\n")]).await;
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("sym", std::slice::from_ref(&link))
        .await
        .unwrap();

    let physical = std::fs::canonicalize(&real).unwrap();
    let roots: Vec<PathBuf> = ledgers
        .ledger("sym")
        .unwrap()
        .roots()
        .map(Path::to_path_buf)
        .collect();
    assert_eq!(roots, vec![physical]);
}

// ── Gate state ─────────────────────────────────────────────────────────────

fn block(path: &str) -> GateBlock {
    GateBlock {
        tool: "edit_file".to_string(),
        path: path.to_string(),
    }
}

#[tokio::test]
async fn a_session_with_no_turn_parked_reports_no_gate_block() {
    let ledgers = Arc::new(ReviewLedgers::default());
    assert!(ledgers.gate_block("sess").is_none());
}

#[tokio::test]
async fn a_held_gate_is_visible_for_as_long_as_the_hold_lives() {
    let ledgers = Arc::new(ReviewLedgers::default());
    let hold = ledgers.hold_gate("sess", block("a.txt"));
    assert_eq!(ledgers.gate_block("sess"), Some(block("a.txt")));
    drop(hold);
    assert!(ledgers.gate_block("sess").is_none());
}

/// The failure the guard exists for, in its real shape: the hold lives inside
/// a future parked on a yield point — exactly where `hold_for_review` sits on
/// its poll sleep — and the turn's cancel arm or execution timeout drops the
/// future there rather than letting it reach the release. Without `Drop` the
/// block outlives the turn, and the session shows a chip claiming to wait on a
/// review that nothing can answer.
#[tokio::test]
async fn a_dropped_hold_clears_the_gate_slot() {
    use std::future::Future;
    use std::task::{Context, Waker};

    let ledgers = Arc::new(ReviewLedgers::default());
    let parked = ledgers.clone();
    let mut turn = Box::pin(async move {
        let _hold = parked.hold_gate("sess", block("a.txt"));
        // Stands in for `sleep(POLL_INTERVAL)`: a yield point the future never
        // gets past.
        std::future::pending::<()>().await;
    });

    assert!(
        turn.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "the turn never parked, so the drop would not land while blocked"
    );
    assert_eq!(ledgers.gate_block("sess"), Some(block("a.txt")));

    drop(turn);
    assert!(
        ledgers.gate_block("sess").is_none(),
        "a cancelled turn left the session claiming to wait on a review"
    );
}

/// Two sessions park independently; releasing one must not clear the other.
#[tokio::test]
async fn a_release_only_clears_its_own_session() {
    let ledgers = Arc::new(ReviewLedgers::default());
    let one = ledgers.hold_gate("one", block("a.txt"));
    let _two = ledgers.hold_gate("two", block("b.txt"));

    drop(one);
    assert!(ledgers.gate_block("one").is_none());
    assert_eq!(ledgers.gate_block("two"), Some(block("b.txt")));
}

/// Session teardown races the turn's own unwind, so the block has to be
/// dropped from both ends: an id that is later reissued must not inherit a
/// block from the session that used it before.
#[tokio::test]
async fn clearing_a_session_drops_its_gate_block() {
    let ledgers = Arc::new(ReviewLedgers::default());
    let _hold = ledgers.hold_gate("sess", block("a.txt"));
    ledgers.clear_session("sess");
    assert!(ledgers.gate_block("sess").is_none());
}

/// A `create_note` names a path relative to the **kiln**, and a kiln that is
/// its own repository is a tracked root of its own. Resolving every relative
/// target against `session.workspace` produced a path in the wrong repository,
/// which matched neither the hunk's absolute path nor its root-relative one —
/// so every note the agent wrote went through the gate unheld.
#[tokio::test]
async fn a_kiln_relative_target_matches_a_hunk_in_the_kilns_own_repo() {
    let workspace = TempDir::new().unwrap();
    repo(workspace.path(), &[("a.txt", "one\n")]).await;
    let kiln = TempDir::new().unwrap();
    repo(kiln.path(), &[("Notes/x.md", "one\n")]).await;

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open(
            "sess",
            &[workspace.path().to_path_buf(), kiln.path().to_path_buf()],
        )
        .await
        .unwrap();
    let handle = ledgers.open_bracket("sess").await.unwrap();
    std::fs::write(kiln.path().join("Notes/x.md"), "BY THE AGENT\n").unwrap();
    ledgers.close("sess", handle, "call-1", 1).await.unwrap();

    let workspace_join = workspace.path().join("Notes/x.md");
    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sess", &only(workspace_join.clone()))
            .await
            .unwrap(),
        Verdict::Clear,
        "the workspace-joined spelling matched, so this test proves nothing"
    );
    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sess", &[workspace_join, PathBuf::from("Notes/x.md")],)
            .await
            .unwrap(),
        Verdict::Unreviewed,
        "an unreviewed note let the next write to it through"
    );
}
