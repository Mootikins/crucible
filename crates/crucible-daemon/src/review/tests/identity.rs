//! Hunk identity: what separates two changes, and what must never renumber one
//! onto another's id.

use super::*;

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

    let ledgers = Arc::new(ReviewLedgers::for_tests(
        crate::test_support::scratch_snapshot_root(),
    ));
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
    let comment = record_comment(
        &fx.session,
        PhysicalRoot::from_top_level(fx.dir.path()),
        LineRange::new(1, 2),
        "why this?",
    );
    fx.ledgers.add_comment(&comment).unwrap();

    assert_eq!(fx.ledgers.comments(&fx.session).unwrap().len(), 1);
    fx.ledgers
        .resolve_comment(&fx.session, &comment.id)
        .unwrap();
    assert!(fx.ledgers.comments(&fx.session).unwrap()[0].resolved);

    let err = fx.ledgers.resolve_comment(&fx.session, "nope").unwrap_err();
    assert!(matches!(err, ReviewError::UnknownComment(_)), "{err:?}");
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

    let ledgers = Arc::new(ReviewLedgers::for_tests(
        crate::test_support::scratch_snapshot_root(),
    ));
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

/// A plain-store snapshot names nothing in a git object store. Handing one to
/// git plumbing is a routing fault in the daemon, so it answers an error the
/// caller can report — never a panic, which the release profile turns into an
/// abort that takes every live session with it.
#[tokio::test]
async fn a_plain_id_handed_to_git_is_an_error_not_a_panic() {
    let fx = Fixture::new("one\n").await;
    let root = PhysicalRoot::from_top_level(fx.dir.path());
    let plain = SnapshotId::plain("0".repeat(64));

    let err = super::git::blob(&root, &plain, "a.txt")
        .await
        .expect_err("git read a plain-store snapshot");
    assert!(
        matches!(err, ReviewError::WrongBackend { .. }),
        "{err:?}, not a backend error the caller can report"
    );

    let err = super::git::changed_paths(&root, &plain, &plain)
        .await
        .expect_err("git diffed a plain-store snapshot");
    assert!(matches!(err, ReviewError::WrongBackend { .. }), "{err:?}");

    // `tree_exists` answers a question, not a result: a snapshot from another
    // backend is not in this object store, which is exactly `false`.
    assert!(!super::git::tree_exists(&root, &plain).await);
}
