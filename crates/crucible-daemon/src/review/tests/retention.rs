//! Keep refs: every tree a ledger names is `write-tree` output reachable from
//! no ref, so something has to claim them before `git gc` collects them.

use super::*;

async fn git_out(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Defect 14, with its own negative control. `write-tree` output is reachable
/// from no ref, so the second half of this test is what proves the first half
/// is doing anything at all.
#[tokio::test]
async fn a_keep_ref_survives_an_aggressive_gc_and_an_unkept_tree_does_not() {
    let dir = TempDir::new().unwrap();
    repo(dir.path(), &[("a.txt", "one\n")]).await;

    std::fs::write(dir.path().join("a.txt"), "kept\n").unwrap();
    let kept = TreeSha::new(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
    std::fs::write(dir.path().join("a.txt"), "unkept\n").unwrap();
    let unkept = TreeSha::new(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
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
        git_out(dir.path(), &["fsck", "--no-progress"])
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
    let tree = TreeSha::new(workspace_snapshot::capture_tree(dir.path()).await.unwrap());
    git::update_keep(dir.path(), "sess", &[tree]).await.unwrap();

    let log = git_out(dir.path(), &["log", "--all", "--format=%s"]).await;
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

    crate::review::drop_keep_refs(fx.session_dir.path(), &fx.session).await;
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

    let ledgers = Arc::new(ReviewLedgers::default());
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
    let dropped = crate::review::sweep_review_refs(&sessions).await;

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

    let ledgers = Arc::new(ReviewLedgers::default());
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
    let dropped = crate::review::sweep_review_refs(&sessions).await;

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
        .list_hunks_with_status(&fx.session)
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
