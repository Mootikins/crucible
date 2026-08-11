//! Hunk identity: what separates two changes, and what must never renumber one
//! onto another's recorded decision.

use super::*;

/// §3 anticipates the agent re-applying a rejected edit — that is the whole
/// reason a rejection is injected back into the conversation. The re-applied
/// change is new work that has never been reviewed at its new position, so it
/// must come back to the queue instead of inheriting the decision recorded
/// about the earlier, now-reverted change.
#[tokio::test]
async fn an_edit_reapplied_after_a_rejection_returns_to_the_queue() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "EDITED\n").await;
    let id = fx.hunks().await[0].id.clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();
    assert_eq!(fx.read(), "one\n");

    // The agent reads the rejection notice and tries the same edit again.
    fx.call("call-2", 2, "EDITED\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(
        hunks[0].state,
        ReviewState::Unreviewed,
        "a re-applied edit reported itself as already rejected while sitting on disk"
    );
    assert_eq!(
        fx.ledgers
            .unreviewed_hunks(&fx.session)
            .await
            .unwrap()
            .len(),
        1,
        "the gate stopped seeing a hunk that is live in the worktree"
    );
}

/// The ordinal disambiguator is positional among identical siblings, so
/// removing an earlier sibling renumbers the later one onto the earlier one's
/// identity. A decision made about the sibling that is gone must not transfer
/// to the one that survived — that is exactly the "stale accepted lands on a
/// different hunk" failure the content-derived identity exists to prevent.
#[tokio::test]
async fn removing_an_accepted_sibling_does_not_hand_its_decision_to_the_next() {
    let fx = Fixture::new("a\nkeep\na\n").await;
    fx.call("call-1", 1, "b\nkeep\nb\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 2);
    let first = hunks[0].id.clone();
    fx.ledgers
        .set_state(&fx.session, &first, ReviewState::Accepted)
        .await
        .unwrap();

    // Only the first occurrence goes back to base. The second was never
    // reviewed.
    fx.call("call-2", 2, "a\nkeep\nb\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].after_content, "b\n");
    assert_eq!(
        hunks[0].state,
        ReviewState::Unreviewed,
        "the surviving hunk inherited the accepted sibling's decision"
    );
}

/// `git rev-parse --show-toplevel` reports the physical path, but the gate
/// resolves a tool's relative target against `session.workspace` as the
/// session registered it. A workspace reached through a symlink therefore
/// produces two spellings of the same file and the per-file gate matches
/// neither.
#[tokio::test]
async fn the_gate_query_matches_a_root_reached_through_a_symlink() {
    let dir = TempDir::new().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    git(&real, &["init", "-q"]).await;
    git(&real, &["config", "user.email", "t@t"]).await;
    git(&real, &["config", "user.name", "t"]).await;
    std::fs::write(real.join("a.txt"), "one\n").unwrap();
    git(&real, &["add", "."]).await;
    git(&real, &["commit", "-q", "-m", "init"]).await;

    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("sym", std::slice::from_ref(&link))
        .await
        .unwrap();
    let handle = ledgers.open_bracket("sym").await.unwrap();
    std::fs::write(real.join("a.txt"), "EDITED\n").unwrap();
    ledgers.close("sym", handle, "call-1", 1).await.unwrap();

    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sym", &only(link.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Unreviewed,
        "the gate did not recognise its own file through the workspace's own path"
    );
}

/// `close` walks the tracked roots one `git write-tree` at a time, so the
/// turn's cancel arm and its execution timeout can drop it between two of
/// them. It used to take the whole handle up front, disarming the `Drop`
/// backstop for roots it had not touched yet: the remainder stayed registered
/// for the daemon's lifetime and every later bracket on them, in every
/// session, read as contested and degraded to `external`.
#[tokio::test]
async fn a_close_cancelled_between_roots_does_not_poison_them() {
    use std::future::Future;
    use std::task::{Context, Waker};

    let dir = TempDir::new().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    repo(&first, &[("a.txt", "one\n")]).await;
    repo(&second, &[("a.txt", "one\n")]).await;

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("cancelled", &[first.clone(), second.clone()])
        .await
        .unwrap();

    let handle = ledgers.open_bracket("cancelled").await.unwrap();
    std::fs::write(first.join("a.txt"), "EDITED\n").unwrap();
    std::fs::write(second.join("a.txt"), "EDITED\n").unwrap();

    // One poll parks the close inside the capture of the root it took first,
    // with the other still to go; dropping there is exactly what the cancel
    // arm does. A noop waker makes the cut point deterministic — a nanosecond
    // timeout would race the git subprocess.
    {
        let mut close = std::pin::pin!(ledgers.close("cancelled", handle, "call-1", 1));
        assert!(
            close
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending(),
            "close never yielded, so the drop did not land mid-loop"
        );
    }

    assert!(
        ledgers.open.iter().all(|entry| entry.value().is_empty()),
        "a cancelled close left roots registered: {:?}",
        ledgers
            .open
            .iter()
            .map(|e| (e.key().clone(), e.value().len()))
            .collect::<Vec<_>>()
    );

    // The observable consequence: the next bracket over both roots is clean,
    // so its writes stay attributed instead of degrading to external.
    let handle = ledgers.open_bracket("cancelled").await.unwrap();
    std::fs::write(first.join("a.txt"), "AGAIN\n").unwrap();
    std::fs::write(second.join("a.txt"), "AGAIN\n").unwrap();
    ledgers
        .close("cancelled", handle, "call-2", 2)
        .await
        .unwrap();

    let ledger = ledgers.ledger("cancelled").unwrap();
    assert_eq!(ledger.intervals().len(), 1);
    assert!(
        !ledger.intervals()[0].contested,
        "the cancelled close poisoned the roots it never reached"
    );
    let hunks = ledgers.list_hunks("cancelled").await.unwrap();
    assert_eq!(hunks.len(), 2, "{hunks:#?}");
    assert!(
        hunks.iter().all(|h| h.tool_call_ids == ["call-2"]),
        "attribution was lost on a root the cancelled close never reached"
    );
}

#[tokio::test]
async fn comments_are_stored_and_resolvable() {
    let fx = Fixture::new("one\n").await;
    let comment = Comment::new(
        PhysicalRoot::from_top_level(fx.dir.path()),
        "a.txt",
        TreeSha::new("deadbeef"),
        LineRange::new(1, 2),
        "why this?",
        CommentAuthor::Human,
    );
    fx.ledgers.add_comment(&fx.session, comment.clone()).await;

    assert_eq!(fx.ledgers.comments(&fx.session).len(), 1);
    fx.ledgers
        .resolve_comment(&fx.session, &comment.id)
        .await
        .unwrap();
    assert!(fx.ledgers.comments(&fx.session)[0].resolved);

    let err = fx
        .ledgers
        .resolve_comment(&fx.session, "nope")
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::UnknownComment(_)), "{err:?}");
}

/// The other half of
/// `removing_an_accepted_sibling_does_not_hand_its_decision_to_the_next`: the
/// fix must not degrade to "any change anywhere loses every decision". The
/// survivor's `session_base` coordinates are untouched by a sibling
/// disappearing above it, so its identity — and the decision made about it —
/// survives. Under the ordinal scheme the survivor was renumbered and lost it.
#[tokio::test]
async fn an_accepted_hunk_keeps_its_decision_when_an_identical_sibling_above_it_is_removed() {
    let fx = Fixture::new("a\nkeep\na\n").await;
    fx.call("call-1", 1, "b\nkeep\nb\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 2, "{hunks:#?}");
    let second = hunks[1].id.clone();
    assert_eq!(hunks[1].base_range, LineRange::new(3, 4));
    fx.ledgers
        .set_state(&fx.session, &second, ReviewState::Accepted)
        .await
        .unwrap();

    // The first occurrence goes back to base; the reviewed one stays.
    fx.call("call-2", 2, "a\nkeep\nb\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].id, second, "the survivor's identity moved");
    assert_eq!(
        hunks[0].state,
        ReviewState::Accepted,
        "a decision was lost to an edit that never touched the hunk"
    );
}

/// Distinct identities are only worth having if they carry distinct
/// decisions: the workspace and the kiln are reviewed separately even when
/// the change is byte-identical in both.
#[tokio::test]
async fn identical_changes_in_two_roots_are_independently_reviewable() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path().join("workspace");
    let kiln = dir.path().join("kiln");
    repo(&workspace, &[("a.txt", "one\n")]).await;
    repo(&kiln, &[("a.txt", "one\n")]).await;

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("two-roots", &[workspace.clone(), kiln.clone()])
        .await
        .unwrap();
    let handle = ledgers.open_bracket("two-roots").await.unwrap();
    std::fs::write(workspace.join("a.txt"), "EDITED\n").unwrap();
    std::fs::write(kiln.join("a.txt"), "EDITED\n").unwrap();
    ledgers
        .close("two-roots", handle, "call-1", 1)
        .await
        .unwrap();

    let in_workspace = ledgers
        .list_hunks("two-roots")
        .await
        .unwrap()
        .into_iter()
        .find(|h| *h.root == *workspace)
        .expect("a hunk in the workspace")
        .id;
    ledgers
        .set_state("two-roots", &in_workspace, ReviewState::Accepted)
        .await
        .unwrap();

    let hunks = ledgers.list_hunks("two-roots").await.unwrap();
    let kiln_hunk = hunks
        .iter()
        .find(|h| *h.root == *kiln)
        .expect("a kiln hunk");
    assert_eq!(
        kiln_hunk.state,
        ReviewState::Unreviewed,
        "accepting the workspace's hunk silently accepted the kiln's"
    );
    assert_eq!(
        ledgers.unreviewed_hunks("two-roots").await.unwrap().len(),
        1,
        "the gate lost sight of the kiln's unreviewed change"
    );
}

/// Pure insertions are the hardest case for a content-derived identity: both
/// carry empty base text and an empty base range, so the range's *position*
/// is the only thing distinguishing them.
#[tokio::test]
async fn identical_insertions_in_one_file_get_distinct_identities() {
    let fx = Fixture::new("one\ntwo\nthree\n").await;
    fx.call("call-1", 1, "one\nNEW\ntwo\nNEW\nthree\n").await;

    let hunks = fx.hunks().await;
    assert_eq!(hunks.len(), 2, "{hunks:#?}");
    assert!(hunks.iter().all(|h| h.before_content.is_empty()));
    assert!(hunks.iter().all(|h| h.after_content == "NEW\n"));
    assert_ne!(hunks[0].id, hunks[1].id, "identical insertions collided");
}

#[tokio::test]
async fn identical_hunks_in_two_files_get_distinct_identities() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("repo");
    repo(&root, &[("a.txt", "one\n"), ("b.txt", "one\n")]).await;

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open("two-files", std::slice::from_ref(&root))
        .await
        .unwrap();
    let handle = ledgers.open_bracket("two-files").await.unwrap();
    std::fs::write(root.join("a.txt"), "EDITED\n").unwrap();
    std::fs::write(root.join("b.txt"), "EDITED\n").unwrap();
    ledgers
        .close("two-files", handle, "call-1", 1)
        .await
        .unwrap();

    let hunks = ledgers.list_hunks("two-files").await.unwrap();
    assert_eq!(hunks.len(), 2, "{hunks:#?}");
    assert_ne!(
        hunks[0].id, hunks[1].id,
        "the same change in two files collided on one identity"
    );
}

/// A binary file has no line hunks. Fabricating an empty one would be
/// indistinguishable from a no-op change and would put an unrevertible entry
/// in the queue — so the interval is still recorded and the composition skips
/// the path.
#[tokio::test]
async fn a_binary_file_is_skipped_rather_than_composed_as_an_empty_hunk() {
    let fx = Fixture::new("one\n").await;
    let handle = fx.ledgers.open_bracket(&fx.session).await.unwrap();
    std::fs::write(fx.dir.path().join("bin.dat"), [0xff, 0xfe, 0x00, 0x01]).unwrap();
    let recorded = fx
        .ledgers
        .close(&fx.session, handle, "call-1", 1)
        .await
        .unwrap();

    assert!(recorded, "the write itself must still be bracketed");
    assert!(
        fx.hunks().await.is_empty(),
        "a binary file produced a line hunk"
    );
}
