//! Journal persistence: what survives a daemon restart, and what a journal that
//! cannot be read back is allowed to conclude.
//!
//! Defect 13 was that `session_base` never survived a daemon restart, so
//! resuming a session re-derived the base from the current worktree and the
//! composed diff came back empty — reported not as an error but as "the agent
//! changed nothing". Defect 14 was that the trees the ledger names are
//! `write-tree` output reachable from no ref, so `git gc --prune=now` deletes
//! them and the queue becomes uncomputable rather than merely stale.
//!
//! No out-of-process restart harness exists (`AgentFactoryOverride` is
//! in-process only), so a full turn → kill → restart test cannot be written
//! today. The substitute used throughout here is a second `ReviewLedgers`
//! built over the same journal, which exercises exactly the write and read
//! halves a restart would.

use super::*;

/// The defect, stated as the property: restart, and the queue is still there.
///
/// Checks the base and the attribution together, because each alone can be
/// preserved while the review surface is still wrong — a base that survives
/// without its intervals makes every hunk external.
#[tokio::test]
async fn a_restart_keeps_the_session_base_and_the_attribution() {
    let fx = Persisted::new("one\ntwo\n").await;
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .base_tree
        .clone();
    fx.call("call-1", "EDITED\ntwo\n").await;

    let before = fx.ledgers.list_hunks(&fx.session).await.unwrap();
    assert_eq!(before.len(), 1);

    let restarted = fx.restart().await;
    assert_eq!(
        restarted.ledger(&fx.session).unwrap().session_base()[0].base_tree,
        base,
        "the base was re-derived instead of restored; the composed diff is now empty"
    );

    let after = restarted.list_hunks(&fx.session).await.unwrap();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].id, before[0].id);
    assert_eq!(
        after[0].tool_call_ids,
        vec!["call-1".to_string()],
        "the interval was lost, so a hunk the agent made reads as the user's own edit"
    );
}

/// A comment that evaporates on restart is a review someone wrote and nobody
/// will read. The comment store keeps it, not the journal.
#[tokio::test]
async fn a_restart_keeps_comments_and_their_resolution() {
    let fx = Persisted::new("one\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();
    let open = record_comment(&fx.session, root.clone(), LineRange::new(1, 2), "why this?");
    let answered = record_comment(&fx.session, root, LineRange::new(1, 2), "and this?");
    fx.ledgers.add_comment(&open).unwrap();
    fx.ledgers.add_comment(&answered).unwrap();
    fx.ledgers
        .resolve_comment(&fx.session, &answered.id)
        .unwrap();

    let restarted = fx.restart().await;
    let comments = restarted.comments(&fx.session).unwrap();
    assert_eq!(comments.len(), 2);
    assert!(!comments.iter().find(|c| c.id == open.id).unwrap().resolved);
    assert!(
        comments
            .iter()
            .find(|c| c.id == answered.id)
            .unwrap()
            .resolved,
        "the resolution did not survive, so an answered comment came back open"
    );
}

/// Write one comment and its resolution to the journal in the shape of a
/// daemon before the comment store.
async fn write_old_comment(fx: &Persisted, id: &str, range: LineRange, resolved: bool) {
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0].clone();
    let line = serde_json::json!({
        "t": "comment",
        "id": id,
        "root": base.root,
        "path": "a.txt",
        "base_tree": base.base_tree,
        "line_range": { "start": range.start, "end": range.end },
        "body": "old review",
        "author": "human",
        "resolved": false,
        "created_at": "2026-01-01T00:00:00Z",
    });
    let mut text = std::fs::read_to_string(fx.journal()).unwrap();
    text.push_str(&format!("{line}\n"));
    if resolved {
        text.push_str(&format!(
            "{}\n",
            serde_json::json!({ "t": "comment_resolved", "comment": id })
        ));
    }
    std::fs::write(fx.journal(), text).unwrap();
}

/// A journal from a daemon before the comment store still reads. Its
/// comment keeps its anchor, and its resolution still applies.
#[tokio::test]
async fn an_old_journal_comment_still_reads() {
    let fx = Persisted::new("one\ntwo\n").await;
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0].clone();
    write_old_comment(&fx, "old-open", LineRange::new(1, 2), false).await;
    write_old_comment(&fx, "old-resolved", LineRange::new(2, 3), true).await;

    let restarted = fx.restart().await;
    assert!(
        restarted.integrity(&fx.session).is_intact(),
        "an old comment record was skipped: {:?}",
        restarted.integrity(&fx.session)
    );
    let comments = restarted.comments(&fx.session).unwrap();
    assert_eq!(comments.len(), 2, "{comments:#?}");
    let open = comments.iter().find(|c| c.id == "old-open").unwrap();
    assert_eq!(open.anchor, CommentAnchor::Snapshot(base.base_tree));
    assert!(!open.resolved);
    assert!(
        comments
            .iter()
            .find(|c| c.id == "old-resolved")
            .unwrap()
            .resolved,
        "the old resolution did not apply"
    );
}

/// The old comment moves to the session record diffset of its session. The
/// file on disk gives its quoted text.
#[tokio::test]
async fn a_session_comment_migrates_to_its_session_record() {
    let fx = Persisted::new("one\ntwo\nthree\n").await;
    write_old_comment(&fx, "old", LineRange::new(2, 4), false).await;

    let restarted = fx.restart().await;
    let record = record_diffset(&fx.session).unwrap();
    assert_eq!(record.as_str(), "session-sess");
    let stored = restarted.comment_store().list(&record).unwrap();
    assert_eq!(stored.len(), 1, "the comment is not keyed by its diffset");
    let comment = &stored[0];
    assert_eq!(comment.diffset, record);
    assert_eq!(comment.side, CommentSide::Current);
    assert_eq!(comment.line_range, LineRange::new(2, 4));
    assert_eq!(comment.quoted, "two\nthree\n");
    assert!(restarted.comment_store().is_migrated(&record).unwrap());
}

/// The migration copies a journal one time. A later restore does not copy
/// the comment again, and does not open a comment that the store resolved.
#[tokio::test]
async fn the_migration_runs_once() {
    let fx = Persisted::new("one\n").await;
    write_old_comment(&fx, "old", LineRange::new(1, 2), false).await;

    let first = fx.restart().await;
    first.resolve_comment(&fx.session, "old").unwrap();

    let second = fx.restart().await;
    let comments = second.comments(&fx.session).unwrap();
    assert_eq!(comments.len(), 1, "the migration ran again: {comments:#?}");
    assert!(
        comments[0].resolved,
        "a second migration opened a comment that the store resolved"
    );
}

/// The trap defect 13 sets for its own fix: a journal that *exists* and cannot
/// be read must never fall through to capturing a fresh base. A fresh base
/// reports that the agent changed nothing, which is the same data loss with a
/// success code on it.
#[tokio::test]
async fn an_unreadable_journal_never_captures_a_fresh_base() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;

    // A directory where the journal should be: present to `try_exists`,
    // unreadable to `read_to_string`. Stands in for a permissions failure
    // without depending on the test not running as root.
    std::fs::remove_file(fx.journal()).unwrap();
    std::fs::create_dir(fx.journal()).unwrap();

    let ledgers = Arc::new(ReviewLedgers::for_tests(
        crate::test_support::scratch_snapshot_root(),
    ));
    let err = ledgers
        .open_or_restore(
            &fx.session,
            fx.session_dir.path(),
            &[fx.repo_dir.path().to_path_buf()],
        )
        .await
        .expect_err("an unreadable journal must not resolve to a fresh base");
    assert!(matches!(err, ReviewError::Journal { .. }), "{err:?}");
    assert!(
        ledgers
            .ledger(&fx.session)
            .expect("the session must still be present to every reader")
            .session_base()
            .is_empty(),
        "a base was captured anyway, so the next capture becomes the new base"
    );
}

/// The other half of the same failure: leaving the session with *no ledger*
/// is the same signal a workspace outside git produces. A journal whose header
/// line is merely corrupt already degrades every root; losing the whole file
/// is strictly more broken and must degrade at least as much.
#[tokio::test]
async fn an_unreadable_journal_degrades_every_root() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;

    std::fs::remove_file(fx.journal()).unwrap();
    std::fs::create_dir(fx.journal()).unwrap();

    let ledgers = Arc::new(ReviewLedgers::for_tests(
        crate::test_support::scratch_snapshot_root(),
    ));
    let _ = ledgers
        .open_or_restore(
            &fx.session,
            fx.session_dir.path(),
            &[fx.repo_dir.path().to_path_buf()],
        )
        .await;

    assert!(
        ledgers.integrity(&fx.session).blocks_everything(),
        "an unreadable journal recorded no unscoped loss"
    );
}

/// Degradation is graded because losing attribution fails *open*: a hunk no
/// interval accounts for is external, and `unreviewed_hunks` excludes external
/// hunks. A skipped interval therefore has to degrade its root.
#[tokio::test]
async fn a_skipped_interval_degrades_its_own_root() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();

    // Truncate the interval line's body while leaving its tag and first root
    // legible — the shape a partial write leaves behind.
    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let interval = lines
        .iter_mut()
        .find(|l| l.contains("\"interval\""))
        .expect("an interval line");
    *interval = format!(
        r#"{{"t":"interval","roots_touched":[{{"root":{}}}]"#,
        serde_json::to_string(&root).unwrap()
    );
    std::fs::write(fx.journal(), lines.join("\n") + "\n").unwrap();

    let restarted = fx.restart().await;
    let skips = restarted.integrity(&fx.session);
    assert_eq!(skips.skips().len(), 1, "{:?}", skips.skips());
    assert_eq!(
        skips.skips()[0].record,
        SkipKind::Root { root: root.clone() },
        "an interval whose root is legible must not escalate to the whole session"
    );
    let (_, statuses) = restarted
        .list_hunks_with_status(&fx.session, ReviewScope::Session, None)
        .await
        .unwrap();
    assert!(
        statuses.iter().any(|s| s.root == root && s.is_degraded()),
        "{statuses:?}"
    );
    // ...and a path under no tracked root is not degraded.
    assert!(!skips.blocks(Path::new("/elsewhere")));
}

/// An interval naming several roots cannot be scoped to one of them.
///
/// Scoping to the *first* legible root degraded that root and left the others
/// intact — and a root whose intervals were lost reports external hunks, which
/// `unreviewed_hunks` excludes, so the listing went quiet on exactly the root
/// whose evidence was destroyed.
#[tokio::test]
async fn a_truncated_interval_naming_two_roots_blocks_the_whole_session() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();

    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let interval = lines
        .iter_mut()
        .find(|l| l.contains("\"interval\""))
        .expect("an interval line");
    *interval = format!(
        r#"{{"t":"interval","roots_touched":[{{"root":{}}},{{"root":{}}}]"#,
        serde_json::to_string(&root).unwrap(),
        serde_json::to_string("/some/other/root").unwrap(),
    );
    std::fs::write(fx.journal(), lines.join("\n") + "\n").unwrap();

    let restarted = fx.restart().await;
    let skips = restarted.integrity(&fx.session);
    assert_eq!(
        skips.skips()[0].record,
        SkipKind::Session,
        "a loss that cannot be scoped must block everything, not its first root"
    );
    assert!(skips.blocks(&root));
}

/// A rebase names one root, so it can only vouch for that one. Clearing every
/// skip on replay let a *partial* rebase — one root recaptured, another still
/// degraded — come back from a restart reporting both intact.
#[tokio::test]
async fn replaying_a_rebase_keeps_another_roots_block() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();

    // A loss scoped to a root nobody is about to rebase...
    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let other = "/some/other/root";
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let interval = lines
        .iter_mut()
        .find(|l| l.contains("\"interval\""))
        .expect("an interval line");
    *interval = format!(
        r#"{{"t":"interval","roots_touched":[{{"root":{}}}]"#,
        serde_json::to_string(other).unwrap()
    );
    // ...followed by a rebase of a different root.
    lines.push(format!(
        r#"{{"t":"rebase","root":{},"base_tree":"{}"}}"#,
        serde_json::to_string(&root).unwrap(),
        "0".repeat(40)
    ));
    std::fs::write(fx.journal(), lines.join("\n") + "\n").unwrap();

    let restarted = fx.restart().await;
    let skips = restarted.integrity(&fx.session);
    assert_eq!(
        skips.skips().len(),
        1,
        "the other root's block was cleared by a rebase that never mentioned it: {:?}",
        skips.skips()
    );
    assert!(skips.blocks(Path::new(other)));
}

/// The other half of the grading: a lost comment costs no safety property, and
/// it degrades no root.
#[tokio::test]
async fn a_skipped_comment_blocks_nothing() {
    let fx = Persisted::new("one\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();
    fx.call("call-1", "EDITED\n").await;

    let mut text = std::fs::read_to_string(fx.journal()).unwrap();
    text.push_str("{\"t\":\"comment\",\"id\":\n");
    std::fs::write(fx.journal(), text).unwrap();

    let restarted = fx.restart().await;
    assert_eq!(
        restarted.integrity(&fx.session).skips()[0].record,
        SkipKind::Informational
    );
    assert!(
        !restarted.integrity(&fx.session).blocks(&root),
        "a lost comment degraded a root it has nothing to do with"
    );
}

/// A journal with no readable base cannot name the roots it tracked, so there
/// is nothing left to scope the loss to. It has to degrade everything.
#[tokio::test]
async fn a_journal_with_no_base_degrades_every_root() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;

    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let kept: Vec<&str> = text.lines().filter(|l| !l.contains("\"base\"")).collect();
    std::fs::write(fx.journal(), kept.join("\n") + "\n").unwrap();

    let restarted = fx.restart().await;
    assert!(restarted.integrity(&fx.session).blocks_everything());
}

/// A decision is a statement about specific lines, named by an id derived from
/// them. Change the derivation and the same id names different lines, so the
/// decision must not be applied — but it must not be deleted either, or
/// rolling the change back would not restore it.
#[tokio::test]
async fn a_decision_from_different_hunk_arithmetic_is_excluded_not_deleted() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let id = fx.ledgers.list_hunks(&fx.session).await.unwrap()[0]
        .id
        .clone();

    // Nothing records a decision now, so the test writes the record that an
    // old daemon wrote.
    let mut text = std::fs::read_to_string(fx.journal()).unwrap();
    text.push_str(&format!(
        "{}\n",
        serde_json::json!({
            "t": "state",
            "hunk": id,
            "state": "accepted",
            "alg": "from-a-future-version",
        })
    ));
    std::fs::write(fx.journal(), text).unwrap();

    let restarted = fx.restart().await;
    let hunks = restarted.list_hunks(&fx.session).await.unwrap();
    assert_eq!(
        hunks[0].state,
        ReviewState::Unreviewed,
        "a decision made under different arithmetic was applied to lines nobody reviewed"
    );
    assert!(
        std::fs::read_to_string(fx.journal())
            .unwrap()
            .contains("from-a-future-version"),
        "the excluded decision was deleted from disk, so a rollback cannot restore it"
    );
}

/// A journal from a daemon that reverted hunks holds `rejected` and `undone`
/// records. The daemon no longer reverts or undoes, but an old journal must
/// still replay intact. If replay reads those records as unknown, every such
/// session loses its whole review record at the upgrade.
#[tokio::test]
async fn an_old_journal_with_reject_records_still_replays() {
    let fx = Persisted::new("one\ntwo\n").await;
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0].clone();
    fx.call("call-1", "EDITED\ntwo\n").await;
    let hunk = fx.ledgers.list_hunks(&fx.session).await.unwrap()[0].clone();

    // The records that an old daemon wrote for one reject, one undo and a
    // second reject whose batch nothing undid.
    let batch = serde_json::json!([{
        "id": hunk.id,
        "root": base.root,
        "path": "a.txt",
        "start": 1,
        "before_content": "one\n",
        "after_content": "EDITED\n",
    }]);
    let mut text = std::fs::read_to_string(fx.journal()).unwrap();
    for record in [
        serde_json::json!({ "t": "rejected", "batch": batch }),
        serde_json::json!({ "t": "undone" }),
        serde_json::json!({ "t": "rejected", "batch": batch }),
    ] {
        text.push_str(&format!("{record}\n"));
    }
    std::fs::write(fx.journal(), text).unwrap();

    let restarted = fx.restart().await;
    assert!(
        restarted.integrity(&fx.session).is_intact(),
        "an old reject record was read as unknown: {:?}",
        restarted.integrity(&fx.session).skips()
    );
    assert_eq!(
        restarted.ledger(&fx.session).unwrap().session_base()[0],
        base
    );
    let hunks = restarted.list_hunks(&fx.session).await.unwrap();
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!(hunks[0].tool_call_ids, vec!["call-1".to_string()]);
}

/// Every journal on disk was written before the plain store existed, and each
/// of its tree ids is bare hex. Reading one as a git snapshot is what keeps a
/// session's base — and with it the whole composed diff — after the upgrade.
#[test]
fn a_journal_line_written_before_snapshot_ids_still_reads() {
    use crate::review::journal::Record;

    let line =
        r#"{"t":"base","root":"/repo","base_tree":"0000000000000000000000000000000000000000"}"#;
    let record = serde_json::from_str::<Record>(line).expect("a pre-snapshot-id base record");

    match record {
        Record::Base { root, base_tree } => {
            assert_eq!(root, Path::new("/repo"));
            assert_eq!(base_tree, SnapshotId::git("0".repeat(40)));
        }
        other => panic!("a base record read back as {other:?}"),
    }
}
