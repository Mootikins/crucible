//! Keep refs: every tree a ledger names is `write-tree` output reachable from
//! no ref, so something has to claim them before `git gc` collects them.

use super::*;
use crate::workspace_snapshot;

/// Defect 14, with its own negative control. `write-tree` output is reachable
/// from no ref, so the second half of this test is what proves the first half
/// is doing anything at all.
#[tokio::test]
async fn a_keep_ref_survives_an_aggressive_gc_and_an_unkept_tree_does_not() {
    let dir = TempDir::new().unwrap();
    repo(dir.path(), &[("a.txt", "one\n")]).await;

    std::fs::write(dir.path().join("a.txt"), "kept\n").unwrap();
    let kept = SnapshotId::git(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
    std::fs::write(dir.path().join("a.txt"), "unkept\n").unwrap();
    let unkept = SnapshotId::git(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
    std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    assert_ne!(kept, unkept);

    git::update_keep(dir.path(), "sess", std::slice::from_ref(&kept))
        .await
        .unwrap();
    git(dir.path(), &["gc", "--prune=now", "--aggressive", "-q"]).await;

    assert!(
        git::tree_exists(dir.path(), &kept).await,
        "gc deleted a tree the keep ref names; the review queue is now uncomputable"
    );
    assert!(
        !git::tree_exists(dir.path(), &unkept).await,
        "gc kept an unreferenced tree, so this test proves nothing about the keep ref"
    );
    assert!(
        git(dir.path(), &["fsck", "--no-progress"])
            .await
            .trim()
            .is_empty(),
        "the keep ref left the repository failing fsck"
    );
}

/// A commit-valued ref would show up in `git log --all` and put daemon
/// bookkeeping in the user's history. A tree-valued one is not a starting
/// point the revision walker accepts, so it stays invisible.
#[tokio::test]
async fn a_keep_ref_is_invisible_to_git_log_all() {
    let dir = TempDir::new().unwrap();
    repo(dir.path(), &[("a.txt", "one\n")]).await;
    std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
    let tree = SnapshotId::git(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
    git::update_keep(dir.path(), "sess", &[tree]).await.unwrap();

    let log = git(dir.path(), &["log", "--all", "--format=%s"]).await;
    assert_eq!(
        log.lines().collect::<Vec<_>>(),
        vec!["init"],
        "the keep ref appeared in the user's history"
    );
}

/// Deleting a session is the last moment its journal — the only record of
/// which repositories it claimed refs in — still exists.
#[tokio::test]
async fn dropping_a_sessions_keep_refs_releases_every_root_its_journal_names() {
    let fx = Persisted::new("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    assert_eq!(
        keep_ref_ids(fx.repo_dir.path()).await.unwrap(),
        vec!["sess".to_string()]
    );

    crate::review::drop_keep_refs(fx.session_dir.path(), &fx.session, Some(fx.snaps.path())).await;
    assert!(keep_ref_ids(fx.repo_dir.path()).await.unwrap().is_empty());
}

/// The backstop for every path that removes a session directory without going
/// through `delete_session`. A ref whose journal is still there is still doing
/// its job and must not be swept, however idle the session looks.
#[tokio::test]
async fn the_sweep_releases_orphaned_keep_refs_and_leaves_live_ones() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();
    repo(repo_dir.path(), &[("a.txt", "one\n")]).await;
    let sessions = home.path().join("sessions");

    let snaps = crate::test_support::scratch_snapshot_root();
    let ledgers = Arc::new(ReviewLedgers::new(snaps.clone()));
    for id in ["live", "orphan"] {
        ledgers
            .open_or_restore(id, &sessions.join(id), &[repo_dir.path().to_path_buf()])
            .await
            .unwrap();
    }
    let mut held = keep_ref_ids(repo_dir.path()).await.unwrap();
    held.sort();
    assert_eq!(held, vec!["live".to_string(), "orphan".to_string()]);

    std::fs::remove_dir_all(sessions.join("orphan")).unwrap();
    let dropped = crate::review::sweep_review_refs(&sessions, &snaps).await;

    assert_eq!(dropped, 1);
    assert_eq!(
        keep_ref_ids(repo_dir.path()).await.unwrap(),
        vec!["live".to_string()],
        "the sweep released a ref whose session is still there"
    );
}

/// Turn-undo keeps its own keep refs in a second namespace. The sweep read
/// only `refs/crucible/sessions/`, so a crashed daemon pinned every snapshot
/// tree it had captured, in the user's own repository, forever.
#[tokio::test]
async fn the_sweep_releases_orphaned_snapshot_refs_too() {
    let home = TempDir::new().unwrap();
    let repo_dir = TempDir::new().unwrap();
    repo(repo_dir.path(), &[("a.txt", "one\n")]).await;
    let sessions = home.path().join("sessions");

    let snaps = crate::test_support::scratch_snapshot_root();
    let ledgers = Arc::new(ReviewLedgers::new(snaps.clone()));
    // A surviving session keeps the repository in the sweep's view at all —
    // the sweep reaches a root only through a live session's journal, so a
    // repo whose every session is gone is unreachable to it by construction.
    for id in ["live", "orphan"] {
        ledgers
            .open_or_restore(id, &sessions.join(id), &[repo_dir.path().to_path_buf()])
            .await
            .unwrap();
    }
    // Undo snapshots for both, in the *other* namespace.
    for id in ["live", "orphan"] {
        let snap =
            crate::workspace_snapshot::WorkspaceSnapshot::create(repo_dir.path(), id, 3).await;
        assert!(snap.tree_id.is_some(), "expected a git-mode snapshot");
    }
    assert_eq!(
        keep_ref_ids(repo_dir.path()).await.unwrap().len(),
        4,
        "two sessions across two namespaces"
    );

    std::fs::remove_dir_all(sessions.join("orphan")).unwrap();
    let dropped = crate::review::sweep_review_refs(&sessions, &snaps).await;

    assert_eq!(dropped, 2, "the orphan's snapshot ref was left behind");
    let mut held = keep_ref_ids(repo_dir.path()).await.unwrap();
    held.sort();
    assert_eq!(
        held,
        vec!["live".to_string(); 2],
        "the sweep must release both namespaces, and only the orphan's"
    );
}

/// The structural failure the rebase RPC exists for: the base tree is gone, so
/// the composed diff cannot be computed at all and no amount of reviewing will
/// produce a hunk to review.
#[tokio::test]
async fn a_gcd_base_tree_blocks_until_a_rebase_and_then_clears() {
    let fx = Persisted::new_with_uncommitted("one\n").await;
    fx.call("call-1", "EDITED\n").await;
    let root = fx.repo_dir.path().to_path_buf();

    // Release the ref, then gc: exactly what a user running `git gc` in a repo
    // an older build had captured would produce.
    git::drop_keep(&root, &fx.session).await.unwrap();
    git(&root, &["gc", "--prune=now", "-q"]).await;

    let (hunks, statuses) = fx
        .ledgers
        .list_hunks_with_status(
            &fx.session,
            crucible_core::session::ReviewScope::Session,
            None,
        )
        .await
        .unwrap();
    assert!(hunks.is_empty());
    assert!(statuses[0].is_degraded(), "{statuses:?}");
    assert!(
        matches!(
            fx.ledgers
                .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
                .await
                .unwrap(),
            Verdict::Degraded { .. }
        ),
        "a root with no readable base let a write through on the strength of having no hunks"
    );

    fx.ledgers
        .rebase_session(
            &fx.session,
            fx.session_dir.path(),
            std::slice::from_ref(&root),
        )
        .await
        .unwrap();
    assert_eq!(
        fx.ledgers
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Clear
    );
    // The rebase is only a real release if it also survives the next restart.
    let restarted = fx.restart().await;
    assert_eq!(
        restarted
            .has_unreviewed_in_file(&fx.session, &only(root.join("a.txt")))
            .await
            .unwrap(),
        Verdict::Clear
    );
}

/// The seam the delegation harvest must use. `Ledger::push_interval` compiles
/// just as cleanly against a `DashMap` guard and never reaches the parent's
/// journal, so the harvested attribution would look right until the next
/// restart — defect 13, reintroduced for delegated work specifically.
#[tokio::test]
async fn absorbed_child_intervals_reach_the_parents_journal() {
    let fx = Persisted::new("one\n").await;
    let child_dir = TempDir::new().unwrap();
    fx.ledgers
        .open_or_restore(
            "child",
            child_dir.path(),
            &[fx.repo_dir.path().to_path_buf()],
        )
        .await
        .unwrap();

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    fx.ledgers
        .close("child", handle, "child-call", 3)
        .await
        .unwrap();

    // Unattributed in the parent until it is harvested: the parent never
    // bracketed the delegation, by design.
    assert!(fx.ledgers.list_hunks(&fx.session).await.unwrap()[0].is_external());

    assert_eq!(
        fx.ledgers
            .absorb_child_intervals(&fx.session, "child")
            .await,
        1
    );
    let restarted = fx.restart().await;
    let hunks = restarted.list_hunks(&fx.session).await.unwrap();
    assert_eq!(
        hunks[0].tool_call_ids,
        vec!["child-call".to_string()],
        "the harvest never reached the journal; delegated work is unattributed again"
    );
}

/// The parent's composed diff can only show changes under roots it tracks, so
/// a child working in its own worktree has nothing to contribute — and copying
/// its intervals up would claim attribution over a repository the parent's
/// diff never looks at.
#[tokio::test]
async fn a_child_in_its_own_worktree_contributes_nothing_to_its_parent() {
    let fx = Persisted::new("one\n").await;
    let elsewhere = TempDir::new().unwrap();
    repo(elsewhere.path(), &[("b.txt", "one\n")]).await;
    let child_dir = TempDir::new().unwrap();
    fx.ledgers
        .open_or_restore("child", child_dir.path(), &[elsewhere.path().to_path_buf()])
        .await
        .unwrap();

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    std::fs::write(elsewhere.path().join("b.txt"), "EDITED\n").unwrap();
    fx.ledgers
        .close("child", handle, "child-call", 3)
        .await
        .unwrap();

    assert_eq!(
        fx.ledgers
            .absorb_child_intervals(&fx.session, "child")
            .await,
        0
    );
}

/// `0` is a real conversation-tree node — the root — so a `node_id` defaulted
/// rather than absent renders a delegation confidently under turn 0 instead of
/// under the turn that issued it.
#[tokio::test]
async fn a_child_link_written_without_a_turn_reads_back_as_absent() {
    let fx = Persisted::new("one\n").await;
    fx.ledgers
        .link_child(&fx.session, "call-d", "sess-child", Some(7))
        .await;

    // A row from a build that predates the field.
    let mut text = std::fs::read_to_string(fx.journal()).unwrap();
    text.push_str(
        "{\"t\":\"child\",\"tool_call_id\":\"call-old\",\"child_session_id\":\"sess-old\"}\n",
    );
    std::fs::write(fx.journal(), text).unwrap();

    let restarted = fx.restart().await;
    let ledger = restarted.ledger(&fx.session).unwrap();
    assert_eq!(ledger.children()[0].node_id, Some(7));
    assert_eq!(
        ledger.children()[1].node_id,
        None,
        "an absent turn came back as turn 0, which is a real turn"
    );
}

/// A directory outside git, with two files, one of which every such directory
/// in a test holds byte for byte — so a blob two snapshots name is in play.
async fn plain_root(own: &str) -> TempDir {
    let root = TempDir::new().unwrap();
    // The premise. A checkout above the system temp directory would make this
    // root git-backed and the test would assert nothing about the plain store;
    // export `GIT_CEILING_DIRECTORIES` or move `TMPDIR` out of the repo.
    assert!(
        super::git::top_level(root.path()).await.is_err(),
        "the temp directory is inside a git repository, so this is not a plain root"
    );
    std::fs::write(root.path().join("shared.md"), "shared\n").unwrap();
    std::fs::write(root.path().join("own.md"), own).unwrap();
    root
}

fn snapshot_file(snaps: &Path, id: &SnapshotId) -> PathBuf {
    snaps
        .join("snapshots")
        .join(format!("{}.json", id.as_str()))
}

/// The blob one path has inside one snapshot, as a file under the store root.
fn blob_file(snaps: &Path, id: &SnapshotId, path: &str) -> PathBuf {
    let bytes = std::fs::read(snapshot_file(snaps, id)).unwrap();
    let manifest: crate::review::plain_store::Manifest = serde_json::from_slice(&bytes).unwrap();
    snaps.join("blobs").join(&manifest.files[path])
}

/// Backdate every file in the store past the sweep's grace period.
///
/// The sweep refuses to collect a young file, because a capture writes its
/// snapshot before the ledger records the interval that claims it. A test that
/// captures and sweeps in the same millisecond is inside that window by
/// construction, so it has to age the disk to ask its own question.
fn age_store(snaps: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    let times = std::fs::FileTimes::new().set_modified(old);
    for dir in ["snapshots", "blobs"] {
        let Ok(entries) = std::fs::read_dir(snaps.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let file = std::fs::File::options().write(true).open(entry.path());
            file.unwrap().set_times(times).unwrap();
        }
    }
}

/// Every snapshot one session's ledger still names.
fn claimed(ledgers: &ReviewLedgers, session_id: &str) -> Vec<SnapshotId> {
    let ledger = ledgers.ledger(session_id).unwrap();
    let root = ledger.session_base()[0].root.clone();
    ledger.trees_for(&root)
}

/// The plain store's half of `the_sweep_releases_orphaned_keep_refs_and_leaves_live_ones`.
/// A plain snapshot is collected by nothing else at all — there is no `git gc`
/// under a kiln that is not a repository — so this sweep is the whole of
/// retention for it, and it has to release the dead without touching a file
/// any live session still names.
#[tokio::test]
async fn the_plain_sweep_removes_snapshots_no_live_journal_names() {
    let home = TempDir::new().unwrap();
    let snaps = TempDir::new().unwrap();
    let sessions = home.path().join("sessions");
    let ledgers = Arc::new(ReviewLedgers::new(snaps.path().to_path_buf()));

    let live_root = plain_root("live\n").await;
    let orphan_root = plain_root("orphan\n").await;
    for (id, root) in [("live", &live_root), ("orphan", &orphan_root)] {
        ledgers
            .open_or_restore(id, &sessions.join(id), &[root.path().to_path_buf()])
            .await
            .unwrap();
    }
    // One bracketed call, so the live session claims its interval's snapshots
    // and not only its base.
    let handle = ledgers.open_bracket("live").await.unwrap();
    std::fs::write(live_root.path().join("own.md"), "EDITED\n").unwrap();
    ledgers.close("live", handle, "call-1", 1).await.unwrap();

    let live_ids = claimed(&ledgers, "live");
    let orphan_ids = claimed(&ledgers, "orphan");
    assert_eq!(live_ids.len(), 2, "a base and the call's after-snapshot");
    assert_eq!(orphan_ids.len(), 1);
    let shared = blob_file(snaps.path(), &orphan_ids[0], "shared.md");
    let orphan_only = blob_file(snaps.path(), &orphan_ids[0], "own.md");
    assert_eq!(
        shared,
        blob_file(snaps.path(), &live_ids[0], "shared.md"),
        "the two roots must share a blob for this test to mean anything"
    );

    std::fs::remove_dir_all(sessions.join("orphan")).unwrap();
    age_store(snaps.path());
    let released = crate::review::sweep_review_refs(&sessions, snaps.path()).await;

    assert_eq!(
        released, 2,
        "the orphan's snapshot and the one blob only it named"
    );
    assert!(
        !snapshot_file(snaps.path(), &orphan_ids[0]).exists(),
        "a snapshot no session names was left on disk"
    );
    assert!(!orphan_only.exists(), "its content was left with it");
    assert!(
        shared.exists(),
        "a blob the live session also names went with the session that died"
    );
    for id in &live_ids {
        assert!(
            snapshot_file(snaps.path(), id).exists(),
            "the sweep took {id}, which the live session still names"
        );
    }

    // The property all of that is for: the surviving review still computes.
    let hunks = ledgers.list_hunks("live").await.unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].after_content, "EDITED\n");
    assert_eq!(hunks[0].before_content, "live\n");
}

/// Deleting a session releases its claims there and then, rather than leaving
/// the disk held until the next sweep.
#[tokio::test]
async fn dropping_a_sessions_claims_releases_its_plain_snapshots() {
    let home = TempDir::new().unwrap();
    let snaps = TempDir::new().unwrap();
    let sessions = home.path().join("sessions");
    let ledgers = Arc::new(ReviewLedgers::new(snaps.path().to_path_buf()));
    let root = plain_root("one\n").await;
    ledgers
        .open_or_restore("sess", &sessions.join("sess"), &[root.path().to_path_buf()])
        .await
        .unwrap();
    let ids = claimed(&ledgers, "sess");
    assert!(snapshot_file(snaps.path(), &ids[0]).exists());

    // The delete path's order: claims first, while the session directory is
    // still there, then the directory.
    crate::review::drop_keep_refs(&sessions.join("sess"), "sess", Some(snaps.path())).await;
    std::fs::remove_dir_all(sessions.join("sess")).unwrap();

    assert!(
        !snaps.path().join("keeps").join("sess").exists(),
        "the session's claims outlived the session"
    );
    age_store(snaps.path());
    assert_eq!(
        crate::review::sweep_review_refs(&sessions, snaps.path()).await,
        3,
        "one snapshot and its two blobs"
    );
}

/// The branch that decides whether an unanswerable question deletes
/// everything. A sessions root the daemon cannot read makes every session look
/// deleted, and reading it that way would collect every review in the store.
#[tokio::test]
async fn a_sweep_that_cannot_see_the_sessions_root_removes_nothing() {
    let home = TempDir::new().unwrap();
    let snaps = TempDir::new().unwrap();
    let sessions = home.path().join("sessions");
    let ledgers = Arc::new(ReviewLedgers::new(snaps.path().to_path_buf()));
    let root = plain_root("one\n").await;
    ledgers
        .open_or_restore("sess", &sessions.join("sess"), &[root.path().to_path_buf()])
        .await
        .unwrap();
    let ids = claimed(&ledgers, "sess");

    let released = crate::review::sweep_review_refs(&home.path().join("gone"), snaps.path()).await;

    assert_eq!(released, 0);
    assert!(
        snapshot_file(snaps.path(), &ids[0]).exists(),
        "a sweep that could not tell which sessions are live collected one anyway"
    );
}

/// The maintenance tick runs every 30 minutes and knows nothing about the
/// brackets in flight. A capture writes its snapshot before the ledger records
/// the interval that claims it, so a sweep landing in that window collects a
/// snapshot a live call is about to name — and an interval's `before_tree` is a
/// past state no later capture reproduces, so the session's hunks never list
/// again. Git has no equivalent exposure: `git gc` prunes only objects older
/// than `gc.pruneExpire`, two weeks by default.
#[tokio::test]
async fn a_sweep_inside_a_bracket_keeps_the_snapshot_the_call_will_name() {
    let home = TempDir::new().unwrap();
    let snaps = TempDir::new().unwrap();
    let sessions = home.path().join("sessions");
    let ledgers = Arc::new(ReviewLedgers::new(snaps.path().to_path_buf()));

    let root = plain_root("one\n").await;
    ledgers
        .open_or_restore("live", &sessions.join("live"), &[root.path().to_path_buf()])
        .await
        .unwrap();

    // An external edit first, so the bracket's before-snapshot is a state no
    // keep file names yet.
    std::fs::write(root.path().join("own.md"), "two\n").unwrap();
    let handle = ledgers.open_bracket("live").await.unwrap();

    let released = crate::review::sweep_review_refs(&sessions, snaps.path()).await;

    std::fs::write(root.path().join("own.md"), "EDITED\n").unwrap();
    ledgers.close("live", handle, "call-1", 1).await.unwrap();

    let hunks = ledgers.list_hunks("live").await;
    assert!(
        hunks.is_ok(),
        "the review of a live session became uncomputable: {hunks:?} \
         (the sweep released {released} files during the bracket)"
    );
    let hunks = hunks.unwrap();
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!(hunks[0].before_content, "one\n");
    assert_eq!(hunks[0].after_content, "EDITED\n");
    assert_eq!(
        hunks[0].tool_call_ids,
        vec!["call-1".to_string()],
        "attribution reads the interval's own snapshots, which is what the sweep took"
    );
}

/// The blob half of the same window, which the snapshot half does not close.
/// A sweep that keeps a young unclaimed snapshot but reads no manifest for it
/// leaves that snapshot naming blobs the pass counted as garbage; and a file
/// the capture answers for out of its stat cache keeps the blob mtime of the
/// capture that first stored it, so those blobs are old enough to take. The
/// loss is the whole session's review, not one hunk: attribution reads the
/// interval's own snapshots, and a `before_tree` is a past state no later
/// capture reproduces.
#[tokio::test]
async fn a_sweep_inside_a_bracket_keeps_the_blobs_that_snapshot_names() {
    let home = TempDir::new().unwrap();
    let snaps = TempDir::new().unwrap();
    let sessions = home.path().join("sessions");
    let ledgers = Arc::new(ReviewLedgers::new(snaps.path().to_path_buf()));

    let root = plain_root("one\n").await;
    ledgers
        .open_or_restore("live", &sessions.join("live"), &[root.path().to_path_buf()])
        .await
        .unwrap();

    // The user edits the root themselves, so the bracket's before-snapshot
    // names content the base does not. The edit is backdated because a capture
    // discards the stat key of a file as young as itself, and the cache hit is
    // what leaves the blob carrying the age of the capture that stored it.
    let own = root.path().join("own.md");
    std::fs::write(&own, "two\n").unwrap();
    age_file(&own);
    // A listing captures the worktree, which stores the edited file's blob.
    ledgers.list_hunks("live").await.unwrap();

    age_store(snaps.path());
    let handle = ledgers.open_bracket("live").await.unwrap();

    let released = crate::review::sweep_review_refs(&sessions, snaps.path()).await;

    std::fs::write(&own, "EDITED\n").unwrap();
    ledgers.close("live", handle, "call-1", 1).await.unwrap();

    let hunks = ledgers.list_hunks("live").await;
    assert!(
        hunks.is_ok(),
        "the review of a live session became uncomputable: {hunks:?} \
         (the sweep released {released} files during the bracket)"
    );
    assert_eq!(released, 0, "the sweep took a file the open bracket names");
    let hunks = hunks.unwrap();
    assert_eq!(hunks.len(), 1, "{hunks:?}");
    assert_eq!(hunks[0].after_content, "EDITED\n");
    assert_eq!(
        hunks[0].tool_call_ids,
        vec!["call-1".to_string()],
        "attribution reads the interval's own snapshots, whose blobs the sweep took"
    );
}

/// Backdate one file in a worktree past the sweep's grace period.
fn age_file(path: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .unwrap();
}
