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
/// Checks the base, the attribution and the decision together, because each
/// alone can be preserved while the review surface is still wrong — a base
/// that survives without its intervals makes every hunk external, and a hunk
/// that survives without its decision goes back to the queue.
#[tokio::test]
async fn a_restart_keeps_the_session_base_the_attribution_and_the_decisions() {
    let fx = Persisted::new("one\ntwo\n").await;
    let base = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .base_tree
        .clone();
    fx.call("call-1", "EDITED\ntwo\n").await;

    let before = fx.ledgers.list_hunks(&fx.session).await.unwrap();
    assert_eq!(before.len(), 1);
    fx.ledgers
        .set_state(&fx.session, &before[0].id, ReviewState::Accepted)
        .await
        .unwrap();

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
    assert_eq!(after[0].state, ReviewState::Accepted);
}

/// Comments are session-scoped review state too, and a comment that evaporates
/// on restart is a review someone wrote and nobody will read.
#[tokio::test]
async fn a_restart_keeps_comments_and_their_resolution() {
    let fx = Persisted::new("one\n").await;
    let root = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .root
        .clone();
    let base_tree = fx.ledgers.ledger(&fx.session).unwrap().session_base()[0]
        .base_tree
        .clone();
    let open = Comment::new(
        root.clone(),
        "a.txt",
        base_tree.clone(),
        LineRange::new(1, 2),
        "why this?",
        CommentAuthor::Human,
    );
    let answered = Comment::new(
        root,
        "a.txt",
        base_tree,
        LineRange::new(1, 2),
        "and this?",
        CommentAuthor::Human,
    );
    fx.ledgers.add_comment(&fx.session, open.clone()).await;
    fx.ledgers.add_comment(&fx.session, answered.clone()).await;
    fx.ledgers
        .resolve_comment(&fx.session, &answered.id)
        .await
        .unwrap();

    let restarted = fx.restart().await;
    let comments = restarted.comments(&fx.session);
    assert_eq!(comments.len(), 2);
    assert!(!comments.iter().find(|c| c.id == open.id).unwrap().resolved);
    assert!(
        comments
            .iter()
            .find(|c| c.id == answered.id)
            .unwrap()
            .resolved,
        "the resolution did not replay, so an answered comment came back open"
    );
}

/// A rejection is recorded against a hunk that the revert has just removed
/// from the composed diff. The entry has to survive anyway: the `reapplied`
/// derivation is defined by finding a recorded `Rejected` for a hunk that is
/// live *again*, and it cannot find one that was pruned on the way out.
#[tokio::test]
async fn a_rejection_survives_a_restart_even_with_no_live_hunk() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let id = fx.ledgers.list_hunks(&fx.session).await.unwrap()[0]
        .id
        .clone();
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Rejected)
        .await
        .unwrap();
    assert!(fx.ledgers.list_hunks(&fx.session).await.unwrap().is_empty());

    let restarted = fx.restart().await;
    // Re-applied by the agent after the restart: the recorded rejection is the
    // only thing that can tell this apart from a first sighting.
    fx.write("EDITED\n");
    let hunks = restarted.list_hunks(&fx.session).await.unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].id, id);
    assert!(
        hunks[0].reapplied,
        "the rejection did not survive, so a re-applied change reads as new work"
    );
    assert_eq!(hunks[0].state, ReviewState::Unreviewed);
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

    let ledgers = Arc::new(ReviewLedgers::default());
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

/// The other half of the same failure, and the one the shape above used to
/// create: leaving the session with *no ledger* is the same signal a workspace
/// outside git produces, so the gate returned before it queried anything and
/// every write in the session proceeded unheld. A journal whose header line is
/// merely corrupt already blocks everything; losing the whole file is strictly
/// more broken and must block at least as much.
#[tokio::test]
async fn an_unreadable_journal_holds_every_write_until_a_rebase() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.repo_dir.path().to_path_buf();

    std::fs::remove_file(fx.journal()).unwrap();
    std::fs::create_dir(fx.journal()).unwrap();

    let ledgers = Arc::new(ReviewLedgers::default());
    let _ = ledgers
        .open_or_restore(
            &fx.session,
            fx.session_dir.path(),
            &[fx.repo_dir.path().to_path_buf()],
        )
        .await;

    assert!(
        ledgers.integrity(&fx.session).blocks_everything(),
        "an unreadable journal recorded no unscoped loss, so nothing blocks"
    );
    assert!(
        matches!(
            ledgers
                .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
                .await
                .unwrap(),
            Verdict::Degraded { .. }
        ),
        "a write proceeded on the strength of a ledger that could not be read"
    );

    // The release has to be reachable, or the block above is a hang. The
    // journal is a directory here, so the append cannot land — but the ledger
    // is rebased in memory and the gate opens, which is what unblocks the turn.
    std::fs::remove_dir(fx.journal()).unwrap();
    ledgers
        .rebase_session(
            &fx.session,
            fx.session_dir.path(),
            std::slice::from_ref(&root),
        )
        .await
        .unwrap();
    assert_eq!(
        ledgers
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Clear,
        "rebase did not release the block it exists to bound"
    );
}

/// Degradation is graded because losing attribution fails *open*: a hunk no
/// interval accounts for is external, `unreviewed_hunks` excludes external
/// hunks, and the gate silently stops blocking. A skipped interval therefore
/// has to block writes under its root.
#[tokio::test]
async fn a_skipped_interval_blocks_writes_under_its_own_root() {
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
    assert!(matches!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Degraded { .. }
    ));
    // ...and a path under no tracked root is still clear, or a gate query the
    // ledger has no business answering would hang the turn.
    assert_eq!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(PathBuf::from("/elsewhere/a.txt")))
            .await
            .unwrap(),
        Verdict::Clear
    );
}

/// An interval naming several roots cannot be scoped to one of them.
///
/// Scoping to the *first* legible root blocked that root and left the others
/// open — and a root whose intervals were lost reports external hunks, which
/// `unreviewed_hunks` excludes, so the gate went quiet on exactly the root
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
    assert!(matches!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Degraded { .. }
    ));
}

/// A rebase names one root, so it can only vouch for that one. Clearing every
/// skip on replay let a *partial* rebase — one root recaptured, another still
/// degraded — come back from a restart reporting both intact, with the gate no
/// longer holding writes under the root nobody rebased.
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
/// blocking the agent on one would be a self-inflicted outage.
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
    assert_eq!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("b.txt")))
            .await
            .unwrap(),
        Verdict::Clear,
        "a lost comment blocked a write it has nothing to do with"
    );
}

/// A journal with no readable base cannot name the roots it tracked, so there
/// is nothing left to scope a block to. It has to block everything — and
/// `review.rebase` is what keeps that from being an unreleasable hang.
#[tokio::test]
async fn a_journal_with_no_base_blocks_every_write_until_a_rebase() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.repo_dir.path().to_path_buf();

    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let kept: Vec<&str> = text.lines().filter(|l| !l.contains("\"base\"")).collect();
    std::fs::write(fx.journal(), kept.join("\n") + "\n").unwrap();

    let restarted = fx.restart().await;
    assert!(restarted.integrity(&fx.session).blocks_everything());
    assert!(matches!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Degraded { .. }
    ));

    restarted
        .rebase_session(
            &fx.session,
            fx.session_dir.path(),
            std::slice::from_ref(&root),
        )
        .await
        .unwrap();
    assert_eq!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Clear,
        "rebase did not release the block it exists to bound"
    );
    // ...and the release survives, or the next restart re-reads the same
    // corrupt line and blocks again with no way out.
    let again = fx.restart().await;
    assert!(again.integrity(&fx.session).is_intact());
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
    fx.ledgers
        .set_state(&fx.session, &id, ReviewState::Accepted)
        .await
        .unwrap();

    let text = std::fs::read_to_string(fx.journal()).unwrap();
    let rewritten: Vec<String> = text
        .lines()
        .map(|line| {
            if line.contains("\"state\"") && line.contains("\"accepted\"") {
                let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
                value["alg"] = serde_json::json!("from-a-future-version");
                serde_json::to_string(&value).unwrap()
            } else {
                line.to_string()
            }
        })
        .collect();
    std::fs::write(fx.journal(), rewritten.join("\n") + "\n").unwrap();

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

/// A rebase is reachable on a session that has never run a turn — the panel
/// opens on a resumed session, the queue reads empty, and rebasing is the
/// natural thing to try. Nothing has registered a journal for it yet, so every
/// append silently no-ops and `refresh_keep_refs` returns at its guard: the
/// rebase answers `Ok` while persisting nothing and leaving the base tree it
/// just captured reachable from no ref, one `git gc` from being uncomputable.
#[tokio::test]
async fn a_rebase_on_a_session_with_no_journal_still_persists_and_keeps_its_trees() {
    let repo_dir = TempDir::new().unwrap();
    repo(repo_dir.path(), &[("a.txt", "one\n")]).await;
    std::fs::write(repo_dir.path().join("uncommitted.txt"), "scratch\n").unwrap();
    let session_dir = TempDir::new().unwrap();

    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .rebase_session("sess", session_dir.path(), &[repo_dir.path().to_path_buf()])
        .await
        .unwrap();

    assert!(
        session_dir.path().join("review.jsonl").exists(),
        "the rebase never reached disk; it is gone on the next restart"
    );
    assert_eq!(
        keep_ref_ids(repo_dir.path()).await.unwrap(),
        vec!["sess".to_string()],
        "the freshly captured base tree is reachable from no ref"
    );

    // ...and the whole thing replays, which is the only proof the append was
    // the right shape rather than merely present.
    let restarted = Arc::new(ReviewLedgers::default());
    restarted
        .open_or_restore("sess", session_dir.path(), &[repo_dir.path().to_path_buf()])
        .await
        .unwrap();
    assert_eq!(
        restarted.ledger("sess").unwrap().session_base(),
        ledgers.ledger("sess").unwrap().session_base()
    );
}

/// A rebase over several roots can fail on one of them. Dropping that root from
/// `session_base` takes it out of the composed diff *and* out of every gate
/// query, so the moment the failure clears the gate answers `Clear` for a root
/// whose changes are no longer in the queue at all — and only another rebase
/// would ever put them back.
#[tokio::test]
async fn a_root_a_rebase_could_not_recapture_stays_tracked() {
    let workspace = TempDir::new().unwrap();
    repo(workspace.path(), &[("a.txt", "one\n")]).await;
    let kiln = TempDir::new().unwrap();
    repo(kiln.path(), &[("b.txt", "one\n")]).await;
    let session_dir = TempDir::new().unwrap();

    let roots = vec![workspace.path().to_path_buf(), kiln.path().to_path_buf()];
    let ledgers = Arc::new(ReviewLedgers::default());
    ledgers
        .open_or_restore("sess", session_dir.path(), &roots)
        .await
        .unwrap();

    // One bracketed write under the kiln, so the root has attribution to lose.
    let handle = ledgers.open_bracket("sess").await.unwrap();
    std::fs::write(kiln.path().join("b.txt"), "BY THE AGENT\n").unwrap();
    ledgers.close("sess", handle, "call-1", 1).await.unwrap();

    // A directory where the kiln's index belongs: `rev-parse --show-toplevel`
    // still answers, so the root is unmistakably a repository, while the
    // capture's index copy cannot run. This is the transient shape — a
    // concurrent `git` holding the index — without depending on file modes the
    // test process might be allowed to ignore.
    let index = kiln.path().join(".git").join("index");
    std::fs::remove_file(&index).unwrap();
    std::fs::create_dir(&index).unwrap();

    let statuses = ledgers
        .rebase_session("sess", session_dir.path(), &roots)
        .await
        .unwrap();
    let kiln_top = std::fs::canonicalize(kiln.path()).unwrap();
    assert!(
        statuses
            .iter()
            .find(|s| *s.root == *kiln_top)
            .expect("the kiln must still be reported")
            .is_degraded(),
        "{statuses:?}"
    );

    // The failure clears; the root has to be there when it does.
    std::fs::remove_dir(&index).unwrap();
    let ledger = ledgers.ledger("sess").unwrap();
    assert!(
        ledger.roots().any(|r| r == kiln_top),
        "the kiln was dropped from session_base, so nothing under it is reviewable again"
    );
    let hunks = ledgers.list_hunks("sess").await.unwrap();
    let kiln_hunk = hunks
        .iter()
        .find(|h| *h.root == *kiln_top)
        .expect("the kiln's change is still in the worktree and must still compose");
    assert_eq!(
        kiln_hunk.tool_call_ids,
        vec!["call-1".to_string()],
        "the un-rebased root lost its intervals, so agent work reads as the user's own edit"
    );
    assert_eq!(
        ledgers
            .has_unreviewed_in_file("sess", &only(kiln_top.join("b.txt")))
            .await
            .unwrap(),
        Verdict::Unreviewed,
        "a write to the un-rebased root went through unheld"
    );
}
