//! The session record: the files that differ between the session base and
//! the disk, read-only.

use super::*;
use crucible_core::diff::{DiffFileText, FileStatus, UnreadableRoot};

fn root_of(fx: &Fixture) -> PhysicalRoot {
    fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone()
}

#[tokio::test]
async fn a_session_record_lists_the_files_the_session_wrote() {
    let fx = Fixture::new("one\ntwo\n").await;
    fx.call("call-1", 1, "one\nTWO\nthree\n").await;
    // A file that the session base does not have.
    std::fs::write(fx.dir.path().join("new.txt"), "fresh\n").unwrap();

    let record = fx.ledgers.record_files(&fx.session).await.unwrap();
    assert!(record.unreadable_roots.is_empty());
    let files = record.files;

    let root = root_of(&fx);
    let listed: Vec<_> = files
        .iter()
        .map(|e| {
            assert_eq!(e.root, root);
            (
                e.path.as_str(),
                e.status.clone(),
                e.added,
                e.removed,
                e.binary,
                e.too_large,
            )
        })
        .collect();
    assert_eq!(
        listed,
        vec![
            ("a.txt", FileStatus::Modified, 2, 1, false, false),
            ("new.txt", FileStatus::Added, 1, 0, false, false),
        ]
    );
}

/// A root that the ledger cannot read has no files in the record. The record
/// must name the root and the reason, so that it does not look complete.
#[tokio::test]
async fn a_session_record_names_a_root_that_the_ledger_cannot_read() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "one\ntwo\n").await;
    let root = root_of(&fx);
    std::fs::remove_dir_all(fx.dir.path()).unwrap();

    let record = fx.ledgers.record_files(&fx.session).await.unwrap();

    assert!(record.files.is_empty(), "{:?}", record.files);
    assert_eq!(
        record.unreadable_roots,
        vec![UnreadableRoot {
            root,
            reason: "tracked root no longer exists".into(),
        }]
    );
}

#[tokio::test]
async fn a_session_record_file_has_the_snapshot_text_as_base() {
    let fx = Fixture::new("one\n").await;
    fx.call("call-1", 1, "one\ntwo\n").await;
    std::fs::write(fx.dir.path().join("new.txt"), "fresh\n").unwrap();
    let root = root_of(&fx);

    let text = fx
        .ledgers
        .record_text(&fx.session, &root, "a.txt")
        .await
        .unwrap();
    assert_eq!(
        text,
        DiffFileText {
            base_text: Some("one\n".into()),
            current_text: Some("one\ntwo\n".into()),
        }
    );

    let text = fx
        .ledgers
        .record_text(&fx.session, &root, "new.txt")
        .await
        .unwrap();
    assert_eq!(
        text,
        DiffFileText {
            base_text: None,
            current_text: Some("fresh\n".into()),
        }
    );

    // A root that the session does not track is refused.
    let other = TempDir::new().unwrap();
    let err = fx
        .ledgers
        .record_text(
            &fx.session,
            &PhysicalRoot::from_top_level(other.path()),
            "a.txt",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(err, ReviewError::PathEscapesRoot { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_session_with_no_ledger_has_an_empty_record() {
    let ledgers = ReviewLedgers::for_tests(crate::test_support::scratch_snapshot_root());
    let record = ledgers.record_files("nobody").await.unwrap();
    assert!(record.files.is_empty());
    assert!(record.unreadable_roots.is_empty());

    let err = ledgers
        .record_text(
            "nobody",
            &PhysicalRoot::from_top_level("/tmp/nowhere"),
            "a.txt",
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ReviewError::NoLedger(_)), "{err:?}");
}
