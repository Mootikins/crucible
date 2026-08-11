//! Delegation harvest and self-write suppression: a child's attribution has to
//! reach its parent, and the daemon's own writes must not read as the user's.

use super::*;

/// Open a child ledger over the parent's own repo, registered as delegated.
async fn delegated_child(fx: &Persisted, child: &str) -> TempDir {
    let child_dir = TempDir::new().unwrap();
    fx.ledgers
        .open_or_restore(child, child_dir.path(), &[fx.repo_dir.path().to_path_buf()])
        .await
        .unwrap();
    fx.ledgers.set_parent(child, &fx.session);
    child_dir
}

/// The whole point of the harvest: it has to happen while the child's ledger
/// still exists. `clear_session` drops exactly the intervals the harvest
/// reads, so a teardown that ran first would leave the delegation's work
/// permanently unattributed with nothing left to recover it from.
#[tokio::test]
async fn clearing_a_delegated_child_harvests_before_it_drops_its_ledger() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    fx.ledgers
        .close("child", handle, "child-call", 3)
        .await
        .unwrap();

    fx.ledgers.harvest_and_clear("child").await;

    assert!(
        !fx.ledgers.is_open("child"),
        "the child's ledger outlived its session"
    );
    let hunks = fx.ledgers.list_hunks(&fx.session).await.unwrap();
    assert_eq!(
        hunks[0].tool_call_ids,
        vec!["child-call".to_string()],
        "delegated work went unattributed in the parent"
    );
}

/// A harvested interval keeps the child's `tool_call_id`, which names a call
/// in a transcript the parent does not have. Without the child's id beside it
/// there is no way back to the delegation it belongs under.
#[tokio::test]
async fn a_harvested_interval_names_the_child_it_came_from() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    fx.ledgers
        .close("child", handle, "child-call", 3)
        .await
        .unwrap();
    fx.ledgers.harvest_and_clear("child").await;

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    let harvested = ledger
        .intervals()
        .iter()
        .find(|i| i.tool_call_id == "child-call")
        .expect("the harvested interval");
    assert_eq!(harvested.child_session_id.as_deref(), Some("child"));
}

/// The parent is torn down first when a whole session tree ends. There is
/// nothing to harvest into and no composed diff left to show the work in, so
/// the child degrades to unattributed rather than erroring or resurrecting a
/// ledger for a session that has gone.
#[tokio::test]
async fn a_child_whose_parent_is_already_gone_is_simply_dropped() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    fx.ledgers
        .close("child", handle, "child-call", 3)
        .await
        .unwrap();

    fx.ledgers.clear_session(&fx.session);
    fx.ledgers.harvest_and_clear("child").await;

    assert!(!fx.ledgers.is_open("child"));
    assert!(!fx.ledgers.is_open(&fx.session));
}

/// A session with no parent takes the plain teardown path, and re-registering
/// one after the fact must not resurrect the link.
#[tokio::test]
async fn clearing_a_session_forgets_that_it_was_delegated() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;
    assert_eq!(fx.ledgers.parent_of("child").as_deref(), Some("sess"));

    fx.ledgers.clear_session("child");
    assert_eq!(fx.ledgers.parent_of("child"), None);
}

/// Reverting is the daemon writing to the user's worktree, and the watcher
/// cannot tell that apart from the user writing to it. Unsuppressed, rejecting
/// a hunk announces an external change for the very file it just reverted —
/// the queue reporting its own drain as new work.
#[tokio::test]
async fn a_rejected_hunks_revert_is_not_reported_as_an_external_edit() {
    let fx = Fixture::new("one\n").await;
    let root = fx.dir.path().canonicalize().unwrap();
    let tracker = Arc::new(crate::watch::external_changes::ExternalChangeTracker::default());
    tracker.track(&fx.session, std::slice::from_ref(&root));
    fx.ledgers.set_external_tracker(Arc::clone(&tracker));

    fx.call("call-1", 1, "two\n").await;
    let hunks = fx.hunks().await;
    fx.ledgers
        .set_state(&fx.session, &hunks[0].id, ReviewState::Rejected)
        .await
        .unwrap();

    // The revert lands on disk through `tokio::fs::write`; the watcher would
    // see exactly this path.
    assert_eq!(
        tracker.observe(&root.join("a.txt")),
        crate::watch::external_changes::Ownership::Bracketed,
        "the daemon's own revert was reported as a user edit"
    );
}

/// `node_id` indexes a `ConversationTree`, and an index is meaningless in
/// another tree. Carrying the child's own index into the parent compares two
/// unrelated coordinate systems: a child that ran many turns produces indices
/// larger than the parent's current turn, so the gate's `node_id < this_turn`
/// reads harvested work as "not yet happened" and stops holding the parent's
/// next delegation on it.
#[tokio::test]
async fn a_harvested_interval_is_stamped_with_the_parents_turn_not_the_childs() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;
    fx.ledgers
        .link_child(&fx.session, "call-d", "child", Some(2))
        .await;

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    // Deep in the child's own tree, and far past where the parent has got to.
    fx.ledgers
        .close("child", handle, "child-call", 40)
        .await
        .unwrap();
    fx.ledgers.harvest_and_clear("child").await;

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    let harvested = ledger
        .intervals()
        .iter()
        .find(|i| i.tool_call_id == "child-call")
        .expect("the harvested interval");
    assert_eq!(
        harvested.node_id, 2,
        "the child's turn coordinate was copied into the parent's ledger, so the \
         parent's next delegation is not held on work it has not reviewed"
    );
}

/// A link written before the parent-side turn was recorded leaves no
/// coordinate at all. `0` is the earliest possible turn, so the harvest still
/// counts as earlier-turn work and still gates — the fail-closed direction.
#[tokio::test]
async fn a_harvest_with_no_recorded_parent_turn_still_counts_as_earlier() {
    let fx = Persisted::new("one\n").await;
    let _child_dir = delegated_child(&fx, "child").await;
    fx.ledgers
        .link_child(&fx.session, "call-d", "child", None)
        .await;

    let handle = fx.ledgers.open_bracket("child").await.unwrap();
    fx.write("BY THE CHILD\n");
    fx.ledgers
        .close("child", handle, "child-call", 40)
        .await
        .unwrap();
    fx.ledgers.harvest_and_clear("child").await;

    let ledger = fx.ledgers.ledger(&fx.session).unwrap();
    assert_eq!(
        ledger
            .intervals()
            .iter()
            .find(|i| i.tool_call_id == "child-call")
            .expect("the harvested interval")
            .node_id,
        0
    );
}
